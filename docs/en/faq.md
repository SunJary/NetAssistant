---
title: FAQ - Does the Network Debugging Assistant Support macOS?
description: "NetAssistant FAQ: macOS & Linux availability, difference from NetAssist, winget install command, Linux deb/AppImage downloads, TCP sticky packet handling and high-concurrency stress testing."
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
            "name": "Does the classic NetAssist network debugging tool support macOS?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Two similarly named tools should be distinguished: NetAssistant (the cross-platform tool documented on this site) supports macOS, Windows and Linux with native x64 and ARM64 builds, while the similarly named classic NetAssist only provides a Windows version. For TCP/UDP debugging on macOS or Linux, use NetAssistant directly."
            }
          },
          {
            "@type": "Question",
            "name": "What is the difference between NetAssistant and NetAssist?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "They are different pieces of software. NetAssist is a closed-source Windows-only tool; NetAssistant is an open-source (Apache-2.0) cross-platform tool built with Rust, supporting Windows/Linux/macOS on x64 and ARM64, with four TCP decoders for sticky packets and built-in high-concurrency stress testing (QPS and p50/p95/p99 latency)."
            }
          },
          {
            "@type": "Question",
            "name": "How do I install NetAssistant with winget?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Run winget install SunJary.NetAssistant on Windows. Later upgrades are available via winget upgrade SunJary.NetAssistant."
            }
          },
          {
            "@type": "Question",
            "name": "Is there a deb package for Linux?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "NetAssistant ships deb, AppImage and tar.gz packages for Linux (x86_64 and aarch64). On Debian/Ubuntu and derivatives the deb package is recommended — sudo apt install resolves dependencies automatically; AppImage suits Fedora/Arch and other distros (libfuse2 required on first run); tar.gz needs GTK3 installed."
            }
          },
          {
            "@type": "Question",
            "name": "Is NetAssistant free and open source?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Yes. NetAssistant is released under the Apache-2.0 license, all features are free, and the source code is hosted on GitHub."
            }
          },
          {
            "@type": "Question",
            "name": "How do I handle TCP sticky packets?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "NetAssistant ships four TCP decoders: raw data, line-delimited, length-prefix and JSON, which split the byte stream automatically according to your protocol format. See the TCP/UDP debugging guide for details."
            }
          }
        ]
      }
---

# FAQ

## Does the classic NetAssist network debugging assistant support macOS?

First, a quick disambiguation — these are **two different products** whose names differ only in the trailing "ant":

- **NetAssistant** (the tool documented on this site): open-source and cross-platform — **Windows, Linux and macOS are all supported**, with native x64 and ARM64 builds, so it runs on a Mac out of the box → [download](/en/download)
- **NetAssist** (the classic Windows-only tool): no macOS or Linux version

So for TCP/UDP debugging on a Mac or on Linux, just use **NetAssistant** — it works the same way as the debugging assistants you already know. See [Getting Started](/en/guide/) and the full [comparison table](/en/comparison).

## What is the difference between NetAssistant and NetAssist?

They are **different pieces of software** with similar names:

| | NetAssistant | NetAssist |
| ---- | ---- | ---- |
| Platforms | Windows / Linux / macOS (x64 + ARM64) | Windows only |
| Open source | ✅ Apache-2.0 | ❌ Closed source |
| TCP decoders (sticky packets) | Raw / line / length-prefix / JSON | Limited |
| High-concurrency stress testing | ✅ QPS + p50/p95/p99 | ❌ |

See the full [comparison table](/en/comparison).

## How do I install NetAssistant with winget?

On Windows, the recommended way is winget (with automatic upgrades):

```bash
winget install SunJary.NetAssistant
```

Upgrade later with:

```bash
winget upgrade SunJary.NetAssistant
```

More installation options on the [download page](/en/download).

## Is there a deb package for Linux?

Yes. **deb**, **AppImage** and **tar.gz** packages are provided (both x86_64 and aarch64):

- **deb** (recommended): for Debian/Ubuntu and derivatives — `sudo apt install ./netassistant-linux-x86_64.deb` resolves dependencies automatically; works on Ubuntu 22.04 and later
- **AppImage**: portable, works out of the box — great for Fedora/Arch and other distros; first run needs `sudo apt install libfuse2`
- **tar.gz**: extract and run, requires GTK3 (`sudo apt install libgtk-3-0`)

Download links on the [download page](/en/download#linux).

## Is NetAssistant free and open source?

Yes. NetAssistant is released under the **Apache-2.0** license with all features free. The source code is hosted on [GitHub](https://github.com/SunJary/NetAssistant) — contributions are welcome.

## How do I handle TCP sticky packets?

NetAssistant ships four TCP decoders that split the byte stream automatically, so you never count bytes by hand:

1. **Raw data**: displayed in arrival order
2. **Line-delimited**: split by newline
3. **Length-prefix**: split by the header length field
4. **JSON**: split per complete JSON object

See the [TCP/UDP debugging guide](/en/guide/tcp-udp) for usage.
