---
title: 下载 NetAssistant - Windows / Linux / macOS 官方免费下载
description: 下载 NetAssistant 跨平台网络调试工具：支持 Windows、Linux、macOS 的 x64 与 ARM64；winget 安装、Linux deb/AppImage/tar.gz、macOS 安装包与源码编译方式，附系统要求。
---

# 下载

NetAssistant 支持 Windows、Linux 和 macOS，各平台均提供 x64 与 ARM64 版本。以下下载链接始终指向[最新版本](https://github.com/sunjary/netassistant/releases/latest)的文件，点击即直接下载；历史版本请见 [GitHub Releases](https://github.com/sunjary/netassistant/releases)。

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
