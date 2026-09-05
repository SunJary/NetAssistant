# Download

NetAssistant is available for Windows, Linux and macOS. All releases can be downloaded from [GitHub Releases](https://github.com/sunjary/netassistant/releases).

## Windows

**Recommended: install with winget** (supports automatic upgrades)

```bash
winget install SunJary.NetAssistant
```

To upgrade:

```bash
winget upgrade SunJary.NetAssistant
```

**Alternative**: download `netassistant-windows-x86_64.zip` from [GitHub Releases](https://github.com/sunjary/netassistant/releases), extract it and run `netassistant.exe`.

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

## macOS

1. Download the archive for your architecture from [GitHub Releases](https://github.com/sunjary/netassistant/releases):
   - Intel: `netassistant-macos-x86_64.tar.gz`
   - Apple Silicon: `netassistant-macos-aarch64.tar.gz`
2. Extract the archive and drag NetAssistant into the Applications folder
3. Right-click the app and choose "Open" to run it (required on first launch)

## System Requirements

| Platform | Requirements |
| -------- | ------------ |
| Windows | Windows 10 or later |
| Linux | GTK3 library (e.g. Ubuntu 22.04 or later), Vulkan-compatible GPU |
| macOS | macOS 10.15 or later |

## Building from Source

To build a custom version or get the latest development snapshot:

```bash
git clone https://github.com/sunjary/netassistant.git
cd netassistant
cargo build --release
```

After building, the executable is located in the `target/release` directory.
