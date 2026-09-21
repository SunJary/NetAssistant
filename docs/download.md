---
title: 下载
description: 下载 NetAssistant 跨平台网络调试工具：支持 Windows、Linux、macOS 的 x64 与 ARM64；winget 安装、AppImage、tar.gz 与源码编译方式，附系统要求。
---

# 下载

NetAssistant 支持 Windows、Linux 和 macOS，各平台均提供 x64 与 ARM64 版本。所有版本均可从 [GitHub Releases](https://github.com/sunjary/netassistant/releases) 下载。

## Windows

**推荐：使用 winget 安装**（支持自动升级）

```bash
winget install SunJary.NetAssistant
```

升级：

```bash
winget upgrade SunJary.NetAssistant
```

**或使用安装程序**：下载 `netassistant-windows-x86_64-setup.exe`，按向导安装（自动创建开始菜单与桌面快捷方式）。

**备选**：从 [GitHub Releases](https://github.com/sunjary/netassistant/releases) 下载 `netassistant-windows-x86_64.zip`，解压后运行 `netassistant.exe`。

**ARM64 设备**（Surface Pro X、骁龙笔记本等）：winget 与安装程序目前仅提供 x64 版本，ARM64 请下载 `netassistant-windows-aarch64.zip`，解压后运行 `netassistant.exe`。该版本为自动构建产物，未经真机完整测试。

## Linux

**推荐：AppImage**（开箱即用）

1. 从 [GitHub Releases](https://github.com/sunjary/netassistant/releases) 下载 `netassistant-linux-x86_64.AppImage`
2. 添加执行权限并运行：

```bash
chmod +x netassistant-linux-x86_64.AppImage
./netassistant-linux-x86_64.AppImage
```

首次运行需安装 libfuse2：`sudo apt install libfuse2`

**备选：tar.gz**（轻量，需自行安装依赖）

```bash
tar -xzf netassistant-linux-x86_64.tar.gz
chmod +x netassistant
./netassistant
```

需自行安装 GTK3 依赖：`sudo apt install libgtk-3-0`

**ARM64 设备**（树莓派 5、ARM 服务器等）：将上述文件名中的 `x86_64` 替换为 `aarch64`，即 `netassistant-linux-aarch64.AppImage` 或 `netassistant-linux-aarch64.tar.gz`。该版本为自动构建产物，未经真机完整测试。

## macOS

1. 从 [GitHub Releases](https://github.com/sunjary/netassistant/releases) 下载对应架构的压缩包：
   - Intel 芯片：`netassistant-macos-x86_64.tar.gz`
   - Apple Silicon：`netassistant-macos-aarch64.tar.gz`
2. 解压安装包，将 NetAssistant 拖放到 Applications 文件夹
3. 右键点击应用程序，选择「打开」运行（首次运行需要此操作）

## 系统要求

| 平台 | 要求 |
| ---- | ---- |
| Windows | Windows 10 或更高版本（x64 / ARM64） |
| Linux | 需要 GTK3 库（如 Ubuntu 22.04 及以上）、Vulkan 兼容 GPU（x86_64 / ARM64） |
| macOS | macOS 10.15 或更高版本 |

## 从源代码编译

如需自定义编译或获取最新开发版本：

```bash
git clone https://github.com/sunjary/netassistant.git
cd netassistant
cargo build --release
```

编译完成后，可执行文件位于 `target/release` 目录下。
