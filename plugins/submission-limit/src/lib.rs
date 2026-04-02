#[cfg(target_arch = "wasm32")]
mod plugin {
    use broccoli_server_sdk::prelude::*;
    use broccoli_server_sdk::types::ConfigSource;
    use extism_pdk::{FnResult, plugin_fn};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Default, Deserialize)]
    struct LimitsConfig {
        #[serde(default)]
        max_submissions: Option<u32>,
    }

    #[derive(Deserialize)]
    struct SubmissionCount {
        count: i64,
    }

    /// Check whether the user has exceeded the submission limit for this problem.
    #[plugin_fn]
    pub fn check_limit(input: String) -> FnResult<String> {
        let host = Host::new();
        let event: BeforeSubmissionEvent = serde_json::from_str(&input)?;

        let eff = match host
            .config
            .get_effective("limits", event.problem_id, event.contest_id)
        {
            Ok(e) => e,
            Err(e) => {
                let _ = host.log.info(&format!(
                    "[submission-limit] Failed to resolve config: {e}, using default (unlimited)"
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

        let config: LimitsConfig = eff.parse_config().unwrap_or_default();
        let max = config.max_submissions.unwrap_or(0);

        // 0 means unlimited
        if max == 0 {
            return Ok(serde_json::to_string(
                &serde_json::json!({"action": "pass"}),
            )?);
        }

        // Query submission count for this user/problem, scoped to contest if present.
        let mut p = Params::new();
        let contest_filter = match event.contest_id {
            Some(cid) => format!("AND contest_id = {}", p.bind(cid)),
            None => "AND contest_id IS NULL".to_string(),
        };
        let sql = format!(
            "SELECT COUNT(*) as count \
             FROM submission \
             WHERE user_id = {} AND problem_id = {} {}",
            p.bind(event.user_id),
            p.bind(event.problem_id),
            contest_filter
        );
        let count = host
            .db
            .query_one_with_args::<SubmissionCount>(&sql, &p.into_args())?
            .map(|r| r.count)
            .unwrap_or(0) as u32;

        if count >= max {
            let resp = serde_json::json!({
                "action": "reject",
                "code": "SUBMISSION_LIMIT_EXCEEDED",
                "message": format!("Submission limit reached ({}/{})", count, max),
                "status_code": 429,
                "details": {
                    "submissions_made": count,
                    "max_submissions": max,
                }
            });
            return Ok(serde_json::to_string(&resp)?);
        }

        Ok(serde_json::to_string(
            &serde_json::json!({"action": "pass"}),
        )?)
    }

    // API: GET /api/plugins/submission-limit/contests/{contest_id}/problems/{problem_id}/status
    // API: GET /api/plugins/submission-limit/problems/{problem_id}/status

    #[derive(Serialize)]
    struct LimitStatusResponse {
        /// Whether the submission-limit plugin is enabled for this resource.
        enabled: bool,
        submissions_made: u32,
        max_submissions: u32,
        remaining: Option<u32>,
        unlimited: bool,
        source: ConfigSource,
    }

    #[plugin_fn]
    pub fn get_limit_status(input: String) -> FnResult<String> {
        let host = Host::new();
        let resp = match handle_limit_status(&host, &input) {
            Ok(r) => r,
            Err(e) => PluginHttpResponse {
                status: 500,
                headers: None,
                body: Some(serde_json::json!({ "error": format!("{e:?}") })),
            },
        };
        Ok(serde_json::to_string(&resp)?)
    }

    #[plugin_fn]
    pub fn get_limit_status_standalone(input: String) -> FnResult<String> {
        let host = Host::new();
        let resp = match handle_limit_status(&host, &input) {
            Ok(r) => r,
            Err(e) => PluginHttpResponse {
                status: 500,
                headers: None,
                body: Some(serde_json::json!({ "error": format!("{e:?}") })),
            },
        };
        Ok(serde_json::to_string(&resp)?)
    }

    fn handle_limit_status(host: &Host, input: &str) -> Result<PluginHttpResponse, SdkError> {
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
            let mut p = Params::new();
            let sql = format!(
                "SELECT \
                    ((activate_time IS NULL OR activate_time <= NOW()) AND \
                     (deactivate_time IS NULL OR deactivate_time > NOW())) AS is_active, \
                    EXISTS(SELECT 1 FROM contest_user WHERE contest_id = {} AND user_id = {}) AS is_participant, \
                    EXISTS(SELECT 1 FROM contest_problem WHERE contest_id = {} AND problem_id = {}) AS has_problem \
                 FROM contest WHERE id = {}",
                p.bind(contest_id),
                p.bind(user_id),
                p.bind(contest_id),
                p.bind(problem_id),
                p.bind(contest_id),
            );
            let access = match host
                .db
                .query_one_with_args::<ContestAccess>(&sql, &p.into_args())?
            {
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

        let eff = host
            .config
            .get_effective("limits", problem_id, contest_id)?;

        if !eff.is_enabled {
            return Ok(PluginHttpResponse {
                status: 200,
                headers: None,
                body: Some(serde_json::to_value(LimitStatusResponse {
                    enabled: false,
                    submissions_made: 0,
                    max_submissions: 0,
                    remaining: None,
                    unlimited: true,
                    source: eff.source,
                })?),
            });
        }

        let config: LimitsConfig = eff.parse_config().unwrap_or_default();
        let max = config.max_submissions.unwrap_or(0);
        let unlimited = max == 0;

        let mut p = Params::new();
        let contest_filter = match contest_id {
            Some(cid) => format!("AND contest_id = {}", p.bind(cid)),
            None => "AND contest_id IS NULL".to_string(),
        };
        let sql = format!(
            "SELECT COUNT(*) as count \
             FROM submission \
             WHERE user_id = {} AND problem_id = {} {}",
            p.bind(user_id),
            p.bind(problem_id),
            contest_filter
        );
        let count = host
            .db
            .query_one_with_args::<SubmissionCount>(&sql, &p.into_args())?
            .map(|r| r.count)
            .unwrap_or(0) as u32;

        let remaining = if unlimited {
            None
        } else {
            Some(max.saturating_sub(count))
        };

        Ok(PluginHttpResponse {
            status: 200,
            headers: None,
            body: Some(serde_json::to_value(LimitStatusResponse {
                enabled: true,
                submissions_made: count,
                max_submissions: max,
                remaining,
                unlimited,
                source: eff.source,
            })?),
        })
    }
}
