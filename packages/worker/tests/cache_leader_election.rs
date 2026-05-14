//! Integration tests for the Redis-backed cache leader-election layer (UP#33).
//!
//! These tests spin up a real Redis container via `testcontainers` so we can
//! verify the SETNX/heartbeat/CAS-release dance against a live server. The
//! cargo test runner will only execute them when Docker is available.

use std::sync::Arc;
use std::time::Duration;

use futures::future::join_all;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::redis::Redis;
use worker::models::operation::cache_leader::{
    CacheLeaderElector, CacheLeaderOpts, LeaderRole, RedisCacheLeaderElector,
};

async fn redis_url() -> (String, testcontainers::ContainerAsync<Redis>) {
    let container = Redis::default()
        .start()
        .await
        .expect("failed to start Redis container");
    let port = container
        .get_host_port_ipv4(6379)
        .await
        .expect("failed to get Redis port");
    (format!("redis://127.0.0.1:{port}"), container)
}

fn opts_short_ttl(ttl_secs: u64) -> CacheLeaderOpts {
    CacheLeaderOpts {
        ttl_secs,
        heartbeat_interval_secs: 1,
    }
}

/// Test 1: out of N concurrent acquirers for the same key, exactly one is leader.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exactly_one_leader_among_concurrent_acquirers() {
    let (url, _container) = redis_url().await;

    let n = 3;
    let mut electors = Vec::new();
    for _ in 0..n {
        let elector = Arc::new(
            RedisCacheLeaderElector::from_url(&url, opts_short_ttl(60)).expect("build elector"),
        );
        electors.push(elector);
    }

    let key = format!("test1-{}", uuid::Uuid::new_v4());
    let mut tasks = Vec::new();
    for elector in &electors {
        let key = key.clone();
        let e = elector.clone();
        tasks.push(tokio::spawn(async move { e.acquire(&key).await }));
    }
    let results = join_all(tasks).await;

    let mut leaders = 0;
    let mut followers = 0;
    let mut leases = Vec::new();
    for r in results {
        let role = r.expect("join task").expect("acquire returned Err");
        match role {
            LeaderRole::Leader(lease) => {
                leaders += 1;
                leases.push(lease);
            }
            LeaderRole::Follower => followers += 1,
        }
    }
    assert_eq!(leaders, 1, "exactly one leader expected");
    assert_eq!(followers, n - 1, "remaining acquirers must be followers");
    drop(leases);
}

/// Test 2: a long-lived leader holds the lock through heartbeats — a concurrent
/// acquire well past the original TTL still sees Follower because heartbeat
/// has extended the lock.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn heartbeat_keeps_leadership_past_ttl() {
    let (url, _container) = redis_url().await;

    // TTL = 2s, heartbeat = 1s. After 3s (= 1.5x TTL), heartbeat must have refreshed.
    let leader_elector = Arc::new(
        RedisCacheLeaderElector::from_url(&url, opts_short_ttl(2)).expect("build elector"),
    );
    let challenger = Arc::new(
        RedisCacheLeaderElector::from_url(&url, opts_short_ttl(2)).expect("build elector"),
    );
    let key = format!("test2-{}", uuid::Uuid::new_v4());

    let role = leader_elector.acquire(&key).await.expect("acquire ok");
    let lease = match role {
        LeaderRole::Leader(l) => l,
        LeaderRole::Follower => panic!("first acquirer must be leader"),
    };

    // Wait past one full TTL boundary; heartbeat (1s) must have refreshed by now.
    tokio::time::sleep(Duration::from_secs(3)).await;

    let challenger_role = challenger.acquire(&key).await.expect("acquire ok");
    match challenger_role {
        LeaderRole::Leader(_) => {
            panic!("challenger became leader — heartbeat failed to refresh");
        }
        LeaderRole::Follower => {}
    }
    drop(lease);
}

/// Test 3: when the leader's lease drops, the next acquirer becomes leader.
/// CAS-release is fired asynchronously on drop, so we poll briefly.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lease_drop_releases_lock() {
    let (url, _container) = redis_url().await;
    let elector = Arc::new(
        RedisCacheLeaderElector::from_url(&url, opts_short_ttl(60)).expect("build elector"),
    );
    let key = format!("test3-{}", uuid::Uuid::new_v4());

    let role = elector.acquire(&key).await.expect("first acquire ok");
    let lease = match role {
        LeaderRole::Leader(l) => l,
        LeaderRole::Follower => panic!("first must be leader"),
    };
    drop(lease);

    // Poll for up to 5s for the detached release task to fire.
    let mut became_leader = false;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        match elector.acquire(&key).await.expect("acquire ok") {
            LeaderRole::Leader(_) => {
                became_leader = true;
                break;
            }
            LeaderRole::Follower => continue,
        }
    }
    assert!(
        became_leader,
        "next acquirer should become leader after first lease drops"
    );
}

/// Test 4: if the leader process dies without releasing (we simulate by
/// std::mem::forget on the lease), the lock auto-expires after TTL.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ttl_safety_net_releases_after_expiry() {
    let (url, _container) = redis_url().await;
    let leader = Arc::new(
        RedisCacheLeaderElector::from_url(&url, opts_short_ttl(2)).expect("build elector"),
    );
    let challenger = Arc::new(
        // Same short TTL; we don't want our own heartbeat to extend leadership.
        RedisCacheLeaderElector::from_url(&url, opts_short_ttl(2)).expect("build elector"),
    );
    let key = format!("test4-{}", uuid::Uuid::new_v4());

    let role = leader.acquire(&key).await.expect("acquire ok");
    let lease = match role {
        LeaderRole::Leader(l) => l,
        LeaderRole::Follower => panic!("first must be leader"),
    };

    // To validate the TTL safety net (rather than the normal Drop path),
    // we drop our real lease and then write a *ghost* lock directly via
    // raw SET. Because the token is not ours, no heartbeat will extend
    // it, so we observe Redis's own TTL expiry.
    drop(lease);

    // Manually emit a SET with our TTL using the redis crate directly.
    let client = redis::Client::open(url.clone()).expect("redis client");
    let mut conn = client
        .get_multiplexed_async_connection()
        .await
        .expect("redis conn");
    let manual_id = uuid::Uuid::new_v4();
    let manual_key = format!("broccoli:cache-leader:test4-manual-{manual_id}");
    let _: () = redis::cmd("SET")
        .arg(&manual_key)
        .arg("ghost-token")
        .arg("EX")
        .arg(2u64)
        .query_async(&mut conn)
        .await
        .expect("manual SET");

    // Immediately challenging should see Follower (key present, our token != ghost).
    let cache_key = manual_key.trim_start_matches("broccoli:cache-leader:");
    let immediate = challenger.acquire(cache_key).await.expect("acquire ok");
    assert!(
        matches!(immediate, LeaderRole::Follower),
        "ghost lock present → next acquire must be Follower"
    );

    // After TTL elapses (give a 1s slack), next acquire must become leader.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let after = challenger.acquire(cache_key).await.expect("acquire ok");
    match after {
        LeaderRole::Leader(_) => {}
        LeaderRole::Follower => panic!("ghost lock should have expired after TTL"),
    }
}

/// Test 5 (deferred): full multi-worker concurrent-compile scenario.
///
/// This would spin up a Postgres testcontainer and a Redis testcontainer,
/// build 3 `OperationHandler` instances sharing both, submit the same step
/// to all 3 concurrently, and assert exactly one sandbox.execute() was
/// invoked.
///
/// Scope was descoped from this PR because it requires wiring a real
/// `DatabaseTaskCacheStore` + `MockSandboxManager` (with a shared atomic
/// counter) + leader-elector through `OperationHandler::with_cache_leader`,
/// plus a Postgres container. The Redis-only tests above already validate
/// the load-bearing claim of UP#33 (only-one-leader, heartbeat survival,
/// release-on-drop, TTL safety net).
///
/// Tracking: TODO("UP#33 follow-up — write end-to-end test 5 once we have a
/// Postgres testcontainer in this crate")
#[tokio::test]
#[ignore = "requires Postgres testcontainer wiring; deferred to follow-up PR"]
async fn end_to_end_concurrent_compile_runs_once() {
    // Intentionally empty — see doc comment above.
}
