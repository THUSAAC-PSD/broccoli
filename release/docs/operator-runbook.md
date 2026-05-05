# Operator Runbook

Run commands on the machine that owns the role.

## Status

Infra node:

```bash
docker compose -f docker-compose.infra.yaml ps
docker compose -f docker-compose.infra.yaml logs --tail=200 db redis seaweedfs seaweedfs-init
```

Server node:

```bash
docker compose -f docker-compose.server.yaml ps
curl -fsS http://127.0.0.1:${BROCCOLI_HTTP_BIND##*:}/healthz
docker compose -f docker-compose.server.yaml logs --tail=200 server
```

Worker node:

```bash
docker compose -f docker-compose.worker.yaml ps
docker compose -f docker-compose.worker.yaml logs --tail=200 worker
```

Gateway node:

```bash
docker compose -f docker-compose.gateway.yaml ps
curl -fsS http://127.0.0.1:${BROCCOLI_GATEWAY_HTTP_BIND##*:}/healthz
docker compose -f docker-compose.gateway.yaml logs --tail=200 gateway
```

## Restart

Restart one role at a time:

```bash
docker compose -f docker-compose.worker.yaml restart worker
docker compose -f docker-compose.server.yaml restart server
docker compose -f docker-compose.gateway.yaml restart gateway
```

For server redundancy, restart one server machine at a time and confirm the
gateway stays healthy before moving to the next server. Avoid restarting infra
during a contest window.

## Plugin Reload

The server image contains the default plugins and also mounts `./plugins`. After
replacing plugin files on a server node, restart that server or use the admin UI
plugin reload action:

```bash
docker compose -f docker-compose.server.yaml restart server
```

Repeat per server node.

## Password Reset

Use the admin UI when another admin account is available. If all admin accounts
are locked out, edit the server node `.env` and restart the server:

```bash
BROCCOLI_BOOTSTRAP_ADMIN_USERNAME=admin
BROCCOLI_BOOTSTRAP_ADMIN_PASSWORD=replace-this-password
docker compose -f docker-compose.server.yaml up -d --no-deps server
```

Remove or rotate the bootstrap password after login.

## Logs

```bash
docker compose -f docker-compose.server.yaml logs -f server
docker compose -f docker-compose.worker.yaml logs -f worker
docker compose -f docker-compose.infra.yaml logs -f db redis seaweedfs
```

For worker sandbox failures, first confirm the worker service is privileged and
that the worker host uses cgroup v2.

## Storage Checks

SeaweedFS runs on the infra node. Confirm the bucket exists:

```bash
docker compose -f docker-compose.infra.yaml run --rm seaweedfs-init
```

Server and worker nodes must use the same
`BROCCOLI__STORAGE__OBJECT_STORAGE__ENDPOINT`,
`BROCCOLI__STORAGE__OBJECT_STORAGE__ACCESS_KEY`, and
`BROCCOLI__STORAGE__OBJECT_STORAGE__SECRET_KEY`.
