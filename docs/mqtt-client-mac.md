---
title: MQTT Client for Mac - Apple Silicon Install
description: "Install the NetAssistant MQTT client on Mac: a universal DMG or zip that runs natively on Apple Silicon and Intel, with MQTT 3.1.1/5.0, TLS and mutual auth."
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
            "name": "How do I install an MQTT client on macOS?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Get netassistant-macos-universal.dmg from the download page, open it and drag NetAssistant into Applications; you can also download the zip and run it after extracting."
            }
          },
          {
            "@type": "Question",
            "name": "Does it support Apple Silicon?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Yes. The universal build runs natively on M1/M2/M3 and other Apple Silicon Macs as well as Intel Macs, with no Rosetta required."
            }
          },
          {
            "@type": "Question",
            "name": "What if macOS says the developer cannot be verified?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "This is the normal prompt for a non-notarized app, not a corrupted file. Right-click NetAssistant and choose Open, or pick Open Anyway under System Settings → Privacy & Security."
            }
          },
          {
            "@type": "Question",
            "name": "Can I install it with Homebrew?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "A universal DMG and a zip are provided today; there is no Homebrew Cask yet. Once installed, launch and upgrade it as usual."
            }
          },
          {
            "@type": "Question",
            "name": "Does the Mac version support MQTT TLS?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Yes. Certificate verification can use system roots, a custom CA or be skipped, and a client certificate and private key (PEM) enable mutual TLS."
            }
          }
        ]
      }
---

# MQTT Client for Mac: Apple Silicon Install and Debugging

Native macOS tools that handle both TCP/UDP and MQTT debugging are rare: the classic serial and network debugging assistants have no Mac version, and most MQTT clients are Electron apps. NetAssistant is built with Rust + GPUI and ships a universal installer, so Apple Silicon and Intel both run natively, using about 20MB of memory.

## Why choose it on Mac

- **Native, not a wrapper**: fast startup, low memory and no fan noise when left running
- **Universal build**: one DMG covers M1/M2/M3 and Intel, with no Rosetta
- **Complete protocol set**: TCP/UDP client and server plus MQTT 3.1.1 / 5.0
- **Complete TLS**: system roots, custom CA or skipped verification, with mTLS support
- **Convenient debugging**: per-subscription switches, QoS 0/1/2, retain, last-will, auto-reply, search and export

## System requirements

- macOS 10.15 or later
- Apple Silicon or Intel
- Access to the broker on port 1883 (plain) or 8883 (TLS)

## Installation

### Recommended: universal DMG

1. Get `netassistant-macos-universal.dmg` from the [download page](/download)
2. Open the DMG and drag NetAssistant into Applications
3. If the first launch says the developer cannot be verified, right-click NetAssistant → Open, or choose "Open Anyway" under "System Settings → Privacy & Security"

This is normal for an app that has not been notarized, and you do not need to disable Gatekeeper.

### Alternative: zip

Download `netassistant-macos-universal.zip`, extract it and run the app directly; the executable permission is preserved.

## First MQTT connection

1. Open NetAssistant, click `+ New` and choose **MQTT** as the protocol
2. Enter the broker address, port, client ID, username and password
3. Turn on TLS if encryption is needed and pick the verification mode that matches where the certificate comes from
4. After connecting, subscribe to `test/#` or a business topic
5. In the send area, fill in topic, QoS, retain and payload, then publish

See the [MQTT debugging tutorial](/guide/mqtt) for the full walkthrough.

## Configuring TLS on macOS

- Certificates and private keys use PEM file paths, supporting PKCS#8, PKCS#1 and SEC1
- For a private CA, choose "Custom CA" and point it at the CA's PEM file; for a public CA, keep the default system roots
- Mutual TLS requires the client certificate and private key to be filled in as a pair; the configuration saves only the paths, never the key content
- Skipping verification is useful for local testing with self-signed certificates, but do not do it in production

For handshake failures, see [MQTT troubleshooting](/mqtt-troubleshooting#tls-handshake-failures).

## Scenarios suited to Mac developers

- Run EMQX / Mosquitto in local Docker and connect to `127.0.0.1` right from the Mac
- Keep test, staging and production connections open at once and switch between tabs
- Debug alongside proprietary TCP/UDP protocols without copying data between two apps
- Debug from a laptop while travelling or on site, where low memory keeps other tools responsive

If you care more about TCP/UDP debugging on macOS, see the [Mac network debugging tool](/mac-network-debug-tool).

## FAQ

### How do I install an MQTT client on macOS?

Download the universal DMG and drag it into Applications, or use the zip and run it after extracting.

### Does it support Apple Silicon?

Yes, the universal build runs natively on both Apple Silicon and Intel with no Rosetta.

### What if macOS says the developer cannot be verified?

Right-click to open, or choose "Open Anyway" under Privacy & Security; it is the normal prompt for a non-notarized app.

### Can I install it with Homebrew?

A DMG and a zip are available today; there is no Homebrew Cask yet.

### Does the Mac version support MQTT TLS?

Yes, with three certificate verification modes and mutual TLS.

## Related pages

- Features and parameters: [MQTT client](/mqtt-client)
- Connect and subscribe tutorial: [MQTT debugging tutorial](/guide/mqtt)
- Diagnosing connection failures: [MQTT troubleshooting](/mqtt-troubleshooting)
- Other platforms: [Windows client](/mqtt-client-windows), [Linux client](/mqtt-client-linux)
- FAQ: [MQTT FAQ](/mqtt-faq)
- [Download for macOS](/download)
