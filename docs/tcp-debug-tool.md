---
title: TCP Debug Tool - Send, Receive & Framing
description: TCP client and server modes, IPv4/IPv6, four framing decoders, hex payloads, periodic send and reply rules. Free for Windows, macOS and Linux.
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
            "description": "A lightweight TCP debug tool with client and server modes, four framing decoders, hex payloads, periodic send and automatic replies.",
            "url": "https://netassistant.trydo.top/tcp-debug-tool",
            "downloadUrl": "https://github.com/SunJary/NetAssistant/releases",
            "license": "https://www.apache.org/licenses/LICENSE-2.0",
            "offers": { "@type": "Offer", "price": "0", "priceCurrency": "USD" }
          },
          {
            "@type": "FAQPage",
            "mainEntity": [
              {
                "@type": "Question",
                "name": "What problems does a TCP debug tool solve?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Reachability, correct message framing and quick response validation—the core of most TCP integration work."
                }
              },
              {
                "@type": "Question",
                "name": "Why does received data arrive glued together?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "TCP is a byte stream with no message boundaries. Define boundaries with newlines, length prefixes or JSON and select the matching decoder."
                }
              },
              {
                "@type": "Question",
                "name": "Can it act as both client and server?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Yes, both modes are supported, and server mode can manage multiple client connections."
                }
              },
              {
                "@type": "Question",
                "name": "Does it support IPv6?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Yes, both client and server modes work with IPv4 and IPv6."
                }
              },
              {
                "@type": "Question",
                "name": "Does it support serial ports or Unix sockets?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "No. NetAssistant handles TCP/UDP/MQTT network connections only and is not a serial port tool."
                }
              }
            ]
          }
        ]
      }
---

# TCP Debug Tool: Connect, Frame and Respond

TCP debugging usually gets stuck in three places: the port will not connect, the received bytes look glued together, or a request gets no reply. NetAssistant puts all three in one window: client and server connections, four decoders, reply rules and periodic send.

> [Download NetAssistant](/download) · [TCP/UDP debugging guide](/guide/tcp-udp) · [All features](/features)

![TCP client send and receive](../assets/screenshots/en/screenshot_client.png)

## Who it is for

- **Embedded and IoT developers**: debug TCP devices, gateways and DTUs
- **Backend developers**: send a raw request to an in-house service without writing a throwaway script
- **Protocol integrators**: inspect frames byte by byte in hex
- **Anyone simulating a server**: listen on a local port and wait for devices to connect

## Three connection modes

| Mode | Purpose | Typical use |
| ---- | ---- | ---- |
| TCP client | Connect to a remote address and port | Devices, service endpoints |
| TCP server | Listen on a local port and accept connections | Simulate a platform or device service |
| Local bind | Choose the source address or NIC | Multi-NIC hosts, source IP checks |

Both IPv4 and IPv6 are supported. Server mode keeps multiple client connections, each with its own send and receive view.

## Fixing sticky packets: four decoders

TCP does not preserve message boundaries. The decoder decides when a message is complete. NetAssistant offers raw, line, length-prefix and JSON decoders for custom binary, text protocols, TLV and JSON over TCP.

Picking the wrong decoder is the most common reason messages look stuck together. Decoders only affect display and splitting; they do not change the bytes sent. See [TCP sticky packets](/tcp-sticky-packet) for boundary rules, trade-offs and troubleshooting.

![TCP decoder settings](../assets/screenshots/en/screenshot_tcp_decoder.png)

## Sending and automatic replies

- **Text / hex modes**: text is sent as UTF-8; hex sends raw bytes, which matters for frames containing `0x00` or non-printable data
- **Line ending**: inherit, none, LF or CRLF, so you do not have to append newlines by hand
- **Periodic send**: repeat at a fixed interval for heartbeats or simulated device reports
- **Reply rules**: match incoming messages by bytes, JSON, regex or checksum and reply automatically to emulate a server
- **File source**: load a payload from a local file with UTF-8 / GBK / ANSI decoding, up to 1 MiB

![Automatic reply rule](../assets/screenshots/en/screenshot_reply_rule.png)

## Quick start

1. Create a connection and choose TCP client or server
2. Enter the target address and port, or the local listening port
3. After connecting, pick the decoder that matches your protocol
4. Type a payload and choose text or hex mode
5. Add a reply rule for automatic responses, or enable periodic send
6. Use search to find a message or export the log as TXT / JSON / CSV

## When to stress test

Once functional checks pass, use the built-in [TCP/UDP stress testing](/tcp-stress-test) to measure QPS and p99 latency instead of installing JMeter or wrk.

## FAQ

### What problems does a TCP debug tool solve?

Reachability, correct message framing and quick response validation—the core of most TCP integration work.

### Why does received data arrive glued together?

TCP is a byte stream with no message boundaries. Define boundaries with newlines, length prefixes or JSON and select the matching decoder.

### Can it act as both client and server?

Yes, both modes are supported, and server mode can manage multiple client connections.

### Does it support IPv6?

Yes, both client and server modes work with IPv4 and IPv6.

### Does it support serial ports or Unix sockets?

No. NetAssistant handles TCP/UDP/MQTT network connections only and is not a serial port tool.

## Related pages

- [TCP sticky packets](/tcp-sticky-packet)
- [TCP/UDP stress testing](/tcp-stress-test)
- [Socket debug tool](/socket-debug-tool)
- [UDP debug tool](/udp-debug-tool)
- [Windows network debugging tool](/windows-network-debug-tool)
- [All features](/features)
