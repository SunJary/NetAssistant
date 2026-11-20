---
title: MQTT Client for Linux - deb and AppImage Install
description: Install the NetAssistant MQTT client for Linux with a deb, AppImage or tar.gz package on Ubuntu 22.04+ x86_64 or ARM64, with Vulkan and TLS notes.
head:
  - - script
    - type: application/ld+json
    - |
      {
        "@context": "https://schema.org",
        "@type": "FAQPage",
        "mainEntity": [
          {
            "@type": "Question",
            "name": "How do I install an MQTT client on Linux?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "On Debian/Ubuntu, download the deb package and install it with sudo apt install ./netassistant-linux-x86_64.deb; on other distributions use the AppImage or tar.gz, add execute permission and run it."
            }
          },
          {
            "@type": "Question",
            "name": "What should I do if startup reports a Vulkan error?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "NetAssistant needs a Vulkan-compatible GPU and driver. On Ubuntu you can install mesa-vulkan-drivers, and in a virtual machine or remote environment make sure 3D acceleration is enabled; an environment without Vulkan cannot start the graphical interface."
            }
          },
          {
            "@type": "Question",
            "name": "Does it support ARM64 devices such as Raspberry Pi?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Yes, linux-aarch64 deb, AppImage and tar.gz builds are provided. These are automatic build artifacts that have not been fully tested on real hardware, so verify them on the target device before using them in production."
            }
          },
          {
            "@type": "Question",
            "name": "What if the AppImage will not run?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "First run chmod +x to add execute permission; if it still fails, check whether FUSE is missing, or switch to the tar.gz build and run the extracted executable."
            }
          },
          {
            "@type": "Question",
            "name": "Does the Linux version support MQTT over WebSocket?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "No. Like the other platforms, the Linux version connects to the broker over TCP only; use a client such as MQTTX for WebSocket access."
            }
          }
        ]
      }
---

# MQTT Client for Linux: Install, Connect and Debug

An MQTT debugging tool on Linux is usually either a command-line utility or an Electron-based graphical client. NetAssistant is a native app built with Rust and GPUI, ships as a deb, AppImage or tar.gz, and keeps MQTT 3.1.1 / 5.0 and TCP/UDP debugging in the same window.

## Why choose it on Linux

- **Native app**: not an Electron wrapper, so it starts fast and uses about 20MB of memory
- **Every install format**: the deb resolves dependencies automatically, the AppImage needs no installation, and the tar.gz suits minimal environments
- **x86_64 and ARM64**: covers common servers, developer machines and Raspberry Pi 5
- **Complete protocol set**: TCP/UDP client and server plus MQTT 3.1.1 / 5.0
- **Full TLS**: system roots, custom CA and skipped verification, with mutual TLS support

## System requirements

| Item | Requirement |
| ---- | ---- |
| Distribution | Ubuntu 22.04 and later plus derivatives; other distributions should use the AppImage |
| Architecture | x86_64 or ARM64 (aarch64) |
| Graphics | Vulkan-compatible GPU and driver |
| Network | Access to the broker on port 1883 (plain) or 8883 (TLS) |

NetAssistant needs Vulkan to render its interface. On Ubuntu you can install `mesa-vulkan-drivers`; in a virtual machine, WSL or remote desktop environment, make sure 3D acceleration is enabled, otherwise the graphical interface cannot start.

## Installation

### Recommended: deb package

```bash
sudo apt install ./netassistant-linux-x86_64.deb
```

After installing, launch it from the application menu or run `netassistant` in a terminal. The deb resolves dependencies automatically.

### AppImage (no installation)

```bash
chmod +x netassistant-linux-x86_64.AppImage
./netassistant-linux-x86_64.AppImage
```

Suits non-Debian distributions such as Fedora and Arch. If the system lacks FUSE, use the tar.gz instead.

### tar.gz (minimal environments)

```bash
tar -xzf netassistant-linux-x86_64.tar.gz
chmod +x netassistant
./netassistant
```

### ARM64

On a Raspberry Pi 5, an ARM server or similar hardware, download the `linux-aarch64` deb, AppImage or tar.gz; usage is the same. ARM64 builds are automatic artifacts that have not been fully tested on real hardware, so verify them on the target device first.

## First MQTT connection

1. Open NetAssistant, click `+ New` and choose **MQTT** as the protocol
2. Enter the broker address, port, client ID, username and password
3. Turn on TLS if encryption is needed and choose the verification mode based on the certificate source
4. After connecting, subscribe to `test/#` or a real business topic
5. In the send area, fill in topic, QoS, retain and payload, then publish

See the [MQTT debugging tutorial](/en/guide/mqtt) for the full walkthrough.

## Configure TLS on Linux

- Certificates and private keys are PEM file paths; PKCS#8, PKCS#1 and SEC1 are supported
- For a private CA choose "Custom CA" and point it at the CA PEM; public CAs use the default system roots
- Mutual TLS needs the client certificate and private key filled in as a pair; only the paths are saved
- For local testing with self-signed certificates you can temporarily skip verification; do not use that in production

For handshake failures, see [MQTT troubleshooting](/en/mqtt-troubleshooting#tls-handshake-failures).

## Scenarios suited to Linux

- Run EMQX / Mosquitto in Docker on a developer machine and connect straight to `127.0.0.1`
- Diagnose device reporting on a server desktop without slowing the system down
- Attach a serial device to a Raspberry Pi and capture MQTT and TCP/UDP on the same machine
- Run the AppImage from a USB drive or shared directory without installing anything

If you care more about TCP/UDP debugging on Linux, see the [Linux network debugging tool](/en/linux-network-debug-tool).

## FAQ

### How do I install an MQTT client on Linux?

Use the deb package on Ubuntu/Debian; on other distributions use the AppImage or tar.gz, add execute permission and run it.

### What should I do if startup reports a Vulkan error?

Install `mesa-vulkan-drivers` and make sure 3D acceleration is enabled in a virtual machine or remote environment; without Vulkan the graphical interface cannot start.

### Does it support ARM64 devices such as Raspberry Pi?

Yes, aarch64 deb, AppImage and tar.gz builds are provided, but they are automatic artifacts, so verify them first.

### What if the AppImage will not run?

Run `chmod +x` first and check FUSE; if it still fails, switch to the tar.gz.

### Does the Linux version support MQTT over WebSocket?

No, it connects to the broker over TCP only.

## Related pages

- Features and parameters: [MQTT client](/en/mqtt-client)
- Connect and subscribe tutorial: [MQTT debugging tutorial](/en/guide/mqtt)
- Diagnosing connection failures: [MQTT troubleshooting](/en/mqtt-troubleshooting)
- Other platforms: [Windows client](/en/mqtt-client-windows), [macOS client](/en/mqtt-client-mac)
- [Download for Linux](/en/download)
