//! Regression test for the vendored `broccoli_queue` `reject` patch (see
//! `VENDOR-PATCHES.md`). Before the patch, rejecting a message that still had
//! retry budget errored with "Missing priority" *after* the message had already
//! been LREM'd from the processing queue, so the retry re-publish never ran and
//! the message was silently lost. After the patch, a rejected message must be
//! re-queued for redelivery.

use serde::{Deserialize, Serialize};
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::redis::Redis;

#[derive(Clone, Serialize, Deserialize)]
struct Msg {
    id: String,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reject_requeues_the_message_instead_of_losing_it() {
    let container = Redis::default().start().await.expect("start redis");
    let port = container
        .get_host_port_ipv4(6379)
        .await
        .expect("redis port");
    let mq = mq::init_mq(mq::MqConfig {
        url: format!("redis://127.0.0.1:{port}"),
        pool_size: 2,
    })
    .await
    .expect("init mq");

    let q = "reject_roundtrip";
    mq.publish::<Msg>(q, None, &Msg { id: "m1".into() }, None)
        .await
        .expect("publish");

    let taken = mq
        .try_consume::<Msg>(q, None)
        .await
        .expect("consume ok")
        .expect("a message is available");
    assert_eq!(taken.payload.id, "m1");
    assert_eq!(taken.attempts, 0);

    // Reject with retry budget remaining. Pre-patch this errored and dropped the
    // message; post-patch it must re-queue it.
    mq.reject::<Msg>(q, taken)
        .await
        .expect("reject must not error");

    let redelivered = mq
        .try_consume::<Msg>(q, None)
        .await
        .expect("consume ok")
        .expect("the rejected message must be redelivered, not lost");
    assert_eq!(redelivered.payload.id, "m1");
    assert_eq!(
        redelivered.attempts, 1,
        "the retry attempt count must be incremented"
    );

    // Keep rejecting: at the 3-attempt cap the broker must dead-letter it to
    // `<queue>_failed` (not re-queue it, not lose it).
    mq.reject::<Msg>(q, redelivered).await.expect("reject 2");
    let third = mq
        .try_consume::<Msg>(q, None)
        .await
        .expect("consume ok")
        .expect("still redelivered at attempts=2");
    assert_eq!(third.attempts, 2);
    mq.reject::<Msg>(q, third)
        .await
        .expect("reject 3 (hits cap)");

    assert!(
        mq.try_consume::<Msg>(q, None)
            .await
            .expect("consume ok")
            .is_none(),
        "past the retry cap the message must NOT be redelivered"
    );

    // It must be in the dead-letter queue, not silently dropped.
    let client = redis::Client::open(format!("redis://127.0.0.1:{port}")).unwrap();
    let mut conn = client.get_multiplexed_async_connection().await.unwrap();
    let failed_depth: i64 = redis::cmd("LLEN")
        .arg(format!("{q}_failed"))
        .query_async(&mut conn)
        .await
        .unwrap();
    assert_eq!(
        failed_depth, 1,
        "the exhausted message must land in the dead-letter queue"
    );
}
