#[cfg(target_arch = "wasm32")]
mod plugin {
    use broccoli_server_sdk::prelude::*;
    use extism_pdk::{FnResult, plugin_fn};
    use serde::{Deserialize, Serialize};

    use broccoli_server_sdk::types::ConfigResult;

    #[derive(Debug, Default, Deserialize)]
    struct LimitsConfig {
        #[serde(default)]
        max_submissions: Option<u32>,
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

    struct ResolvedLimit {
        max_submissions: u32,
        source: ConfigSource,
    }

    /// Try to extract a limit value from a config result.
    /// Returns `None` if the config is a manifest default (allowing cascade to continue).
    fn try_extract(result: &ConfigResult) -> Option<u32> {
        if result.is_default {
            return None;
        }
        serde_json::from_value::<LimitsConfig>(result.config.clone())
            .ok()
            .and_then(|c| c.max_submissions)
    }

    /// Resolve effective max_submissions by cascading: contest_problem > contest > problem > default.
    /// Returns 0 for "unlimited" (no limit enforced).
    ///
    /// When `contest_id` is `None` (non-contest submission), only problem and default scopes apply.
    fn resolve_max_submissions(
        host: &Host,
        contest_id: Option<i32>,
        problem_id: i32,
    ) -> Result<ResolvedLimit, SdkError> {
        if let Some(cid) = contest_id {
            let r = host.config.get_contest_problem(cid, problem_id, "limits")?;
            if let Some(max) = try_extract(&r) {
                return Ok(ResolvedLimit {
                    max_submissions: max,
                    source: ConfigSource::ContestProblem,
                });
            }

            let r = host.config.get_contest(cid, "limits")?;
            if let Some(max) = try_extract(&r) {
                return Ok(ResolvedLimit {
                    max_submissions: max,
                    source: ConfigSource::Contest,
                });
            }
        }

        let r = host.config.get_problem(problem_id, "limits")?;
        if let Some(max) = try_extract(&r) {
            return Ok(ResolvedLimit {
                max_submissions: max,
                source: ConfigSource::Problem,
            });
        }

        // Manifest defaults (from plugin.toml [config.limits.properties.max_submissions] default)
        let fallback: LimitsConfig = serde_json::from_value(r.config).unwrap_or_default();
        Ok(ResolvedLimit {
            max_submissions: fallback.max_submissions.unwrap_or(0),
            source: ConfigSource::Default,
        })
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

        let resolved = match resolve_max_submissions(&host, event.contest_id, event.problem_id) {
            Ok(r) => r,
            Err(e) => {
                let _ = host.log.info(&format!(
                    "[submission-limit] Failed to resolve config: {e}, using default (unlimited)"
                ));
                ResolvedLimit {
                    max_submissions: 0,
                    source: ConfigSource::Default,
                }
            }
        };

        // 0 means unlimited
        if resolved.max_submissions == 0 {
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

        if count >= resolved.max_submissions {
            let resp = serde_json::json!({
                "action": "reject",
                "code": "SUBMISSION_LIMIT_EXCEEDED",
                "message": format!("Submission limit reached ({}/{})", count, resolved.max_submissions),
                "status_code": 429,
                "details": {
                    "submissions_made": count,
                    "max_submissions": resolved.max_submissions,
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
        submissions_made: u32,
        max_submissions: u32,
        remaining: Option<u32>,
        unlimited: bool,
        /// Where the effective limit came from: "contest_problem", "contest", "problem", or "default"
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

        let resolved = resolve_max_submissions(host, contest_id, problem_id)?;
        let unlimited = resolved.max_submissions == 0;

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
            Some(resolved.max_submissions.saturating_sub(count))
        };

        Ok(PluginHttpResponse {
            status: 200,
            headers: None,
            body: Some(serde_json::to_value(LimitStatusResponse {
                submissions_made: count,
                max_submissions: resolved.max_submissions,
                remaining,
                unlimited,
                source: resolved.source,
            })?),
        })
    }
}
