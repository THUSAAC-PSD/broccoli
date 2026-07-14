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

    /// Plugin-owned claim table backing the atomic submission-limit gate.
    ///
    /// Counting `submission` rows and then passing is a check-then-insert TOCTOU:
    /// the server inserts the submission row only AFTER this hook returns, so N
    /// parallel POSTs all read the same COUNT, all pass, and the cap is exceeded.
    /// Instead each pass atomically increments a per-(user, problem, contest)
    /// counter in one statement, so Postgres serializes concurrent claims on the
    /// row and the (max+1)-th is rejected.
    ///
    /// `contest_id` uses 0 as the "no contest" sentinel (real contest ids start
    /// at 1) so standalone slots share a key under Postgres' NULLs-are-distinct
    /// unique semantics. The counter is authoritative and independent of the
    /// `submission` table, so it starts empty on first deploy of this gate.
    const SCHEMA_SQL: &str = "CREATE TABLE IF NOT EXISTS submission_limit_claim (\
         user_id    INTEGER NOT NULL, \
         problem_id INTEGER NOT NULL, \
         contest_id INTEGER NOT NULL DEFAULT 0, \
         count      INTEGER NOT NULL DEFAULT 0, \
         PRIMARY KEY (user_id, problem_id, contest_id))";

    fn ensure_schema(host: &Host) -> Result<(), SdkError> {
        host.db.execute(SCHEMA_SQL)?;
        Ok(())
    }

    /// Idempotent schema bootstrap, invoked by the host on plugin load.
    #[plugin_fn]
    pub fn init() -> FnResult<String> {
        let host = Host::new();
        ensure_schema(&host)?;
        Ok("ok".into())
    }

    /// The claim table's `contest_id` key: real id, or 0 for a standalone submission.
    fn contest_key(contest_id: Option<i32>) -> i32 {
        contest_id.unwrap_or(0)
    }

    /// Atomically claim a submission slot. Returns Ok(true) when the claim wins
    /// (the counter was below `max` and was incremented), Ok(false) when the cap
    /// is reached. Single statement: `ON CONFLICT DO UPDATE ... WHERE` re-checks
    /// the row version committed by a concurrent winner, so two racing POSTs can
    /// never both take the last slot.
    fn try_claim(
        host: &Host,
        user_id: i32,
        problem_id: i32,
        contest_key: i32,
        max: u32,
    ) -> Result<bool, SdkError> {
        let mut p = Params::new();
        let sql = format!(
            "INSERT INTO submission_limit_claim (user_id, problem_id, contest_id, count) \
             VALUES ({}, {}, {}, 1) \
             ON CONFLICT (user_id, problem_id, contest_id) DO UPDATE \
             SET count = submission_limit_claim.count + 1 \
             WHERE submission_limit_claim.count < {}",
            p.bind(user_id),
            p.bind(problem_id),
            p.bind(contest_key),
            p.bind(max)
        );
        Ok(host.db.execute_with_args(&sql, &p.into_args())? > 0)
    }

    /// The committed claim count for a slot (advisory: for the status endpoint and
    /// the rejection message; the atomic claim is the authority).
    fn claim_count(
        host: &Host,
        user_id: i32,
        problem_id: i32,
        contest_key: i32,
    ) -> Result<u32, SdkError> {
        let mut p = Params::new();
        let sql = format!(
            "SELECT count::bigint AS count FROM submission_limit_claim \
             WHERE user_id = {} AND problem_id = {} AND contest_id = {}",
            p.bind(user_id),
            p.bind(problem_id),
            p.bind(contest_key)
        );
        Ok(host
            .db
            .query_one_with_args::<SubmissionCount>(&sql, &p.into_args())?
            .map(|r| r.count)
            .unwrap_or(0) as u32)
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
                return Ok(serde_json::to_string(&HookResponse::pass())?);
            }
        };

        if !eff.is_enabled {
            return Ok(serde_json::to_string(&HookResponse::pass())?);
        }

        let config: LimitsConfig = eff.parse_config().unwrap_or_default();
        let max = config.max_submissions.unwrap_or(0);

        // 0 means unlimited
        if max == 0 {
            return Ok(serde_json::to_string(&HookResponse::pass())?);
        }

        // Atomically claim a slot: wins iff the committed count is below max.
        let ck = contest_key(event.contest_id);
        if try_claim(&host, event.user_id, event.problem_id, ck, max)? {
            return Ok(serde_json::to_string(&HookResponse::pass())?);
        }

        // Cap reached: report the committed count (which equals max).
        let count = claim_count(&host, event.user_id, event.problem_id, ck)?;
        let resp = HookResponse::reject(
            "SUBMISSION_LIMIT_EXCEEDED",
            format!("Submission limit reached ({}/{})", count, max),
            429,
            Some(serde_json::json!({
                "submissions_made": count,
                "max_submissions": max,
            })),
        );
        Ok(serde_json::to_string(&resp)?)
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

        // Read the same authoritative claim counter the gate enforces, so the
        // reported count matches what a submission would be checked against.
        let count = claim_count(host, user_id, problem_id, contest_key(contest_id))?;

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
