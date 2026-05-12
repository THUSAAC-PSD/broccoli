#!/usr/bin/env bash
# Base bootstrap for every Broccoli profiling host.
# Idempotent: safe to re-run.
#
# Args: $1 = host role label (gateway|loadgen|observability|api|postgres|redis|storage|worker)
#       $2 = admin SSH source IP (e.g. 43.239.95.25) — allowed to reach :22
#
# Effects:
#   - hostname set by DO already (kept)
#   - apt update + install base packages
#   - Docker CE installed via official repo
#   - UFW: allow 22 from admin IP, allow all from 10.104.0.0/20, default-deny incoming
#   - chrony for time sync
set -euo pipefail

ROLE="${1:?role required}"
ADMIN_IP="${2:?admin IP required}"
VPC_CIDR="10.104.0.0/20"

export DEBIAN_FRONTEND=noninteractive

# Wait for any process actually holding the apt/dpkg lock to release it.
# Don't match by command name (Ubuntu always has unattended-upgrade-shutdown
# running waiting for shutdown signal — it doesn't hold any lock).
for _ in $(seq 1 60); do
  if ! fuser /var/lib/dpkg/lock-frontend /var/lib/apt/lists/lock /var/lib/dpkg/lock >/dev/null 2>&1; then
    break
  fi
  sleep 5
done

apt-get update -qq
apt-get install -y -qq \
  ca-certificates curl gnupg jq htop iotop sysstat \
  ufw chrony unzip git make build-essential \
  net-tools dnsutils tcpdump rsync

# --- Docker (via official Docker repo) ---
if ! command -v docker >/dev/null 2>&1; then
  install -m 0755 -d /etc/apt/keyrings
  curl -fsSL https://download.docker.com/linux/ubuntu/gpg | gpg --dearmor -o /etc/apt/keyrings/docker.gpg
  chmod a+r /etc/apt/keyrings/docker.gpg
  . /etc/os-release
  echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/ubuntu ${VERSION_CODENAME} stable" > /etc/apt/sources.list.d/docker.list
  apt-get update -qq
  apt-get install -y -qq docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
fi
systemctl enable --now docker

# --- Time sync ---
systemctl enable --now chrony
timedatectl set-timezone UTC

# --- sshd: add port 2222 listener (campus blocks SSH→DO on port 22). ---
# Ubuntu 24.04 uses socket activation; ports come from ssh.socket, not sshd_config.
mkdir -p /etc/systemd/system/ssh.socket.d
cat >/etc/systemd/system/ssh.socket.d/listen-extra.conf <<'EOF'
[Socket]
ListenStream=2222
EOF
systemctl daemon-reload
systemctl reset-failed ssh.socket || true
systemctl restart ssh.socket

# --- UFW ---
ufw --force reset >/dev/null
ufw default deny incoming
ufw default allow outgoing
# Admin SSH allowed on both 22 and 2222 so we always have at least one path.
ufw allow from "${ADMIN_IP}" to any port 22 proto tcp comment 'admin ssh 22'
ufw allow 2222/tcp comment 'admin ssh 2222 (campus bypass)'
ufw allow from "${VPC_CIDR}" comment 'vpc internal'
# Gateway also accepts public 80/443 for the API entry
if [[ "${ROLE}" == "gateway" ]]; then
  ufw allow 80/tcp comment 'public api http'
  ufw allow 443/tcp comment 'public api https'
fi
ufw --force enable

# --- /etc/hosts: short names for sibling private IPs ---
# We append-or-replace a managed block
cat >/etc/hosts.broccoli <<'EOF'
10.104.0.13  broccoli-build
10.104.0.14  broccoli-gateway
10.104.0.15  broccoli-observability
10.104.0.16  broccoli-loadgen
10.104.0.17  broccoli-postgres
10.104.0.18  broccoli-redis
10.104.0.19  broccoli-storage
10.104.0.20  broccoli-api-1
10.104.0.21  broccoli-api-2
10.104.0.22  broccoli-worker-1
10.104.0.23  broccoli-worker-2
EOF
sed -i '/# >>> broccoli >>>/,/# <<< broccoli <<</d' /etc/hosts
{ echo '# >>> broccoli >>>'; cat /etc/hosts.broccoli; echo '# <<< broccoli <<<'; } >> /etc/hosts

# --- sysctl tuning for chatty network workloads ---
cat >/etc/sysctl.d/99-broccoli.conf <<'EOF'
net.core.somaxconn = 4096
net.ipv4.tcp_max_syn_backlog = 4096
net.ipv4.ip_local_port_range = 10000 65535
net.ipv4.tcp_tw_reuse = 1
fs.file-max = 2097152
vm.swappiness = 10
EOF
sysctl --system >/dev/null

# --- node_exporter and cAdvisor on every host so observability can scrape ---
# Run them as containers so they restart with docker. Bind to private IP only.
PRIVATE_IP=$(ip -4 -o addr show | awk '/10\.104\./{split($4,a,"/"); print a[1]; exit}')
if [[ -z "${PRIVATE_IP}" ]]; then
  echo "ERROR: could not find 10.104.x.x private IP" >&2
  exit 1
fi

mkdir -p /opt/broccoli
cat >/opt/broccoli/exporters.compose.yaml <<EOF
services:
  node-exporter:
    image: prom/node-exporter:v1.8.2
    container_name: node-exporter
    restart: unless-stopped
    pid: host
    network_mode: host
    command:
      - "--path.rootfs=/host"
      - "--web.listen-address=${PRIVATE_IP}:9100"
      - "--collector.filesystem.mount-points-exclude=^/(sys|proc|dev|host|etc|run/credentials)($|/)"
    volumes:
      - "/:/host:ro,rslave"
  cadvisor:
    image: gcr.io/cadvisor/cadvisor:v0.49.1
    container_name: cadvisor
    restart: unless-stopped
    privileged: true
    network_mode: host
    command:
      - "--listen_ip=${PRIVATE_IP}"
      - "--port=8080"
      - "--docker_only=true"
    volumes:
      - "/:/rootfs:ro"
      - "/var/run:/var/run:ro"
      - "/sys:/sys:ro"
      - "/var/lib/docker/:/var/lib/docker:ro"
      - "/dev/disk/:/dev/disk:ro"
EOF
docker compose -f /opt/broccoli/exporters.compose.yaml up -d

echo "BOOTSTRAP-OK role=${ROLE} private_ip=${PRIVATE_IP}"
