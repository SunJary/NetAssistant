---
title: Download NetAssistant - Free for Windows, Linux & macOS
titleTemplate: ':title'
description: "Download NetAssistant for Windows, Linux and macOS on x64 and ARM64: winget, deb/AppImage/tar.gz packages, or build from source, with system requirements."
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
            "description": "A cross-platform network debugging tool with TCP/UDP client and server, MQTT 3.1.1, framing decoders, hex payloads and stress testing.",
            "url": "https://netassistant.trydo.top/en/download",
            "downloadUrl": "https://github.com/SunJary/NetAssistant/releases",
            "license": "https://www.apache.org/licenses/LICENSE-2.0",
            "offers": { "@type": "Offer", "price": "0", "priceCurrency": "USD" }
          },
          {
            "@type": "BreadcrumbList",
            "itemListElement": [
              { "@type": "ListItem", "position": 1, "name": "Home", "item": "https://netassistant.trydo.top/en/" },
              { "@type": "ListItem", "position": 2, "name": "Download", "item": "https://netassistant.trydo.top/en/download" }
            ]
          },
          {
            "@type": "FAQPage",
            "mainEntity": [
              {
                "@type": "Question",
                "name": "Is NetAssistant free?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Yes. NetAssistant is open source under the Apache-2.0 license with all features free to use, no in-app purchases, subscriptions or feature limits."
                }
              },
              {
                "@type": "Question",
                "name": "Which operating systems are supported?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Windows 10 or later, macOS 10.15 or later, and Linux distributions such as Ubuntu 22.04 or later. Each platform ships both x64 and ARM64 builds."
                }
              },
              {
                "@type": "Question",
                "name": "How do I install on Windows?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Use winget: run winget install SunJary.NetAssistant in a terminal and upgrade later with winget upgrade. You can also use the installer or a portable zip."
                }
              },
              {
                "@type": "Question",
                "name": "Is there an ARM64 build?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Yes. Windows and Linux ship aarch64 builds, and macOS offers a universal DMG for both Intel and Apple Silicon, covering Surface Pro X, Raspberry Pi 5 and M-series Macs."
                }
              },
              {
                "@type": "Question",
                "name": "macOS says it cannot verify the developer. What should I do?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "This is normal for unsigned, non-notarized apps and does not mean the app is damaged. Right-click NetAssistant and choose Open, or go to System Settings → Privacy & Security and click Open Anyway."
                }
              }
            ]
          }
        ]
      }
---

# Download

NetAssistant is available for Windows, Linux and macOS, with both x64 and ARM64 builds for each platform. The download links below always point to the files of the [latest release](https://github.com/sunjary/netassistant/releases/latest) — click to download directly. Older versions can be found on [GitHub Releases](https://github.com/sunjary/netassistant/releases).

Platform-specific install guides: [Windows](/en/windows-network-debug-tool) · [macOS](/en/mac-network-debug-tool) · [Linux](/en/linux-network-debug-tool).

## Windows

**Recommended: install with winget** (supports automatic upgrades)

```bash
winget install SunJary.NetAssistant
```

To upgrade:

```bash
winget upgrade SunJary.NetAssistant
```

**Or use the installer**: download [netassistant-windows-x86_64-setup.exe](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-windows-x86_64-setup.exe) and follow the wizard (creates Start Menu and desktop shortcuts automatically).

**Alternative**: download [netassistant-windows-x86_64.zip](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-windows-x86_64.zip), extract it and run `netassistant.exe`.

**ARM64 devices** (Surface Pro X, Snapdragon laptops, etc.): winget and the installer currently ship x64 only. On ARM64, download [netassistant-windows-aarch64.zip](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-windows-aarch64.zip), extract it and run `netassistant.exe`. This build is produced automatically and has not been fully tested on real hardware.

## Linux

**Recommended: deb package** (Debian/Ubuntu and derivatives, installs dependencies automatically)

1. Download [netassistant-linux-x86_64.deb](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-x86_64.deb)
2. Install (dependencies resolved automatically):

```bash
sudo apt install ./netassistant-linux-x86_64.deb
```

3. Launch from the application menu, or run `netassistant` in a terminal

**AppImage** (portable, for Fedora/Arch and other distros)

1. Download [netassistant-linux-x86_64.AppImage](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-x86_64.AppImage)
2. Add execute permission and run:

```bash
chmod +x netassistant-linux-x86_64.AppImage
./netassistant-linux-x86_64.AppImage
```

**Alternative: tar.gz** (lightweight)

1. Download [netassistant-linux-x86_64.tar.gz](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-x86_64.tar.gz)
2. Extract and run:

```bash
tar -xzf netassistant-linux-x86_64.tar.gz
chmod +x netassistant
./netassistant
```

**ARM64 devices** (Raspberry Pi 5, ARM servers, etc.): download [netassistant-linux-aarch64.deb](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-aarch64.deb), [netassistant-linux-aarch64.AppImage](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-aarch64.AppImage) or [netassistant-linux-aarch64.tar.gz](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-aarch64.tar.gz), usage is the same as above. This build is produced automatically and has not been fully tested on real hardware.

## macOS

**Recommended: universal DMG** (works on both Intel and Apple Silicon)

1. Download [netassistant-macos-universal.dmg](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-macos-universal.dmg), open it and drag NetAssistant into the Applications folder
2. If macOS warns "cannot verify the developer" on first launch (normal for unsigned, non-notarized apps — not corruption): right-click NetAssistant → Open; or go to System Settings → Privacy & Security and click "Open Anyway"

**Alternative**: download [netassistant-macos-universal.zip](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-macos-universal.zip) and extract it to run directly (executable permissions are preserved).

## System Requirements

| Platform | Requirements |
| -------- | ------------ |
| Windows | Windows 10 or later (x64 / ARM64) |
| Linux | Ubuntu 22.04 or later (x86_64 / ARM64), Vulkan-compatible GPU |
| macOS | macOS 10.15 or later |

## Building from Source

To build a custom version or get the latest development snapshot:

```bash
git clone https://github.com/sunjary/netassistant.git
cd netassistant
cargo build --release
```

After building, the executable is located in the `target/release` directory.

## Troubleshooting Dependency Errors

Most desktop systems already ship the required dependencies and run out of the box. On Linux, only install these if startup fails with an error:

- AppImage fails with a FUSE error (e.g. `AppImages require FUSE to run`):

```bash
sudo apt install libfuse2
```

- Bare binary / tar.gz fails with a missing GTK3 library (e.g. `error while loading shared libraries: libgtk-3.so.0`):

```bash
sudo apt install libgtk-3-0
```

## FAQ

### Is NetAssistant free?

Yes. NetAssistant is open source under the Apache-2.0 license with all features free to use, no in-app purchases, subscriptions or feature limits.

### Which operating systems are supported?

Windows 10 or later, macOS 10.15 or later, and Linux distributions such as Ubuntu 22.04 or later. Each platform ships both x64 and ARM64 builds.

### How do I install on Windows?

Use winget: run `winget install SunJary.NetAssistant` in a terminal and upgrade later with `winget upgrade`. You can also use the installer or a portable zip.

### Is there an ARM64 build?

Yes. Windows and Linux ship aarch64 builds, and macOS offers a universal DMG for both Intel and Apple Silicon, covering Surface Pro X, Raspberry Pi 5 and M-series Macs.

### macOS says it cannot verify the developer. What should I do?

This is normal for unsigned, non-notarized apps and does not mean the app is damaged. Right-click NetAssistant and choose **Open**, or go to **System Settings → Privacy & Security** and click **Open Anyway**.

See the [FAQ](/en/faq) for more questions and the [changelog](/en/changelog) for releases.
