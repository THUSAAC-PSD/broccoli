#[cfg(target_arch = "wasm32")]
mod plugin {
    use broccoli_server_sdk::prelude::*;
    use broccoli_server_sdk::types::ConfigSource;
    use extism_pdk::{FnResult, plugin_fn};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Default, Deserialize)]
    struct CooldownConfig {
        #[serde(default)]
        cooldown_seconds: Option<u32>,
    }

    #[derive(Deserialize)]
    struct SecondsSinceLast {
        seconds_since_last: Option<i64>,
    }

    #[plugin_fn]
    pub fn check_cooldown(input: String) -> FnResult<String> {
        let host = Host::new();
        let event: BeforeSubmissionEvent = serde_json::from_str(&input)?;

        let eff = match host
            .config
            .get_effective("cooldown", event.problem_id, event.contest_id)
        {
            Ok(e) => e,
            Err(e) => {
                let _ = host.log.info(&format!(
                    "[cooldown] Failed to resolve config: {e}, using default (disabled)"
                ));
                return Ok(serde_json::to_string(
                    &serde_json::json!({"action": "pass"}),
                )?);
            }
        };

        if !eff.is_enabled {
            return Ok(serde_json::to_string(
                &serde_json::json!({"action": "pass"}),
            )?);
        }

        let config: CooldownConfig = eff.parse_config().unwrap_or_default();
        let cooldown = config.cooldown_seconds.unwrap_or(0);

        if cooldown == 0 {
            return Ok(serde_json::to_string(
                &serde_json::json!({"action": "pass"}),
            )?);
        }

        let mut p = Params::new();
        let contest_filter = match event.contest_id {
            Some(cid) => format!("AND contest_id = {}", p.bind(cid)),
            None => "AND contest_id IS NULL".to_string(),
        };
        let sql = format!(
            "SELECT EXTRACT(EPOCH FROM (NOW() - MAX(created_at)))::int as seconds_since_last \
             FROM submission \
             WHERE user_id = {} AND problem_id = {} {}",
            p.bind(event.user_id),
            p.bind(event.problem_id),
            contest_filter
        );
        let seconds_since_last = host
            .db
            .query_one_with_args::<SecondsSinceLast>(&sql, &p.into_args())?
            .and_then(|r| r.seconds_since_last)
            .map(|s| s.max(0) as u64);

        // First submission — no cooldown
        if seconds_since_last.is_none() {
            return Ok(serde_json::to_string(
                &serde_json::json!({"action": "pass"}),
            )?);
        }

        let elapsed = seconds_since_last.unwrap();
        if elapsed < cooldown as u64 {
            let remaining = cooldown as u64 - elapsed;
            let resp = serde_json::json!({
                "action": "reject",
                "code": "COOLDOWN_ACTIVE",
                "message": format!("Please wait {} more second{}", remaining, if remaining == 1 { "" } else { "s" }),
                "status_code": 429,
                "details": {
                    "remaining_seconds": remaining,
                    "cooldown_seconds": cooldown,
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
        /// Whether the cooldown plugin is enabled for this resource.
        enabled: bool,
        cooldown_seconds: u32,
        seconds_since_last: Option<i64>,
        can_submit: bool,
        source: ConfigSource,
    }

    // API: GET /api/plugins/cooldown/contests/{contest_id}/problems/{problem_id}/status
    #[plugin_fn]
    pub fn get_cooldown_status(input: String) -> FnResult<String> {
        run_api_handler(&input, handle_cooldown_status)
    }

    // API: GET /api/plugins/cooldown/problems/{problem_id}/status
    #[plugin_fn]
    pub fn get_cooldown_status_standalone(input: String) -> FnResult<String> {
        run_api_handler(&input, handle_cooldown_status)
    }

    fn handle_cooldown_status(
        host: &Host,
        req: &PluginHttpRequest,
    ) -> Result<PluginHttpResponse, ApiError> {
        let user_id = req
            .require_user_id()
            .map_err(|_| PluginHttpResponse::error(401, "Authentication required"))?;

        let contest_id: Option<i32> = req.params.get("contest_id").and_then(|s| s.parse().ok());
        let problem_id: i32 = req.param("problem_id")?;

        // Contest access check (only when contest_id is present)
        if let Some(contest_id) = contest_id {
            contest::check_problem_access(host, req, contest_id, user_id, problem_id)?;
        }

        let eff = host
            .config
            .get_effective("cooldown", problem_id, contest_id)?;

        if !eff.is_enabled {
            return Ok(PluginHttpResponse {
                status: 200,
                headers: None,
                body: Some(serde_json::to_value(CooldownStatusResponse {
                    enabled: false,
                    cooldown_seconds: 0,
                    seconds_since_last: None,
                    can_submit: true,
                    source: eff.source,
                })?),
            });
        }

        let config: CooldownConfig = eff.parse_config().unwrap_or_default();
        let cooldown = config.cooldown_seconds.unwrap_or(0);

        let mut p = Params::new();
        let contest_filter = match contest_id {
            Some(cid) => format!("AND contest_id = {}", p.bind(cid)),
            None => "AND contest_id IS NULL".to_string(),
        };
        let sql = format!(
            "SELECT EXTRACT(EPOCH FROM (NOW() - MAX(created_at)))::int as seconds_since_last \
             FROM submission \
             WHERE user_id = {} AND problem_id = {} {}",
            p.bind(user_id),
            p.bind(problem_id),
            contest_filter
        );
        let seconds_since_last = host
            .db
            .query_one_with_args::<SecondsSinceLast>(&sql, &p.into_args())?
            .and_then(|r| r.seconds_since_last);

        let can_submit = if cooldown == 0 {
            true
        } else {
            match seconds_since_last {
                None => true, // first submission
                Some(s) => s.max(0) as u64 >= cooldown as u64,
            }
        };

        Ok(PluginHttpResponse {
            status: 200,
            headers: None,
            body: Some(serde_json::to_value(CooldownStatusResponse {
                enabled: true,
                cooldown_seconds: cooldown,
                seconds_since_last,
                can_submit,
                source: eff.source,
            })?),
        })
    }
}
