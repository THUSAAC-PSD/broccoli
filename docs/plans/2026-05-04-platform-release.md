# Platform Release & Distribution — Implementation Plan

**Date:** 2026-05-04 **Status:** Implemented and VPS-verified **Owner:** Joseph
**Design:** `2026-05-04-platform-release-design.md` **Companion plan:**
`2026-05-03-stress-test-release.md` (stress-test CLI; some CI steps are shared)

## Implementation Evidence

- Local release checks: `bash -n` for release scripts, `shellcheck`,
  `actionlint`, and all role compose templates with their env examples.
- Local artifact rehearsal: built `broccoli-platform-v0.0.0-local.tar.gz`,
  verified its checksum, installed `single-host` from a fresh extraction, then
  installed split `infra`, `server`, `worker`, and `gateway` roles from separate
  fresh extractions.
- VPS rehearsal on `81.68.195.202`: installed the bundle, ran the installer
  correctness smoke, verified SeaweedFS bucket/chunks, served the frontend on
  public port 80, and ran the bounded load test with `completed=200`,
  `passed=200`, `error_count=0`, `p95_ms=12103`.
- Bundled downloads:
  `cargo test -p server --features bundled-stress-test --test integration downloads:: --locked`
  and
  `cargo test -p server --features bundled-stress-test --lib downloads:: --locked`
  passed.

## Conventions

- Each phase has independent tasks marked `[task: ...]` that can be assigned to
  subagents.
- Inter-phase ordering matters; intra-phase tasks may run in parallel where
  noted.
- Acceptance criteria are observable: a command to run and the expected output.
  No "looks right" judgments.
- Where a task touches an existing file, the path is given relative to the repo
  root.
- All Rust changes follow the conventions in `CLAUDE.md` (handler structure,
  error codes, instrumentation, etc.).

---

## Phase 0 — Server prerequisites (no Docker work yet)

These changes land before the Dockerfile rewrite so the rewritten image has
something to run.

### [task 0.1] Health endpoints + `--healthcheck` CLI flag

**Goal:** Server exposes `/healthz` (plain) and `/api/v1/health`
(utoipa-documented). Binary supports `--healthcheck` for use in
`HEALTHCHECK CMD`.

**Files:**

- New: `packages/server/src/handlers/health.rs`
- Edit: `packages/server/src/handlers/mod.rs`,
  `packages/server/src/routes/v1.rs`, `packages/server/src/lib.rs` (mount
  `/healthz` on the plain Router), `packages/server/src/main.rs` (parse
  `--healthcheck`, exit 0/1)

**Behavior:**

- `GET /healthz` and `GET /api/v1/health`: pings DB (`SELECT 1`, 2s timeout) and
  MQ (`PING`, 2s timeout). Returns 200 with
  `{"status":"ok","db":"ok","mq":"ok","version":"X.Y.Z","git_sha":"abc123"}` if
  both healthy. Returns 503 with the same shape (with failing fields set to
  `"down"`) otherwise.
- `--healthcheck`: makes a local HTTP GET to `http://127.0.0.1:{port}/healthz`
  (port from config), exits 0 if 200, 1 otherwise. No JWT required.

**Acceptance:**

- `curl -sf http://localhost:3000/healthz | jq .status` → `"ok"`.
- `cargo run -p server -- --healthcheck` → exit 0 when server is up.
- Integration test: `health::returns_200_when_db_and_mq_are_up`,
  `health::returns_503_when_db_is_down`.

### [task 0.2] Horizontal-scaling fix (per-replica result queue)

**Goal:** Multiple server replicas correctly receive their own operation
results.

**Files:**

- Edit: `packages/common/src/worker.rs` — add `reply_queue: Option<String>` to
  `Task` (use `#[serde(default, skip_serializing_if = "Option::is_none")]` for
  backward compat, with existing `result_queue` as the fallback).
- Edit: `packages/server/src/config.rs` — add `[server.id]` defaulting to
  `hostname()`.
- Edit: `packages/server/src/host_funcs/dispatch.rs` — fill `reply_queue` with
  `format!("operation_results.{}", server_id)`.
- Edit: `packages/server/src/main.rs` — pass per-replica queue name to
  `consume_operation_results`. Keep a separate consumer on the legacy
  `operation_results` queue that logs a warning on receipt (compat for in-flight
  v0.2-worker results during rollout).
- Edit: `packages/worker/src/handler.rs` (or wherever results are published) —
  publish to `task.reply_queue.as_deref().unwrap_or(&task.result_queue)`.
- New: `packages/server/tests/integration/scaling.rs` — two-replica test.

**Acceptance:**

- New integration test passes: dispatch via replica A → result delivered to A's
  waiter, never to B.
- Existing `cargo test --workspace` stays green.
- Rolling-upgrade path documented in `docs/upgrade.md` (created in Phase 2).

### [task 0.3] axum edge gaps

**Goal:** Server stands alone behind any L7 LB without nginx.

**Files:**

- Edit: `Cargo.toml` workspace — `tower-http` features
  `cors, trace, fs, compression-gzip, timeout`. Add `axum-client-ip = "0.6"`.
- Edit: `packages/server/Cargo.toml` — add `axum-client-ip` dep.
- Edit: `packages/server/src/lib.rs` — add `CompressionLayer`, `TimeoutLayer`
  (60s on `/api`), `axum_client_ip::SecureClientIpSource::ConnectInfo`
  middleware.
- Edit: `packages/server/src/main.rs` — wrap `axum::serve(...)` in
  `with_graceful_shutdown(shutdown_signal())`. Configure
  `Http1Builder::header_read_timeout(Duration::from_secs(30))`.
- Edit: `packages/server/src/config.rs` — add
  `[server.trusted_proxies]: Vec<String>` (CIDRs), default empty.

**Acceptance:**

- `curl -H 'Accept-Encoding: gzip' -I http://localhost:3000/api/v1/contests` →
  `Content-Encoding: gzip`.
- Slow body request closed by 30s.
- SIGTERM → server logs "graceful shutdown started", drains in-flight requests,
  exits within 30s.
- `cargo test --workspace` green.

### [task 0.4] SPA serving from baked frontend

**Goal:** Server serves `frontend/dist/` with SPA fallback. No nginx needed.

**Files:**

- Edit: `packages/server/src/lib.rs` — register
  `ServeDir::new("/srv/dist/assets")` at `/assets/*` (lower precedence than the
  existing plugin asset route), and a catch-all fallback that tries the path
  against `/srv/dist/` and falls back to `index.html`.
- Edit: `packages/server/src/config.rs` — `[server.frontend_dist]` path, default
  `/srv/dist`.

**Acceptance:**

- With a pretend `dist/` containing `index.html` and `assets/main.js`:
  - `curl -I http://localhost:3000/` → 200, `Content-Type: text/html`.
  - `curl -I http://localhost:3000/assets/main.js` → 200,
    `Content-Type: application/javascript`.
  - `curl -I http://localhost:3000/contests/42` → 200, `text/html` (SPA
    fallback).
  - `curl -I http://localhost:3000/api/v1/health` → 200, `application/json` (API
    takes precedence).
  - `curl -I http://localhost:3000/assets/typo.js` → 404.
- Unit tests for the fallback resolver.

### [task 0.5] Optional auth rate limiting

**Goal:** `tower-governor` throttles `/api/v1/auth/login` when enabled.

**Files:**

- Edit: `packages/server/Cargo.toml` — `tower_governor = "0.4"`.
- Edit: `packages/server/src/lib.rs` — wrap `/api/v1/auth/login` route in a
  Governor layer when `BROCCOLI__SERVER__RATE_LIMIT_AUTH=true`.

**Acceptance:**

- With `RATE_LIMIT_AUTH=true`, 11 logins from the same IP in a minute → 11th
  returns 429 with `Retry-After`.
- Default (off) → no throttling.

---

## Phase 1 — Dockerfile rewrite

Depends on Phase 0 being green (the new `--healthcheck` flag is referenced by
the Dockerfile `HEALTHCHECK`).

### [task 1.1] Replace `Dockerfile.server`

**Goal:** Production-grade server image. Distroless runtime, non-root,
cargo-chef + BuildKit caching, frontend baked in, multi-arch ready.

**Files:**

- Replace: `Dockerfile.server`
- New: `docs/build.md` (CN mirrors usage)
- Verify: `.dockerignore` excludes `target/`, `node_modules/`, `dist/`, `.git/`,
  build context under 5 MB.

**Acceptance:**

- `DOCKER_BUILDKIT=1 docker build -f Dockerfile.server -t broccoli-server:dev .`
  → succeeds.
- `docker run --rm broccoli-server:dev --version` → prints version.
- `docker image inspect broccoli-server:dev --format '{{ .Config.User }}'` →
  non-root.
- `docker image inspect broccoli-server:dev` → has
  `org.opencontainers.image.{title,version,revision,source}` labels.
- Image size under 200 MB.
- `hadolint Dockerfile.server` → 0 errors.
- Cold build under 8 min on `ubuntu-latest` runner; warm rebuild (no source
  change) under 1 min.

### [task 1.2] Replace `Dockerfile.worker` with multi-target stages

**Goal:** Three image variants from one Dockerfile. cargo-chef + BuildKit
caching, isolate pinned, non-root binary user.

**Files:**

- Replace: `Dockerfile.worker`
- Verify: `docker-entrypoint-worker.sh` performs cgroup/capability checks
  correctly.

**Acceptance:**

- `DOCKER_BUILDKIT=1 docker build --target runtime-base -t broccoli-worker:dev-base .`
  → succeeds, image under 100 MB.
- `DOCKER_BUILDKIT=1 docker build --target runtime-icpc -t broccoli-worker:dev-icpc .`
  → succeeds, image under 700 MB.
- `DOCKER_BUILDKIT=1 docker build --target runtime-full -t broccoli-worker:dev-full .`
  → succeeds, image under 1.5 GB.
- `docker run --rm broccoli-worker:dev-base --version` → prints version.
- `docker run --rm broccoli-worker:dev-icpc which g++` → prints path.
- `docker run --rm broccoli-worker:dev-full which kotlinc` → prints path.
- `hadolint Dockerfile.worker` → 0 errors.

### [task 1.3] CN-mirror build arg (opt-in)

**Goal:** Operators inside CN can build locally with
`--build-arg USE_CN_MIRRORS=true`. Default builds touch only upstream.

**Files:**

- Already in scope of 1.1 and 1.2; verify `ARG USE_CN_MIRRORS=false` defaults
  correctly and only triggers the mirror swap when `=true`.
- Document in `docs/build.md`.

**Acceptance:**

- `docker build -f Dockerfile.server -t s:default .` → no calls to rsproxy.cn /
  TUNA / ghproxy in build logs.
- `docker build --build-arg USE_CN_MIRRORS=true -f Dockerfile.server -t s:cn .`
  → uses mirrors.

---

## Phase 2 — Bundle, compose, installer, docs

Independent of Phase 1's _contents_ but depends on the image _names_.

### [task 2.1] Production role compose templates

**Goal:** Compose files referenced by image _tags only_, split by deploy role.

**Files:**

- New: `release/docker-compose.infra.yaml.template` — `db` (postgres:18-alpine,
  named volume mounted at `/var/lib/postgresql`), `redis`, `seaweedfs`, and
  `seaweedfs-init`.
- New: `release/docker-compose.server.yaml.template` — one server process
  connected to external DB/Redis/object storage.
- New: `release/docker-compose.worker.yaml.template` — one privileged worker
  process connected to external DB/Redis/object storage.
- New: `release/docker-compose.gateway.yaml.template` — optional Caddy load
  balancer for server machines.
- New: `release/docker-compose.single-host.yaml.template` — rehearsal-only
  all-in-one topology.

**Acceptance:**

- Each role template parses with its matching `.env.<role>.example`.
- All long-running services define `healthcheck` and `restart: unless-stopped`.

### [task 2.2] `install.sh`

**Goal:** Role-aware install on fresh Linux+Docker hosts.

**Files:**

- New: `release/install.sh`
- Behavior per design "Bundle layout & install UX".

**Acceptance:**

- On fresh ubuntu:24.04 + docker hosts, `./install.sh infra`,
  `./install.sh server`, and `./install.sh worker` install the corresponding
  local role.
- `./install.sh server` and `./install.sh worker` fail before
  `docker compose up` when infra URLs/storage credentials are missing.
- Re-running with existing `.env` → does not regenerate secrets, prints clear
  message.

### [task 2.3] `.env.example`, `examples/`, `docs/`

**Goal:** Operator documentation that's actually self-sufficient.

**Files:**

- New: `release/.env.example` and role-specific `.env.<role>.example` files —
  every documented variable, sane defaults, comments explaining each.
- New: `release/examples/Dockerfile.worker.custom` — adds `fp-compiler` and
  `kotlin` against `-base`.
- New: `release/examples/Caddyfile` — TLS reverse proxy/load balancer.
- New: `release/examples/Caddyfile.gateway` — HTTP gateway Caddyfile for the
  `gateway` role.
- New: `release/docs/upgrade.md` — workers-first ordering, version pinning,
  rollback steps.
- New: `release/docs/operator-runbook.md` — common ops (logs, restart, plugin
  reload, password reset).
- New: `release/docs/tls-with-caddy.md` — DNS, ports, certs, troubleshooting.

**Acceptance:**

- Role env examples cover every variable referenced by the corresponding role
  compose template.
- `caddy validate --config release/examples/Caddyfile` → success.
- All role compose templates parse with `docker compose config`.

### [task 2.4] Bundle assembler script

**Goal:** Local script to produce the bundle for testing without going through
CI.

**Files:**

- New: `scripts/build-bundle.sh` — takes a version arg, pulls images,
  `docker save | gzip`s them, copies bundle layout, downloads stress-test binary
  from a local build, tars.

**Acceptance:**

- `./scripts/build-bundle.sh v0.0.0-dev` → produces
  `dist/broccoli-platform-v0.0.0-dev.tar.gz`.
- Untarring it and running `install.sh` on a fresh container completes
  successfully.

---

## Phase 3 — CI release workflow

Depends on Phases 1 and 2.

### [task 3.1] Extend `.github/workflows/release.yml`

**Goal:** One tag push produces all artifacts. Wall time under 15 min.

**Files:**

- Edit: `.github/workflows/release.yml` (created by the stress-test plan; this
  task adds the `cargo-test`, `build-images`, `build-bundle` jobs and wires the
  release job to include the bundle).

**Acceptance:**

- Pushing a `v0.0.0-rc.1` tag triggers the workflow.
- All four images (`server`, `worker-base`, `worker-icpc`, `worker-full`) appear
  in ACR and GHCR for both `linux/amd64` and `linux/arm64`.
- The GitHub Release includes the platform tarball + `.sha256` + manifest + the
  existing stress-test binaries.
- Workflow wall time under 15 min on the main runner pool.

### [task 3.2] Required secrets + environment

**Goal:** ACR credentials configured; `release` environment requires reviewer
approval.

**Files:**

- GitHub repo settings (manual): `release` environment with `ACR_USERNAME`,
  `ACR_PASSWORD` secrets, required reviewer.
- Document in `docs/release.md`.

**Acceptance:**

- A test tag push pauses for reviewer approval before publishing release images.

### [task 3.3] Image security + lint gates

**Goal:** Each released image passes `hadolint` and `trivy` HIGH/CRITICAL gates.

**Files:**

- Edit: `.github/workflows/release.yml` — add hadolint step before image builds,
  trivy step after.

**Acceptance:**

- A Dockerfile with a deliberate `HIGH` CVE-bearing base image fails the
  workflow.

---

## Phase 4 — End-to-end verification

Depends on Phase 3 producing artifacts.

### [task 4.1] Bundle smoke test in CI

**Goal:** Every release tag is self-verified.

**Files:**

- Edit: `.github/workflows/release.yml` — new job `bundle-smoke-test` that runs
  after `build-bundle`. Spins up a fresh `docker-in-docker` container, copies
  the bundle in, runs `install.sh` non-interactively, asserts containers healthy
  and stress-test correctness pass exits 0.

**Acceptance:**

- Failure of the smoke test blocks the release.

### [task 4.2] Manual rehearsal on a real cloud VM

**Goal:** Catch the things CI's docker-in-docker can't.

**Steps (manual, documented in `docs/release.md`):**

1. Provision an Aliyun ECS instance (any 2-vCPU / 4-GB tier).
2. Install Docker.
3. `docker compose pull` against ACR for the candidate tag,
   `docker compose up -d`, run stress-test correctness pass.
4. Provision a second instance, repeat with the offline path (download bundle,
   `install.sh`).
5. Run the stress-test load phase against the cloud instance to validate p95
   budgets.

**Acceptance:**

- Both rehearsals pass before the GitHub Release is marked non-prerelease.

### [task 4.3] Rolling-upgrade rehearsal

**Goal:** Verify the workers-first upgrade order works in practice.

**Steps:**

1. Stand up a cluster on the previous tag (n-1).
2. Rolling-update workers to n.
3. Verify v(n-1) servers + v(n) workers process submissions correctly (legacy
   global queue path).
4. Rolling-update servers to n.
5. Verify per-replica queue path takes over without dropped messages.

**Acceptance:**

- No submission fails during the upgrade window.
- Documented in `docs/upgrade.md` with command samples.

---

## Risk register

| Risk                                                         | Mitigation                                                                                                              |
| ------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------- |
| ACR credentials leak from CI                                 | `release` environment + required reviewer; rotate credentials quarterly.                                                |
| isolate upstream changes break our pin                       | Pinned tag, not master. Bump via PR with manual rehearsal.                                                              |
| Distroless server image misses a runtime lib                 | Image smoke test in CI runs the binary; trivy check catches drift.                                                      |
| Multi-arch isolate build fails on aarch64                    | CI builds both arches; failure surfaces immediately.                                                                    |
| Bundle tarball balloons past 1 GB                            | Default uses `-icpc` worker (~600 MB compressed); `-full` is opt-in via env, not bundled.                               |
| HA fix breaks rolling upgrade                                | Workers-first order documented; legacy queue subscriber on v0.3 server logs warnings to surface mistakes.               |
| Operator skips `install.sh` and runs raw `docker compose up` | Compose file works on its own; `install.sh` is convenience, not required.                                               |
| Distroless prevents shell debugging                          | Document `docker compose exec server sh` does not work; recommend a debug image variant (out of scope for v1, tracked). |

## Done criteria

- All Phase 0–4 tasks have green acceptance criteria.
- A `v0.3.0` tag (or whatever the first release version becomes) cuts a complete
  release that:
  - Publishes images to ACR and GHCR.
  - Publishes a GitHub Release with platform tarball + stress-test binaries +
    manifest.
  - Passes the bundle smoke test.
  - Passes manual rehearsals on a real cloud VM and a fresh Linux+Docker box.
- The README links to `docs/release.md` for operators.
- The legacy global `operation_results` queue is documented as deprecated, with
  removal scheduled for the next release.
