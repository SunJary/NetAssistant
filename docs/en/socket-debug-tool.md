---
title: Socket Debug Tool - TCP/UDP Port Testing
description: Create TCP/UDP client and server sockets, bind local addresses, listen on ports and exchange raw bytes with IPv4/IPv6 and hex support. Free, cross-platform.
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
            "description": "A socket debug tool for TCP/UDP client and server endpoints, local address binding, port listening, IPv4/IPv6 and hex payloads.",
            "url": "https://netassistant.trydo.top/en/socket-debug-tool",
            "downloadUrl": "https://github.com/SunJary/NetAssistant/releases",
            "license": "https://www.apache.org/licenses/LICENSE-2.0",
            "offers": { "@type": "Offer", "price": "0", "priceCurrency": "USD" }
          },
          {
            "@type": "BreadcrumbList",
            "itemListElement": [
              { "@type": "ListItem", "position": 1, "name": "Home", "item": "https://netassistant.trydo.top/en/" },
              { "@type": "ListItem", "position": 2, "name": "Features", "item": "https://netassistant.trydo.top/en/features" },
              { "@type": "ListItem", "position": 3, "name": "Socket debug tool", "item": "https://netassistant.trydo.top/en/socket-debug-tool" }
            ]
          },
          {
            "@type": "FAQPage",
            "mainEntity": [
              {
                "@type": "Question",
                "name": "What is a socket debug tool for?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "It works directly with TCP/UDP socket endpoints: connect to a remote port as a client or listen on a local port as a server, exchange raw bytes and inspect connection state without involving the business application."
                }
              },
              {
                "@type": "Question",
                "name": "Can it debug serial ports or Unix domain sockets?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "No. NetAssistant supports TCP/UDP/MQTT network connections only. It has no serial port, COM port or Unix domain socket support."
                }
              },
              {
                "@type": "Question",
                "name": "What if listening on a port fails?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Check whether another process owns the port, whether a port below 1024 needs elevated privileges, whether the bound address belongs to a local interface, and whether a firewall or security tool blocks listening. Trying a high port quickly rules out permission issues."
                }
              },
              {
                "@type": "Question",
                "name": "Can it test whether a port is reachable?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Yes. Connect with a TCP client. Success means the handshake completed and the port is reachable. Connection refused usually means nothing is listening; a timeout points to a firewall or routing problem."
                }
              }
            ]
          }
        ]
      }
---

# Socket Debug Tool: From Port to Raw Bytes

Socket debugging means working directly with TCP/UDP endpoints: connect to a port or listen on one yourself, then check whether the connection is established and whether bytes flow. NetAssistant provides exactly that—TCP/UDP client and server, local address binding, IPv4/IPv6 and hex payloads.

> [Download NetAssistant](/en/download) · [TCP/UDP debugging guide](/en/guide/tcp-udp) · [All features](/en/features)

![TCP server listening for multiple clients](../../assets/screenshots/en/screenshot_server.png)

## Who it is for

- **Reachability checks**: confirm a service is listening and a firewall allows it
- **Client/server simulation**: validate the path before real devices or services exist
- **Multi-NIC hosts**: bind a local address and verify the source IP used
- **Protocol integration**: exchange raw bytes without framework noise

## What the tool covers

| Capability | Description |
| ---- | ---- |
| TCP client | Connect to `ip:port` and verify handshake and traffic |
| TCP server | Listen on a local port and manage multiple connections |
| UDP client | Send datagrams to a target without connecting |
| UDP server | Listen locally and show the source address of each datagram |
| Local bind | Choose the source or listening address for multi-NIC hosts |
| IPv4 / IPv6 | Both address families are supported |

## Common troubleshooting flows

### The port will not connect

1. Connect with a TCP client to the target `ip:port`
2. Connection refused: the host is reachable but nothing is listening
3. Timeout: more likely a firewall drop, routing issue or wrong address
4. Start a local TCP server on the same port to rule out the tool itself

### Listening fails

- Another process already owns the port; test with a high port
- Ports below 1024 need elevated privileges on some systems
- The bound address does not belong to a local interface
- Security software blocks the listen operation

### Data arrives but is unreadable

Switch to hex mode to inspect raw bytes. For inbound pushes, remember that TCP is a byte stream and may need [framing decoders](/en/tcp-sticky-packet).

![Creating a socket connection](../../assets/screenshots/en/screenshot_new_client.png)

## Not a serial port tool

A socket debug tool works with network endpoints; a serial assistant works with COM ports, baud rates and serial frames. They solve problems at different layers. NetAssistant does **not** debug serial ports or Unix domain sockets. For RS-232/RS-485, AT commands over serial or local inter-process sockets, use a serial tool or the operating system utilities.

## Quick start

1. Create a connection and choose TCP or UDP, client or server
2. Enter the target address and port, or the listening port
3. On multi-NIC hosts, optionally bind a local address
4. After connecting or listening, type a payload and choose text or hex
5. Watch messages and connection state, and export the log if needed

## FAQ

### What is a socket debug tool for?

Working directly with TCP/UDP endpoints to connect, listen, exchange raw bytes and inspect connection state.

### Can it debug serial ports or Unix domain sockets?

No. Only TCP/UDP/MQTT network connections are supported.

### What if listening on a port fails?

Check port ownership, privileges, bound address and firewall rules; a high port rules out permissions quickly.

### Can it test whether a port is reachable?

Yes. A successful TCP connection means the port is reachable; refused usually means no listener, timeout suggests a firewall or route issue.

## Related pages

- [TCP debug tool](/en/tcp-debug-tool)
- [UDP debug tool](/en/udp-debug-tool)
- [TCP sticky packets](/en/tcp-sticky-packet)
- [Windows network debugging tool](/en/windows-network-debug-tool)
- [All features](/en/features)
- [Download NetAssistant](/en/download)
