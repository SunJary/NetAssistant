---
layout: home

title: NetAssistant - TCP/UDP/MQTT Network Debugging Tool
titleTemplate: ':title'
description: "NetAssistant: Rust network debugging tool with TCP/UDP client and server, MQTT 3.1.1 / 5.0, four decoders, toolbox and TCP/UDP stress testing for Windows, Linux & macOS."

hero:
  name: NetAssistant
  text: High-Performance Cross-Platform Network Debugging Tool
  tagline: Built with Rust · Open-source alternative to NetAssist and MQTTX · TCP/UDP/MQTT Client & Server · Multiple Decoders · Built-in Toolbox & TCP/UDP Stress Testing · GPU-Accelerated UI
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
    details: Full support for TCP/UDP client and server modes plus an MQTT 3.1.1 / 5.0 client (TLS, per-subscription switches, QoS 0/1/2, last-will messages), with IPv4/IPv6 dual stack for any network environment.
  - icon: 🧰
    title: Built-in Toolbox
    details: Seven everyday groups — convert, checksum, hash, time, JSON, encoding and random — with instant results and one-click copy, no need to switch to external sites.
  - icon: 📦
    title: Smart Decoders
    details: Four TCP decoders — raw data, line-delimited, length-prefix and JSON — effectively solving sticky packet issues; send and receive in hex mode.
  - icon: 💬
    title: Chat-Style Message Log
    details: Intuitive display of packet exchanges, with favorites & remarks, keyword search, real-time logging and TXT/JSON/CSV export.
  - icon: 🚀
    title: TCP/UDP Stress Testing
    details: Built-in TCP/UDP stress engine with real-time QPS, latency percentiles (p50/p95/p99), failure breakdown, variable templates and CSV report export; MQTT stress testing is not supported.
  - icon: 🤖
    title: Automated Testing
    details: Auto-reply to simulate the peer, periodic send for long-run stability tests, and smart UDP broadcast reply display for device discovery.
  - icon: 🎨
    title: Modern Interface
    details: GPU-accelerated rendering powered by the GPUI framework at a smooth 60fps; dark mode follows the system automatically, with multi-tab connection management.
---

## Screenshots

### Client Mode

![Client mode screenshot](../assets/screenshots/en/screenshot_client.png)

### Server Mode

![Server mode screenshot](../assets/screenshots/en/screenshot_server.png)

### MQTT Client

![MQTT client screenshot](../assets/screenshots/en/screenshot_mqtt_client.png)

### Built-in Toolbox

![Toolbox hash screenshot](../assets/screenshots/en/screenshot_toolbox_hash.png)

### Stress Testing

![Stress testing screenshot](../assets/screenshots/en/screenshot_stress.png)

### Dark Mode

![UDP server dark mode screenshot](../assets/screenshots/en/screenshot_udp_server_dark.png)

## Why NetAssistant

- **Blazing performance**: Rust + Tokio async runtime, startup < 100ms, memory footprint < 20MB, million-level concurrent connections
- **Built for debugging**: from network app development to hardware and embedded debugging, covering the entire communication verification workflow
- **Cross-platform**: full support for Windows, Linux and macOS, with native builds for both x64 and ARM64
- **Open source and free**: released under the Apache-2.0 License, contributions welcome

For more details, see [Features](/features); to get started, read the [Guide](/guide/). To see how it compares with NetAssist and similar tools, check the [Comparison](/comparison).

## Download by platform

- [Windows network debugging tool](/windows-network-debug-tool): one-command winget install, Windows 10/11 on x64 and ARM64
- [Mac network debugging tool](/mac-network-debug-tool): universal DMG for Apple Silicon and Intel
- [Linux network debugging tool](/linux-network-debug-tool): deb, AppImage and tar.gz for x86_64 and ARM64

## Popular guides

- [NetAssist alternative: comparison and migration](/compare/netassist)
- [TCP debug tool: send, receive and framing](/tcp-debug-tool)
- [UDP debug tool: connectionless send and multi-device replies](/udp-debug-tool)
- [Socket debug tool: port listening and reachability](/socket-debug-tool)
- [Hex send: HEX payloads and text conversion](/hex-send)
- [Modbus TCP debugging: frames and CRC16](/modbus-tcp)
- [TCP sticky packets: causes, four solutions and debugging](/tcp-sticky-packet)
- [MQTT debugging guide: find pages by task](/mqtt)
- [MQTT client comparison: MQTTX, Explorer and MQTT.fx](/compare/)
- [MQTT client: a lightweight MQTT 3.1.1 / 5.0 debugging tool](/mqtt-client)
- [MQTTX alternative: lightweight MQTT client comparison](/comparison-mqttx)
- [MQTT tutorial: connect, subscribe and publish](/guide/mqtt)
- [MQTT troubleshooting: connections and missing messages](/mqtt-troubleshooting)
- [MQTT TLS: certificate verification and mutual auth](/mqtt-tls)
- [MQTT 5.0: session expiry and reason codes](/mqtt-5)
- [MQTT Explorer alternative comparison](/compare/mqtt-explorer)
- [MQTT.fx alternative comparison](/compare/mqtt-fx)
- [MQTT FAQ](/mqtt-faq)
- [Linux MQTT client installation](/mqtt-client-linux)
- [TCP/UDP stress testing: QPS and p99 latency](/tcp-stress-test)
- [UDP broadcast device discovery: LAN search and troubleshooting](/udp-broadcast-device-discovery)
