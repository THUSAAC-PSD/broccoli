use broccoli_server_sdk::prelude::*;
use extism_pdk::{FnResult, plugin_fn};
use serde::{Deserialize, Serialize};

use host::config::ConfigResult;

#[derive(Debug, Default, Deserialize)]
struct CooldownConfig {
    #[serde(default)]
    cooldown_seconds: Option<u32>,
}

/// Where a resolved config value came from.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
enum ConfigSource {
    ContestProblem,
    Contest,
    Problem,
    Default,
}

struct ResolvedCooldown {
    cooldown_seconds: u32,
    source: ConfigSource,
}

/// Try to extract a cooldown value from a config result.
///
/// Returns `None` if the config is a manifest default (allowing cascade to continue).
fn try_extract(result: &ConfigResult) -> Option<u32> {
    if result.is_default {
        return None;
    }
    serde_json::from_value::<CooldownConfig>(result.config.clone())
        .ok()
        .and_then(|c| c.cooldown_seconds)
}

/// Resolve effective cooldown by cascading: contest_problem > contest > problem > default.
/// Returns 0 for "disabled" (no cooldown enforced).
fn resolve_cooldown(
    contest_id: Option<i32>,
    problem_id: i32,
) -> Result<ResolvedCooldown, SdkError> {
    if let Some(cid) = contest_id {
        let r = host::config::get_contest_problem_config(cid, problem_id, "cooldown")?;
        if let Some(secs) = try_extract(&r) {
            return Ok(ResolvedCooldown {
                cooldown_seconds: secs,
                source: ConfigSource::ContestProblem,
            });
        }

        let r = host::config::get_contest_config(cid, "cooldown")?;
        if let Some(secs) = try_extract(&r) {
            return Ok(ResolvedCooldown {
                cooldown_seconds: secs,
                source: ConfigSource::Contest,
            });
        }
    }

    let r = host::config::get_problem_config(problem_id, "cooldown")?;
    if let Some(secs) = try_extract(&r) {
        return Ok(ResolvedCooldown {
            cooldown_seconds: secs,
            source: ConfigSource::Problem,
        });
    }

    let fallback: CooldownConfig = serde_json::from_value(r.config).unwrap_or_default();
    Ok(ResolvedCooldown {
        cooldown_seconds: fallback.cooldown_seconds.unwrap_or(0),
        source: ConfigSource::Default,
    })
}

#[derive(Deserialize)]
struct SecondsSinceLast {
    seconds_since_last: Option<i64>,
}

#[plugin_fn]
pub fn check_cooldown(input: String) -> FnResult<String> {
    let event: BeforeSubmissionEvent = serde_json::from_str(&input)?;

    let resolved = match resolve_cooldown(event.contest_id, event.problem_id) {
        Ok(r) => r,
        Err(e) => {
            let _ = host::logger::log_info(format!(
                "[cooldown] Failed to resolve config: {e}, using default (disabled)"
            ));
            ResolvedCooldown {
                cooldown_seconds: 0,
                source: ConfigSource::Default,
            }
        }
    };

    if resolved.cooldown_seconds == 0 {
        return Ok(serde_json::to_string(
            &serde_json::json!({"action": "pass"}),
        )?);
    }

    let contest_filter = match event.contest_id {
        Some(cid) => format!("AND contest_id = {}", cid),
        None => "AND contest_id IS NULL".to_string(),
    };
    let rows: Vec<SecondsSinceLast> = host::db::db_query(&format!(
        "SELECT EXTRACT(EPOCH FROM (NOW() - MAX(created_at)))::int as seconds_since_last \
         FROM submission \
         WHERE user_id = {} AND problem_id = {} {}",
        event.user_id, event.problem_id, contest_filter
    ))?;

    let seconds_since_last = rows
        .first()
        .and_then(|r| r.seconds_since_last)
        .map(|s| s.max(0) as u64);

    // First submission, so no cooldown
    if seconds_since_last.is_none() {
        return Ok(serde_json::to_string(
            &serde_json::json!({"action": "pass"}),
        )?);
    }

    let elapsed = seconds_since_last.unwrap();
    if elapsed < resolved.cooldown_seconds as u64 {
        let remaining = resolved.cooldown_seconds as u64 - elapsed;
        let resp = serde_json::json!({
            "action": "reject",
            "code": "COOLDOWN_ACTIVE",
            "message": format!("Please wait {} more second{}", remaining, if remaining == 1 { "" } else { "s" }),
            "status_code": 429,
            "details": {
                "remaining_seconds": remaining,
                "cooldown_seconds": resolved.cooldown_seconds,
            }
        });
        return Ok(serde_json::to_string(&resp)?);
    }

    Ok(serde_json::to_string(
        &serde_json::json!({"action": "pass"}),
    )?)
}

#[derive(Serialize)]
struct CooldownStatusResponse {
    cooldown_seconds: u32,
    seconds_since_last: Option<i64>,
    can_submit: bool,
    /// Where the effective cooldown came from: "contest_problem", "contest", "problem", or "default"
    source: ConfigSource,
}

// API: GET /api/plugins/cooldown/contests/{contest_id}/problems/{problem_id}/status
#[plugin_fn]
pub fn get_cooldown_status(input: String) -> FnResult<String> {
    let resp = match handle_cooldown_status(&input) {
        Ok(r) => r,
        Err(e) => PluginHttpResponse {
            status: 500,
            headers: None,
            body: Some(serde_json::json!({ "error": format!("{e:?}") })),
        },
    };
    Ok(serde_json::to_string(&resp)?)
}

// API: GET /api/plugins/cooldown/problems/{problem_id}/status
#[plugin_fn]
pub fn get_cooldown_status_standalone(input: String) -> FnResult<String> {
    let resp = match handle_cooldown_status(&input) {
        Ok(r) => r,
        Err(e) => PluginHttpResponse {
            status: 500,
            headers: None,
            body: Some(serde_json::json!({ "error": format!("{e:?}") })),
        },
    };
    Ok(serde_json::to_string(&resp)?)
}

fn handle_cooldown_status(input: &str) -> Result<PluginHttpResponse, SdkError> {
    let req: PluginHttpRequest =
        serde_json::from_str(input).map_err(|e| SdkError::Serialization(e.to_string()))?;

    let user_id = match req.user_id() {
        Some(id) => id,
        None => {
            return Ok(PluginHttpResponse {
                status: 401,
                headers: None,
                body: Some(serde_json::json!({ "error": "Authentication required" })),
            });
        }
    };

    let contest_id: Option<i32> = req.params.get("contest_id").and_then(|s| s.parse().ok());

    let problem_id: i32 = req
        .params
        .get("problem_id")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| SdkError::Other("Missing problem_id".into()))?;

    // Contest access check (only when contest_id is present)
    if let Some(contest_id) = contest_id {
        #[derive(Deserialize)]
        struct ContestAccess {
            is_active: bool,
            is_participant: bool,
            has_problem: bool,
        }
        let access_rows: Vec<ContestAccess> = host::db::db_query(&format!(
            "SELECT \
                ((activate_time IS NULL OR activate_time <= NOW()) AND \
                 (deactivate_time IS NULL OR deactivate_time > NOW())) AS is_active, \
                EXISTS(SELECT 1 FROM contest_user WHERE contest_id = {contest_id} AND user_id = {user_id}) AS is_participant, \
                EXISTS(SELECT 1 FROM contest_problem WHERE contest_id = {contest_id} AND problem_id = {problem_id}) AS has_problem \
             FROM contest WHERE id = {contest_id}"
        ))?;
        let access = match access_rows.first() {
            Some(access) => access,
            None => {
                return Ok(PluginHttpResponse {
                    status: 404,
                    headers: None,
                    body: Some(serde_json::json!({ "error": "Contest not found" })),
                });
            }
        };
        if !access.has_problem
            || (!req.has_permission("contest:manage")
                && (!access.is_active || !access.is_participant))
        {
            return Ok(PluginHttpResponse {
                status: 404,
                headers: None,
                body: Some(serde_json::json!({ "error": "Contest not found" })),
            });
        }
    }

    let resolved = resolve_cooldown(contest_id, problem_id)?;

    let contest_filter = match contest_id {
        Some(cid) => format!("AND contest_id = {}", cid),
        None => "AND contest_id IS NULL".to_string(),
    };
    let rows: Vec<SecondsSinceLast> = host::db::db_query(&format!(
        "SELECT EXTRACT(EPOCH FROM (NOW() - MAX(created_at)))::int as seconds_since_last \
         FROM submission \
         WHERE user_id = {} AND problem_id = {} {}",
        user_id, problem_id, contest_filter
    ))?;

    let seconds_since_last = rows.first().and_then(|r| r.seconds_since_last);

    let can_submit = if resolved.cooldown_seconds == 0 {
        true
    } else {
        match seconds_since_last {
            None => true, // first submission
            Some(s) => s.max(0) as u64 >= resolved.cooldown_seconds as u64,
        }
    };

    Ok(PluginHttpResponse {
        status: 200,
        headers: None,
        body: Some(serde_json::to_value(CooldownStatusResponse {
            cooldown_seconds: resolved.cooldown_seconds,
            seconds_since_last,
            can_submit,
            source: resolved.source,
        })?),
    })
}
