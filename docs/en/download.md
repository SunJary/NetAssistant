---
title: Download
description: "Download NetAssistant cross-platform network debugging tool: available for Windows, Linux and macOS on x64 and ARM64 via winget, AppImage, tar.gz or building from source, with system requirements."
---

# Download

NetAssistant is available for Windows, Linux and macOS, with both x64 and ARM64 builds for each platform. All releases can be downloaded from [GitHub Releases](https://github.com/sunjary/netassistant/releases).

## Windows

**Recommended: install with winget** (supports automatic upgrades)

```bash
winget install SunJary.NetAssistant
```

To upgrade:

```bash
winget upgrade SunJary.NetAssistant
```

**Or use the installer**: download `netassistant-windows-x86_64-setup.exe` and follow the wizard (creates Start Menu and desktop shortcuts automatically).

**Alternative**: download `netassistant-windows-x86_64.zip` from [GitHub Releases](https://github.com/sunjary/netassistant/releases), extract it and run `netassistant.exe`.

**ARM64 devices** (Surface Pro X, Snapdragon laptops, etc.): winget and the installer currently ship x64 only. On ARM64, download `netassistant-windows-aarch64.zip`, extract it and run `netassistant.exe`. This build is produced automatically and has not been fully tested on real hardware.

## Linux

**Recommended: AppImage** (works out of the box)

1. Download `netassistant-linux-x86_64.AppImage` from [GitHub Releases](https://github.com/sunjary/netassistant/releases)
2. Add execute permission and run:

```bash
chmod +x netassistant-linux-x86_64.AppImage
./netassistant-linux-x86_64.AppImage
```

libfuse2 is required on first run: `sudo apt install libfuse2`

**Alternative: tar.gz** (lightweight, dependencies must be installed manually)

```bash
tar -xzf netassistant-linux-x86_64.tar.gz
chmod +x netassistant
./netassistant
```

GTK3 must be installed manually: `sudo apt install libgtk-3-0`

**ARM64 devices** (Raspberry Pi 5, ARM servers, etc.): replace `x86_64` with `aarch64` in the filenames above, i.e. `netassistant-linux-aarch64.AppImage` or `netassistant-linux-aarch64.tar.gz`. This build is produced automatically and has not been fully tested on real hardware.

## macOS

1. Download the archive for your architecture from [GitHub Releases](https://github.com/sunjary/netassistant/releases):
   - Intel: `netassistant-macos-x86_64.tar.gz`
   - Apple Silicon: `netassistant-macos-aarch64.tar.gz`
2. Extract the archive and drag NetAssistant into the Applications folder
3. Right-click the app and choose "Open" to run it (required on first launch)

## System Requirements

| Platform | Requirements |
| -------- | ------------ |
| Windows | Windows 10 or later (x64 / ARM64) |
| Linux | GTK3 library (e.g. Ubuntu 22.04 or later), Vulkan-compatible GPU (x86_64 / ARM64) |
| macOS | macOS 10.15 or later |

## Building from Source

To build a custom version or get the latest development snapshot:

```bash
git clone https://github.com/sunjary/netassistant.git
cd netassistant
cargo build --release
```

After building, the executable is located in the `target/release` directory.
