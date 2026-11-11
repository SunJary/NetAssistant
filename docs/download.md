---
title: 下载 NetAssistant - Windows / Linux / macOS 官方免费下载
titleTemplate: ':title'
description: 下载 NetAssistant 跨平台网络调试工具：Windows、Linux、macOS 的 x64 与 ARM64，winget、deb/AppImage/tar.gz 与源码编译方式，附系统要求。
head:
  - - script
    - type: application/ld+json
    - |
      {
        "@context": "https://schema.org",
        "@graph": [
          {
            "@type": "SoftwareApplication",
            "name": "NetAssistant",
            "applicationCategory": "DeveloperApplication",
            "operatingSystem": "Windows 10+, macOS 10.15+, Linux",
            "description": "跨平台网络调试工具，支持 TCP/UDP 客户端与服务端、MQTT 3.1.1、粘包解码、十六进制收发与压力测试。",
            "url": "https://netassistant.trydo.top/download",
            "downloadUrl": "https://github.com/SunJary/NetAssistant/releases",
            "license": "https://www.apache.org/licenses/LICENSE-2.0",
            "offers": { "@type": "Offer", "price": "0", "priceCurrency": "USD" }
          },
          {
            "@type": "BreadcrumbList",
            "itemListElement": [
              { "@type": "ListItem", "position": 1, "name": "首页", "item": "https://netassistant.trydo.top/" },
              { "@type": "ListItem", "position": 2, "name": "下载", "item": "https://netassistant.trydo.top/download" }
            ]
          },
          {
            "@type": "FAQPage",
            "mainEntity": [
              {
                "@type": "Question",
                "name": "NetAssistant 是免费的吗？",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "是的。NetAssistant 基于 Apache-2.0 许可证开源，全部功能免费使用，没有内购、订阅或功能限制。"
                }
              },
              {
                "@type": "Question",
                "name": "支持哪些操作系统？",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "支持 Windows 10 及以上、macOS 10.15 及以上、Ubuntu 22.04 及以上的 Linux 发行版，每个平台都提供 x64 与 ARM64 构建。"
                }
              },
              {
                "@type": "Question",
                "name": "Windows 上怎么安装？",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "推荐使用 winget：在终端执行 winget install SunJary.NetAssistant 即可，后续用 winget upgrade 升级。也可以下载安装程序或免安装 zip。"
                }
              },
              {
                "@type": "Question",
                "name": "有 ARM64 版本吗？",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "有。Windows、Linux 均提供 aarch64 构建，macOS 提供 Intel 与 Apple Silicon 通用的 DMG，适合 Surface Pro X、树莓派 5、M 系列 Mac 等设备。"
                }
              },
              {
                "@type": "Question",
                "name": "macOS 提示无法验证开发者怎么办？",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "这是未公证应用的正常提示，并非应用损坏。右键点击 NetAssistant 选择「打开」，或在「系统设置 → 隐私与安全性」中选择「仍要打开」即可。"
                }
              }
            ]
          }
        ]
      }
---

# 下载

NetAssistant 支持 Windows、Linux 和 macOS，各平台均提供 x64 与 ARM64 版本。以下下载链接始终指向[最新版本](https://github.com/sunjary/netassistant/releases/latest)的文件，点击即直接下载；历史版本请见 [GitHub Releases](https://github.com/sunjary/netassistant/releases)。

按平台查看详细安装说明：[Windows](/windows-network-debug-tool) · [macOS](/mac-network-debug-tool) · [Linux](/linux-network-debug-tool)。

## Windows

**推荐：使用 winget 安装**（支持自动升级）

```bash
winget install SunJary.NetAssistant
```

升级：

```bash
winget upgrade SunJary.NetAssistant
```

**或使用安装程序**：下载 [netassistant-windows-x86_64-setup.exe](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-windows-x86_64-setup.exe)，按向导安装（自动创建开始菜单与桌面快捷方式）。

**备选**：下载 [netassistant-windows-x86_64.zip](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-windows-x86_64.zip)，解压后运行 `netassistant.exe`。

**ARM64 设备**（Surface Pro X、骁龙笔记本等）：winget 与安装程序目前仅提供 x64 版本，ARM64 请下载 [netassistant-windows-aarch64.zip](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-windows-aarch64.zip)，解压后运行 `netassistant.exe`。该版本为自动构建产物，未经真机完整测试。

## Linux

**推荐：deb 包**（Debian/Ubuntu 及衍生版，自动安装依赖）

1. 下载 [netassistant-linux-x86_64.deb](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-x86_64.deb)
2. 安装（自动解析依赖）：

```bash
sudo apt install ./netassistant-linux-x86_64.deb
```

3. 从应用菜单启动，或终端运行 `netassistant`

**AppImage**（免安装，适合 Fedora/Arch 等其他发行版）

1. 下载 [netassistant-linux-x86_64.AppImage](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-x86_64.AppImage)
2. 添加执行权限并运行：

```bash
chmod +x netassistant-linux-x86_64.AppImage
./netassistant-linux-x86_64.AppImage
```

**备选：tar.gz**（轻量）

1. 下载 [netassistant-linux-x86_64.tar.gz](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-x86_64.tar.gz)
2. 解压并运行：

```bash
tar -xzf netassistant-linux-x86_64.tar.gz
chmod +x netassistant
./netassistant
```

**ARM64 设备**（树莓派 5、ARM 服务器等）：下载 [netassistant-linux-aarch64.AppImage](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-aarch64.AppImage)、[netassistant-linux-aarch64.deb](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-aarch64.deb) 或 [netassistant-linux-aarch64.tar.gz](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-aarch64.tar.gz)，用法同上。该版本为自动构建产物，未经真机完整测试。

## macOS

**推荐：通用 DMG**（Intel 与 Apple Silicon 通用）

1. 下载 [netassistant-macos-universal.dmg](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-macos-universal.dmg)，打开后将 NetAssistant 拖入 Applications 文件夹
2. 首次打开若提示「无法验证开发者」（未公证应用的正常现象，并非应用损坏）：右键点击 NetAssistant → 打开；或在「系统设置 → 隐私与安全性」中选择仍要打开

**备选**：下载 [netassistant-macos-universal.zip](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-macos-universal.zip)，解压即用（保留可执行权限）。

## 系统要求

| 平台 | 要求 |
| ---- | ---- |
| Windows | Windows 10 或更高版本（x64 / ARM64） |
| Linux | Ubuntu 22.04 及以上（x86_64 / ARM64），需 Vulkan 兼容 GPU |
| macOS | macOS 10.15 或更高版本 |

## 从源代码编译

如需自定义编译或获取最新开发版本：

```bash
git clone https://github.com/sunjary/netassistant.git
cd netassistant
cargo build --release
```

编译完成后，可执行文件位于 `target/release` 目录下。

## 依赖报错处理

多数桌面系统已自带所需依赖，可直接运行；仅当启动报错时按提示安装即可（Linux）：

- AppImage 启动报 FUSE 相关错误（如 `AppImages require FUSE to run`）：

```bash
sudo apt install libfuse2
```

- 裸二进制 / tar.gz 启动报缺少 GTK3（如 `error while loading shared libraries: libgtk-3.so.0`）：

```bash
sudo apt install libgtk-3-0
```

## 常见问题

### NetAssistant 是免费的吗？

是的。NetAssistant 基于 Apache-2.0 许可证开源，全部功能免费使用，没有内购、订阅或功能限制。

### 支持哪些操作系统？

支持 Windows 10 及以上、macOS 10.15 及以上、Ubuntu 22.04 及以上的 Linux 发行版，每个平台都提供 x64 与 ARM64 构建。

### Windows 上怎么安装？

推荐使用 winget：在终端执行 `winget install SunJary.NetAssistant` 即可，后续用 `winget upgrade` 升级。也可以下载安装程序或免安装 zip。

### 有 ARM64 版本吗？

有。Windows、Linux 均提供 aarch64 构建，macOS 提供 Intel 与 Apple Silicon 通用的 DMG，适合 Surface Pro X、树莓派 5、M 系列 Mac 等设备。

### macOS 提示无法验证开发者怎么办？

这是未公证应用的正常提示，并非应用损坏。右键点击 NetAssistant 选择「打开」，或在「系统设置 → 隐私与安全性」中选择「仍要打开」即可。

更多问题见 [常见问题 FAQ](/faq)，版本更新见 [更新日志](/changelog)。
