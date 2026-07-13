//! Isolated reproducer for the concurrent "invalid byte sequence ... 0x00"
//! error (see docs/plans/2026-06-25-nul-byte-root-cause.md).
//!
//! Hammers a clean 64 KiB text INSERT concurrently against Postgres in one of
//! several driving modes, to bisect WHICH access pattern desyncs the pooled
//! connection. The inserted value is pure ASCII digits — provably 0x00-free —
//! so any 0x00 error is wire-protocol corruption, not data.
//!
//! Usage (on a host with Postgres at the configured URL):
//!   cargo run --release -p server --example nul_repro -- <mode> <concurrency> <iters>
//! modes:
//!   async   - plain `db.execute_raw(stmt).await` on normal runtime tasks
//!   bridge  - mimics the host fn: spawn_blocking + Handle::current().block_on(execute_raw)
//!   spawn   - spawn_blocking + block_on(Handle::spawn(execute_raw)) [the detach variant]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement, Value,
};

fn db_url() -> String {
    std::env::var("BROCCOLI__DATABASE__URL")
        .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/broccoli".to_string())
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "bridge".to_string());
    let concurrency: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(16);
    let iters: usize = std::env::args()
        .nth(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);

    let mut opt = ConnectOptions::new(db_url());
    opt.max_connections(100)
        .min_connections(5)
        .sqlx_logging(false);
    let db = Database::connect(opt).await.expect("connect");

    db.execute_unprepared(
        "DROP TABLE IF EXISTS nul_repro; CREATE TABLE nul_repro (id serial primary key, data text)",
    )
    .await
    .expect("create table");

    let big: String = "9".repeat(64 * 1024); // 64 KiB clean ASCII; zero 0x00 bytes
    assert!(!big.as_bytes().contains(&0));

    let nul = Arc::new(AtomicU64::new(0));
    let ok = Arc::new(AtomicU64::new(0));
    let other = Arc::new(AtomicU64::new(0));

    // "cancel" mode: alongside the clean inserts, run a set of CANCELLER tasks
    // that start a DB query and abort it mid-flight via a tiny timeout. A
    // dropped-mid-flush query leaves the pooled connection desynced; if the
    // clean inserts then inherit such a connection they get the 0x00 error.
    // This mimics HTTP-handler / dispatcher DB futures cancelled on client
    // disconnect / timeout under load — the suspected real trigger.
    let cancellers = if mode == "cancel" { concurrency } else { 0 };
    let mut tasks = Vec::new();
    for _ in 0..cancellers {
        let db = db.clone();
        tasks.push(tokio::spawn(async move {
            for _ in 0..iters * 8 {
                // Server-side sleep so the query is reliably in-flight (response
                // not yet received) when we abort it. Aborting mid-protocol
                // leaves the pooled connection desynced.
                let stmt = Statement::from_string(
                    DbBackend::Postgres,
                    "SELECT pg_sleep(0.05)".to_string(),
                );
                let _ = tokio::time::timeout(
                    std::time::Duration::from_micros(300),
                    db.execute_raw(stmt),
                )
                .await;
            }
        }));
    }
    for _ in 0..concurrency {
        let db = db.clone();
        let big = big.clone();
        let (nul, ok, other) = (nul.clone(), ok.clone(), other.clone());
        let mode = mode.clone();
        tasks.push(tokio::spawn(async move {
            for _ in 0..iters {
                let stmt = Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    "INSERT INTO nul_repro (data) VALUES ($1::text)",
                    [Value::from(big.clone())],
                );
                let res: Result<_, sea_orm::DbErr> = match mode.as_str() {
                    "async" => db.execute_raw(stmt).await,
                    "bridge" | "cancel" => {
                        let db = db.clone();
                        tokio::task::spawn_blocking(move || {
                            tokio::runtime::Handle::current()
                                .block_on(async { db.execute_raw(stmt).await })
                        })
                        .await
                        .expect("join")
                    }
                    "spawn" => {
                        let db = db.clone();
                        tokio::task::spawn_blocking(move || {
                            let h = tokio::runtime::Handle::current();
                            let j = h.spawn(async move { db.execute_raw(stmt).await });
                            h.block_on(j).expect("join")
                        })
                        .await
                        .expect("join")
                    }
                    m => panic!("unknown mode {m}"),
                };
                match res {
                    Ok(_) => {
                        ok.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        if msg.contains("0x00") && msg.contains("invalid byte sequence") {
                            nul.fetch_add(1, Ordering::Relaxed);
                        } else {
                            other.fetch_add(1, Ordering::Relaxed);
                            eprintln!("OTHER ERR: {msg}");
                        }
                    }
                }
            }
        }));
    }
    for t in tasks {
        let _ = t.await;
    }

    println!(
        "RESULT mode={mode} concurrency={concurrency} iters={iters} total={} ok={} nul_0x00={} other_err={}",
        concurrency * iters,
        ok.load(Ordering::Relaxed),
        nul.load(Ordering::Relaxed),
        other.load(Ordering::Relaxed),
    );
}
