# Broccoli profiling observability runbook

This runbook is for short-lived contest profiling runs on rented VPSs. It uses
Prometheus, Grafana, Jaeger, Loki, Promtail, node-exporter, cAdvisor,
postgres-exporter, and redis-exporter.

## Local smoke

Start the base stack plus observability:

```bash
docker compose -f docker-compose.yaml -f docker-compose.observability.yml up -d
```

For server and worker profiling logs, run them with JSON logs and OTLP enabled:

```bash
export BROCCOLI__OBSERVABILITY__LOG_FORMAT=json
export BROCCOLI__OBSERVABILITY__OTLP__ENDPOINT=http://localhost:4317
export RUST_LOG=info
```

`RUST_LOG` overrides `observability.log_filter`; do not leave it at `warn` for
trace profiling, or most request and plugin spans will be filtered before they
reach Jaeger. Local Promtail scrapes Docker container logs for the `broccoli`
Compose project by default. If server/worker are run as bare `cargo run`
processes, their stdout logs are not sent to Loki unless you redirect them to a
file/journal target or run them as containers.

Open:

- Grafana: `http://localhost:3001`
- Prometheus: `http://localhost:9090`
- Jaeger: `http://localhost:16686`
- Loki: `http://localhost:3100/ready`

Run a stable smoke run id:

```bash
cargo run -p stress-test -- \
  --url http://localhost:3000 \
  --admin-token "$BROCCOLI_ADMIN_TOKEN" \
  --run-id smoke-observability \
  --profile mixed \
  --duration 300 \
  --rate 5 \
  --concurrency 10
```

Confirm that the same run is visible in:

- Prometheus/Grafana metrics by time window
- Jaeger traces for `broccoli-server` and `broccoli-worker`
- Loki logs via `{service=~"server|worker"} |= "smoke-observability"`

## Multi-VPS target inventory

Prometheus uses file-based target discovery under `config/prometheus/targets`.
For a 7-10 VPS profiling run, replace the local defaults with private IPs.

Example 9-VPS layout:

```json
[
  { "role": "loadgen", "private_ip": "10.0.0.10" },
  { "role": "observability", "private_ip": "10.0.0.11" },
  { "role": "server", "private_ip": "10.0.0.12" },
  { "role": "server", "private_ip": "10.0.0.13" },
  { "role": "postgres", "private_ip": "10.0.0.14" },
  { "role": "redis", "private_ip": "10.0.0.15" },
  { "role": "object-storage", "private_ip": "10.0.0.16" },
  { "role": "worker", "private_ip": "10.0.0.17", "worker_id": "worker-a" },
  { "role": "worker", "private_ip": "10.0.0.18", "worker_id": "worker-b" }
]
```

Target files should use these ports unless explicitly changed:

- server: `3000`
- worker metrics: `9091`
- postgres-exporter: `9187`
- redis-exporter: `9121`
- node-exporter: `9100`
- cAdvisor: `8080`
- SeaweedFS metrics: `9327`

Keep Prometheus labels low-cardinality. Do not add `submission_id`,
`request_id`, or `run_id` to target labels; those belong in logs and traces.

## Promtail on each VPS

Run Promtail on every app/worker/storage VPS. Point it at central Loki:

```bash
export LOKI_PUSH_URL=http://10.0.0.11:3100/loki/api/v1/push
export BROCCOLI_OBS_HOST=$(hostname)
export BROCCOLI_OBS_ROLE=worker
docker run -d --name promtail \
  -v /var/run/docker.sock:/var/run/docker.sock:ro \
  -v /var/lib/docker/containers:/var/lib/docker/containers:ro \
  -v "$PWD/config/promtail-vps.yaml:/etc/promtail/config.yaml:ro" \
  grafana/promtail:3.4.3 \
  -config.file=/etc/promtail/config.yaml \
  -config.expand-env=true
```

Set `BROCCOLI_OBS_ROLE` to `server`, `worker`, `database`, `mq`,
`object-storage`, or `loadgen`.

## Required firewall openings

On the observability VPS, allow private-network access to:

- `9090` Prometheus
- `3001` Grafana
- `16686` Jaeger UI
- `4317` OTLP gRPC
- `3100` Loki

On monitored VPSs, allow private-network access to exporter ports listed above.
Do not expose exporters publicly.

## First-pass bottleneck reading

- API bottleneck: high `http_server_request_duration_seconds`, high 5xx/429s,
  normal worker and DB.
- DB bottleneck: saturated Postgres connections, lock waits, or high
  `pg_stat_statements` time while API latency rises.
- MQ bottleneck: rising `broccoli_mq_queue_depth` or
  `broccoli_mq_message_age_seconds`.
- Worker bottleneck: high `broccoli_worker_active_tasks`, high sandbox wall
  time, and stable API/DB.
- Object-storage bottleneck: high blob-store latency or materialization latency
  with worker slots occupied.
- Plugin bottleneck: high plugin instance acquire duration or plugin call
  duration.

For one slow submission, start from Loki by `submission_id`, copy the
`request_id` or `job_id`, then inspect the matching Jaeger trace.
