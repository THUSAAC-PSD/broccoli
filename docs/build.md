# Build Broccoli Images

This document covers local production image builds. Default builds use upstream
registries only.

## Server Image

Build the server image with BuildKit:

```bash
DOCKER_BUILDKIT=1 docker build -f Dockerfile.server -t broccoli-server:dev .
```

The image contains:

- `/usr/local/bin/broccoli-server`
- baked frontend assets under `/srv/dist`
- bundled plugin files under `/plugins`

Useful smoke checks:

```bash
docker run --rm broccoli-server:dev --version
docker run --rm --entrypoint=/usr/bin/tini broccoli-server:dev --version
docker image inspect broccoli-server:dev --format '{{ .Config.User }}'
docker image inspect broccoli-server:dev \
  --format '{{ json .Config.Labels }}'
```

The runtime image sets:

- `BROCCOLI__SERVER__HOST=0.0.0.0`
- `BROCCOLI__SERVER__FRONTEND_DIST=/srv/dist`
- `BROCCOLI__PLUGIN__PLUGINS_DIR=/plugins`

Override database, Redis, storage, and auth secrets with normal Broccoli
environment variables at deployment time.

## Worker Images

`Dockerfile.worker` publishes three targets from the same file:

```bash
DOCKER_BUILDKIT=1 docker build --target runtime-base -t broccoli-worker:dev-base -f Dockerfile.worker .
DOCKER_BUILDKIT=1 docker build --target runtime-icpc -t broccoli-worker:dev-icpc -f Dockerfile.worker .
DOCKER_BUILDKIT=1 docker build --target runtime-full -t broccoli-worker:dev-full -f Dockerfile.worker .
```

The variants are:

- `runtime-base`: Broccoli worker plus pinned isolate, no language toolchains.
- `runtime-icpc`: base plus `gcc`, `g++`, `python3`, and a headless JDK.
- `runtime-full`: ICPC plus `nodejs`, `golang-go`, `rustc`, `fpc`, and `kotlin`.

Useful smoke checks:

```bash
docker run --rm broccoli-worker:dev-base --version
docker run --rm broccoli-worker:dev-icpc which g++
docker run --rm broccoli-worker:dev-full which kotlinc
docker run --rm broccoli-worker:dev-base id -u
```

The worker image expects a privileged container when the isolate sandbox uses
cgroups. The entrypoint prepares the cgroup v2 layout for isolate and then drops
to the `worker` user before execing the requested command.

## OCI Labels

CI should pass these build args so image metadata matches the release:

```bash
docker build -f Dockerfile.server -t broccoli-server:v0.3.0 \
  --build-arg VERSION=v0.3.0 \
  --build-arg REVISION="$(git rev-parse HEAD)" \
  --build-arg CREATED="$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --build-arg SOURCE=https://github.com/THUSAAC-PSD/broccoli \
  .
```

## CN Mirror Build

Operators inside mainland China can opt into mirrors:

```bash
DOCKER_BUILDKIT=1 docker build -f Dockerfile.server -t broccoli-server:cn \
  --build-arg USE_CN_MIRRORS=true \
  .
```

When `USE_CN_MIRRORS=true`, the Rust stages switch Cargo to rsproxy and Rustup
to TUNA, Debian utility stages use the TUNA Debian mirror, and the Node builder
uses the npmmirror npm registry for pnpm installs. Worker builds can also point
the isolate source checkout at an operator-controlled mirror:

```bash
DOCKER_BUILDKIT=1 docker build -f Dockerfile.worker --target runtime-icpc \
  -t broccoli-worker:cn \
  --build-arg USE_CN_MIRRORS=true \
  --build-arg ISOLATE_REPO_CN_MIRROR=https://example.com/mirrors/ioi/isolate.git \
  .
```

The default value is `false`, so upstream builds do not contact CN mirrors.

## Build Context

`.dockerignore` excludes generated outputs and local state such as `target/`,
`node_modules/`, `dist/`, `build/`, `.git/`, `.env*`, and `config/`. It then
whitelists only the workspace files needed by the Dockerfiles.
