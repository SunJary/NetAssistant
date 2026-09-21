---
layout: home

title: NetAssistant - Open Source Cross-Platform Network Debugging Tool
description: "NetAssistant is an open-source cross-platform network debugging tool built with Rust: TCP/UDP client & server, four decoders, message management and high-concurrency stress testing, for Windows, Linux and macOS with native x64 and ARM64 builds."

hero:
  name: NetAssistant
  text: High-Performance Cross-Platform Network Debugging Tool
  tagline: Built with Rust · TCP/UDP Client & Server · Multiple Decoders · Built-in Stress Testing · GPU-Accelerated UI
  actions:
    - theme: brand
      text: Download Now
      link: /en/download
    - theme: alt
      text: User Guide
      link: /en/guide/
    - theme: alt
      text: GitHub
      link: https://github.com/SunJary/NetAssistant

features:
  - icon: 🔌
    title: Multi-Protocol Support
    details: Full support for TCP/UDP client and server modes, IPv4/IPv6 dual stack, adapting to any network environment.
  - icon: 📦
    title: Smart Decoders
    details: Four TCP decoders — raw data, line-delimited, length-prefix and JSON — effectively solving sticky packet issues; send and receive in hex mode.
  - icon: 💬
    title: Chat-Style Message Log
    details: Intuitive display of packet exchanges, with favorites & remarks, keyword search, real-time logging and TXT/JSON/CSV export.
  - icon: 🚀
    title: High-Concurrency Stress Testing
    details: Built-in stress engine with real-time QPS, latency percentiles (p50/p95/p99), failure breakdown, variable templates and CSV report export.
  - icon: 🤖
    title: Automated Testing
    details: Auto-reply to simulate the peer, periodic send for long-run stability tests, and smart UDP broadcast reply display for device discovery.
  - icon: 🎨
    title: Modern Interface
    details: GPU-accelerated rendering powered by the GPUI framework at a smooth 60fps; dark mode follows the system automatically, with multi-tab connection management.
---

## Screenshots

### Client Mode

![Client mode screenshot](../../assets/screenshots/en/screenshot_client.png)

### Server Mode

![Server mode screenshot](../../assets/screenshots/en/screenshot_server.png)

### Stress Testing

![Stress testing screenshot](../../assets/screenshots/en/screenshot_stress.png)

### Dark Mode

![UDP server dark mode screenshot](../../assets/screenshots/en/screenshot_udp_server_dark.png)

## Why NetAssistant

- **Blazing performance**: Rust + Tokio async runtime, startup < 100ms, memory footprint < 20MB, million-level concurrent connections
- **Built for debugging**: from network app development to hardware and embedded debugging, covering the entire communication verification workflow
- **Cross-platform**: full support for Windows, Linux and macOS, with native builds for both x64 and ARM64
- **Open source and free**: released under the Apache-2.0 License, contributions welcome

For more details, see [Features](/en/features); to get started, read the [Guide](/en/guide/). To see how it compares with NetAssist and similar tools, check the [Comparison](/en/comparison).
