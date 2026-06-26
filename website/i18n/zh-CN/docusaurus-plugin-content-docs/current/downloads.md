---
title: 下载
sidebar_label: 下载
sidebar_position: 2
---

# 下载

每个打了标签的发布版本都会发布参赛者命令行工具、服务器与评测机镜像、用于自托管的平台捆绑包、压力测试程序，以及打印站客户端。它们都位于[发布页面](https://github.com/THUSAAC-PSD/broccoli/releases)，旁边的 `manifest.json` 列出了每个文件及其大小和 SHA256 校验和。

## 参赛者命令行工具

`broccoli` 是面向参赛者的命令行工具。你可以登录、测试并提交解答、浏览比赛与题目、提出疑问，并在终端中实时观看比赛。它是单个文件，无需额外安装其他东西。

| 系统          | 文件                                                                                                                                  |
| ------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| Linux x86_64  | [broccoli-cli-linux-x86_64](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-cli-linux-x86_64)             |
| Linux aarch64 | [broccoli-cli-linux-aarch64](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-cli-linux-aarch64)           |
| Windows       | [broccoli-cli-windows-x86_64.exe](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-cli-windows-x86_64.exe) |
| macOS         | [broccoli-cli-macos-universal](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-cli-macos-universal)       |

每个链接都始终指向最新的发布版本。

### 让它可执行

在 macOS 和 Linux 上，将文件标记为可执行，并把它移动到你的可执行路径中，命名为 `broccoli`。

```bash
chmod +x broccoli-cli-linux-x86_64
mv broccoli-cli-linux-x86_64 /usr/local/bin/broccoli
```

在 Windows 上，将文件重命名为 `broccoli.exe`，放在便于查找的位置，并从终端运行。双击不会有任何用处，因为这是一个终端程序。

```bash
broccoli --version
```

### 登录

将命令行工具指向你的比赛服务器。服务器地址由比赛的组织者提供。

```bash
broccoli login --server https://judge.example.com
```

这会打开浏览器进行授权，随后让你保持登录状态以便执行后续命令。确认你的身份。

```bash
broccoli whoami
```

### 第一组命令

```bash
broccoli contest list                            # contests you can see
broccoli contest info "Spring Round"             # details and your registration
broccoli test sol.cpp -c "Spring Round" -p A     # run the sample cases first
broccoli submit sol.cpp -c "Spring Round" -p A   # submit problem A
broccoli watch "Spring Round"                    # live contest dashboard
```

比赛以其 id 或标题来指定，题目以其标签（例如 `A`）、编号或标题来指定。运行 `broccoli --help`，或在任意命令后加上 `--help`，即可查看其余内容。

### 自行构建

如果没有适合你系统的构建，或者你想要最新的代码，可以使用 Rust 从源码构建。

```bash
git clone https://github.com/THUSAAC-PSD/broccoli
cargo install --path broccoli/packages/contestant-cli
```

这会将同一个 `broccoli` 命令安装到你的 Cargo bin 目录中。

## 运行服务器

有两种方式来部署 Broccoli。使用平台捆绑包进行引导式安装，或者拉取容器镜像并用你自己的编排系统运行。

### 平台捆绑包

捆绑包包含 compose 文件、按角色划分的安装程序，以及内嵌的压力测试程序。设置你想要的版本，下载归档文件，校验后解压。

```bash
VERSION=v0.1.0
curl -LO "https://github.com/THUSAAC-PSD/broccoli/releases/download/$VERSION/broccoli-platform-$VERSION.tar.gz"
curl -LO "https://github.com/THUSAAC-PSD/broccoli/releases/download/$VERSION/broccoli-platform-$VERSION.tar.gz.sha256"
sha256sum -c "broccoli-platform-$VERSION.tar.gz.sha256"
tar -xzf "broccoli-platform-$VERSION.tar.gz"
cd "broccoli-platform-$VERSION"
```

部署由多个角色组成，每个角色运行在各自的机器上。先安装 infra，然后是服务器，再是一个或多个评测机。

```bash
./install.sh infra      # PostgreSQL, Redis, and object storage
./install.sh server     # the API server and web interface
./install.sh worker     # the judging sandbox
./install.sh gateway    # optional load balancer in front of several servers
```

不带角色运行 `./install.sh` 会显示一个引导菜单。`single-host` 角色会把所有组件放在一台机器上，仅适用于演示或彩排，不适用于正式比赛。解压后的捆绑包附带自己的 README 和一份包含日常命令的运维手册。

### 容器镜像

如果你使用自己的编排系统，可以直接拉取镜像。设置版本，然后拉取服务器以及你需要的评测机变体。

```bash
VERSION=v0.1.0
docker pull "ghcr.io/thusaac-psd/broccoli/broccoli-server:$VERSION"
docker pull "ghcr.io/thusaac-psd/broccoli/broccoli-worker:$VERSION-base"
docker pull "ghcr.io/thusaac-psd/broccoli/broccoli-worker:$VERSION-icpc"
docker pull "ghcr.io/thusaac-psd/broccoli/broccoli-worker:$VERSION-full"
```

评测机有三种规格。`base` 镜像不包含任何语言环境。`icpc` 镜像增加了 C 和 C++。`full` 镜像增加了其余语言，包括 Java、Kotlin 和 Python。

对于中国大陆的网络，相同的镜像在阿里云上有镜像源，位于 `registry.cn-hangzhou.aliyuncs.com/broccoli/`。

## 检查部署

压力测试程序会用模拟的参赛者和提交来驱动一个真实的服务器，因此你可以在活动开始前确认全新安装的行为是否正常。平台捆绑包已经包含它。仅当你从另一台机器进行测试时才需要单独下载。

| 系统          | 文件                                                                                                                                          |
| ------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| Linux x86_64  | [broccoli-stress-test-linux-x86_64](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-stress-test-linux-x86_64)       |
| Linux aarch64 | [broccoli-stress-test-linux-aarch64](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-stress-test-linux-aarch64)     |
| Windows       | [broccoli-stress-test-windows-x86_64.exe](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-stress-test-windows-x86_64.exe) |
| macOS         | [broccoli-stress-test-macos-universal](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-stress-test-macos-universal) |

```bash
chmod +x broccoli-stress-test-linux-x86_64
./broccoli-stress-test-linux-x86_64 --help
```

## 打印站

用于打印站的 `broccoli-print-client` 以相同的方式发布，相关说明见[打印](./plugins/printing.md)。
