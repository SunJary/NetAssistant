---
title: MQTT Client for Windows - winget Install Guide
description: "Install the NetAssistant MQTT client on Windows 10/11 via winget, setup program or portable zip, with MQTT 3.1.1/5.0, TLS and about 20MB of memory."
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
            "name": "How do I install an MQTT client on Windows?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Run winget install SunJary.NetAssistant in a terminal and upgrade later with winget upgrade; you can also get the setup program or a portable zip from the download page."
            }
          },
          {
            "@type": "Question",
            "name": "Which Windows versions are supported?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Windows 10 1709 and later, plus Windows 11, with x64 and ARM64 builds available."
            }
          },
          {
            "@type": "Question",
            "name": "What if winget cannot find the package?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Update App Installer and the winget sources, then retry; if it is still missing, use the setup program or portable zip from the download page."
            }
          },
          {
            "@type": "Question",
            "name": "Do I need to open any firewall ports?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "The client connects outward to the broker, so outbound traffic usually needs no rule; if a corporate network restricts outbound ports, allow 1883 or 8883."
            }
          },
          {
            "@type": "Question",
            "name": "Is this the same app as the Windows network debugging assistant?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Yes. NetAssistant provides TCP/UDP client and server, the MQTT client, sticky-packet decoding and TCP/UDP stress testing in one app; stress testing is not available for MQTT."
            }
          }
        ]
      }
---

# MQTT Client for Windows: Install, Connect and Field Debugging

There is no shortage of MQTT clients on Windows, but most of them are dedicated Electron tools: keeping several hundred MB of memory resident just to glance at a few messages rarely pays off. NetAssistant is a Rust-native network debugging tool that puts MQTT 3.1.1 / 5.0 and TCP/UDP debugging in the same app, using about 20MB of memory — a good fit for machines that sit on a remote desktop or low-spec field hardware for long periods.

## Why choose it on Windows

- **Native desktop app**: not an Electron wrapper, so it starts fast and stays light on memory
- **Complete protocol set**: TCP/UDP client and server plus MQTT 3.1.1 / 5.0
- **Flexible installation**: winget, setup program or portable zip
- **Full debugging toolkit**: per-subscription switches, QoS 0/1/2, retain, last-will, auto-reply, search and export
- **TLS ready**: three certificate verification modes, with client certificate and private key for mutual TLS

## System requirements

- Windows 10 1709 or later, or Windows 11
- x64 or ARM64
- Outbound network access to the broker on port 1883 (plain) or 8883 (TLS)

## Installation

### Recommended: winget

```powershell
winget install SunJary.NetAssistant
```

To upgrade:

```powershell
winget upgrade SunJary.NetAssistant
```

### Setup program or portable zip

Get the Windows setup program from the [download page](/en/download), or download the zip and run the app directly after extracting it. The portable version suits machines without administrator rights, or when you need to carry the tool on a USB drive.

On first launch SmartScreen may warn about an unknown publisher; once you trust the source, choose "More info → Run anyway". If a corporate device is restricted by policy, follow your IT requirements.

## First MQTT connection

1. Open NetAssistant, click `+ New` and choose **MQTT** as the protocol
2. Enter the broker address, port (1883 or 8883), client ID, username and password
3. Turn on TLS if encryption is needed and choose the certificate verification mode
4. After connecting, subscribe to `test/#` or a real business topic in the left panel
5. In the send area, fill in topic, QoS, retain and payload, then publish

See the [MQTT debugging tutorial](/en/guide/mqtt) for the full walkthrough with screenshots.

## Scenarios suited to Windows field work

- **Remote desktop / jump hosts**: low memory use, so leaving it running all day does not slow the machine down
- **Switching between environments**: name connections for test, staging and production, and open them in parallel tabs
- **TCP/UDP and MQTT together**: troubleshoot gateways, proprietary protocols and MQTT in one tool
- **Offline installation**: the zip version does not depend on an online installer, which suits intranet machines
- **Saved configuration**: connections are stored automatically and reconnect on the next launch

If you care more about TCP/UDP debugging on Windows, see the [Windows network debugging tool](/en/windows-network-debug-tool).

## FAQ

### How do I install an MQTT client on Windows?

Run `winget install SunJary.NetAssistant`, or get the setup program / portable zip from the download page.

### Which Windows versions are supported?

Windows 10 1709 and later plus Windows 11, with x64 and ARM64 builds available.

### What if winget cannot find the package?

Update App Installer and the winget sources, then retry; if it is still missing, use the setup program or zip.

### Do I need to open any firewall ports?

Connecting to the broker is outbound traffic, so usually nothing needs to be allowed; if a corporate network restricts outbound ports, allow 1883 / 8883.

### Is this the same app as the Windows network debugging assistant?

Yes. MQTT and TCP/UDP debugging both live in NetAssistant.

## Related pages

- Features and parameters: [MQTT client](/en/mqtt-client)
- Connect and subscribe tutorial: [MQTT debugging tutorial](/en/guide/mqtt)
- Diagnosing connection failures: [MQTT troubleshooting](/en/mqtt-troubleshooting)
- Other platforms: [macOS client](/en/mqtt-client-mac), [Linux client](/en/mqtt-client-linux)
- FAQ: [MQTT FAQ](/en/mqtt-faq)
- [Download for Windows](/en/download)
