---
title: Downloads
sidebar_label: Downloads
sidebar_position: 2
---

# Downloads

Every tagged release publishes the server images and a bundle you install
yourself, the stress test, the contestant CLI, and the print station client. They
all live on the
[Releases page](https://github.com/THUSAAC-PSD/broccoli/releases), and a
`manifest.json` beside them lists every file with its size and SHA256 checksum.

## Run a server

There are two ways to stand up Broccoli. Use the platform bundle for a guided
install, or pull the container images and run them with your own orchestration.

### The platform bundle

The bundle carries the compose files, a role aware installer, and an embedded
copy of the stress test. Set the version you want, download the archive, verify
it, and extract.

```bash
VERSION=v0.1.0
curl -LO "https://github.com/THUSAAC-PSD/broccoli/releases/download/$VERSION/broccoli-platform-$VERSION.tar.gz"
curl -LO "https://github.com/THUSAAC-PSD/broccoli/releases/download/$VERSION/broccoli-platform-$VERSION.tar.gz.sha256"
sha256sum -c "broccoli-platform-$VERSION.tar.gz.sha256"
tar -xzf "broccoli-platform-$VERSION.tar.gz"
cd "broccoli-platform-$VERSION"
```

A deployment is built from roles, each on its own machine. Install infra first,
then the server, then one or more workers.

```bash
./install.sh infra      # PostgreSQL, Redis, and object storage
./install.sh server     # the API server and web interface
./install.sh worker     # the judging sandbox
./install.sh gateway    # optional load balancer in front of several servers
```

Run `./install.sh` with no role for a guided menu. The `single-host` role puts
everything on one machine and is meant for a demo or a rehearsal, not a real
contest. The extracted bundle ships its own README and an operator runbook with
the day to day commands.

### The container images

If you run your own orchestration, pull the images directly. Every image is built
for x86_64 and arm64 Linux, so Docker fetches the right one for the machine you
run it on.

| Image        | Reference                                                       |
| ------------ | -------------------------------------------------------------- |
| Server       | `ghcr.io/thusaac-psd/broccoli/broccoli-server:$VERSION`        |
| Worker, base | `ghcr.io/thusaac-psd/broccoli/broccoli-worker:$VERSION-base`   |
| Worker, icpc | `ghcr.io/thusaac-psd/broccoli/broccoli-worker:$VERSION-icpc`   |
| Worker, full | `ghcr.io/thusaac-psd/broccoli/broccoli-worker:$VERSION-full`   |

Set the version, then pull the server and the worker variant you want.

```bash
VERSION=v0.1.0
docker pull "ghcr.io/thusaac-psd/broccoli/broccoli-server:$VERSION"
docker pull "ghcr.io/thusaac-psd/broccoli/broccoli-worker:$VERSION-base"
docker pull "ghcr.io/thusaac-psd/broccoli/broccoli-worker:$VERSION-icpc"
docker pull "ghcr.io/thusaac-psd/broccoli/broccoli-worker:$VERSION-full"
```

The worker comes in three sizes. The `base` image ships the worker with no
bundled languages. The `icpc` image adds C and C++. The `full` image adds the
rest, including Java, Kotlin, and Python.

For networks inside China, the same images mirror to Alibaba Cloud under
`registry.cn-hangzhou.aliyuncs.com/broccoli/`.

## Stress test

The stress test drives a real server with synthetic contestants and submissions,
so you can confirm a fresh install behaves before an event. The platform bundle
already contains it. Download it on its own when you test from another machine.

| System        | File                                                                                                                                                |
| ------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| Linux x86_64  | [broccoli-stress-test-linux-x86_64](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-stress-test-linux-x86_64)             |
| Linux aarch64 | [broccoli-stress-test-linux-aarch64](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-stress-test-linux-aarch64)           |
| Windows       | [broccoli-stress-test-windows-x86_64.exe](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-stress-test-windows-x86_64.exe) |
| macOS         | [broccoli-stress-test-macos-universal](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-stress-test-macos-universal)       |

```bash
chmod +x broccoli-stress-test-linux-x86_64
./broccoli-stress-test-linux-x86_64 --help
```

## Contestant CLI

`broccoli` is the command line tool for contestants. Download the build for your
system, then read the [Contestant CLI](./cli/contestant.md) page for logging in
and the full set of commands.

| System        | File                                                                                                                                |
| ------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| Linux x86_64  | [broccoli-cli-linux-x86_64](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-cli-linux-x86_64)             |
| Linux aarch64 | [broccoli-cli-linux-aarch64](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-cli-linux-aarch64)           |
| Windows       | [broccoli-cli-windows-x86_64.exe](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-cli-windows-x86_64.exe) |
| macOS         | [broccoli-cli-macos-universal](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-cli-macos-universal)       |

## Print stations

A print station runs a small client on a computer next to a printer and turns
each print request into a printed page. Download the build for each station, then
follow [Printing](./plugins/printing.md) to set it up.

| System        | File                                                                                                                                                  |
| ------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| Linux x86_64  | [broccoli-print-client-linux-x86_64](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-print-client-linux-x86_64)             |
| Linux aarch64 | [broccoli-print-client-linux-aarch64](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-print-client-linux-aarch64)           |
| Windows       | [broccoli-print-client-windows-x86_64.exe](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-print-client-windows-x86_64.exe) |
| macOS         | [broccoli-print-client-macos-universal](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-print-client-macos-universal)       |
