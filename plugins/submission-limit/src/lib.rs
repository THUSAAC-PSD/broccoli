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
        run_api_handler(&input, handle_limit_status)
    }

    #[plugin_fn]
    pub fn get_limit_status_standalone(input: String) -> FnResult<String> {
        run_api_handler(&input, handle_limit_status)
    }

    fn handle_limit_status(
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
