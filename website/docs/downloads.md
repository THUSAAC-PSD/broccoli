---
title: Downloads
sidebar_label: Downloads
sidebar_position: 2
---

# Downloads

Every tagged release publishes the contestant CLI, the server and worker images,
a platform bundle for self hosting, the stress test binary, and the print
station client. They all live on the
[Releases page](https://github.com/THUSAAC-PSD/broccoli/releases), and a
`manifest.json` beside them lists every file with its size and SHA256 checksum.

## The contestant CLI

`broccoli` is a command line tool for contestants. You log in, test and submit
solutions, browse contests and problems, ask clarifications, and watch a contest
live in your terminal. It is one file with nothing to install around it.

| System        | File                                                                                                                                |
| ------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| Linux x86_64  | [broccoli-cli-linux-x86_64](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-cli-linux-x86_64)             |
| Linux aarch64 | [broccoli-cli-linux-aarch64](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-cli-linux-aarch64)           |
| Windows       | [broccoli-cli-windows-x86_64.exe](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-cli-windows-x86_64.exe) |
| macOS         | [broccoli-cli-macos-universal](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-cli-macos-universal)       |

Each link always points at the newest release.

### Make it runnable

On macOS and Linux, mark the file runnable and move it onto your path as
`broccoli`.

```bash
chmod +x broccoli-cli-linux-x86_64
mv broccoli-cli-linux-x86_64 /usr/local/bin/broccoli
```

On Windows, rename the file to `broccoli.exe`, keep it somewhere easy to find,
and run it from a terminal. Double clicking does nothing useful, because this is
a terminal program.

```bash
broccoli --version
```

### Log in

Point the CLI at your contest server. The address comes from whoever runs your
contest.

```bash
broccoli login --server https://judge.example.com
```

This opens your browser to authorize, then keeps you signed in for later
commands. Confirm who you are.

```bash
broccoli whoami
```

### First commands

```bash
broccoli contest list                            # contests you can see
broccoli contest info "Spring Round"             # details and your registration
broccoli test sol.cpp -c "Spring Round" -p A     # run the sample cases first
broccoli submit sol.cpp -c "Spring Round" -p A   # submit problem A
broccoli watch "Spring Round"                    # live contest dashboard
```

A contest is named by its id or its title, and a problem by its label such as
`A`, its number, or its title. Run `broccoli --help`, or any command followed by
`--help`, to see the rest.

### Build it yourself

If there is no build for your system, or you want the newest code, build from
source with Rust.

```bash
git clone https://github.com/THUSAAC-PSD/broccoli
cargo install --path broccoli/packages/contestant-cli
```

This installs the same `broccoli` command into your Cargo bin folder.

## Run a server

There are two ways to stand up Broccoli. Use the platform bundle for a guided
install, or pull the container images and run them with your own orchestration.

### The platform bundle

The bundle carries the compose files, a role aware installer, and an embedded
copy of the stress test binary. Set the version you want, download the archive,
verify it, and extract.

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

If you run your own orchestration, pull the images directly. Set the version,
then pull the server and the worker variant you need.

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

## Check a deployment

The stress test binary drives a real server with synthetic contestants and
submissions, so you can confirm a fresh install behaves before an event. The
platform bundle already contains it. Download it on its own when you test from
another machine.

| System        | File                                                                                                                                          |
| ------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| Linux x86_64  | [broccoli-stress-test-linux-x86_64](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-stress-test-linux-x86_64)       |
| Linux aarch64 | [broccoli-stress-test-linux-aarch64](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-stress-test-linux-aarch64)     |
| Windows       | [broccoli-stress-test-windows-x86_64.exe](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-stress-test-windows-x86_64.exe) |
| macOS         | [broccoli-stress-test-macos-universal](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-stress-test-macos-universal) |

```bash
chmod +x broccoli-stress-test-linux-x86_64
./broccoli-stress-test-linux-x86_64 --help
```

## Print stations

The `broccoli-print-client` for print stations is released the same way and is
covered in [Printing](./plugins/printing.md).
