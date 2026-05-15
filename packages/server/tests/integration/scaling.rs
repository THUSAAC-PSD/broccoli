use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use common::worker::TaskResult;
use dashmap::DashMap;
use mq::{MqConfig, init_mq};
use server::config::per_replica_result_queue_name;
use server::consumers::consume_operation_results;
use server::host_funcs::evaluate_ops_registry::EvaluateBatchOpsRegistry;
use server::registry::{OperationWaiter, OperationWaiters};
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::redis::Redis;
use tokio::sync::oneshot;
use tokio::time::timeout;

#[tokio::test]
async fn per_replica_result_queue_delivers_to_originating_replica_only() {
    let redis = Redis::default()
        .start()
        .await
        .expect("failed to start Redis container");
    let port = redis
        .get_host_port_ipv4(6379)
        .await
        .expect("failed to get Redis port");
    let redis_url = format!("redis://127.0.0.1:{port}");

    let mq_a = Arc::new(
        init_mq(MqConfig {
            url: redis_url.clone(),
            pool_size: 2,
        })
        .await
        .expect("failed to create replica A MQ client"),
    );
    let mq_b = Arc::new(
        init_mq(MqConfig {
            url: redis_url.clone(),
            pool_size: 2,
        })
        .await
        .expect("failed to create replica B MQ client"),
    );
    let publisher = init_mq(MqConfig {
        url: redis_url,
        pool_size: 2,
    })
    .await
    .expect("failed to create publisher MQ client");

    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_nanos();
    let base_queue = format!("operation_results.scaling_{suffix}");
    let queue_a = per_replica_result_queue_name(&base_queue, "replica-a");
    let queue_b = per_replica_result_queue_name(&base_queue, "replica-b");
    let (metrics, _registry) = common::observability::init_metrics("broccoli-scaling-test");

    let waiters_a: OperationWaiters = Arc::new(DashMap::new());
    let waiters_b: OperationWaiters = Arc::new(DashMap::new());
    let evaluate_ops_registry = EvaluateBatchOpsRegistry::default();
    let (tx_a, rx_a) = oneshot::channel();
    let (tx_b, rx_b) = oneshot::channel();
    waiters_a.insert("task-1".to_string(), OperationWaiter::new(tx_a));
    waiters_b.insert("task-1".to_string(), OperationWaiter::new(tx_b));

    let consumer_a = tokio::spawn(consume_operation_results(
        Arc::clone(&mq_a),
        Arc::clone(&waiters_a),
        evaluate_ops_registry.clone(),
        queue_a.clone(),
        metrics.clone(),
    ));
    let consumer_b = tokio::spawn(consume_operation_results(
        Arc::clone(&mq_b),
        Arc::clone(&waiters_b),
        evaluate_ops_registry,
        queue_b,
        metrics,
    ));

    tokio::time::sleep(Duration::from_millis(250)).await;

    publisher
        .publish(
            &queue_a,
            None,
            &TaskResult {
                task_id: "task-1".to_string(),
                success: true,
                output: serde_json::json!({ "replica": "a" }),
                error: None,
                task_type: Some("operation".to_string()),
                operation: Some("operation".to_string()),
                worker_id: Some("worker-a".to_string()),
                enqueued_at_unix_ms: Some(1_234),
            },
            None,
        )
        .await
        .expect("failed to publish task result");

    let delivered = timeout(Duration::from_secs(5), rx_a)
        .await
        .expect("replica A did not receive its result")
        .expect("replica A waiter dropped");
    assert_eq!(delivered.task_id, "task-1");
    assert!(delivered.success);

    assert!(
        timeout(Duration::from_millis(500), rx_b).await.is_err(),
        "replica B must not receive replica A's result"
    );
    assert!(waiters_b.contains_key("task-1"));

    consumer_a.abort();
    consumer_b.abort();
}
