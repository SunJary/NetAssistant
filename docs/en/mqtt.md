---
title: "MQTT Debugging Guide: Tutorial, TLS, 5.0 and Troubleshooting"
description: "MQTT debugging guide for NetAssistant: connect to a broker, subscribe and publish, configure TLS, test MQTT 5.0 sessions and QoS/retain/last will, and troubleshoot failures."
---

# MQTT Debugging Guide: From Connecting to Troubleshooting

This page ties the MQTT content together by task. If you are new, start with Quick start; if you can already connect but messages or certificates are giving you trouble, go straight to troubleshooting and TLS; if you are choosing a tool, see the comparison overview.

## Quick start

1. [Download and install NetAssistant](/en/download), available for Windows, macOS and Linux
2. Follow the [MQTT tutorial](/en/guide/mqtt) to create a connection, subscribe and publish your first message
3. If you cannot connect or receive messages, check [MQTT troubleshooting](/en/mqtt-troubleshooting)
4. For encrypted connections, choose a certificate verification mode in [MQTT TLS configuration](/en/mqtt-tls)

## Find a page by task

| What you want to do | Go to |
| ---- | ---- |
| Understand what the client supports | [MQTT client](/en/mqtt-client) |
| Connect to a broker, subscribe and publish for the first time | [MQTT tutorial](/en/guide/mqtt) |
| Configure TLS, self-signed certificates, mutual authentication | [MQTT TLS](/en/mqtt-tls) |
| Use session expiry, receive maximum and reason codes | [MQTT 5.0](/en/mqtt-5) |
| Verify QoS, retain, last will, wildcards | [MQTT testing scenarios](/en/mqtt-testing) |
| Connection refused, subscription failed, no messages arriving | [MQTT troubleshooting](/en/mqtt-troubleshooting) |
| Install on Windows / macOS / Linux | [Windows](/en/mqtt-client-windows), [macOS](/en/mqtt-client-mac), [Linux](/en/mqtt-client-linux) |
| Compare with MQTTX, Explorer, MQTT.fx | [MQTT client comparison](/en/compare/) |
| Quickly check ports, Client ID, QoS and similar questions | [MQTT FAQ](/en/mqtt-faq) |

## Capabilities at a glance

- Protocol versions: MQTT 3.1.1 and 5.0, switchable in the same connection config
- Transport: direct TCP; TLS supports system roots, a custom CA, skipped verification and mutual TLS
- Send and receive: QoS 0/1/2, retain, last-will messages, wildcard subscriptions, per-subscription switches
- 5.0: Clean Start, session expiry, receive maximum, connect/subscribe/publish reason codes
- Debugging: message search, favorites, TXT / JSON / CSV export, auto-reply, periodic send
- Boundaries: no MQTT over WebSocket, no scripting, no shared subscriptions, no MQTT stress testing

For the full settings, see [MQTT client](/en/mqtt-client).

## Platform installation

| Platform | Installation method |
| ---- | ---- |
| Windows | winget, installer or portable zip, see [Windows version](/en/mqtt-client-windows) |
| macOS | Universal DMG or zip, see [macOS version](/en/mqtt-client-mac) |
| Linux | deb, AppImage, tar.gz, see [Linux version](/en/mqtt-client-linux) |

## Choosing and comparing

- [MQTT client comparison overview](/en/compare/): MQTTX vs MQTT Explorer vs MQTT.fx and NetAssistant, and where each one fits
- [MQTTX comparison](/en/comparison-mqttx): the differences in WebSocket, scripting and full 5.0
- [MQTT Explorer comparison](/en/compare/mqtt-explorer): topic tree or message stream
- [MQTT.fx comparison](/en/compare/mqtt-fx): commercial licensing or free and open source

## Suggested reading order

- New users: tutorial → client → troubleshooting
- Platform/backend: 5.0 → testing scenarios → troubleshooting
- Field/hardware: Windows or Linux install page → tutorial → TLS
- Choosing a tool: comparison overview → the matching competitor comparison → client

## Related pages

- [MQTT client](/en/mqtt-client)
- [MQTT tutorial](/en/guide/mqtt)
- [MQTT FAQ](/en/mqtt-faq)
- [Download NetAssistant](/en/download)
