#[cfg(target_arch = "wasm32")]
mod plugin {
    use broccoli_server_sdk::error::SdkError;
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

    /// Plugin-owned claim table backing the atomic cooldown gate.
    ///
    /// Reading `MAX(created_at) FROM submission` and then passing is a classic
    /// check-then-insert TOCTOU: the server inserts the submission row only
    /// AFTER this hook returns, so N parallel POSTs all see the old
    /// `MAX(created_at)`, all pass, and the cooldown is bypassed entirely.
    /// Instead, each pass must atomically claim the (user, problem, contest)
    /// slot in a single statement (`INSERT .. ON CONFLICT DO UPDATE .. WHERE`)
    /// so Postgres serializes concurrent claims on the row and exactly one
    /// submission can win per cooldown window.
    ///
    /// `contest_id` uses 0 as the "no contest" sentinel (real contest ids start
    /// at 1) so it can participate in the primary key; a nullable column would
    /// make every standalone (user, problem) pair a distinct key under
    /// Postgres' NULLs-are-distinct unique semantics.
    const SCHEMA_SQL: &str = "CREATE TABLE IF NOT EXISTS cooldown_claim (\
         user_id       INTEGER NOT NULL, \
         problem_id    INTEGER NOT NULL, \
         contest_id    INTEGER NOT NULL DEFAULT 0, \
         last_claim_at TIMESTAMPTZ NOT NULL DEFAULT NOW(), \
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

    /// Atomically claim the right to submit. Returns Ok(true) when the claim
    /// wins (first submission ever, or the cooldown has fully elapsed) and the
    /// slot's timestamp was advanced to NOW(); Ok(false) when the cooldown is
    /// still active. Single statement, so concurrent claims cannot interleave:
    /// `ON CONFLICT DO UPDATE` re-evaluates the `WHERE` against the row version
    /// committed by a concurrent winner.
    fn try_claim(
        host: &Host,
        user_id: i32,
        problem_id: i32,
        contest_key: i32,
        cooldown: u32,
    ) -> Result<bool, SdkError> {
        let mut p = Params::new();
        let sql = format!(
            "INSERT INTO cooldown_claim (user_id, problem_id, contest_id, last_claim_at) \
             VALUES ({}, {}, {}, NOW()) \
             ON CONFLICT (user_id, problem_id, contest_id) DO UPDATE \
             SET last_claim_at = NOW() \
             WHERE cooldown_claim.last_claim_at <= NOW() - ({} * INTERVAL '1 second')",
            p.bind(user_id),
            p.bind(problem_id),
            p.bind(contest_key),
            p.bind(cooldown)
        );
        Ok(host.db.execute_with_args(&sql, &p.into_args())? > 0)
    }

    /// How long until the claimed slot frees up, for the rejection message.
    /// Advisory only — the atomic claim is the authority.
    fn remaining_from_claim(
        host: &Host,
        user_id: i32,
        problem_id: i32,
        contest_key: i32,
        cooldown: u32,
    ) -> Result<u64, SdkError> {
        #[derive(Deserialize)]
        struct RemainingRow {
            remaining: Option<i64>,
        }
        let mut p = Params::new();
        let sql = format!(
            "SELECT CEIL(EXTRACT(EPOCH FROM \
                (last_claim_at + ({} * INTERVAL '1 second') - NOW())))::bigint AS remaining \
             FROM cooldown_claim \
             WHERE user_id = {} AND problem_id = {} AND contest_id = {}",
            p.bind(cooldown),
            p.bind(user_id),
            p.bind(problem_id),
            p.bind(contest_key)
        );
        Ok(host
            .db
            .query_one_with_args::<RemainingRow>(&sql, &p.into_args())?
            .and_then(|r| r.remaining)
            .map(|r| r.clamp(1, cooldown as i64) as u64)
            .unwrap_or(1))
    }

    fn reject_response(remaining: u64, cooldown: u32) -> FnResult<String> {
        let resp = HookResponse::reject(
            "COOLDOWN_ACTIVE",
            format!(
                "Please wait {} more second{}",
                remaining,
                if remaining == 1 { "" } else { "s" }
            ),
            429,
            Some(serde_json::json!({
                "remaining_seconds": remaining,
                "cooldown_seconds": cooldown,
            })),
        );
        Ok(serde_json::to_string(&resp)?)
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
                return Ok(serde_json::to_string(&HookResponse::pass())?);
            }
        };

        if !eff.is_enabled {
            return Ok(serde_json::to_string(&HookResponse::pass())?);
        }

        let config: CooldownConfig = eff.parse_config().unwrap_or_default();
        let cooldown = config.cooldown_seconds.unwrap_or(0);

        if cooldown == 0 {
            return Ok(serde_json::to_string(&HookResponse::pass())?);
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

        // Fast pre-reject against actual submissions: keeps accurate messages
        // and honors history that predates the claim table. Advisory only —
        // passing here is NOT sufficient, since concurrent requests all pass
        // this read before any of their inserts commit.
        if let Some(elapsed) = seconds_since_last {
            if elapsed < cooldown as u64 {
                return reject_response(cooldown as u64 - elapsed, cooldown);
            }
        }

        // Authoritative gate: atomically claim the (user, problem, contest)
        // slot. Exactly one of N concurrent submissions can win the claim per
        // cooldown window; a "first submission" must claim too, or N parallel
        // first submissions would all pass. Retry once after an idempotent
        // schema bootstrap so a failed/missed init() cannot wedge submissions.
        let contest_key = event.contest_id.unwrap_or(0);
        let claimed = match try_claim(
            &host,
            event.user_id,
            event.problem_id,
            contest_key,
            cooldown,
        ) {
            Ok(claimed) => claimed,
            Err(e) => {
                let _ = host.log.info(&format!(
                    "[cooldown] claim failed ({e}), re-ensuring schema and retrying"
                ));
                ensure_schema(&host)?;
                try_claim(
                    &host,
                    event.user_id,
                    event.problem_id,
                    contest_key,
                    cooldown,
                )?
            }
        };

        if !claimed {
            let remaining = remaining_from_claim(
                &host,
                event.user_id,
                event.problem_id,
                contest_key,
                cooldown,
            )
            .unwrap_or(1);
            return reject_response(remaining, cooldown);
        }

        Ok(serde_json::to_string(&HookResponse::pass())?)
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

    /// Seconds since the last cooldown-relevant event: the newer of the last
    /// actual submission and the last successful claim for this
    /// (user, problem, contest) slot.
    fn query_status_elapsed(
        host: &Host,
        user_id: i32,
        problem_id: i32,
        contest_id: Option<i32>,
    ) -> Result<Option<i64>, SdkError> {
        let mut p = Params::new();
        let contest_filter = match contest_id {
            Some(cid) => format!("AND contest_id = {}", p.bind(cid)),
            None => "AND contest_id IS NULL".to_string(),
        };
        let sql = format!(
            "SELECT EXTRACT(EPOCH FROM (NOW() - GREATEST( \
                (SELECT MAX(created_at) FROM submission \
                 WHERE user_id = {} AND problem_id = {} {}), \
                (SELECT MAX(last_claim_at) FROM cooldown_claim \
                 WHERE user_id = {} AND problem_id = {} AND contest_id = {}) \
             )))::int as seconds_since_last",
            p.bind(user_id),
            p.bind(problem_id),
            contest_filter,
            p.bind(user_id),
            p.bind(problem_id),
            p.bind(contest_id.unwrap_or(0))
        );
        Ok(host
            .db
            .query_one_with_args::<SecondsSinceLast>(&sql, &p.into_args())?
            .and_then(|r| r.seconds_since_last))
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

        // Mirror the gate in check_cooldown: the clock runs from the newer of
        // the last actual submission and the last successful claim (GREATEST
        // ignores NULLs), so the status shown to the contestant cannot claim
        // "can submit" while the atomic gate would still reject.
        let seconds_since_last = match query_status_elapsed(host, user_id, problem_id, contest_id) {
            Ok(v) => v,
            Err(_) => {
                // Likely a missing claim table (failed/missed init); bootstrap
                // is idempotent, then retry once.
                ensure_schema(host)?;
                query_status_elapsed(host, user_id, problem_id, contest_id)?
            }
        };

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
