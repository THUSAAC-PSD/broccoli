# Broccoli Platform Bundle

This bundle contains the Broccoli server image, base/ICPC/full worker images,
PostgreSQL, Redis, SeaweedFS object storage, Caddy, default plugins, the
role-aware installer, and the stress-test smoke binary.

## Deployment Shape

Use the same roles for LAN and cloud deployments:

- `infra`: one machine for PostgreSQL, Redis, and SeaweedFS.
- `server`: one Broccoli server process per server machine.
- `worker`: one judge worker per worker machine.
- `gateway`: optional Caddy load balancer in front of multiple servers.

The `single-host` role exists only for release rehearsal or small demos.

## Install

Install infra first:

```bash
./install.sh infra
```

When run from a terminal without a role argument, the installer presents a
guided role menu. Worker and single-host installs also present a worker image
menu:

- `icpc`: C, C++, Java, Python; recommended for normal contests.
- `full`: ICPC plus Node.js, Go, Rust, Pascal, Kotlin.
- `base`: isolate sandbox only; intended for custom derived images.
- `custom`: an operator-provided image tag.

Copy the generated `.env` values for `BROCCOLI__DATABASE__URL`,
`BROCCOLI__MQ__URL`, `BROCCOLI__AUTH__JWT_SECRET`, and
`BROCCOLI__STORAGE__OBJECT_STORAGE__*` to each server and worker node, then:

```bash
./install.sh server
./install.sh worker
```

For server redundancy, install a gateway node with:

```bash
BROCCOLI_UPSTREAMS='10.0.0.21:3000 10.0.0.22:3000' ./install.sh gateway
```

## Files

- `docker-compose.infra.yaml.template`: PostgreSQL, Redis, SeaweedFS.
- `docker-compose.server.yaml.template`: one HTTP/API server.
- `docker-compose.worker.yaml.template`: one judge worker.
- `docker-compose.gateway.yaml.template`: optional Caddy load balancer.
- `.env.*.example`: role-specific configuration examples.
- `plugins/`: default bundled plugins.
- `docs/`: upgrade, TLS, and operations guides.
- `examples/`: Caddy configs and custom worker image example.
