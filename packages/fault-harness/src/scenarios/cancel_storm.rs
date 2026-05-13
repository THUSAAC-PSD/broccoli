use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use hdrhistogram::Histogram;
use redis::AsyncCommands;
use tokio::sync::Mutex;

use super::{Scenario, ScenarioContext, ScenarioOutcome};
use crate::transcript::{EventKind, Severity, Transcript};

pub struct CancelStorm {
    pub batch_count: usize,
    pub readers: usize,
    pub key_ttl_secs: u64,
}

impl Default for CancelStorm {
    fn default() -> Self {
        Self {
            batch_count: 1000,
            readers: 64,
            key_ttl_secs: 21600,
        }
    }
}

fn cancel_batch_key(batch_id: &str) -> String {
    format!("broccoli:cancel:batch:{batch_id}")
}

#[async_trait]
impl Scenario for CancelStorm {
    fn name(&self) -> &'static str {
        "cancel_storm"
    }

    async fn run(&self, ctx: &ScenarioContext) -> anyhow::Result<ScenarioOutcome> {
        let mut transcript = Transcript::new(self.name())
            .with_param("batch_count", self.batch_count.into())
            .with_param("readers", self.readers.into())
            .with_param("key_ttl_secs", self.key_ttl_secs.into());

        let client = redis::Client::open(ctx.redis_url.as_str())?;

        transcript.record(
            EventKind::Setup,
            Severity::Info,
            "scenario started; preparing batch ids",
        );

        let batch_ids: Vec<String> = (0..self.batch_count)
            .map(|i| format!("storm-{i:06}"))
            .collect();

        let prime_start = Instant::now();
        {
            let mut conn = client.get_multiplexed_async_connection().await?;
            for chunk in batch_ids.chunks(500) {
                let mut pipe = redis::pipe();
                for id in chunk {
                    pipe.cmd("SET")
                        .arg(cancel_batch_key(id))
                        .arg("1")
                        .arg("EX")
                        .arg(self.key_ttl_secs);
                }
                let _: () = pipe.query_async(&mut conn).await?;
            }
        }
        let prime_ms = prime_start.elapsed().as_millis();
        transcript.record_with(
            EventKind::Inject,
            Severity::Info,
            "set N cancel-batch keys",
            [(
                "prime_ms".to_string(),
                serde_json::Value::from(prime_ms as u64),
            )]
            .into_iter()
            .collect(),
        );

        let hist = Arc::new(Mutex::new(Histogram::<u64>::new(3)?));
        let observed_hits = Arc::new(Mutex::new(0u64));
        let observed_misses = Arc::new(Mutex::new(0u64));

        let mut handles = Vec::with_capacity(self.readers);
        let observe_start = Instant::now();
        for reader_idx in 0..self.readers {
            let client = client.clone();
            let batch_ids = batch_ids.clone();
            let hist = Arc::clone(&hist);
            let hits = Arc::clone(&observed_hits);
            let misses = Arc::clone(&observed_misses);
            handles.push(tokio::spawn(async move {
                let mut conn = client.get_multiplexed_async_connection().await?;
                let stride = batch_ids.len().div_ceil(64).max(1);
                let start = reader_idx * stride % batch_ids.len();
                for offset in 0..batch_ids.len() {
                    let id = &batch_ids[(start + offset) % batch_ids.len()];
                    let t = Instant::now();
                    let exists: i64 = conn.exists(cancel_batch_key(id)).await?;
                    let micros = t.elapsed().as_micros() as u64;
                    hist.lock().await.record(micros).ok();
                    if exists > 0 {
                        *hits.lock().await += 1;
                    } else {
                        *misses.lock().await += 1;
                    }
                }
                Ok::<_, anyhow::Error>(())
            }));
        }

        let mut reader_errors = 0;
        for h in handles {
            match h.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    reader_errors += 1;
                    transcript.record(
                        EventKind::Observe,
                        Severity::Error,
                        format!("reader task error: {e}"),
                    );
                }
                Err(e) => {
                    reader_errors += 1;
                    transcript.record(
                        EventKind::Observe,
                        Severity::Error,
                        format!("reader task join error: {e}"),
                    );
                }
            }
        }
        let observe_ms = observe_start.elapsed().as_millis();

        let hist = hist.lock().await;
        let total_checks = (self.readers * self.batch_count) as u64;
        let hits_final = *observed_hits.lock().await;
        let misses_final = *observed_misses.lock().await;

        transcript.record_with(
            EventKind::Observe,
            Severity::Info,
            "readers completed",
            [
                ("observe_ms".into(), serde_json::json!(observe_ms as u64)),
                ("total_checks".into(), serde_json::json!(total_checks)),
                ("hits".into(), serde_json::json!(hits_final)),
                ("misses".into(), serde_json::json!(misses_final)),
                ("reader_errors".into(), serde_json::json!(reader_errors)),
                (
                    "p50_us".into(),
                    serde_json::json!(hist.value_at_quantile(0.50)),
                ),
                (
                    "p95_us".into(),
                    serde_json::json!(hist.value_at_quantile(0.95)),
                ),
                (
                    "p99_us".into(),
                    serde_json::json!(hist.value_at_quantile(0.99)),
                ),
                ("max_us".into(), serde_json::json!(hist.max())),
            ]
            .into_iter()
            .collect(),
        );

        transcript.set_summary("total_checks", serde_json::json!(total_checks));
        transcript.set_summary("hits", serde_json::json!(hits_final));
        transcript.set_summary("misses", serde_json::json!(misses_final));
        transcript.set_summary("reader_errors", serde_json::json!(reader_errors));
        transcript.set_summary("prime_ms", serde_json::json!(prime_ms as u64));
        transcript.set_summary("observe_ms", serde_json::json!(observe_ms as u64));
        transcript.set_summary("p50_us", serde_json::json!(hist.value_at_quantile(0.50)));
        transcript.set_summary("p95_us", serde_json::json!(hist.value_at_quantile(0.95)));
        transcript.set_summary("p99_us", serde_json::json!(hist.value_at_quantile(0.99)));

        let expected_hits = total_checks;
        let passed = misses_final == 0 && reader_errors == 0 && hits_final == expected_hits;
        if passed {
            transcript.record(
                EventKind::Assertion,
                Severity::Info,
                format!("all {expected_hits} checks observed the cancel key (no misses)"),
            );
        } else {
            transcript.record(
                EventKind::Assertion,
                Severity::Error,
                format!(
                    "expected {expected_hits} hits and 0 errors; got {hits_final} hits, \
                     {misses_final} misses, {reader_errors} errors"
                ),
            );
        }

        transcript.record(EventKind::Summary, Severity::Info, "scenario complete");
        transcript.finish(passed);

        Ok(ScenarioOutcome { passed, transcript })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use testcontainers::runners::AsyncRunner;
    use testcontainers_modules::redis::Redis;

    #[tokio::test]
    #[ignore = "requires Docker to start a Redis testcontainer"]
    async fn cancel_storm_passes_against_real_redis() {
        let redis = Redis::default()
            .start()
            .await
            .expect("failed to start Redis container");
        let port = redis
            .get_host_port_ipv4(6379)
            .await
            .expect("failed to get Redis port");
        let ctx = ScenarioContext {
            redis_url: format!("redis://127.0.0.1:{port}"),
        };
        let scenario = CancelStorm {
            batch_count: 50,
            readers: 8,
            key_ttl_secs: 300,
        };
        let outcome = scenario.run(&ctx).await.expect("scenario failed to run");
        assert!(
            outcome.passed,
            "scenario should pass: {:?}",
            outcome.transcript.summary
        );
        assert!(!outcome.transcript.events.is_empty());
    }
}
