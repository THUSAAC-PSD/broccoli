#[cfg(target_arch = "wasm32")]
mod plugin {
    use broccoli_server_sdk::prelude::*;
    use broccoli_server_sdk::types::{AfterSubmissionEvent, ConfigSource};
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

    /// Seconds a PENDING claim counts toward the cap before it is reclaimed.
    ///
    /// A claim is made PENDING by `check_limit` in the `before_submission` hook
    /// and CONFIRMED by `on_submission_created` in the `after_submission` hook -
    /// which fires ONLY after the submission row durably commits. A submission
    /// rejected by a later hook (e.g. the cooldown gate) or a failed insert never
    /// confirms, so its pending claim must be reclaimed rather than permanently
    /// consuming a slot (the old monotonic counter never reclaimed, so such a
    /// failure permanently burned a slot). A successful submission is always
    /// counted regardless of confirm timing (see [`confirm_claim`], which bumps
    /// `confirmed` even if the pending was already reclaimed). The only tradeoff
    /// is a temporary false-lockout: after a failed claim the slot is held for up
    /// to this long. The dominant failure - a cooldown rejection after this gate
    /// already claimed - masks it: the user is serving that cooldown meanwhile.
    const PENDING_GRACE_SECS: i64 = 30;

    /// Plugin-owned single-row counter backing the atomic submission-limit gate.
    ///
    /// ONE row per (user, problem, contest): `confirmed` counts durably-committed
    /// submissions, `pending` counts in-flight claims not yet confirmed, and
    /// `oldest_pending_at` ages the pending set. A slot counts toward the cap as
    /// `confirmed + (pending while oldest_pending_at is within the grace window)`.
    ///
    /// Every mutation is a SINGLE atomic `INSERT .. ON CONFLICT DO UPDATE`
    /// statement, exactly like the original design: `ON CONFLICT` takes the row
    /// lock so concurrent claims serialize on that one row and cannot both take
    /// the last slot. Crucially this uses NO explicit transaction and NO advisory
    /// lock - the plugin host runs each `host.db.*` call via a blocking
    /// `block_on`, so holding a transaction open across several such calls (or
    /// blocking one on `pg_advisory_xact_lock`) starves the runtime's worker
    /// threads under concurrency and deadlocks. A single self-contained statement
    /// never holds a lock across host calls.
    ///
    /// `contest_id` uses 0 as the "no contest" sentinel (real ids start at 1).
    const SCHEMA_SQL: &str = "CREATE TABLE IF NOT EXISTS submission_limit_claim (\
         user_id           INTEGER NOT NULL, \
         problem_id        INTEGER NOT NULL, \
         contest_id        INTEGER NOT NULL DEFAULT 0, \
         confirmed         INTEGER NOT NULL DEFAULT 0, \
         pending           INTEGER NOT NULL DEFAULT 0, \
         oldest_pending_at TIMESTAMPTZ, \
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

    /// The claim key's `contest_id`: real id, or 0 for a standalone submission.
    fn contest_key(contest_id: Option<i32>) -> i32 {
        contest_id.unwrap_or(0)
    }

    /// Atomically claim a submission slot. Returns Ok(true) when the claim wins
    /// (`confirmed + live pending < max`, and a pending was recorded), Ok(false)
    /// when the cap is reached. One statement: `ON CONFLICT DO UPDATE ... WHERE`
    /// re-evaluates the cap against the row version a concurrent winner committed,
    /// so two racing POSTs can never both take the last slot. When
    /// `oldest_pending_at` is already past the grace window the pending set is
    /// reclaimed (reset to just this new claim) in the same statement.
    fn try_claim(
        host: &Host,
        user_id: i32,
        problem_id: i32,
        contest_key: i32,
        max: u32,
    ) -> Result<bool, SdkError> {
        let mut p = Params::new();
        // Bind order matters; the grace value is reused several times.
        let u = p.bind(user_id);
        let pr = p.bind(problem_id);
        let c = p.bind(contest_key);
        let g1 = p.bind(PENDING_GRACE_SECS);
        let g2 = p.bind(PENDING_GRACE_SECS);
        let g3 = p.bind(PENDING_GRACE_SECS);
        let m = p.bind(max);
        let sql = format!(
            "INSERT INTO submission_limit_claim \
               (user_id, problem_id, contest_id, confirmed, pending, oldest_pending_at) \
             VALUES ({u}, {pr}, {c}, 0, 1, NOW()) \
             ON CONFLICT (user_id, problem_id, contest_id) DO UPDATE SET \
               pending = CASE \
                 WHEN submission_limit_claim.oldest_pending_at IS NULL \
                   OR submission_limit_claim.oldest_pending_at <= NOW() - ({g1} * INTERVAL '1 second') \
                 THEN 1 ELSE submission_limit_claim.pending + 1 END, \
               oldest_pending_at = CASE \
                 WHEN submission_limit_claim.oldest_pending_at IS NULL \
                   OR submission_limit_claim.oldest_pending_at <= NOW() - ({g2} * INTERVAL '1 second') \
                 THEN NOW() ELSE submission_limit_claim.oldest_pending_at END \
             WHERE submission_limit_claim.confirmed + (CASE \
                 WHEN submission_limit_claim.oldest_pending_at IS NOT NULL \
                   AND submission_limit_claim.oldest_pending_at > NOW() - ({g3} * INTERVAL '1 second') \
                 THEN submission_limit_claim.pending ELSE 0 END) < {m}"
        );
        Ok(host.db.execute_with_args(&sql, &p.into_args())? > 0)
    }

    /// Confirm a durably-committed submission: bump `confirmed` and release one
    /// pending. Single atomic statement. If no row exists yet (a confirm arriving
    /// before/without a recorded pending) it inserts `confirmed = 1`, so a
    /// successful submission is NEVER left uncounted regardless of timing.
    fn confirm_claim(
        host: &Host,
        user_id: i32,
        problem_id: i32,
        contest_key: i32,
    ) -> Result<(), SdkError> {
        let mut p = Params::new();
        let sql = format!(
            "INSERT INTO submission_limit_claim \
               (user_id, problem_id, contest_id, confirmed, pending, oldest_pending_at) \
             VALUES ({}, {}, {}, 1, 0, NULL) \
             ON CONFLICT (user_id, problem_id, contest_id) DO UPDATE SET \
               confirmed = submission_limit_claim.confirmed + 1, \
               pending = GREATEST(submission_limit_claim.pending - 1, 0), \
               oldest_pending_at = CASE \
                 WHEN GREATEST(submission_limit_claim.pending - 1, 0) = 0 \
                 THEN NULL ELSE submission_limit_claim.oldest_pending_at END",
            p.bind(user_id),
            p.bind(problem_id),
            p.bind(contest_key)
        );
        host.db.execute_with_args(&sql, &p.into_args())?;
        Ok(())
    }

    /// The active slot count: `confirmed` plus `pending` while the pending set is
    /// still within the grace window (advisory: for the status endpoint and the
    /// rejection message; the atomic claim is the authority).
    fn active_count(
        host: &Host,
        user_id: i32,
        problem_id: i32,
        contest_key: i32,
    ) -> Result<u32, SdkError> {
        let mut p = Params::new();
        let sql = format!(
            "SELECT (confirmed + CASE \
                 WHEN oldest_pending_at IS NOT NULL \
                   AND oldest_pending_at > NOW() - ({} * INTERVAL '1 second') \
                 THEN pending ELSE 0 END)::bigint AS count \
             FROM submission_limit_claim \
             WHERE user_id = {} AND problem_id = {} AND contest_id = {}",
            p.bind(PENDING_GRACE_SECS),
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

        let ck = contest_key(event.contest_id);
        if try_claim(&host, event.user_id, event.problem_id, ck, max)? {
            return Ok(serde_json::to_string(&HookResponse::pass())?);
        }

        // Cap reached: report the active count (which equals max).
        let count = active_count(&host, event.user_id, event.problem_id, ck)?;
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

    /// Confirm the submission's pending claim once it has durably committed.
    ///
    /// The host fires `after_submission` ONLY after the submission row is inserted
    /// and committed. A claim that never reaches this point stays pending and is
    /// reclaimed by [`try_claim`] after the grace window instead of permanently
    /// consuming a slot.
    #[plugin_fn]
    pub fn on_submission_created(input: String) -> FnResult<String> {
        let host = Host::new();
        let event: AfterSubmissionEvent = serde_json::from_str(&input)?;

        // Only confirm when the gate is actually active for this resource,
        // mirroring `check_limit`. If the limit is disabled or unlimited (max=0),
        // `check_limit` short-circuited and took NO claim, so there is nothing to
        // confirm - confirming anyway would create a phantom count for a
        // submission that was never limited.
        let eff = match host
            .config
            .get_effective("limits", event.problem_id, event.contest_id)
        {
            Ok(e) => e,
            Err(_) => return Ok(serde_json::to_string(&HookResponse::pass())?),
        };
        if !eff.is_enabled {
            return Ok(serde_json::to_string(&HookResponse::pass())?);
        }
        let config: LimitsConfig = eff.parse_config().unwrap_or_default();
        if config.max_submissions.unwrap_or(0) == 0 {
            return Ok(serde_json::to_string(&HookResponse::pass())?);
        }

        confirm_claim(
            &host,
            event.user_id,
            event.problem_id,
            contest_key(event.contest_id),
        )?;
        Ok(serde_json::to_string(&HookResponse::pass())?)
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

        // Read the same active count the gate enforces, so the reported count
        // matches what a submission would be checked against.
        let count = active_count(host, user_id, problem_id, contest_key(contest_id))?;

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
