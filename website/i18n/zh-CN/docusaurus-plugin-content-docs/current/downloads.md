---
title: 下载
sidebar_label: 下载
sidebar_position: 2
---

# 下载

每个打了标签的发布版本都会发布服务器镜像和一份你自行安装的捆绑包、压力测试程序、参赛者命令行工具，以及打印站客户端。它们都位于[发布页面](https://github.com/THUSAAC-PSD/broccoli/releases)，旁边的 `manifest.json` 列出了每个文件及其大小和 SHA256 校验和。

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

如果你使用自己的编排系统，可以直接拉取镜像。每个镜像都为 x86_64 和 arm64 Linux 构建，因此 Docker 会为你运行它的机器拉取正确的那一个。

| 镜像          | 引用                                                          |
| ------------- | ------------------------------------------------------------ |
| 服务器        | `ghcr.io/thusaac-psd/broccoli/broccoli-server:$VERSION`      |
| 评测机，base  | `ghcr.io/thusaac-psd/broccoli/broccoli-worker:$VERSION-base` |
| 评测机，icpc  | `ghcr.io/thusaac-psd/broccoli/broccoli-worker:$VERSION-icpc` |
| 评测机，full  | `ghcr.io/thusaac-psd/broccoli/broccoli-worker:$VERSION-full` |

设置版本，然后拉取服务器以及你需要的评测机变体。

```bash
VERSION=v0.1.0
docker pull "ghcr.io/thusaac-psd/broccoli/broccoli-server:$VERSION"
docker pull "ghcr.io/thusaac-psd/broccoli/broccoli-worker:$VERSION-base"
docker pull "ghcr.io/thusaac-psd/broccoli/broccoli-worker:$VERSION-icpc"
docker pull "ghcr.io/thusaac-psd/broccoli/broccoli-worker:$VERSION-full"
```

评测机有三种规格。`base` 镜像不包含任何语言环境。`icpc` 镜像增加了 C 和 C++。`full` 镜像增加了其余语言，包括 Java、Kotlin 和 Python。

对于中国大陆的网络，相同的镜像在阿里云上有镜像源，位于 `registry.cn-hangzhou.aliyuncs.com/broccoli/`。

## 压力测试

压力测试程序会用模拟的参赛者和提交来驱动一个真实的服务器，因此你可以在活动开始前确认全新安装的行为是否正常。平台捆绑包已经包含它。仅当你从另一台机器进行测试时才需要单独下载。

| 系统          | 文件                                                                                                                                                |
| ------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| Linux x86_64  | [broccoli-stress-test-linux-x86_64](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-stress-test-linux-x86_64)             |
| Linux aarch64 | [broccoli-stress-test-linux-aarch64](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-stress-test-linux-aarch64)           |
| Windows       | [broccoli-stress-test-windows-x86_64.exe](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-stress-test-windows-x86_64.exe) |
| macOS         | [broccoli-stress-test-macos-universal](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-stress-test-macos-universal)       |

```bash
chmod +x broccoli-stress-test-linux-x86_64
./broccoli-stress-test-linux-x86_64 --help
```

## 参赛者命令行工具

`broccoli` 是面向参赛者的命令行工具。下载适合你系统的构建，然后阅读[参赛者命令行工具](./cli/contestant.md)页面，了解如何登录以及全部命令。

| 系统          | 文件                                                                                                                                |
| ------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| Linux x86_64  | [broccoli-cli-linux-x86_64](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-cli-linux-x86_64)             |
| Linux aarch64 | [broccoli-cli-linux-aarch64](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-cli-linux-aarch64)           |
| Windows       | [broccoli-cli-windows-x86_64.exe](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-cli-windows-x86_64.exe) |
| macOS         | [broccoli-cli-macos-universal](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-cli-macos-universal)       |

## 打印站

打印站会在打印机旁边的计算机上运行一个小型客户端，把每一个打印请求变成打印出来的纸张。为每个打印站下载相应的构建，然后按照[打印](./plugins/printing.md)进行设置。

| 系统          | 文件                                                                                                                                                  |
| ------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| Linux x86_64  | [broccoli-print-client-linux-x86_64](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-print-client-linux-x86_64)             |
| Linux aarch64 | [broccoli-print-client-linux-aarch64](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-print-client-linux-aarch64)           |
| Windows       | [broccoli-print-client-windows-x86_64.exe](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-print-client-windows-x86_64.exe) |
| macOS         | [broccoli-print-client-macos-universal](https://github.com/THUSAAC-PSD/broccoli/releases/latest/download/broccoli-print-client-macos-universal)       |
