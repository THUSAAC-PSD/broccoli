# Air-Gap LAN Deployment Runbook

This runbook walks an operator through assembling a broccoli deployment bundle
on a networked staging box and installing it on a zero-network LAN (a live-boot
contest venue, for example). Follow the sections in order: the server must be
installed and reachable before workers and contestant machines trust its
certificate.

## 1. Overview

A normal broccoli deployment leans on the internet in three places: it pulls
container images from a registry, it fetches TLS certificates via ACME (Let's
Encrypt), and it resolves DNS for the public hostname. None of that is available
at an air-gapped venue. The `release/airgap/` tooling replaces all three with a
**single self-contained bundle** built ahead of time on a machine that _does_
have internet access, then carried to the venue on removable media (USB drive,
external SSD) and installed with zero network calls.

The bundle covers three roles:

- **server** — the broccoli API/dispatcher plus its infra (Postgres, Redis,
  SeaweedFS/S3), fronted by Caddy serving an internal-CA TLS leaf.
- **worker** — the judging box that runs the isolate sandbox and executes
  submissions.
- **contestant** — a contestant's machine, which only needs the LAN root CA
  trusted (so HTTPS to the server doesn't warn) and the `broccoli` CLI on its
  `PATH`.

Every role installs from the same bundle tree via one dispatcher,
`install.sh --role {server|worker|contestant}`. Nothing under `release/airgap/`
ever shells out to `curl`, `wget`, `apt`, `pip`, or `docker pull` — that
invariant is enforced by `release/airgap/test/offline_guard_test.sh` and
`install_test.sh`, and it's what makes the target-side scripts safe to run with
no network at all.

## 2. On the staging box (networked)

The staging box is any machine with internet access, Docker, and this repo
checked out. It assembles the bundle:

```bash
release/airgap/build-bundle.sh --version <V> [--lan-host <host-or-ip>] --tar
```

- `--version <V>` names the bundle (`[A-Za-z0-9._-]` only); the tree lands at
  `./dist/broccoli-airgap-<V>/` by default (override with `--output`).
- `--lan-host <host-or-ip>` is optional at assembly time. If you already know
  the venue's LAN IP or hostname, pass it here and `build-bundle.sh` pre-issues
  the server's TLS leaf during assembly (see step 7, `root.key` security).
- `--tar` additionally produces `broccoli-airgap-<V>.tar.zst` next to the tree,
  convenient for copying to a single file on USB.

`build-bundle.sh` runs `ca/mint-ca.sh` to mint a fresh internal root CA for this
bundle (`ca/root.crt` + `ca/root.key`, the latter `chmod 0600`), then stages
everything an install needs:

- `images/` — `docker save` tarballs for the server, worker, Postgres, Redis,
  and SeaweedFS images (skipped when assembling structurally with
  `--skip-images`, which the bundle's own CI test uses).
- `compose/` — the server and infra compose templates, plus
  `compose/.env.server.example` and `compose/.env.infra.example` (real env files
  are _not_ shipped — see the server install section below).
- `cli/` — the musl-static `broccoli` contestant CLI binary.
- `ca/` — `root.crt` (public, ships everywhere) and `root.key` (private,
  server-only, 0600).
- `caddy/Caddyfile.airgap` — the explicit-TLS Caddy site rendered at install
  time.
- `trust-ca/` — the per-OS root-CA trust helpers (`linux.sh`, `macos.sh`,
  `windows.ps1`).
- `install.sh`, `load-bundle.sh` — the target-side scripts.
- `bundle.json` — provenance (version, source git SHA, role list).
- `manifest.sha256` — a `sha256sum` of every other file in the tree, sorted by
  relative path, used to detect transfer corruption or tampering.

The frontend is baked fresh into the server image as part of this staging build
(not copied by mtime from a stale artifact — see the
`e2e-frontend-deploy-staleness` lesson), and should be verified behaviorally
(load the page, check the network trace) before trusting the staged image, since
a stale bundle can look correct on disk while serving old assets.

## 3. Transfer

Copy the bundle tree (or the `.tar.zst`, then extract it) onto the USB drive or
other media you'll carry to the air-gapped venue. Transfers to removable media
can silently truncate or corrupt files, so **before trusting the media**, verify
it against the manifest that was generated at assembly time:

```bash
release/airgap/load-bundle.sh --bundle <dir> --verify-only
```

This re-hashes every file in `<dir>` (except `manifest.sha256` itself) and diffs
the result against `manifest.sha256`. It exits non-zero and prints
`ABORT: bundle integrity check failed` on any mismatch — do not proceed with an
install until this passes. The same verification runs again, unconditionally, as
part of the contestant install (step 6) as a second trust-boundary check after
the media has changed hands.

## 4. Server install

Before running the installer, the operator must supply real environment files —
the bundle only ships `.example` templates so no secrets ever sit in a
distributable tarball. On the server machine:

```bash
cp compose/.env.infra.example compose/.env.infra
cp compose/.env.server.example compose/.env.server
```

Then edit both files and fill in real values, at minimum:

- Postgres and Redis passwords (`POSTGRES_PASSWORD`, `REDIS_PASSWORD` in
  `.env.infra`, mirrored into `BROCCOLI__DATABASE__URL` / `BROCCOLI__MQ__URL` in
  `.env.server`).
- `BROCCOLI__AUTH__JWT_SECRET` (at least 32 characters).
- SeaweedFS/S3 credentials (`BROCCOLI__STORAGE__OBJECT_STORAGE__ACCESS_KEY` /
  `..._SECRET_KEY`, and the endpoint if it differs from the compose default).
- `BROCCOLI__BOOTSTRAP__ADMIN_PASSWORD` (`BROCCOLI_BOOTSTRAP_ADMIN_PASSWORD` in
  the env file) for the initial admin account.
- `BROCCOLI_SERVER_IMAGE`, pointed at the image tag that was baked and loaded
  into this bundle.

`install.sh --role server` checks for `compose/.env.infra` and
`compose/.env.server` **before** doing any other work (loading images, issuing
the TLS leaf) and fails fast with a clear message if either is missing — it will
not silently bring up containers with empty or placeholder config. Once the env
files are in place:

```bash
release/airgap/install.sh --role server --bundle <dir> --lan-host <host-or-ip>
```

This:

1. Runs `load-bundle.sh --bundle <dir>` to re-verify `manifest.sha256` and
   `docker load` every tarball in `images/`.
2. Issues the server's TLS leaf via `ca/issue-leaf.sh` if one wasn't already
   pre-issued at assembly time (see step 7) — the leaf's SAN covers
   `<host-or-ip>`.
3. Renders `caddy/Caddyfile.airgap` to `<dir>/caddy/Caddyfile` with the LAN
   host, leaf cert/key paths, and upstream substituted in.
4. If `--burn-ca-key` was passed, deletes `ca/root.key` right after the leaf is
   issued (see step 7).
5. Brings up infra and server with
   `docker compose --env-file .env.infra --env-file .env.server -f docker-compose.infra.yaml.template -f docker-compose.server.yaml.template up -d --pull never`
   — `--pull never` guarantees Compose only uses the images already loaded from
   `images/*.tar`, never reaching for a registry.

## 5. Worker install

On each judging box:

```bash
release/airgap/install.sh --role worker --bundle <dir>
```

This loads the worker image via `load-bundle.sh`, then runs the sandbox go/no-go
check staged inside the bundle at `native/live-boot-preflight.sh` (arch/OS,
cgroup-v2 controllers, isolate setuid + cgroup delegation, live MLE/TLE probes,
and a real C/C++/Python compile). A failed preflight only warns
(`WARN: worker sandbox preflight reported issues`) — inspect its `[FAIL]` lines
and fix them before trusting the box for judging, but it does not by itself
abort the install.

The installer then trusts the bundle's `ca/root.crt` using the matching
`trust-ca/` helper for the host OS (`trust-ca/linux.sh` or `trust-ca/macos.sh`,
selected automatically from `uname -s`). Unlike the preflight, **a genuine
CA-trust failure aborts the install** — the script prints
`ERROR: CA trust failed` and exits non-zero, because a worker that doesn't trust
the server's certificate can't reliably fetch submissions over HTTPS. An
unsupported OS (neither Linux nor macOS) is only a warning; in that case trust
`ca/root.crt` manually using your OS's certificate tooling.

## 6. Contestant machines

On each contestant machine:

```bash
release/airgap/install.sh --role contestant --bundle <dir>
```

This re-runs `load-bundle.sh --bundle <dir> --verify-only` first — a second,
independent `manifest.sha256` check at the point the media reaches the
contestant machine, since the trust boundary crosses again here. It then trusts
`ca/root.crt` via the appropriate per-OS `trust-ca/` helper (`linux.sh` installs
into `/usr/local/share/ca-certificates` and runs `update-ca-certificates`;
`macos.sh` adds it to the System keychain via `security add-trusted-cert`;
`windows.ps1` runs `certutil -addstore -f Root` in an elevated PowerShell — this
one must be run manually on Windows, since `install.sh` only auto-detects Linux
and macOS). Finally, if `cli/broccoli` is present in the bundle, it installs the
musl-static CLI to `/usr/local/bin/broccoli`.

**Firefox keeps its own certificate store**, separate from the OS trust store
that `trust-ca/` populates. Every trust helper prints a reminder of this; if
contestants will use Firefox against the LAN server, import `ca/root.crt` into
Firefox separately (Settings → Privacy & Security → Certificates → View
Certificates → Authorities → Import).

## 7. root.key security

The bundle always ships `ca/root.key` — the private key that can mint new TLS
leaves for the LAN's root CA — mode `0600`, intended to be used only on the
server. Because that key is powerful (anyone holding it can issue a leaf that
impersonates the server to any client that trusts `root.crt`), you have two ways
to keep it from ever reaching worker or contestant machines:

1. **Burn after issue** — pass `--burn-ca-key` to `install.sh --role server`.
   The installer issues the server leaf as usual, then deletes `ca/root.key`
   from the bundle directory immediately afterward. Use this when you don't know
   the venue's LAN host/IP until you arrive on-site.
2. **Pre-issue at assembly** — pass `--lan-host <host-or-ip>` to
   `build-bundle.sh` on the staging box (step 2) when you already know the
   venue's address ahead of time. The leaf is issued during assembly, so
   `install.sh --role server` finds `ca/server.crt` already present and skips
   leaf issuance entirely — `root.key` was only ever needed momentarily during
   the staging build and does not need to travel to the venue at all if you drop
   it from the tree before copying to media.

Either way, only `ca/root.crt` (the public certificate, safe to distribute) is
ever installed on worker or contestant machines by `trust-ca`. `ca/root.key` and
`ca/server.key` should never leave the server.

## 8. Troubleshooting

**`ABORT: bundle integrity check failed` / `manifest verification failed`** —
the copy onto removable media was corrupted or incomplete (or the tree was
edited after `manifest.sha256` was generated). Re-copy the bundle from the
staging box (or re-extract the `.tar.zst`) and re-run
`load-bundle.sh --bundle <dir> --verify-only` before doing anything else. Do not
attempt to "fix" a mismatched manifest by hand — regenerate the bundle.

**Browser/client shows a TLS warning even though the CA was trusted** — the
server's leaf certificate's SAN only covers the host/IP it was issued for. If
contestants reach the server via a different hostname or IP than the one passed
to `--lan-host` (at either `build-bundle.sh` or `install.sh --role server`
time), the leaf won't match. Re-issue the leaf for the correct host with
`ca/issue-leaf.sh --ca-dir <dir>/ca --host <correct-host> --out <dir>/ca`
(requires `root.key`, so do this before burning it) and re-render the Caddyfile
by re-running
`install.sh --role server --bundle <dir> --lan-host <correct-host>`.

**Browser trust warnings persist after running the trust helper** — most
browsers read the OS certificate store and pick up `trust-ca/`'s changes
immediately, but Firefox uses its own store (see step 6) and needs `root.crt`
imported separately. Confirm which store the browser is reading from before
assuming the trust helper failed.

**Worker preflight reports `[FAIL]` items** — `native/live-boot-preflight.sh`
does not block the install by itself, but a worker with failing isolate,
cgroup-v2, or toolchain checks will misjudge submissions. Fix the specific
`[FAIL]` line (e.g. install `build-essential`, boot with
`systemd.unified_cgroup_hierarchy=1`) and re-run the preflight before sending
real submissions to that worker.

**Server won't start / config looks empty** — check that `compose/.env.infra`
and `compose/.env.server` exist and are filled in (step 4);
`install.sh --role server` refuses to run without them, but if you brought up
the compose files by hand without `install.sh` you can end up with variables
expanding to empty strings.
