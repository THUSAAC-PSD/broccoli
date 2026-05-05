#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"
BUNDLE_VERSION_DEFAULT="v0.3.0"

die() {
  echo "error: $*" >&2
  exit 1
}

usage() {
  cat >&2 <<'EOF'
usage: ./install.sh <role>

Roles:
  infra       PostgreSQL, Redis, and SeaweedFS object storage
  server      one Broccoli HTTP/API server connected to infra
  worker      one Broccoli judge worker connected to infra
  gateway     optional Caddy load balancer for one or more server machines
  single-host rehearsal-only all-in-one install

Server and worker nodes need BROCCOLI__DATABASE__URL, BROCCOLI__MQ__URL, and
shared storage credentials from the infra node. Use LAN IPs or private cloud
addresses for those URLs.
EOF
}

is_interactive() {
  case "${BROCCOLI_NONINTERACTIVE:-false}" in
    1|true|yes) return 1 ;;
  esac
  case "${BROCCOLI_INTERACTIVE:-false}" in
    1|true|yes) return 0 ;;
  esac
  [ -t 0 ]
}

prompt_value() {
  local name prompt default answer
  name="$1"
  prompt="$2"
  default="$3"
  [ -z "${!name:-}" ] || return 0
  is_interactive || return 0

  if [ -n "$default" ]; then
    printf "%s [%s]: " "$prompt" "$default" >&2
  else
    printf "%s: " "$prompt" >&2
  fi
  read -r answer
  if [ -z "$answer" ]; then
    answer="$default"
  fi
  printf -v "$name" '%s' "$answer"
}

prompt_secret() {
  local name prompt default_mode answer confirm
  name="$1"
  prompt="$2"
  default_mode="${3:-required}"
  [ -z "${!name:-}" ] || return 0
  is_interactive || return 0

  if [ "$default_mode" = "generate" ]; then
    printf "%s [leave blank to generate]: " "$prompt" >&2
  else
    printf "%s: " "$prompt" >&2
  fi
  if [ -t 0 ]; then
    stty -echo
  fi
  read -r answer
  if [ -t 0 ]; then
    stty echo
    printf "\n" >&2
  fi

  if [ -z "$answer" ] && [ "$default_mode" = "generate" ]; then
    return 0
  fi
  [ -n "$answer" ] || return 0

  if [ "$default_mode" != "copied" ]; then
    printf "Confirm %s: " "$prompt" >&2
    if [ -t 0 ]; then
      stty -echo
    fi
    read -r confirm
    if [ -t 0 ]; then
      stty echo
      printf "\n" >&2
    fi
    [ "$answer" = "$confirm" ] || die "secret values did not match"
  fi
  printf -v "$name" '%s' "$answer"
}

choose_role_interactive() {
  local answer
  cat >&2 <<'EOF'
Choose this machine's Broccoli role:
  1) infra       PostgreSQL, Redis, SeaweedFS
  2) server      HTTP/API server connected to infra
  3) worker      one judge worker connected to infra
  4) gateway     Caddy load balancer for server machines
  5) single-host rehearsal/demo only
EOF
  printf "Role [1]: " >&2
  read -r answer
  case "${answer:-1}" in
    1|infra) ROLE=infra ;;
    2|server) ROLE=server ;;
    3|worker) ROLE=worker ;;
    4|gateway) ROLE=gateway ;;
    5|single-host) ROLE=single-host ;;
    *) die "unknown role selection '$answer'" ;;
  esac
}

need() {
  local cmd hint
  cmd="$1"
  hint="${2:-Install $cmd and retry.}"
  command -v "$cmd" >/dev/null 2>&1 || die "$cmd is required. $hint"
}

random_secret() {
  openssl rand -hex 32
}

primary_ip() {
  hostname -I 2>/dev/null | awk '{print $1}' || true
}

env_quote() {
  local value
  value="$1"
  case "$value" in
    *$'\n'*|*$'\r'*) die "environment values must not contain newlines" ;;
  esac
  value=${value//\'/\'\\\'\'}
  printf "'%s'" "$value"
}

bind_port() {
  local bind
  bind="$1"
  printf '%s' "${bind##*:}"
}

json_string_value() {
  local name value
  name="$1"
  value="$2"
  case "$value" in
    *$'\n'*|*$'\r'*|*\"*|*\\*) die "$name cannot contain newlines, quotes, or backslashes" ;;
  esac
  printf '"%s"' "$value"
}

compose() {
  docker compose -f "$COMPOSE_FILE" "$@"
}

load_image() {
  local archive loaded image_tag
  archive="$1"
  loaded="$(gzip -dc "$archive" | docker load)"
  printf '%s\n' "$loaded" >&2
  image_tag="$(printf '%s\n' "$loaded" | awk -F': ' '/Loaded image:/{print $2; exit}')"
  [ -n "$image_tag" ] || die "docker load did not report an image tag for $archive"
  printf '%s\n' "$image_tag"
}

load_bundled_images() {
  local image loaded
  loaded_server_image=""
  loaded_worker_base_image=""
  loaded_worker_icpc_image=""
  loaded_worker_full_image=""
  loaded_worker_image=""
  loaded_postgres_image=""
  loaded_redis_image=""
  loaded_seaweedfs_image=""
  loaded_caddy_image=""

  [ -d images ] || return 0
  for image in images/*.tar.gz; do
    [ -e "$image" ] || continue
    case "$ROLE:$(basename "$image")" in
      infra:postgres.tar.gz|infra:redis.tar.gz|infra:seaweedfs.tar.gz|\
      server:server.tar.gz|worker:worker-base.tar.gz|worker:worker-icpc.tar.gz|\
      worker:worker-full.tar.gz|gateway:caddy.tar.gz|\
      single-host:server.tar.gz|single-host:worker-base.tar.gz|\
      single-host:worker-icpc.tar.gz|single-host:worker-full.tar.gz|single-host:postgres.tar.gz|\
      single-host:redis.tar.gz|single-host:seaweedfs.tar.gz|single-host:caddy.tar.gz)
        echo "loading $image"
        loaded="$(load_image "$image")"
        case "$(basename "$image")" in
          server.tar.gz) loaded_server_image="$loaded" ;;
          worker-base.tar.gz) loaded_worker_base_image="$loaded" ;;
          worker-icpc.tar.gz) loaded_worker_icpc_image="$loaded"; loaded_worker_image="$loaded" ;;
          worker-full.tar.gz) loaded_worker_full_image="$loaded" ;;
          postgres.tar.gz) loaded_postgres_image="$loaded" ;;
          redis.tar.gz) loaded_redis_image="$loaded" ;;
          seaweedfs.tar.gz) loaded_seaweedfs_image="$loaded" ;;
          caddy.tar.gz) loaded_caddy_image="$loaded" ;;
        esac
        ;;
    esac
  done
}

worker_image_for_variant() {
  local version variant default_repo
  version="$1"
  variant="$2"
  default_repo="ghcr.io/thusaac-psd/broccoli/broccoli-worker"
  case "$variant" in
    base) printf '%s\n' "${loaded_worker_base_image:-${BROCCOLI_WORKER_BASE_IMAGE:-$default_repo:$version-base}}" ;;
    icpc) printf '%s\n' "${loaded_worker_icpc_image:-${loaded_worker_image:-${BROCCOLI_WORKER_ICPC_IMAGE:-$default_repo:$version-icpc}}}" ;;
    full) printf '%s\n' "${loaded_worker_full_image:-${BROCCOLI_WORKER_FULL_IMAGE:-$default_repo:$version-full}}" ;;
    *) die "unknown worker image variant '$variant'" ;;
  esac
}

choose_worker_image_interactive() {
  local version answer custom
  version="$1"
  [ -z "${BROCCOLI_WORKER_IMAGE:-}" ] || return 0
  is_interactive || return 0

  cat >&2 <<'EOF'
Choose worker image:
  1) icpc  C, C++, Java, Python (recommended for normal contests)
  2) full  ICPC plus Node.js, Go, Rust, Pascal, Kotlin
  3) base  isolate sandbox only; use with a custom derived image
  4) custom image tag
EOF
  printf "Worker image [1]: " >&2
  read -r answer
  case "${answer:-1}" in
    1|icpc) BROCCOLI_WORKER_IMAGE="$(worker_image_for_variant "$version" icpc)" ;;
    2|full) BROCCOLI_WORKER_IMAGE="$(worker_image_for_variant "$version" full)" ;;
    3|base) BROCCOLI_WORKER_IMAGE="$(worker_image_for_variant "$version" base)" ;;
    4|custom)
      printf "Custom worker image tag: " >&2
      read -r custom
      [ -n "$custom" ] || die "custom worker image tag is required"
      BROCCOLI_WORKER_IMAGE="$custom"
      ;;
    *) die "unknown worker image selection '$answer'" ;;
  esac
}

host_health_url() {
  local bind host port
  bind="${BROCCOLI_HTTP_BIND:-0.0.0.0:3000}"
  port="${bind##*:}"
  host="${bind%:*}"
  case "$host" in
    ""|"0.0.0.0"|"::"|"[::]") host="127.0.0.1" ;;
  esac
  printf 'http://%s:%s/healthz\n' "$host" "$port"
}

gateway_health_url() {
  local bind host port
  bind="${BROCCOLI_GATEWAY_HTTP_BIND:-0.0.0.0:80}"
  port="${bind##*:}"
  host="${bind%:*}"
  case "$host" in
    ""|"0.0.0.0"|"::"|"[::]") host="127.0.0.1" ;;
  esac
  printf 'http://%s:%s/healthz\n' "$host" "$port"
}

should_run_stress_smoke() {
  local setting
  setting="${BROCCOLI_RUN_STRESS_SMOKE:-auto}"
  case "$setting" in
    1|true|yes) return 0 ;;
    0|false|no) return 1 ;;
    auto|"")
      [ "$ROLE" = "single-host" ]
      ;;
    *) die "BROCCOLI_RUN_STRESS_SMOKE must be true, false, or auto" ;;
  esac
}

require_env() {
  local name
  for name in "$@"; do
    [ -n "${!name:-}" ] || die "set $name before installing role '$ROLE'"
  done
}

validate_role_env() {
  case "$ROLE" in
    infra)
      require_env POSTGRES_USER POSTGRES_PASSWORD POSTGRES_DB REDIS_PASSWORD \
        BROCCOLI__STORAGE__OBJECT_STORAGE__BUCKET \
        BROCCOLI__STORAGE__OBJECT_STORAGE__ACCESS_KEY \
        BROCCOLI__STORAGE__OBJECT_STORAGE__SECRET_KEY
      ;;
    server)
      require_env BROCCOLI_SERVER_IMAGE BROCCOLI__SERVER__ID BROCCOLI__DATABASE__URL \
        BROCCOLI__MQ__URL BROCCOLI__AUTH__JWT_SECRET BROCCOLI_BOOTSTRAP_ADMIN_PASSWORD \
        BROCCOLI__STORAGE__OBJECT_STORAGE__ENDPOINT \
        BROCCOLI__STORAGE__OBJECT_STORAGE__ACCESS_KEY \
        BROCCOLI__STORAGE__OBJECT_STORAGE__SECRET_KEY
      ;;
    worker)
      require_env BROCCOLI_WORKER_IMAGE BROCCOLI__WORKER__ID BROCCOLI__DATABASE__URL \
        BROCCOLI__MQ__URL BROCCOLI__STORAGE__OBJECT_STORAGE__ENDPOINT \
        BROCCOLI__STORAGE__OBJECT_STORAGE__ACCESS_KEY \
        BROCCOLI__STORAGE__OBJECT_STORAGE__SECRET_KEY
      ;;
    gateway)
      require_env CADDY_IMAGE BROCCOLI_UPSTREAMS
      ;;
    single-host)
      require_env BROCCOLI_SERVER_IMAGE BROCCOLI_WORKER_IMAGE POSTGRES_USER POSTGRES_PASSWORD \
        POSTGRES_DB REDIS_PASSWORD BROCCOLI__AUTH__JWT_SECRET \
        BROCCOLI_BOOTSTRAP_ADMIN_PASSWORD BROCCOLI__STORAGE__OBJECT_STORAGE__BUCKET \
        BROCCOLI__STORAGE__OBJECT_STORAGE__ACCESS_KEY \
        BROCCOLI__STORAGE__OBJECT_STORAGE__SECRET_KEY
      ;;
  esac
}

write_seaweedfs_config() {
  mkdir -p config
  umask 077
  cat > config/seaweedfs-s3.json <<EOF
{
  "identities": [
    {
      "name": "broccoli",
      "credentials": [
        {
          "accessKey": $(json_string_value BROCCOLI__STORAGE__OBJECT_STORAGE__ACCESS_KEY "$BROCCOLI__STORAGE__OBJECT_STORAGE__ACCESS_KEY"),
          "secretKey": $(json_string_value BROCCOLI__STORAGE__OBJECT_STORAGE__SECRET_KEY "$BROCCOLI__STORAGE__OBJECT_STORAGE__SECRET_KEY")
        }
      ],
      "actions": ["Admin", "Read", "Write", "List", "Tagging"]
    }
  ]
}
EOF
}

write_env_file() {
  local version server_image worker_image postgres_image redis_image seaweedfs_image caddy_image
  version="${BROCCOLI_VERSION:-$BUNDLE_VERSION_DEFAULT}"
  if [ "$ROLE" = "worker" ] || [ "$ROLE" = "single-host" ]; then
    choose_worker_image_interactive "$version"
  fi
  server_image="${loaded_server_image:-${BROCCOLI_SERVER_IMAGE:-ghcr.io/thusaac-psd/broccoli/broccoli-server:$version}}"
  worker_image="${BROCCOLI_WORKER_IMAGE:-$(worker_image_for_variant "$version" icpc)}"
  postgres_image="${loaded_postgres_image:-${POSTGRES_IMAGE:-postgres:18-alpine}}"
  redis_image="${loaded_redis_image:-${REDIS_IMAGE:-redis:7-alpine}}"
  seaweedfs_image="${loaded_seaweedfs_image:-${SEAWEEDFS_IMAGE:-chrislusf/seaweedfs:4.15}}"
  caddy_image="${loaded_caddy_image:-${CADDY_IMAGE:-caddy:2-alpine}}"

  umask 077
  case "$ROLE" in
    infra|single-host)
      local infra_host postgres_user postgres_password postgres_db redis_password s3_access s3_secret s3_bucket s3_endpoint jwt_secret admin_user admin_pass postgres_bind redis_bind s3_bind postgres_port redis_port s3_port
      prompt_value BROCCOLI_INFRA_HOST "Infra address that server/worker machines will use" "$(primary_ip)"
      prompt_value POSTGRES_BIND "PostgreSQL bind address" "0.0.0.0:5432"
      prompt_value REDIS_BIND "Redis bind address" "0.0.0.0:6379"
      prompt_value SEAWEEDFS_S3_BIND "SeaweedFS S3 bind address" "0.0.0.0:8333"
      prompt_value SEAWEEDFS_MASTER_BIND "SeaweedFS master bind address" "127.0.0.1:9333"
      prompt_value BROCCOLI_ADMIN_USERNAME "Initial admin username" "admin"
      prompt_secret BROCCOLI_ADMIN_PASSWORD "Initial admin password" generate
      infra_host="${BROCCOLI_INFRA_HOST:-$(primary_ip)}"
      infra_host="${infra_host:-127.0.0.1}"
      postgres_bind="${POSTGRES_BIND:-0.0.0.0:5432}"
      redis_bind="${REDIS_BIND:-0.0.0.0:6379}"
      s3_bind="${SEAWEEDFS_S3_BIND:-0.0.0.0:8333}"
      postgres_port="$(bind_port "$postgres_bind")"
      redis_port="$(bind_port "$redis_bind")"
      s3_port="$(bind_port "$s3_bind")"
      postgres_user="${POSTGRES_USER:-postgres}"
      postgres_password="${POSTGRES_PASSWORD:-$(random_secret)}"
      postgres_db="${POSTGRES_DB:-broccoli}"
      redis_password="${REDIS_PASSWORD:-$(random_secret)}"
      s3_access="${BROCCOLI__STORAGE__OBJECT_STORAGE__ACCESS_KEY:-$(random_secret)}"
      s3_secret="${BROCCOLI__STORAGE__OBJECT_STORAGE__SECRET_KEY:-$(random_secret)}"
      s3_bucket="${BROCCOLI__STORAGE__OBJECT_STORAGE__BUCKET:-broccoli-blobs}"
      s3_endpoint="${BROCCOLI__STORAGE__OBJECT_STORAGE__ENDPOINT:-http://${infra_host}:${s3_port}}"
      jwt_secret="${BROCCOLI__AUTH__JWT_SECRET:-$(random_secret)}"
      admin_user="${BROCCOLI_ADMIN_USERNAME:-admin}"
      admin_pass="${BROCCOLI_ADMIN_PASSWORD:-$(random_secret)}"
      cat > .env <<EOF
BROCCOLI_ROLE=$(env_quote "$ROLE")
BROCCOLI_VERSION=$(env_quote "$version")
BROCCOLI_SERVER_IMAGE=$(env_quote "$server_image")
BROCCOLI_WORKER_IMAGE=$(env_quote "$worker_image")
POSTGRES_IMAGE=$(env_quote "$postgres_image")
REDIS_IMAGE=$(env_quote "$redis_image")
SEAWEEDFS_IMAGE=$(env_quote "$seaweedfs_image")
CADDY_IMAGE=$(env_quote "$caddy_image")

POSTGRES_BIND=$(env_quote "$postgres_bind")
REDIS_BIND=$(env_quote "$redis_bind")
SEAWEEDFS_S3_BIND=$(env_quote "$s3_bind")
SEAWEEDFS_MASTER_BIND=$(env_quote "${SEAWEEDFS_MASTER_BIND:-127.0.0.1:9333}")
POSTGRES_USER=$(env_quote "$postgres_user")
POSTGRES_PASSWORD=$(env_quote "$postgres_password")
POSTGRES_DB=$(env_quote "$postgres_db")
REDIS_PASSWORD=$(env_quote "$redis_password")

BROCCOLI__DATABASE__URL=$(env_quote "postgres://${postgres_user}:${postgres_password}@${infra_host}:${postgres_port}/${postgres_db}")
BROCCOLI__MQ__URL=$(env_quote "redis://:${redis_password}@${infra_host}:${redis_port}")
BROCCOLI__STORAGE__BACKEND='object_storage'
BROCCOLI__STORAGE__OBJECT_STORAGE__BUCKET=$(env_quote "$s3_bucket")
BROCCOLI__STORAGE__OBJECT_STORAGE__REGION='us-east-1'
BROCCOLI__STORAGE__OBJECT_STORAGE__ENDPOINT=$(env_quote "$s3_endpoint")
BROCCOLI__STORAGE__OBJECT_STORAGE__ACCESS_KEY=$(env_quote "$s3_access")
BROCCOLI__STORAGE__OBJECT_STORAGE__SECRET_KEY=$(env_quote "$s3_secret")
BROCCOLI__STORAGE__OBJECT_STORAGE__PATH_STYLE='true'

BROCCOLI__AUTH__JWT_SECRET=$(env_quote "$jwt_secret")
BROCCOLI__AUTH__SECURE_COOKIES='false'
BROCCOLI_BOOTSTRAP_ADMIN_USERNAME=$(env_quote "$admin_user")
BROCCOLI_BOOTSTRAP_ADMIN_PASSWORD=$(env_quote "$admin_pass")
BROCCOLI_HTTP_BIND=$(env_quote "${BROCCOLI_HTTP_BIND:-0.0.0.0:3000}")
BROCCOLI__SERVER__ID=$(env_quote "${BROCCOLI__SERVER__ID:-server-1}")
BROCCOLI__WORKER__ID=$(env_quote "${BROCCOLI__WORKER__ID:-worker-1}")
BROCCOLI__SERVER__TRUSTED_PROXIES=$(env_quote "${BROCCOLI__SERVER__TRUSTED_PROXIES:-[]}")
BROCCOLI__SERVER__RATE_LIMIT_AUTH=$(env_quote "${BROCCOLI__SERVER__RATE_LIMIT_AUTH:-true}")
BROCCOLI__SUBMISSION__RATE_LIMIT_PER_MINUTE=$(env_quote "${BROCCOLI__SUBMISSION__RATE_LIMIT_PER_MINUTE:-10000}")
BROCCOLI__SERVER_DATABASE_MAX_CONNECTIONS=$(env_quote "${BROCCOLI__SERVER_DATABASE_MAX_CONNECTIONS:-40}")
BROCCOLI__WORKER_DATABASE_MAX_CONNECTIONS=$(env_quote "${BROCCOLI__WORKER_DATABASE_MAX_CONNECTIONS:-5}")
BROCCOLI__OBSERVABILITY__LOG_FORMAT=$(env_quote "${BROCCOLI__OBSERVABILITY__LOG_FORMAT:-json}")
BROCCOLI__OBSERVABILITY__LOG_FILTER=$(env_quote "${BROCCOLI__OBSERVABILITY__LOG_FILTER:-info}")
EOF
      ;;
    server)
      prompt_value BROCCOLI__SERVER__ID "Server ID" "$(hostname -s 2>/dev/null || echo server-1)"
      prompt_value BROCCOLI_HTTP_BIND "Server HTTP bind address" "0.0.0.0:3000"
      prompt_value BROCCOLI__DATABASE__URL "PostgreSQL URL from infra .env" ""
      prompt_value BROCCOLI__MQ__URL "Redis URL from infra .env" ""
      prompt_value BROCCOLI__STORAGE__OBJECT_STORAGE__ENDPOINT "SeaweedFS/S3 endpoint from infra .env" ""
      prompt_value BROCCOLI__STORAGE__OBJECT_STORAGE__ACCESS_KEY "SeaweedFS/S3 access key from infra .env" ""
      prompt_secret BROCCOLI__STORAGE__OBJECT_STORAGE__SECRET_KEY "SeaweedFS/S3 secret key from infra .env" copied
      prompt_secret BROCCOLI__AUTH__JWT_SECRET "JWT secret from infra .env" copied
      prompt_secret BROCCOLI_BOOTSTRAP_ADMIN_PASSWORD "Initial admin password from infra .env" copied
      require_env BROCCOLI__DATABASE__URL BROCCOLI__MQ__URL BROCCOLI__AUTH__JWT_SECRET BROCCOLI_BOOTSTRAP_ADMIN_PASSWORD BROCCOLI__STORAGE__OBJECT_STORAGE__ENDPOINT BROCCOLI__STORAGE__OBJECT_STORAGE__ACCESS_KEY BROCCOLI__STORAGE__OBJECT_STORAGE__SECRET_KEY
      cat > .env <<EOF
BROCCOLI_ROLE='server'
BROCCOLI_VERSION=$(env_quote "$version")
BROCCOLI_SERVER_IMAGE=$(env_quote "$server_image")
BROCCOLI_HTTP_BIND=$(env_quote "${BROCCOLI_HTTP_BIND:-0.0.0.0:3000}")
BROCCOLI__SERVER__ID=$(env_quote "${BROCCOLI__SERVER__ID:-$(hostname -s 2>/dev/null || echo server-1)}")
BROCCOLI__SERVER__TRUSTED_PROXIES=$(env_quote "${BROCCOLI__SERVER__TRUSTED_PROXIES:-[]}")
BROCCOLI__SERVER__RATE_LIMIT_AUTH=$(env_quote "${BROCCOLI__SERVER__RATE_LIMIT_AUTH:-true}")
BROCCOLI__SERVER_DATABASE_MAX_CONNECTIONS=$(env_quote "${BROCCOLI__SERVER_DATABASE_MAX_CONNECTIONS:-40}")
BROCCOLI__DATABASE__URL=$(env_quote "$BROCCOLI__DATABASE__URL")
BROCCOLI__MQ__URL=$(env_quote "$BROCCOLI__MQ__URL")
BROCCOLI__AUTH__JWT_SECRET=$(env_quote "$BROCCOLI__AUTH__JWT_SECRET")
BROCCOLI__AUTH__SECURE_COOKIES=$(env_quote "${BROCCOLI__AUTH__SECURE_COOKIES:-false}")
BROCCOLI_BOOTSTRAP_ADMIN_USERNAME=$(env_quote "${BROCCOLI_ADMIN_USERNAME:-admin}")
BROCCOLI_BOOTSTRAP_ADMIN_PASSWORD=$(env_quote "$BROCCOLI_BOOTSTRAP_ADMIN_PASSWORD")
BROCCOLI__SUBMISSION__RATE_LIMIT_PER_MINUTE=$(env_quote "${BROCCOLI__SUBMISSION__RATE_LIMIT_PER_MINUTE:-10000}")
BROCCOLI__STORAGE__BACKEND='object_storage'
BROCCOLI__STORAGE__OBJECT_STORAGE__BUCKET=$(env_quote "${BROCCOLI__STORAGE__OBJECT_STORAGE__BUCKET:-broccoli-blobs}")
BROCCOLI__STORAGE__OBJECT_STORAGE__REGION=$(env_quote "${BROCCOLI__STORAGE__OBJECT_STORAGE__REGION:-us-east-1}")
BROCCOLI__STORAGE__OBJECT_STORAGE__ENDPOINT=$(env_quote "$BROCCOLI__STORAGE__OBJECT_STORAGE__ENDPOINT")
BROCCOLI__STORAGE__OBJECT_STORAGE__ACCESS_KEY=$(env_quote "$BROCCOLI__STORAGE__OBJECT_STORAGE__ACCESS_KEY")
BROCCOLI__STORAGE__OBJECT_STORAGE__SECRET_KEY=$(env_quote "$BROCCOLI__STORAGE__OBJECT_STORAGE__SECRET_KEY")
BROCCOLI__STORAGE__OBJECT_STORAGE__PATH_STYLE=$(env_quote "${BROCCOLI__STORAGE__OBJECT_STORAGE__PATH_STYLE:-true}")
BROCCOLI__OBSERVABILITY__LOG_FORMAT=$(env_quote "${BROCCOLI__OBSERVABILITY__LOG_FORMAT:-json}")
BROCCOLI__OBSERVABILITY__LOG_FILTER=$(env_quote "${BROCCOLI__OBSERVABILITY__LOG_FILTER:-info}")
EOF
      ;;
    worker)
      prompt_value BROCCOLI__WORKER__ID "Worker ID" "$(hostname -s 2>/dev/null || echo worker-1)"
      prompt_value BROCCOLI__DATABASE__URL "PostgreSQL URL from infra .env" ""
      prompt_value BROCCOLI__MQ__URL "Redis URL from infra .env" ""
      prompt_value BROCCOLI__STORAGE__OBJECT_STORAGE__ENDPOINT "SeaweedFS/S3 endpoint from infra .env" ""
      prompt_value BROCCOLI__STORAGE__OBJECT_STORAGE__ACCESS_KEY "SeaweedFS/S3 access key from infra .env" ""
      prompt_secret BROCCOLI__STORAGE__OBJECT_STORAGE__SECRET_KEY "SeaweedFS/S3 secret key from infra .env" copied
      require_env BROCCOLI__DATABASE__URL BROCCOLI__MQ__URL BROCCOLI__STORAGE__OBJECT_STORAGE__ENDPOINT BROCCOLI__STORAGE__OBJECT_STORAGE__ACCESS_KEY BROCCOLI__STORAGE__OBJECT_STORAGE__SECRET_KEY
      cat > .env <<EOF
BROCCOLI_ROLE='worker'
BROCCOLI_VERSION=$(env_quote "$version")
BROCCOLI_WORKER_IMAGE=$(env_quote "$worker_image")
BROCCOLI__WORKER__ID=$(env_quote "${BROCCOLI__WORKER__ID:-$(hostname -s 2>/dev/null || echo worker-1)}")
BROCCOLI__WORKER_DATABASE_MAX_CONNECTIONS=$(env_quote "${BROCCOLI__WORKER_DATABASE_MAX_CONNECTIONS:-5}")
BROCCOLI__DATABASE__URL=$(env_quote "$BROCCOLI__DATABASE__URL")
BROCCOLI__MQ__URL=$(env_quote "$BROCCOLI__MQ__URL")
BROCCOLI__STORAGE__BACKEND='object_storage'
BROCCOLI__STORAGE__OBJECT_STORAGE__BUCKET=$(env_quote "${BROCCOLI__STORAGE__OBJECT_STORAGE__BUCKET:-broccoli-blobs}")
BROCCOLI__STORAGE__OBJECT_STORAGE__REGION=$(env_quote "${BROCCOLI__STORAGE__OBJECT_STORAGE__REGION:-us-east-1}")
BROCCOLI__STORAGE__OBJECT_STORAGE__ENDPOINT=$(env_quote "$BROCCOLI__STORAGE__OBJECT_STORAGE__ENDPOINT")
BROCCOLI__STORAGE__OBJECT_STORAGE__ACCESS_KEY=$(env_quote "$BROCCOLI__STORAGE__OBJECT_STORAGE__ACCESS_KEY")
BROCCOLI__STORAGE__OBJECT_STORAGE__SECRET_KEY=$(env_quote "$BROCCOLI__STORAGE__OBJECT_STORAGE__SECRET_KEY")
BROCCOLI__STORAGE__OBJECT_STORAGE__PATH_STYLE=$(env_quote "${BROCCOLI__STORAGE__OBJECT_STORAGE__PATH_STYLE:-true}")
BROCCOLI__OBSERVABILITY__LOG_FORMAT=$(env_quote "${BROCCOLI__OBSERVABILITY__LOG_FORMAT:-json}")
BROCCOLI__OBSERVABILITY__LOG_FILTER=$(env_quote "${BROCCOLI__OBSERVABILITY__LOG_FILTER:-info}")
EOF
      ;;
    gateway)
      prompt_value BROCCOLI_UPSTREAMS "Server upstreams, separated by spaces" ""
      prompt_value BROCCOLI_GATEWAY_HTTP_BIND "Gateway HTTP bind address" "0.0.0.0:80"
      require_env BROCCOLI_UPSTREAMS
      cat > .env <<EOF
BROCCOLI_ROLE='gateway'
CADDY_IMAGE=$(env_quote "$caddy_image")
BROCCOLI_GATEWAY_HTTP_BIND=$(env_quote "${BROCCOLI_GATEWAY_HTTP_BIND:-0.0.0.0:80}")
BROCCOLI_UPSTREAMS=$(env_quote "$BROCCOLI_UPSTREAMS")
EOF
      ;;
  esac
  chmod 0600 .env
  echo "created .env for role '$ROLE'"
}

wait_http() {
  local url label attempt
  url="$1"
  label="$2"
  echo "waiting for $label"
  for attempt in $(seq 1 24); do
    if curl -fsS "$url" >/dev/null 2>&1; then
      return 0
    fi
    if [ "$attempt" = 24 ]; then
      compose ps
      die "$label did not become healthy within 120s"
    fi
    sleep 5
  done
}

wait_service_healthy() {
  local service label attempt container status
  service="$1"
  label="$2"
  echo "waiting for $label"
  for attempt in $(seq 1 24); do
    container="$(compose ps -q "$service" || true)"
    if [ -n "$container" ]; then
      status="$(docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' "$container" 2>/dev/null || true)"
      case "$status" in
        healthy|running) return 0 ;;
      esac
    fi
    if [ "$attempt" = 24 ]; then
      compose ps
      die "$label did not become healthy within 120s"
    fi
    sleep 5
  done
}

wait_service_completed() {
  local service label attempt container status exit_code
  service="$1"
  label="$2"
  echo "waiting for $label"
  for attempt in $(seq 1 24); do
    container="$(compose ps -a -q "$service" || true)"
    if [ -n "$container" ]; then
      status="$(docker inspect --format '{{.State.Status}}' "$container" 2>/dev/null || true)"
      exit_code="$(docker inspect --format '{{.State.ExitCode}}' "$container" 2>/dev/null || true)"
      if [ "$status" = "exited" ] && [ "$exit_code" = "0" ]; then
        return 0
      fi
      if [ "$status" = "exited" ]; then
        compose logs "$service" || true
        die "$label exited with status $exit_code"
      fi
    fi
    if [ "$attempt" = 24 ]; then
      compose ps
      die "$label did not complete within 120s"
    fi
    sleep 5
  done
}

ROLE="${1:-${BROCCOLI_ROLE:-${BROCCOLI_NODE_ROLE:-}}}"
if [ -z "$ROLE" ] && is_interactive; then
  choose_role_interactive
fi
[ -n "$ROLE" ] || { usage; exit 64; }
case "$ROLE" in
  infra|server|worker|gateway|single-host) ;;
  *) usage; die "unknown role '$ROLE'" ;;
esac

COMPOSE_FILE="docker-compose.${ROLE}.yaml"
TEMPLATE_FILE="${COMPOSE_FILE}.template"

need docker "Install Docker Engine from https://docs.docker.com/engine/install/."
need openssl "Install OpenSSL with your OS package manager."
need gzip "Install gzip with your OS package manager."
if [ "$ROLE" = "server" ] || [ "$ROLE" = "gateway" ] || [ "$ROLE" = "single-host" ]; then
  need curl "Install curl with your OS package manager."
fi
docker compose version >/dev/null 2>&1 || die "docker compose is required. Install the Docker Compose plugin from the official Docker docs."

load_bundled_images

if [ ! -f "$COMPOSE_FILE" ]; then
  [ -f "$TEMPLATE_FILE" ] || die "$TEMPLATE_FILE is missing"
  cp "$TEMPLATE_FILE" "$COMPOSE_FILE"
fi

if [ -f .env ]; then
  echo ".env already exists; preserving existing secrets"
else
  write_env_file
fi

set -a
# shellcheck disable=SC1091
source .env
set +a

if [ -n "${BROCCOLI_ROLE:-}" ] && [ "$BROCCOLI_ROLE" != "$ROLE" ]; then
  die ".env was created for role '$BROCCOLI_ROLE'; use a separate directory or pass role '$BROCCOLI_ROLE'"
fi
validate_role_env

if [ "$ROLE" = "infra" ] || [ "$ROLE" = "single-host" ]; then
  write_seaweedfs_config
fi

compose up -d

case "$ROLE" in
  infra)
    wait_service_healthy db "PostgreSQL"
    wait_service_healthy redis "Redis"
    wait_service_healthy seaweedfs "SeaweedFS"
    wait_service_completed seaweedfs-init "SeaweedFS bucket init"
    ;;
  server)
    wait_http "$(host_health_url)" "server health"
    ;;
  worker)
    wait_service_healthy worker "worker"
    ;;
  gateway)
    wait_http "$(gateway_health_url)" "gateway health"
    ;;
  single-host)
    wait_service_healthy db "PostgreSQL"
    wait_service_healthy redis "Redis"
    wait_service_healthy seaweedfs "SeaweedFS"
    wait_service_completed seaweedfs-init "SeaweedFS bucket init"
    wait_http "$(host_health_url)" "server health"
    wait_service_healthy worker "worker"
    ;;
esac

if should_run_stress_smoke; then
  [ -x stress-test/broccoli-stress-test ] || die "stress-test/broccoli-stress-test is missing or not executable"
  case "$ROLE" in
    server|gateway|single-host)
      base_url="$(if [ "$ROLE" = gateway ]; then gateway_health_url; else host_health_url; fi)"
      ./stress-test/broccoli-stress-test \
        --url "${base_url%/healthz}" \
        --admin-username "${BROCCOLI_BOOTSTRAP_ADMIN_USERNAME:-admin}" \
        --admin-password "${BROCCOLI_BOOTSTRAP_ADMIN_PASSWORD:-}" \
        --correctness-only
      ;;
  esac
fi

cat <<EOF
Broccoli role '$ROLE' is running.
Compose file: $COMPOSE_FILE
Logs: docker compose -f $COMPOSE_FILE logs -f
Runbook: docs/operator-runbook.md
EOF
