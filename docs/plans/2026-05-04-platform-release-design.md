# Platform Release & Distribution — Design

**Date:** 2026-05-04 **Status:** Approved **Owner:** Joseph **Implementation
plan:** `2026-05-04-platform-release.md` **Companion design:**
`2026-05-03-stress-test-release-design.md` (stress-test CLI distribution; this
document extends its two-channel pattern to the whole platform)

## Context

The stress-test CLI release plan carved out distribution for the verifier binary
and explicitly deferred server release. This design covers the platform itself:
server image, worker image with admin-configurable language toolchains,
frontend, default plugin set, CI pipeline, and the operator UX for the two
deployment shapes the platform actually sees.

Two operators:

- **Cloud admin (virtual contests).** Rents a CN cloud VM (Aliyun, Tencent, or a
  university intranet host), wants the platform up in ten minutes with a
  sensible TLS story and the option to scale out.
- **Lab admin (offline contests).** Drops a tarball onto a Linux+Docker machine
  they have never seen before, in a building with no VPN and possibly flaky
  internet. Wants one command to bring the platform up and a fast way to verify
  it will survive a four-hour contest.

Today there is no platform release. `cargo build`, `docker build`, and a
development `docker-compose.yaml` exist; nothing else. Lab admins can't
realistically deploy.

## Goals

1. One git tag publishes the entire platform — server image, worker image
   variants, frontend (baked into server), default plugin set, stress-test CLI —
   in under fifteen minutes of CI wall time.
2. Cloud admins run the same role topology as LAN installs — infra, one or more
   servers, one worker per judge host, and optional gateway — with pinned
   registry images.
3. Lab admins extract one tarball on each role machine, run
   `./install.sh infra|server|worker|gateway`, and end up with a
   verified-working platform without registry access.
4. Horizontal scaling works: multiple `broccoli-server` replicas behind any L7
   load balancer route plugin operation results correctly.
5. Admins control which language toolchains the worker image carries without
   forking the project.
6. Dockerfiles meet production standards: pinned base image digests, layered
   dependency caching, non-root runtime where possible, healthchecks,
   multi-arch, OCI metadata, deterministic builds.
7. Nothing in the default build path requires Chinese mirrors. CN mirrors stay
   an opt-in build arg, documented for operators inside CN but not the default.

## Non-goals (v1)

- Kubernetes, Helm, or operator. Compose only.
- Mid-contest hot upgrades. Admins stop, upgrade, restart between contests.
- Auto-update channel for installed deployments.
- Backup or export tooling. Operational concern, handled externally.
- Code signing for the platform binaries. (Stress-test CLI signing tracked in
  the prior design.)
- Server-side TLS termination as a default. Cloud LB or Caddy in front; we
  document, we don't bundle.
- Removing the legacy global `operation_results` queue (deferred one release for
  rolling-upgrade compat).

## Distribution channels

Extends the stress-test design's two-channel pattern.

| Channel                                                           | Audience                                                             | Artifacts                                                                                          |
| ----------------------------------------------------------------- | -------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| **Aliyun ACR** — `registry.cn-hangzhou.aliyuncs.com/broccoli/...` | Cloud admins, well-connected labs inside CN                          | `broccoli-server:vX.Y.Z`, `broccoli-worker:vX.Y.Z-{base,icpc,full}`                                |
| **GitHub Container Registry** — `ghcr.io/<org>/broccoli/...`      | Anyone outside CN; external CI                                       | Same images                                                                                        |
| **Server `/downloads`** (already designed for stress-test CLI)    | Lab admins; anyone offline                                           | `broccoli-platform-vX.Y.Z.tar.gz` (saved images + compose + plugins + installer + stress-test CLI) |
| **GitHub Releases**                                               | First-ever bootstrap before any Broccoli server exists; CI consumers | Above tarball + per-platform stress-test binaries (existing)                                       |

Same artifacts in every channel. ACR and GHCR exist to skip the "download a 500
MB tarball" step when the lab actually has good internet.

## Worker image: tiered language toolchains

The current worker bakes g++, python3, openjdk, nodejs, rustc, and golang into
one image. That image is bigger than most contests need and impossible to extend
without forking.

Three tagged variants build from a single Dockerfile via
`docker buildx build --target`:

| Tag suffix | Toolchains                                  | Approx size | Audience                                |
| ---------- | ------------------------------------------- | ----------- | --------------------------------------- |
| `-base`    | isolate + worker binary only. No compilers. | ~80 MB      | Admins extending with custom toolchains |
| `-icpc`    | base + gcc, g++, python3, openjdk-17        | ~600 MB     | Standard ICPC-style contests            |
| `-full`    | icpc + nodejs, rustc, golang, kotlin, fpc   | ~1.4 GB     | Open-format contests                    |

The compose file in the platform tarball references `-icpc` by default. Admins
switch by editing one line in `.env`:

```
BROCCOLI_WORKER_IMAGE=ghcr.io/<org>/broccoli-worker:v0.3.0-full
```

For toolchains beyond `-full`, admins build a derived image against `-base`. The
repository ships a working example at `examples/Dockerfile.worker.custom`:

```dockerfile
ARG BROCCOLI_VERSION=v0.3.0
FROM ghcr.io/<org>/broccoli/broccoli-worker:${BROCCOLI_VERSION}-base

USER root
RUN apt-get update && apt-get install -y --no-install-recommends \
    fp-compiler kotlin \
    && rm -rf /var/lib/apt/lists/*
USER worker
```

Language _invocation_ (paths, flags, time budget) is already configurable
through the `standard-languages` plugin's `[config.compilation]` section,
scopable per plugin/contest/problem. Admins who add a new toolchain also drop a
new language plugin or extend `standard-languages`. The framework is in place;
this release does not change it.

## Production-grade Dockerfile rewrite

Both Dockerfiles are rewritten from scratch. The current "stub source files"
trick is replaced with `cargo-chef` for clean dependency caching. BuildKit cache
mounts pin compiler caches across builds. Base images pin SHA digests for
reproducibility. Runtime users are non-root where possible. Healthchecks are
explicit. OCI labels are populated. `tini` handles PID 1 signal forwarding.

### Server (`Dockerfile.server`)

Five stages:

1. **`chef`** — `lukemathwalker/cargo-chef:0.1-rust-1.84-bookworm@sha256:...`.
   Plans the Rust dependency graph
   (`cargo chef prepare --recipe-path recipe.json`).
2. **`rust-builder`** — same base. Restores cache
   (`cargo chef cook --release --recipe-path recipe.json -p server`), copies
   real source, builds `cargo build --release --locked -p server`, strips debug
   symbols. Output: `/out/broccoli-server`.
3. **`web-builder`** — `node:20-bookworm-slim@sha256:...`. Runs
   `pnpm install --frozen-lockfile` and `pnpm --filter @broccoli/web build`.
   Output: `/out/dist/`.
4. _(parallel to 1–3)_ **`plugin-stage`** — copies `plugins/` from the build
   context into `/out/plugins/` (no compilation; plugin WASM blobs live in their
   own subtree, built by their own pipelines or vendored).
5. **`runtime`** — `gcr.io/distroless/cc-debian12:nonroot@sha256:...`. Copies
   the server binary, the frontend `dist/`, and the bundled plugins.
   `USER nonroot:nonroot`. `EXPOSE 3000`.
   `HEALTHCHECK CMD ["/usr/local/bin/broccoli-server", "--healthcheck"]` (a
   built-in flag that hits `/healthz` on the local port).
   `ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/broccoli-server"]`.

OCI labels on the runtime stage:
`org.opencontainers.image.{title,description,version,revision,source,licenses,vendor,documentation,created}`,
all populated from build args wired in CI.

CN mirrors stay an opt-in `--build-arg USE_CN_MIRRORS=true` toggle. Off by
default. Documented in `docs/build.md`, not in the README.

### Worker (`Dockerfile.worker`)

Six stages, multi-target:

1. **`chef`** and 2. **`rust-builder`** — mirror the server: cargo-chef
   plan/cook for the `worker` crate. Output: `/out/broccoli-worker` stripped.
2. **`isolate-builder`** — `debian:bookworm-slim@sha256:...`. Installs build
   deps (`git`, `make`, `libcap-dev`, `libsystemd-dev`, `libseccomp-dev`,
   `pkg-config`). Clones isolate at a **pinned tag**
   (`ARG ISOLATE_VERSION=v2.0`), builds, and copies the resulting binaries + man
   pages + `default.cf` into `/out/isolate/`. Pinned tag — never `master`. The
   pin bumps via PR.
3. **`runtime-base`** — `debian:bookworm-slim@sha256:...`. Installs only runtime
   libraries (`libcap2`, `libsystemd0`, `libseccomp2`, `ca-certificates`,
   `tini`). Copies `broccoli-worker` and the isolate tree. Creates a non-root
   `worker:worker` (uid/gid 10001). Sets up `subuid`/`subgid` per isolate's
   README.
   `HEALTHCHECK CMD ["/usr/local/bin/broccoli-worker", "--healthcheck"]`.
   `ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/docker-entrypoint-worker.sh"]`.
   The entrypoint script verifies cgroups, then `exec broccoli-worker "$@"`.
   Worker still requires `--privileged` because isolate manages cgroups; the
   binary itself runs as `worker` and isolate drops further per its design.
4. **`runtime-icpc`** — `FROM runtime-base`.
   `apt-get install -y --no-install-recommends gcc g++ python3 default-jdk-headless`.
5. **`runtime-full`** — `FROM runtime-icpc`.
   `apt-get install -y --no-install-recommends nodejs golang-go rustc fpc kotlin`.

`docker buildx build --target runtime-base|runtime-icpc|runtime-full` selects
the variant.

### `.dockerignore`

Replaced. Excludes everything by default, whitelists the source paths the build
needs:

```
*
!Cargo.toml
!Cargo.lock
!rust-toolchain.toml
!packages/
!plugins/
!config/
!docker-entrypoint-worker.sh
!package.json
!pnpm-lock.yaml
!pnpm-workspace.yaml
**/target/
**/node_modules/
**/dist/
**/.DS_Store
```

Build context drops from ~1 GB (current) to under 5 MB.

### Multi-arch

CI builds `linux/amd64` and `linux/arm64` from the same Dockerfile via
`docker buildx build --platform linux/amd64,linux/arm64`. Worker arm64 needs the
arm64 isolate build (isolate compiles cleanly on aarch64). Server arm64 needs no
special handling — pure Rust with vendored TLS via rustls plus a frontend
`dist/`.

## Frontend serving in-process

The server gains static-file routes on a plain axum Router, outside `/api`,
hidden from OpenAPI:

- `GET /assets/*path` (when not matching the existing `/assets/{plugin_id}/*`
  plugin asset route) → `tower-http::services::ServeDir` over
  `/srv/dist/assets/`.
- `GET /*path` (catch-all fallback) → if file exists in `/srv/dist/`, serve it;
  else serve `dist/index.html` so client-side routing works.

`tower-http` features add `fs` and `compression-gzip`. A single
`CompressionLayer` applies to all responses. The plugin asset route at
`/assets/{plugin_id}/*` keeps precedence over the SPA's generic `/assets/`.

## Server gaps to close (besides SPA serving)

| Gap                                                                            | Fix                                                                                                                                                                                                     |
| ------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| No global compression                                                          | `tower-http::CompressionLayer::new()`                                                                                                                                                                   |
| No `X-Forwarded-For` parsing (telemetry handler already references client IPs) | `axum-client-ip` crate. Trusted-proxy CIDR list comes from `BROCCOLI__SERVER__TRUSTED_PROXIES` (default empty = trust nothing, fall back to socket address)                                             |
| Hyper waits forever for client headers                                         | `Http1Builder::header_read_timeout(Duration::from_secs(30))`                                                                                                                                            |
| No request timeout on `/api`                                                   | `tower_http::timeout::TimeoutLayer::new(Duration::from_secs(60))`, applied to the `/api` Router only; large-body routes (test-case upload) opt out per their existing layered limits                    |
| Server has no graceful shutdown                                                | `with_graceful_shutdown(ctrl_c())`, mirroring the worker                                                                                                                                                |
| No `/healthz` or `/api/v1/health`                                              | New handler. `/healthz` returns 200 with DB + MQ ping status (each with 2s timeout), 503 if either fails. `/api/v1/health` is the same body, served from the documented API surface (utoipa-annotated). |
| No `--healthcheck` CLI flag                                                    | New flag on the binary: hits `/healthz` on the configured port and exits 0/1. Used by the Dockerfile `HEALTHCHECK`.                                                                                     |
| `/auth/login` has no rate limit                                                | Optional `tower-governor` middleware throttles `/api/v1/auth/login` to 10 req/min/IP. Off by default; on via `BROCCOLI__SERVER__RATE_LIMIT_AUTH=true`.                                                  |

## Horizontal scaling fix

Today `OperationWaiters` (in `packages/server/src/registry.rs`) is a per-process
`DashMap<String, oneshot::Sender<TaskResult>>`. The plugin host_func dispatches
a `Task` onto a shared MQ queue; the worker publishes results to a single shared
`operation_results` queue; whichever server replica's
`consume_operation_results` consumer happens to receive the message owns the
answer — but the waiter lives only on the originating replica. Results land on
the wrong replica `(N-1)/N` of the time and the request times out. Verified by
reading `consumers/operation_result.rs` and `host_funcs/dispatch.rs`.

Fix:

1. **Server identity.** New env var `BROCCOLI__SERVER__ID`, defaulting to
   `hostname()`. Mirrors `BROCCOLI__WORKER__ID`. Validated for safe characters
   (alphanumeric + `_-`).
2. **Reply-queue field on the MQ envelope.** `common::worker::Task` gains
   `reply_queue: Option<String>`. The server's dispatch
   (`host_funcs/dispatch.rs`) fills it as
   `Some(format!("operation_results.{server_id}"))` while preserving the
   existing `result_queue` field as the compatibility fallback.
3. **Worker honors the reply queue.** `worker::handle_operation_task` publishes
   the `TaskResult` to
   `task.reply_queue.as_deref().unwrap_or(&task.result_queue)`. The fallback
   keeps older envelopes working for one release.
4. **Per-replica consumer.** `consume_operation_results` already takes a
   `queue_name` parameter. The server passes its per-replica name
   (`format!("operation_results.{server_id}")`) at startup.
5. **Rolling-upgrade compat.** A v0.3 server dispatching against a v0.2 worker:
   the worker ignores `reply_queue`, publishes to the legacy `operation_results`
   queue, the v0.3 server's per-replica consumer never sees it, request times
   out. **Upgrade order is workers first, then servers.** Documented in
   `docs/upgrade.md`. The legacy global queue stays a no-op subscriber on v0.3
   servers (warns on receipt) so a stuck v0.2 server doesn't drop messages
   mid-rollout. Removed in v0.4.

Other HA-relevant audit results — no change needed:

- JWT signing: shared secret across replicas — works.
- Refresh tokens: DB-backed via `refresh_token::Entity` (verified in
  `handlers/auth.rs`) — replica-safe.
- Plugin storage host fn: `plugin_storage` table — replica-safe.
- No SSE, WebSocket, or `tokio::sync::broadcast` anywhere in
  `packages/server/src` — confirmed by grep.
- `LoadedPlugins` is per-process; each replica loads its own plugin instances
  from the shared `/plugins` mount. Plugin state crosses replicas only via
  DB-backed `plugin_storage`.

## Bundle layout & install UX

Tarball name: `broccoli-platform-vX.Y.Z.tar.gz`. Approximate size: 600 MB with
`-icpc` worker (the dominant cost).

```
broccoli-platform-vX.Y.Z/
├── images/
│   ├── server.tar.gz                   # docker save of broccoli-server:vX.Y.Z
│   ├── worker-base.tar.gz              # docker save of broccoli-worker:vX.Y.Z-base
│   ├── worker-icpc.tar.gz              # docker save of broccoli-worker:vX.Y.Z-icpc
│   ├── worker-full.tar.gz              # docker save of broccoli-worker:vX.Y.Z-full
│   ├── postgres.tar.gz                 # docker save of postgres:18-alpine
│   ├── redis.tar.gz                    # docker save of redis:7-alpine
│   ├── seaweedfs.tar.gz                # docker save of chrislusf/seaweedfs:4.15
│   └── caddy.tar.gz                    # docker save of caddy:2-alpine
├── docker-compose.infra.yaml.template
├── docker-compose.server.yaml.template
├── docker-compose.worker.yaml.template
├── docker-compose.gateway.yaml.template
├── docker-compose.single-host.yaml.template
├── .env.example                        # documented secrets + tunables
├── .env.infra.example
├── .env.server.example
├── .env.worker.example
├── .env.gateway.example
├── plugins/                            # icpc, ioi, batch-evaluator, communication-evaluator,
│                                       # standard-checkers, standard-languages, broccoli-zh-cn,
│                                       # cooldown, submission-limit
├── stress-test/
│   ├── broccoli-stress-test            # native binary for the bundle's target arch
│   └── README.md
├── examples/
│   ├── Dockerfile.worker.custom        # add a custom language toolchain
│   ├── Caddyfile                       # TLS reverse proxy for self-hosted virtual contests
│   └── Caddyfile.gateway               # HTTP gateway role load balancer
├── install.sh                          # opinionated installer
├── docker-entrypoint-worker.sh
├── docs/
│   ├── upgrade.md
│   ├── operator-runbook.md
│   └── tls-with-caddy.md
└── README.md
```

`install.sh` flow:

1. Verify `docker` and `docker compose` exist; refuse otherwise with a one-line
   install hint pointing at the official docker docs.
2. `docker load` each `images/*.tar.gz`.
3. If `.env` already exists, warn and skip; otherwise:
   - Generate random 64-char secrets for `POSTGRES_PASSWORD`, `REDIS_PASSWORD`,
     `BROCCOLI__AUTH__JWT_SECRET` via
     `openssl rand -base64 48 | tr -d '+/=' | head -c 64`.
   - Prompt once for the initial admin password (input hidden, confirm).
   - Write `.env` with `0600` perms.
4. `docker compose up -d`.
5. Poll `/healthz` until 200 or 90s timeout, with an exponential backoff that's
   friendly to log scrapers.
6. Run
   `./stress-test/broccoli-stress-test --url http://localhost:3000 --admin-username admin --admin-password "$ADMIN_PASS" --correctness-only`.
   Aborts on failure with a clear message and `docker compose logs` hint.
7. Print: URL, admin credentials, log-tail hint (`docker compose logs -f`),
   upgrade pointer (`see docs/upgrade.md`).

The script never reaches the network. Everything it needs lives inside the
tarball.

## TLS & reverse-proxy guidance

| Deployment                              | Recommended TLS path                                                                                                                                                                |
| --------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Aliyun / Tencent cloud, virtual contest | Cloud LB (Aliyun ALB / Tencent CLB) terminates TLS, forwards plain HTTP to one or more `broccoli-server` replicas. Set `BROCCOLI__SERVER__TRUSTED_PROXIES` to the LB's egress CIDR. |
| Self-hosted VPS, virtual contest        | Caddy in front of the compose stack. `examples/Caddyfile` provides a 5-line config with Let's Encrypt auto-renewal.                                                                 |
| Lab LAN, offline contest                | Plain HTTP. The contest is on a closed network.                                                                                                                                     |

We bundle neither nginx nor Caddy. The Caddyfile is documentation, not
infrastructure.

## CI release workflow

`.github/workflows/release.yml`. Extends the workflow defined in the stress-test
design.

```yaml
on:
  push:
    tags: ['v*']

concurrency:
  group: release-${{ github.ref }}
  cancel-in-progress: false

jobs:
  cargo-test:
    runs-on: ubuntu-latest
    steps: [checkout, cargo test --workspace --locked]

  build-stress-test:
    # As designed in 2026-05-03-stress-test-release-design.md.
    # Five artifacts: linux-x86_64, linux-aarch64, windows-x86_64, macos-universal.

  build-images:
    needs: cargo-test
    strategy:
      fail-fast: false
      matrix:
        include:
          - image: server
            target: runtime
          - image: worker
            target: runtime-base
          - image: worker
            target: runtime-icpc
          - image: worker
            target: runtime-full
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: docker/setup-qemu-action@v3
      - uses: docker/setup-buildx-action@v3
      - uses: docker/login-action@v3 # ACR
      - uses: docker/login-action@v3 # GHCR
      - uses: docker/build-push-action@v5
        with:
          platforms: linux/amd64,linux/arm64
          target: ${{ matrix.target }}
          file: Dockerfile.${{ matrix.image }}
          tags: |
            registry.cn-hangzhou.aliyuncs.com/broccoli/broccoli-${{ matrix.image }}:${{ github.ref_name }}-${{ matrix.target }}
            ghcr.io/${{ github.repository }}/broccoli-${{ matrix.image }}:${{ github.ref_name }}-${{ matrix.target }}
          cache-from:
            type=registry,ref=ghcr.io/${{ github.repository }}/cache:${{
            matrix.image }}-${{ matrix.target }}
          cache-to:
            type=registry,ref=ghcr.io/${{ github.repository }}/cache:${{
            matrix.image }}-${{ matrix.target }},mode=max
          push: true
          provenance: true
          sbom: true
          labels: |
            org.opencontainers.image.source=https://github.com/${{ github.repository }}
            org.opencontainers.image.version=${{ github.ref_name }}
            org.opencontainers.image.revision=${{ github.sha }}
            org.opencontainers.image.created=${{ steps.date.outputs.iso }}

  build-bundle:
    needs: [build-images, build-stress-test]
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - docker pull (server, worker-base, worker-icpc, worker-full,
        postgres:18-alpine, redis:7-alpine, chrislusf/seaweedfs:4.15,
        caddy:2-alpine)
      - docker save | gzip >
        images/{server,worker-base,worker-icpc,worker-full,postgres,redis,seaweedfs,caddy}.tar.gz
      - download stress-test linux-x86_64 artifact into stress-test/
      - copy plugins/, role compose templates, .env examples, install.sh,
        examples/, docs/
      - tar czf broccoli-platform-${{ github.ref_name }}.tar.gz
        broccoli-platform-${{ github.ref_name }}/
      - sha256sum > .sha256
      - upload-artifact

  release:
    needs: [build-images, build-stress-test, build-bundle]
    runs-on: ubuntu-latest
    steps:
      - download-artifact (all)
      - generate manifest.json (extends the stress-test manifest with image
        digests + bundle entry)
      - softprops/action-gh-release:
          create GitHub Release with bundle, .sha256, manifest, stress-test
          binaries, auto-generated changelog
```

Total wall-time target: 15 minutes. Rough breakdown: `cargo-test` 2m,
`build-stress-test` 4m (parallel matrix), `build-images` 6m (parallel matrix,
cargo-chef cache hot), `build-bundle` 2m, `release` 30s.

Cache strategy:
`cache-to: type=registry,ref=ghcr.io/.../cache:buildcache,mode=max`. Survives
across releases.

Secrets the workflow needs: `ACR_USERNAME`, `ACR_PASSWORD` (Aliyun ACR robot
account), `GITHUB_TOKEN` (provided). All scoped to the `release` environment
with required reviewers.

## Versioning

Lockstep, monorepo-wide, mirroring the stress-test design. One git tag
(`v0.3.0`) drives every artifact's version. CI updates `[package].version` in
`packages/server/Cargo.toml`, `packages/worker/Cargo.toml`,
`packages/stress-test/Cargo.toml`, and the JS workspace `package.json`s before
building, so `--version` output and image tags always match.

Default plugins are versioned independently in `plugin.toml` `version` fields.
The bundled set is whatever's in the repo at tag time. A future "plugin
compatibility matrix" feature is out of scope here.

## Test strategy

- Unit tests for the SPA fallback handler: file resolution, real 404s for
  missing assets, `index.html` for unknown routes that look like client routes.
- Unit test for `Task::reply_queue` defaulting and the worker's per-task publish
  path.
- Integration test (in `packages/server/tests/integration/`) for HA: spin up two
  `TestApp` instances sharing one PostgreSQL database and one MQ, dispatch an
  operation through replica A's plugin host fn, assert the reply lands on A even
  though replica B is also subscribed (to its own per-replica queue).
- Integration test for `/healthz`: returns 200 with healthy backends; returns
  503 when DB is down.
- CI smoke test: after building each image,
  `docker run --rm registry.../broccoli-server:vX --version` to confirm the
  binary at minimum links and starts.
- Bundle smoke test: in CI, download the just-built bundle, untar, run
  `install.sh` non-interactively (with pre-set env vars), assert all containers
  report healthy and the stress-test correctness pass exits 0.
- Dockerfile lint: `hadolint Dockerfile.server Dockerfile.worker` in CI; treat
  warnings as errors except for documented exceptions.
- Image vulnerability scan: `trivy image --severity HIGH,CRITICAL --exit-code 1`
  against each pushed image. Failure breaks the release.

## Out-of-scope follow-ups

- Helm chart / Kubernetes operator.
- Plugin compatibility matrix (`min_server_version` in plugin.toml + startup
  rejection).
- Mid-contest rolling upgrade.
- Built-in backup/export tooling.
- Auto-update channel for installed deployments.
- Aliyun OSS mirror for the bundle (defer until someone reports actual ACR/GHCR
  pain).
- Code signing for the platform binaries (relevant only if we ever ship a
  desktop installer).
- Removing the legacy global `operation_results` queue (deferred one release for
  compat).
- Distroless worker. The worker needs apt-installable compilers, so distroless
  is impractical without per-language stages — outside scope. Server uses
  distroless; worker stays on `debian:bookworm-slim`.
