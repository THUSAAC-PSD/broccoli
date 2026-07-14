use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex, OnceLock, TryLockError};
use std::time::{Duration, Instant};

use extism::host_fn;
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DatabaseTransaction, DbBackend,
    Statement, TransactionTrait,
};
use serde::Serialize;
use serde_json::Value as JsonValue;
use tracing::error;
use uuid::Uuid;

/// A plugin-opened DB transaction plus the bookkeeping needed to keep it from
/// leaking or being hijacked: the owning plugin (txn ids are only usable by the
/// plugin that opened them) and the last time the plugin touched it (drives the
/// idle-TTL reaper).
#[derive(Debug)]
pub struct PluginTransaction {
    txn: DatabaseTransaction,
    plugin_id: String,
    last_used: Instant,
}

pub type TransactionMap = Arc<StdMutex<HashMap<String, PluginTransaction>>>;

/// How long an open plugin transaction may sit untouched before the reaper
/// rolls it back and returns its pooled connection. Every db_query_in /
/// db_execute_in refreshes the timestamp, so a live transaction is only at
/// risk if the guest goes silent for the whole window. Sized at 2x the default
/// plugin `call_timeout_secs` (300s): a transaction legitimately held across a
/// call can idle for at most one call timeout, so anything past this is an
/// orphan from a trapped/killed guest (WASM OOM, host timeout, panic) whose
/// commit/rollback will never arrive.
const TXN_IDLE_TTL: Duration = Duration::from_secs(600);

/// How often each per-transaction reaper task re-checks its transaction.
const TXN_REAP_INTERVAL: Duration = Duration::from_secs(30);

/// Maximum transactions a single plugin may hold open concurrently. Bounds the
/// damage between reaper sweeps: without a cap, one misbehaving plugin that
/// traps after `db_begin` in a loop can check out the entire plugin DB pool
/// (20 connections by default) and starve every other plugin's DB access.
const MAX_OPEN_TXNS_PER_PLUGIN: usize = 8;

/// Watchdog for one plugin transaction: once the transaction has been idle for
/// [`TXN_IDLE_TTL`] it is removed from the map and rolled back, returning its
/// pooled connection. This is the only cleanup path when a guest traps between
/// `db_begin` and `db_commit`/`db_rollback` — the host_fn-side commit/rollback
/// removals never run for a dead guest, and a leaked `DatabaseTransaction`
/// pins a plugin-pool connection forever. The task exits as soon as the
/// transaction is committed/rolled back (entry gone) or reaped.
fn spawn_txn_reaper(txn_map: TransactionMap, txn_id: String) {
    spawn_txn_reaper_with(txn_map, txn_id, TXN_IDLE_TTL, TXN_REAP_INTERVAL)
}

/// Inner helper split out from [`spawn_txn_reaper`] so the reaping behaviour
/// is testable with short timings instead of the production 10-minute TTL.
fn spawn_txn_reaper_with(
    txn_map: TransactionMap,
    txn_id: String,
    idle_ttl: Duration,
    reap_interval: Duration,
) {
    tokio::runtime::Handle::current().spawn(async move {
        loop {
            tokio::time::sleep(reap_interval).await;
            let orphaned = {
                // try_lock: db_query_in/db_execute_in hold this mutex across
                // their blocking DB call, so a contended lock means some
                // transaction is actively in use. Skip the tick instead of
                // parking a runtime worker on a std mutex.
                let mut guard = match txn_map.try_lock() {
                    Ok(guard) => guard,
                    // A host_fn thread panicked while holding the lock; the
                    // transactions still need reaping, and the map itself is
                    // just a HashMap that is valid at every await-free point.
                    Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
                    Err(TryLockError::WouldBlock) => continue,
                };
                match guard.get(&txn_id) {
                    // Committed or rolled back through the normal path.
                    None => return,
                    Some(entry) if entry.last_used.elapsed() < idle_ttl => None,
                    Some(_) => guard.remove(&txn_id),
                }
            };
            if let Some(entry) = orphaned {
                tracing::warn!(
                    plugin_id = %entry.plugin_id,
                    txn_id = %txn_id,
                    idle_ttl_secs = idle_ttl.as_secs(),
                    "Rolling back orphaned plugin DB transaction (guest never committed or rolled back; likely trapped)"
                );
                if let Err(e) = entry.txn.rollback().await {
                    error!(txn_id = %txn_id, "Failed to roll back orphaned plugin DB transaction: {}", e);
                }
                return;
            }
        }
    });
}

/// Database URL used to open a FRESH one-shot connection as the guaranteed
/// last-resort fallback when pool retries are all exhausted by poisoned-
/// connection 0x00 errors. Set once at startup (see runtime.rs). A brand-new
/// connection has no prior in-flight query, so its write buffer / protocol
/// state are clean by construction — it CANNOT be poisoned — making the insert
/// succeed deterministically (modulo a genuine DB failure).
static PLUGIN_DB_FALLBACK_URL: OnceLock<String> = OnceLock::new();

/// Register the URL for the fresh-connection fallback. Idempotent.
pub fn set_plugin_db_fallback_url(url: String) {
    let _ = PLUGIN_DB_FALLBACK_URL.set(url);
}

/// Open a fresh single connection and execute `stmt` on it, then drop it. The
/// connection is brand-new, so it is guaranteed free of stale wire-protocol
/// framing from any prior operation — the guaranteed cure for a poisoned-
/// connection 0x00 when the pool retries are exhausted.
fn execute_on_fresh_connection(stmt: Statement) -> Result<sea_orm::ExecResult, sea_orm::DbErr> {
    let url = PLUGIN_DB_FALLBACK_URL
        .get()
        .ok_or_else(|| sea_orm::DbErr::Custom("plugin DB fallback URL not set".to_string()))?
        .clone();
    tokio::runtime::Handle::current().block_on(async move {
        let mut opt = ConnectOptions::new(url);
        opt.max_connections(1)
            .min_connections(1)
            .sqlx_logging(false);
        let fresh = Database::connect(opt).await?;
        let result = fresh.execute_raw(stmt).await;
        fresh.close().await.ok();
        result
    })
}

/// Read-side counterpart to [`execute_on_fresh_connection`]: open a fresh single
/// connection and run `stmt` (a json_agg-wrapped query) on it, then drop it.
/// `db_query` previously had only the probabilistic pool-retry layer; under
/// sustained poisoning the pool can keep handing back the same desynced
/// connection, so a query could still surface a `0x00` error to the plugin — and
/// a plugin DB error becomes a `SystemError`. A brand-new connection is clean by
/// construction, turning that into a deterministic recovery, matching the
/// guarantee the write path already has.
fn query_on_fresh_connection(
    stmt: Statement,
) -> Result<Option<sea_orm::QueryResult>, sea_orm::DbErr> {
    let url = PLUGIN_DB_FALLBACK_URL
        .get()
        .ok_or_else(|| sea_orm::DbErr::Custom("plugin DB fallback URL not set".to_string()))?
        .clone();
    query_on_fresh_connection_url(&url, stmt)
}

/// Inner helper split out from [`query_on_fresh_connection`] so the
/// fresh-connection behaviour is testable against a real database without
/// depending on the process-global `PLUGIN_DB_FALLBACK_URL` `OnceLock`.
fn query_on_fresh_connection_url(
    url: &str,
    stmt: Statement,
) -> Result<Option<sea_orm::QueryResult>, sea_orm::DbErr> {
    let url = url.to_string();
    tokio::runtime::Handle::current().block_on(async move {
        let mut opt = ConnectOptions::new(url);
        opt.max_connections(1)
            .min_connections(1)
            .sqlx_logging(false);
        let fresh = Database::connect(opt).await?;
        let result = fresh.query_one_raw(stmt).await;
        fresh.close().await.ok();
        result
    })
}

/// Look up a transaction by id, enforcing that the caller is the plugin that
/// opened it. Txn ids are unguessable UUIDs, but capability boundaries must
/// not rest on unguessability alone: a leaked id (logs, shared guest memory)
/// must not let one plugin commit, roll back, or query inside another
/// plugin's transaction. A foreign txn id is reported exactly like a
/// nonexistent one so plugins cannot probe for other plugins' transactions.
fn lookup_owned_txn<'a>(
    map: &'a mut HashMap<String, PluginTransaction>,
    txn_id: &str,
    plugin_id: &str,
    host_fn: &str,
) -> Result<&'a mut PluginTransaction, extism::Error> {
    match map.get_mut(txn_id) {
        Some(entry) if entry.plugin_id == plugin_id => Ok(entry),
        Some(entry) => {
            tracing::warn!(
                plugin_id,
                owner = %entry.plugin_id,
                txn_id,
                host_fn,
                "Plugin attempted to use a DB transaction it does not own"
            );
            Err(extism::Error::msg(format!(
                "Transaction not found: {txn_id}"
            )))
        }
        None => Err(extism::Error::msg(format!(
            "Transaction not found: {txn_id}"
        ))),
    }
}

#[derive(Serialize)]
struct HostDbResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl HostDbResponse {
    fn ok(data: JsonValue) -> Self {
        Self {
            data: Some(data),
            error: None,
        }
    }
    fn err(msg: impl Into<String>) -> Self {
        Self {
            data: None,
            error: Some(msg.into()),
        }
    }
    fn to_json_string(&self) -> Result<String, extism::Error> {
        serde_json::to_string(self)
            .map_err(|e| extism::Error::msg(format!("Serialization error: {}", e)))
    }
}

fn sanitize_nul_bytes(s: String, plugin_id: &str, sql: &str) -> String {
    if s.contains('\0') {
        let nul_count = s.matches('\0').count();
        let preview: String = s.chars().take(120).collect();
        let sql_preview: String = sql.chars().take(160).collect();
        tracing::warn!(
            plugin_id,
            nul_count,
            sql = %sql_preview,
            value_preview = %preview,
            "Sanitized NUL bytes from plugin SQL bind value",
        );
        s.replace('\0', "\u{FFFD}")
    } else {
        s
    }
}

fn sanitize_sql_text(label: &str, plugin_id: &str, sql: String) -> String {
    if sql.contains('\0') {
        let nul_count = sql.matches('\0').count();
        let mut nul_offsets: Vec<usize> = sql
            .bytes()
            .enumerate()
            .filter(|(_, b)| *b == 0)
            .map(|(i, _)| i)
            .take(8)
            .collect();
        nul_offsets.sort_unstable();
        let len = sql.len();
        let preview: String = sql.chars().take(240).collect();
        tracing::warn!(
            plugin_id,
            param = label,
            nul_count,
            len,
            nul_offsets = ?nul_offsets,
            preview = %preview,
            "Sanitized NUL bytes from plugin SQL host_fn parameter",
        );
        sql.replace('\0', "\u{FFFD}")
    } else {
        sql
    }
}

fn json_to_sea_value(v: JsonValue, plugin_id: &str, sql: &str) -> sea_orm::Value {
    match v {
        JsonValue::Null => sea_orm::Value::String(None),
        JsonValue::Bool(b) => sea_orm::Value::Bool(Some(b)),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                sea_orm::Value::BigInt(Some(i))
            } else if let Some(f) = n.as_f64() {
                sea_orm::Value::Double(Some(f))
            } else {
                sea_orm::Value::String(Some(n.to_string()))
            }
        }
        JsonValue::String(s) => sea_orm::Value::String(Some(sanitize_nul_bytes(s, plugin_id, sql))),
        v => sea_orm::Value::Json(Some(Box::new(sanitize_json_nul_bytes(v, plugin_id, sql)))),
    }
}

fn sanitize_json_nul_bytes(v: JsonValue, plugin_id: &str, sql: &str) -> JsonValue {
    match v {
        JsonValue::String(s) => JsonValue::String(sanitize_nul_bytes(s, plugin_id, sql)),
        JsonValue::Array(values) => JsonValue::Array(
            values
                .into_iter()
                .map(|value| sanitize_json_nul_bytes(value, plugin_id, sql))
                .collect(),
        ),
        JsonValue::Object(map) => JsonValue::Object(
            map.into_iter()
                .map(|(key, value)| {
                    (
                        sanitize_nul_bytes(key, plugin_id, sql),
                        sanitize_json_nul_bytes(value, plugin_id, sql),
                    )
                })
                .collect(),
        ),
        other => other,
    }
}

/// Final, unbypassable NUL strip on the fully-built bound values, applied at the
/// execute choke point right before `Statement::from_sql_and_values`. PostgreSQL
/// TEXT columns reject 0x00; this guarantees no NUL reaches libpq regardless of
/// what any (possibly stale or future) plugin/SDK sent, independent of the
/// earlier per-value sanitizers in `json_to_sea_value`. Returns how many values
/// were modified so callers can surface that a NUL slipped past upstream layers.
/// How many times a poisoned-connection 0x00 error is retried on a fresh
/// connection before giving up. With `run_detached_db` eliminating the
/// poisoning at its source this should rarely fire; it remains as a safety net
/// for any other connection-desync source.
const POISONED_CONNECTION_RETRIES: u32 = 6;

/// True when a DB error is the server-side SQLSTATE 22021 "invalid byte sequence
/// for encoding UTF8: 0x00". Because every value we bind is sanitized + swept
/// NUL-free at the choke point, this error does NOT mean our data is dirty — it
/// means the pooled sqlx connection is DESYNCED: sqlx 0.8.6 has no `Drop` reset
/// for `PgConnection`, so a query future dropped mid-flush (our cross-thread
/// `block_on(execute_raw)` bridge can produce this under load) leaves a partial
/// message in the persistent write buffer, and `ping()` on pool checkout
/// flushes that dirty tail (length-prefix/NUL framing bytes, which contain 0x00)
/// as the prefix of the NEXT statement. The cure is to retry on a different
/// connection: `execute_raw` pool-acquires a fresh one each call, and the 0x00
/// is rejected by Postgres before execution, so retrying is side-effect-free.
fn is_poisoned_connection_nul_error(e: &sea_orm::DbErr) -> bool {
    let msg = e.to_string();
    msg.contains("0x00") && msg.contains("invalid byte sequence")
}

fn json_value_contains_nul(v: &JsonValue) -> bool {
    match v {
        JsonValue::String(s) => s.contains('\0'),
        JsonValue::Array(a) => a.iter().any(json_value_contains_nul),
        JsonValue::Object(o) => o
            .iter()
            .any(|(k, val)| k.contains('\0') || json_value_contains_nul(val)),
        _ => false,
    }
}

fn strip_nul_from_sea_values(values: &mut [sea_orm::Value], plugin_id: &str, sql: &str) -> usize {
    let mut stripped = 0usize;
    for value in values.iter_mut() {
        match value {
            sea_orm::Value::String(Some(s)) if s.contains('\0') => {
                let cleaned = s.replace('\0', "\u{FFFD}");
                *s = cleaned;
                stripped += 1;
            }
            sea_orm::Value::Json(Some(j)) if json_value_contains_nul(j) => {
                let cleaned = sanitize_json_nul_bytes((**j).clone(), plugin_id, sql);
                **j = cleaned;
                stripped += 1;
            }
            _ => {}
        }
    }
    stripped
}

fn parse_args(
    args_json: &str,
    plugin_id: &str,
    sql: &str,
) -> Result<Vec<sea_orm::Value>, extism::Error> {
    if args_json.trim().is_empty() {
        return Ok(vec![]);
    }
    let json_arr: Vec<JsonValue> = serde_json::from_str(args_json)
        .map_err(|e| extism::Error::msg(format!("Invalid args JSON: {}", e)))?;

    Ok(json_arr
        .into_iter()
        .map(|v| json_to_sea_value(v, plugin_id, sql))
        .collect())
}

host_fn!(pub db_execute(user_data: (String, DatabaseConnection); sql: String, args: String) -> String {
    // Clone the connection handle out of the UserData lock and release the lock
    // BEFORE the blocking DB call. `DatabaseConnection` is a cheap pool handle;
    // holding the std `Mutex` across `block_on(execute_raw)` serializes every
    // plugin DB call on one mutex AND pins the lock across an await point — an
    // anti-pattern. `db_begin` already clones-then-releases; match it here.
    let (plugin_id, db) = {
        let user_data_guard = user_data.get()?;
        let ctx = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (ctx.0.clone(), ctx.1.clone())
    };
    let span = super::host_fn_span("db_execute", &plugin_id);
    let _enter = span.enter();

    let sql = sanitize_sql_text("sql", &plugin_id, sql);
    let args = sanitize_sql_text("args", &plugin_id, args);
    let mut values = parse_args(&args, &plugin_id, &sql)?;
    let stripped = strip_nul_from_sea_values(&mut values, &plugin_id, &sql);
    if stripped > 0 {
        tracing::warn!(
            plugin_id = %plugin_id,
            stripped,
            sql = %sql.chars().take(120).collect::<String>(),
            "Stripped NUL bytes from bound values at db_execute choke point"
        );
    }
    let stmt = Statement::from_sql_and_values(DbBackend::Postgres, &sql, values);

    let mut exec_result = tokio::runtime::Handle::current().block_on(async {
        db.execute_raw(stmt.clone()).await
    });
    let mut poison_retries = 0;
    while poison_retries < POISONED_CONNECTION_RETRIES
        && matches!(&exec_result, Err(e) if is_poisoned_connection_nul_error(e))
    {
        poison_retries += 1;
        tracing::warn!(
            plugin_id = %plugin_id,
            attempt = poison_retries,
            "db_execute hit a poisoned-connection 0x00 error on clean params; retrying on a fresh connection"
        );
        exec_result = tokio::runtime::Handle::current().block_on(async {
            db.execute_raw(stmt.clone()).await
        });
    }

    // GUARANTEED last resort: if every pool retry was exhausted by a
    // poisoned-connection 0x00, run the statement once on a brand-new
    // connection, which cannot carry stale wire framing. Turns the probabilistic
    // retry into a deterministic recovery.
    if matches!(&exec_result, Err(e) if is_poisoned_connection_nul_error(e)) {
        tracing::warn!(
            plugin_id = %plugin_id,
            "db_execute exhausted pool retries on poisoned-connection 0x00; falling back to a fresh connection"
        );
        exec_result = execute_on_fresh_connection(stmt);
    }

    match exec_result {
        Ok(res) => HostDbResponse::ok(JsonValue::from(res.rows_affected())).to_json_string(),
        Err(e) => {
            let sql_preview: String = sql.chars().take(800).collect();
            let args_preview: String = args.chars().take(800).collect();
            error!(plugin_id = %plugin_id, sql_len = sql.len(), args_len = args.len(), poison_retries, sql = %sql_preview, args = %args_preview, "DB execution error: {}", e);
            HostDbResponse::err(e.to_string()).to_json_string()
        }
    }
});

host_fn!(pub db_query(user_data: (String, DatabaseConnection); sql: String, args: String) -> String {
    let (plugin_id, db) = {
        let user_data_guard = user_data.get()?;
        let ctx = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (ctx.0.clone(), ctx.1.clone())
    };
    let span = super::host_fn_span("db_query", &plugin_id);
    let _enter = span.enter();

    let sql = sanitize_sql_text("sql", &plugin_id, sql);
    let args = sanitize_sql_text("args", &plugin_id, args);
    let values = parse_args(&args, &plugin_id, &sql)?;
    let wrapped_sql = format!(
        "SELECT COALESCE(json_agg(t), '[]'::json) AS json_data FROM ({}) AS t",
        sql
    );
    let stmt = Statement::from_sql_and_values(DbBackend::Postgres, wrapped_sql, values);

    let mut query_result = tokio::runtime::Handle::current().block_on(async {
        db.query_one_raw(stmt.clone()).await
    });
    let mut poison_retries = 0;
    while poison_retries < POISONED_CONNECTION_RETRIES
        && matches!(&query_result, Err(e) if is_poisoned_connection_nul_error(e))
    {
        poison_retries += 1;
        tracing::warn!(
            plugin_id = %plugin_id,
            attempt = poison_retries,
            "db_query hit a poisoned-connection 0x00 error on clean params; retrying on a fresh connection"
        );
        query_result = tokio::runtime::Handle::current().block_on(async {
            db.query_one_raw(stmt.clone()).await
        });
    }

    // GUARANTEED last resort, symmetric with db_execute: if every pool retry was
    // exhausted by a poisoned-connection 0x00, run the query once on a brand-new
    // connection, which cannot carry stale wire framing. Without this a poisoned
    // db_query surfaces an error to the plugin, which a plugin turns into a
    // SystemError — a system condition standing as the contestant's verdict.
    if matches!(&query_result, Err(e) if is_poisoned_connection_nul_error(e)) {
        tracing::warn!(
            plugin_id = %plugin_id,
            "db_query exhausted pool retries on poisoned-connection 0x00; falling back to a fresh connection"
        );
        query_result = query_on_fresh_connection(stmt);
    }

    match query_result {
        Ok(Some(res)) => {
            let json_val: serde_json::Value = res.try_get("", "json_data")
                .unwrap_or(serde_json::json!([]));
            HostDbResponse::ok(json_val).to_json_string()
        },
        Ok(None) => HostDbResponse::ok(serde_json::json!([])).to_json_string(),
        Err(e) => {
            error!("DB query error: {}", e);
            HostDbResponse::err(e.to_string()).to_json_string()
        }
    }
});

host_fn!(pub db_begin(user_data: (String, DatabaseConnection, TransactionMap); _input: String) -> String {
    let (plugin_id, db, txn_map) = {
        let user_data_guard = user_data.get()?;
        let ctx = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (ctx.0.clone(), ctx.1.clone(), ctx.2.clone())
    };
    let span = super::host_fn_span("db_begin", &plugin_id);
    let _enter = span.enter();

    // Per-plugin cap on concurrently open transactions, checked BEFORE
    // checking out a pool connection: each open transaction pins one plugin
    // DB pool connection, so without a cap a plugin that keeps opening
    // transactions (or trapping after db_begin) exhausts the pool for everyone.
    {
        let map_guard = txn_map.lock()
            .map_err(|_| extism::Error::msg("Transaction map lock poisoned"))?;
        let open = map_guard.values().filter(|t| t.plugin_id == plugin_id).count();
        if open >= MAX_OPEN_TXNS_PER_PLUGIN {
            tracing::warn!(
                plugin_id = %plugin_id,
                open,
                cap = MAX_OPEN_TXNS_PER_PLUGIN,
                "db_begin rejected: plugin hit its open-transaction cap"
            );
            return HostDbResponse::err(format!(
                "Too many open transactions for this plugin (max {MAX_OPEN_TXNS_PER_PLUGIN}); commit or roll back existing transactions first"
            )).to_json_string();
        }
    }

    let txn = tokio::runtime::Handle::current()
        .block_on(db.begin())
        .map_err(|e| {
            error!("DB begin error: {}", e);
            extism::Error::msg(e.to_string())
        })?;

    let txn_id = Uuid::new_v4().to_string();
    txn_map.lock()
        .map_err(|_| extism::Error::msg("Transaction map lock poisoned"))?
        .insert(txn_id.clone(), PluginTransaction {
            txn,
            plugin_id: plugin_id.clone(),
            last_used: Instant::now(),
        });
    // Guarantees cleanup if the guest traps before commit/rollback.
    spawn_txn_reaper(txn_map.clone(), txn_id.clone());

    HostDbResponse::ok(serde_json::json!({"txn_id": txn_id})).to_json_string()
});

host_fn!(pub db_query_in(user_data: (String, DatabaseConnection, TransactionMap); txn_id: String, sql: String, args: String) -> String {
    let (plugin_id, txn_map) = {
        let user_data_guard = user_data.get()?;
        let ctx = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (ctx.0.clone(), ctx.2.clone())
    };
    let span = super::host_fn_span("db_query_in", &plugin_id);
    let _enter = span.enter();

    let sql = sanitize_sql_text("sql", &plugin_id, sql);
    let args = sanitize_sql_text("args", &plugin_id, args);
    let values = parse_args(&args, &plugin_id, &sql)?;
    let wrapped_sql = format!(
        "SELECT COALESCE(json_agg(t), '[]'::json) AS json_data FROM ({}) AS t",
        sql
    );
    let stmt = Statement::from_sql_and_values(DbBackend::Postgres, wrapped_sql, values);

    let mut map_guard = txn_map.lock()
        .map_err(|_| extism::Error::msg("Transaction map lock poisoned"))?;
    let entry = lookup_owned_txn(&mut map_guard, &txn_id, &plugin_id, "db_query_in")?;
    entry.last_used = Instant::now();

    let query_result = tokio::runtime::Handle::current().block_on(entry.txn.query_one_raw(stmt));
    entry.last_used = Instant::now();

    match query_result {
        Ok(Some(res)) => {
            let json_val: serde_json::Value = res.try_get("", "json_data")
                .unwrap_or(serde_json::json!([]));
            HostDbResponse::ok(json_val).to_json_string()
        },
        Ok(None) => HostDbResponse::ok(serde_json::json!([])).to_json_string(),
        Err(e) => {
            error!("DB query_in error: {}", e);
            HostDbResponse::err(e.to_string()).to_json_string()
        }
    }
});

host_fn!(pub db_execute_in(user_data: (String, DatabaseConnection, TransactionMap); txn_id: String, sql: String, args: String) -> String {
    let (plugin_id, txn_map) = {
        let user_data_guard = user_data.get()?;
        let ctx = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (ctx.0.clone(), ctx.2.clone())
    };
    let span = super::host_fn_span("db_execute_in", &plugin_id);
    let _enter = span.enter();

    let sql = sanitize_sql_text("sql", &plugin_id, sql);
    let args = sanitize_sql_text("args", &plugin_id, args);
    let mut values = parse_args(&args, &plugin_id, &sql)?;
    let stripped = strip_nul_from_sea_values(&mut values, &plugin_id, &sql);
    if stripped > 0 {
        tracing::warn!(
            plugin_id = %plugin_id,
            stripped,
            sql = %sql.chars().take(120).collect::<String>(),
            "Stripped NUL bytes from bound values at db_execute_in choke point"
        );
    }
    let stmt = Statement::from_sql_and_values(DbBackend::Postgres, sql, values);

    let mut map_guard = txn_map.lock()
        .map_err(|_| extism::Error::msg("Transaction map lock poisoned"))?;
    let entry = lookup_owned_txn(&mut map_guard, &txn_id, &plugin_id, "db_execute_in")?;
    entry.last_used = Instant::now();

    let exec_result = tokio::runtime::Handle::current().block_on(entry.txn.execute_raw(stmt));
    entry.last_used = Instant::now();

    match exec_result {
        Ok(res) => HostDbResponse::ok(JsonValue::from(res.rows_affected())).to_json_string(),
        Err(e) => {
            error!("DB execute_in error: {}", e);
            HostDbResponse::err(e.to_string()).to_json_string()
        }
    }
});

host_fn!(pub db_commit(user_data: (String, DatabaseConnection, TransactionMap); txn_id: String) -> String {
    let (plugin_id, txn_map) = {
        let user_data_guard = user_data.get()?;
        let ctx = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (ctx.0.clone(), ctx.2.clone())
    };
    let span = super::host_fn_span("db_commit", &plugin_id);
    let _enter = span.enter();

    let entry = {
        let mut map_guard = txn_map.lock()
            .map_err(|_| extism::Error::msg("Transaction map lock poisoned"))?;
        // Ownership check first; only the opening plugin may commit.
        lookup_owned_txn(&mut map_guard, &txn_id, &plugin_id, "db_commit")?;
        map_guard.remove(&txn_id)
            .ok_or_else(|| extism::Error::msg(format!("Transaction not found: {txn_id}")))?
    };

    let result = tokio::runtime::Handle::current().block_on(entry.txn.commit());

    match result {
        Ok(()) => HostDbResponse::ok(serde_json::json!({"ok": true})).to_json_string(),
        Err(e) => {
            error!("DB commit error: {}", e);
            HostDbResponse::err(e.to_string()).to_json_string()
        }
    }
});

host_fn!(pub db_rollback(user_data: (String, DatabaseConnection, TransactionMap); txn_id: String) -> String {
    let (plugin_id, txn_map) = {
        let user_data_guard = user_data.get()?;
        let ctx = user_data_guard.lock().map_err(|_| extism::Error::msg("Lock poisoned"))?;
        (ctx.0.clone(), ctx.2.clone())
    };
    let span = super::host_fn_span("db_rollback", &plugin_id);
    let _enter = span.enter();

    let entry = {
        let mut map_guard = txn_map.lock()
            .map_err(|_| extism::Error::msg("Transaction map lock poisoned"))?;
        match map_guard.get(&txn_id) {
            // A foreign transaction must behave exactly like a missing one
            // (rollback of a missing txn is an idempotent no-op success), so
            // one plugin can neither roll back nor probe another's txn.
            Some(entry) if entry.plugin_id != plugin_id => {
                tracing::warn!(
                    plugin_id = %plugin_id,
                    owner = %entry.plugin_id,
                    txn_id = %txn_id,
                    "db_rollback: plugin attempted to roll back a DB transaction it does not own"
                );
                None
            }
            Some(_) => map_guard.remove(&txn_id),
            None => None,
        }
    };

    match entry {
        Some(entry) => {
            let result = tokio::runtime::Handle::current().block_on(entry.txn.rollback());
            match result {
                Ok(()) => HostDbResponse::ok(serde_json::json!({"ok": true})).to_json_string(),
                Err(e) => {
                    error!("DB rollback error: {}", e);
                    HostDbResponse::err(e.to_string()).to_json_string()
                }
            }
        }
        None => HostDbResponse::ok(serde_json::json!({"ok": true})).to_json_string(),
    }
});

/// Run a plugin-authored write on the shared plugin DB pool and return the same
/// `HostDbResponse` JSON envelope `db_execute` produces (`{"data": <rows>}` or
/// `{"error": <msg>}`). Shared by the capability-gated `host.submission.*` write
/// host functions, which build the SQL server-side but must hit the DB through
/// the exact same NUL-sanitization, arg-parsing, and poisoned-connection
/// recovery path as raw `db_execute`. Kept as a separate helper (rather than
/// refactoring `db_execute` to call it) so the raw-SQL machinery above stays
/// byte-for-byte unchanged in this phase.
pub(super) fn execute_on_pool(
    db: &DatabaseConnection,
    plugin_id: &str,
    sql: String,
    args: String,
) -> Result<String, extism::Error> {
    let sql = sanitize_sql_text("sql", plugin_id, sql);
    let args = sanitize_sql_text("args", plugin_id, args);
    let mut values = parse_args(&args, plugin_id, &sql)?;
    let stripped = strip_nul_from_sea_values(&mut values, plugin_id, &sql);
    if stripped > 0 {
        tracing::warn!(
            plugin_id = %plugin_id,
            stripped,
            sql = %sql.chars().take(120).collect::<String>(),
            "Stripped NUL bytes from bound values at submission execute choke point"
        );
    }
    let stmt = Statement::from_sql_and_values(DbBackend::Postgres, &sql, values);

    let mut exec_result =
        tokio::runtime::Handle::current().block_on(async { db.execute_raw(stmt.clone()).await });
    let mut poison_retries = 0;
    while poison_retries < POISONED_CONNECTION_RETRIES
        && matches!(&exec_result, Err(e) if is_poisoned_connection_nul_error(e))
    {
        poison_retries += 1;
        tracing::warn!(
            plugin_id = %plugin_id,
            attempt = poison_retries,
            "submission execute hit a poisoned-connection 0x00 error on clean params; retrying on a fresh connection"
        );
        exec_result = tokio::runtime::Handle::current()
            .block_on(async { db.execute_raw(stmt.clone()).await });
    }

    if matches!(&exec_result, Err(e) if is_poisoned_connection_nul_error(e)) {
        tracing::warn!(
            plugin_id = %plugin_id,
            "submission execute exhausted pool retries on poisoned-connection 0x00; falling back to a fresh connection"
        );
        exec_result = execute_on_fresh_connection(stmt);
    }

    match exec_result {
        Ok(res) => HostDbResponse::ok(JsonValue::from(res.rows_affected())).to_json_string(),
        Err(e) => {
            let sql_preview: String = sql.chars().take(800).collect();
            let args_preview: String = args.chars().take(800).collect();
            error!(plugin_id = %plugin_id, sql_len = sql.len(), args_len = args.len(), poison_retries, sql = %sql_preview, args = %args_preview, "Submission DB execution error: {}", e);
            HostDbResponse::err(e.to_string()).to_json_string()
        }
    }
}

/// Read-side counterpart to [`execute_on_pool`]: run a plugin-authored read on
/// the shared plugin DB pool, json_agg-wrapped exactly like `db_query`, and
/// return the same `HostDbResponse` JSON envelope (`{"data": [rows]}`). Used by
/// `host.submission.query_test_cases`.
pub(super) fn query_on_pool(
    db: &DatabaseConnection,
    plugin_id: &str,
    sql: String,
    args: String,
) -> Result<String, extism::Error> {
    let sql = sanitize_sql_text("sql", plugin_id, sql);
    let args = sanitize_sql_text("args", plugin_id, args);
    let values = parse_args(&args, plugin_id, &sql)?;
    let wrapped_sql = format!(
        "SELECT COALESCE(json_agg(t), '[]'::json) AS json_data FROM ({}) AS t",
        sql
    );
    let stmt = Statement::from_sql_and_values(DbBackend::Postgres, wrapped_sql, values);

    let mut query_result =
        tokio::runtime::Handle::current().block_on(async { db.query_one_raw(stmt.clone()).await });
    let mut poison_retries = 0;
    while poison_retries < POISONED_CONNECTION_RETRIES
        && matches!(&query_result, Err(e) if is_poisoned_connection_nul_error(e))
    {
        poison_retries += 1;
        tracing::warn!(
            plugin_id = %plugin_id,
            attempt = poison_retries,
            "submission query hit a poisoned-connection 0x00 error on clean params; retrying on a fresh connection"
        );
        query_result = tokio::runtime::Handle::current()
            .block_on(async { db.query_one_raw(stmt.clone()).await });
    }

    if matches!(&query_result, Err(e) if is_poisoned_connection_nul_error(e)) {
        tracing::warn!(
            plugin_id = %plugin_id,
            "submission query exhausted pool retries on poisoned-connection 0x00; falling back to a fresh connection"
        );
        query_result = query_on_fresh_connection(stmt);
    }

    match query_result {
        Ok(Some(res)) => {
            let json_val: serde_json::Value = res
                .try_get("", "json_data")
                .unwrap_or(serde_json::json!([]));
            HostDbResponse::ok(json_val).to_json_string()
        }
        Ok(None) => HostDbResponse::ok(serde_json::json!([])).to_json_string(),
        Err(e) => {
            error!("Submission DB query error: {}", e);
            HostDbResponse::err(e.to_string()).to_json_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_poisoned_connection_nul_error() {
        let poisoned = sea_orm::DbErr::Custom(
            "error returned from database: invalid byte sequence for encoding \"UTF8\": 0x00"
                .to_string(),
        );
        assert!(is_poisoned_connection_nul_error(&poisoned));

        let unrelated = sea_orm::DbErr::Custom("duplicate key value violates unique".to_string());
        assert!(!is_poisoned_connection_nul_error(&unrelated));
    }

    #[test]
    fn strip_nul_from_sea_values_cleans_string_and_json_and_counts() {
        let mut values = vec![
            sea_orm::Value::String(Some("ok\0bad".to_string())),
            sea_orm::Value::BigInt(Some(42)),
            sea_orm::Value::Json(Some(Box::new(serde_json::json!({"k": "a\u{0000}b"})))),
            sea_orm::Value::String(Some("clean".to_string())),
            sea_orm::Value::String(None),
        ];

        let stripped = strip_nul_from_sea_values(&mut values, "test-plugin", "INSERT ...");
        assert_eq!(stripped, 2, "two values carried NUL");

        match &values[0] {
            sea_orm::Value::String(Some(s)) => {
                assert_eq!(s.as_str(), "ok\u{FFFD}bad");
                assert!(!s.contains('\0'));
            }
            other => panic!("unexpected: {other:?}"),
        }
        match &values[2] {
            sea_orm::Value::Json(Some(j)) => {
                assert_eq!(j["k"], "a\u{FFFD}b");
                assert!(!json_value_contains_nul(j));
            }
            other => panic!("unexpected: {other:?}"),
        }
        // Untouched value is unchanged.
        match &values[3] {
            sea_orm::Value::String(Some(s)) => assert_eq!(s.as_str(), "clean"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn strip_nul_no_op_when_clean() {
        let mut values = vec![
            sea_orm::Value::String(Some("hello".to_string())),
            sea_orm::Value::Json(Some(Box::new(serde_json::json!(["a", "b"])))),
        ];
        assert_eq!(strip_nul_from_sea_values(&mut values, "p", "sql"), 0);
    }

    #[test]
    fn parse_args_sanitizes_top_level_nul_string() {
        let values = parse_args(r#"["ok\u0000bad"]"#, "test-plugin", "select $1").unwrap();

        match &values[0] {
            sea_orm::Value::String(Some(value)) => {
                assert_eq!(value, "ok\u{FFFD}bad");
                assert!(!value.contains('\0'));
            }
            other => panic!("unexpected value: {other:?}"),
        }
    }

    /// The read-side fresh-connection fallback must actually open a brand-new
    /// connection, run the json_agg-wrapped query, and return the rows in the
    /// same shape `db_query` produces. This is the deterministic recovery that
    /// keeps a poisoned `db_query` from surfacing a `SystemError` to the plugin.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn query_on_fresh_connection_runs_query_on_a_new_connection() {
        use testcontainers::ImageExt;
        use testcontainers::runners::AsyncRunner;
        use testcontainers_modules::postgres::Postgres;

        let container = Postgres::default()
            .with_tag("17-alpine")
            .start()
            .await
            .expect("start postgres container");
        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("postgres host port");
        let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

        // Mirrors the json_agg wrapping `db_query` applies, so this exercises the
        // exact result shape the fallback must return.
        let stmt = Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT COALESCE(json_agg(t), '[]'::json) AS json_data FROM (SELECT 7 AS n) AS t",
            Vec::<sea_orm::Value>::new(),
        );

        // The helper uses `Handle::current().block_on`, exactly as the `db_query`
        // host function does in production, where it runs on a `spawn_blocking`
        // thread (not a runtime worker). Replicate that here so `block_on` is
        // legal — calling it directly on the test's runtime thread would panic.
        let row = tokio::task::spawn_blocking(move || query_on_fresh_connection_url(&url, stmt))
            .await
            .expect("blocking task joins")
            .expect("fresh-connection query should succeed")
            .expect("a row is returned");
        let json: serde_json::Value = row
            .try_get("", "json_data")
            .expect("json_data column present");
        assert_eq!(json, serde_json::json!([{ "n": 7 }]));
    }

    /// A transaction orphaned by a trapped guest (never committed or rolled
    /// back) must be rolled back by the reaper and its pooled connection
    /// returned. The pool is sized at ONE connection, so the second `begin`
    /// can only succeed if the reaper actually released the orphan's
    /// connection back to the pool.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn orphaned_transaction_is_reaped_and_returns_its_pool_connection() {
        use testcontainers::ImageExt;
        use testcontainers::runners::AsyncRunner;
        use testcontainers_modules::postgres::Postgres;

        let container = Postgres::default()
            .with_tag("17-alpine")
            .start()
            .await
            .expect("start postgres container");
        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("postgres host port");
        let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

        let mut opt = ConnectOptions::new(url);
        opt.max_connections(1)
            .min_connections(1)
            .sqlx_logging(false);
        let db = Database::connect(opt).await.expect("connect");

        let txn = db.begin().await.expect("begin orphan txn");
        let txn_map: TransactionMap = Arc::new(StdMutex::new(HashMap::new()));
        let txn_id = "orphan-txn".to_string();
        txn_map.lock().unwrap().insert(
            txn_id.clone(),
            PluginTransaction {
                txn,
                plugin_id: "trapped-plugin".to_string(),
                last_used: Instant::now(),
            },
        );

        spawn_txn_reaper_with(
            txn_map.clone(),
            txn_id,
            Duration::ZERO,
            Duration::from_millis(50),
        );

        // The pool's only connection is pinned by the orphan until the reaper
        // rolls it back; this begin succeeding proves it was returned.
        let reclaimed = tokio::time::timeout(Duration::from_secs(20), db.begin())
            .await
            .expect("reaper should return the orphaned connection to the pool")
            .expect("begin after reap");
        reclaimed.rollback().await.expect("rollback");
        assert!(
            txn_map.lock().unwrap().is_empty(),
            "reaper should remove the orphaned entry from the map"
        );
    }

    /// A txn id owned by another plugin must be rejected exactly like a
    /// nonexistent one, while the owner keeps full access.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn foreign_transaction_id_is_rejected_like_missing() {
        use testcontainers::ImageExt;
        use testcontainers::runners::AsyncRunner;
        use testcontainers_modules::postgres::Postgres;

        let container = Postgres::default()
            .with_tag("17-alpine")
            .start()
            .await
            .expect("start postgres container");
        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("postgres host port");
        let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
        let db = Database::connect(url).await.expect("connect");

        let txn = db.begin().await.expect("begin");
        let mut map: HashMap<String, PluginTransaction> = HashMap::new();
        map.insert(
            "txn-1".to_string(),
            PluginTransaction {
                txn,
                plugin_id: "owner".to_string(),
                last_used: Instant::now(),
            },
        );

        let foreign = lookup_owned_txn(&mut map, "txn-1", "intruder", "db_query_in")
            .expect_err("foreign txn id must be rejected");
        assert_eq!(
            foreign.to_string(),
            "Transaction not found: txn-1",
            "foreign txn must be indistinguishable from a missing one"
        );
        let missing = lookup_owned_txn(&mut map, "txn-2", "intruder", "db_query_in")
            .expect_err("missing txn id must be rejected");
        assert_eq!(missing.to_string(), "Transaction not found: txn-2");

        assert!(
            lookup_owned_txn(&mut map, "txn-1", "owner", "db_query_in").is_ok(),
            "owner keeps access to its own transaction"
        );
        assert!(
            map.contains_key("txn-1"),
            "rejected lookups must not remove the entry"
        );
    }

    #[test]
    fn parse_args_sanitizes_nested_json_nul_strings() {
        let values = parse_args(
            r#"[{"std\u0000out":"ok\u0000bad","nested":["x\u0000y"]}]"#,
            "test-plugin",
            "select $1",
        )
        .unwrap();

        match &values[0] {
            sea_orm::Value::Json(Some(value)) => {
                assert_eq!(value["std\u{FFFD}out"], "ok\u{FFFD}bad");
                assert_eq!(value["nested"][0], "x\u{FFFD}y");
                assert!(!value.to_string().contains('\0'));
            }
            other => panic!("unexpected value: {other:?}"),
        }
    }
}
