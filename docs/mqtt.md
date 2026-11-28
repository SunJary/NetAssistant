---
title: "MQTT Debugging Guide: Tutorial, TLS, 5.0 and Troubleshooting"
description: "MQTT debugging guide for NetAssistant: connect to a broker, subscribe and publish, configure TLS, test MQTT 5.0 sessions and QoS/retain/last will, and troubleshoot failures."
---

# MQTT Debugging Guide: From Connecting to Troubleshooting

This page ties the MQTT content together by task. If you are new, start with Quick start; if you can already connect but messages or certificates are giving you trouble, go straight to troubleshooting and TLS; if you are choosing a tool, see the comparison overview.

## Quick start

1. [Download and install NetAssistant](/download), available for Windows, macOS and Linux
2. Follow the [MQTT tutorial](/guide/mqtt) to create a connection, subscribe and publish your first message
3. If you cannot connect or receive messages, check [MQTT troubleshooting](/mqtt-troubleshooting)
4. For encrypted connections, choose a certificate verification mode in [MQTT TLS configuration](/mqtt-tls)

## Find a page by task

| What you want to do | Go to |
| ---- | ---- |
| Understand what the client supports | [MQTT client](/mqtt-client) |
| Connect to a broker, subscribe and publish for the first time | [MQTT tutorial](/guide/mqtt) |
| Configure TLS, self-signed certificates, mutual authentication | [MQTT TLS](/mqtt-tls) |
| Use session expiry, receive maximum and reason codes | [MQTT 5.0](/mqtt-5) |
| Verify QoS, retain, last will, wildcards | [MQTT testing scenarios](/mqtt-testing) |
| Connection refused, subscription failed, no messages arriving | [MQTT troubleshooting](/mqtt-troubleshooting) |
| Install on Windows / macOS / Linux | [Windows](/mqtt-client-windows), [macOS](/mqtt-client-mac), [Linux](/mqtt-client-linux) |
| Compare with MQTTX, Explorer, MQTT.fx | [MQTT client comparison](/compare/) |
| Quickly check ports, Client ID, QoS and similar questions | [MQTT FAQ](/mqtt-faq) |

## Capabilities at a glance

- Protocol versions: MQTT 3.1.1 and 5.0, switchable in the same connection config
- Transport: direct TCP; TLS supports system roots, a custom CA, skipped verification and mutual TLS
- Send and receive: QoS 0/1/2, retain, last-will messages, wildcard subscriptions, per-subscription switches
- 5.0: Clean Start, session expiry, receive maximum, connect/subscribe/publish reason codes
- Debugging: message search, a session topic tree that filters by exact topic or prefix, favorites, TXT / JSON / CSV export, auto-reply, periodic send
- Boundaries: no MQTT over WebSocket, no scripting, no shared subscriptions, no MQTT stress testing

For the full settings, see [MQTT client](/mqtt-client).

## Platform installation

| Platform | Installation method |
| ---- | ---- |
| Windows | winget, installer or portable zip, see [Windows version](/mqtt-client-windows) |
| macOS | Universal DMG or zip, see [macOS version](/mqtt-client-mac) |
| Linux | deb, AppImage, tar.gz, see [Linux version](/mqtt-client-linux) |

## Choosing and comparing

- [MQTT client comparison overview](/compare/): MQTTX vs MQTT Explorer vs MQTT.fx and NetAssistant, and where each one fits
- [MQTTX comparison](/comparison-mqttx): the differences in WebSocket, scripting and full 5.0
- [MQTT Explorer comparison](/compare/mqtt-explorer): session topic tree or broker-wide topic tree
- [MQTT.fx comparison](/compare/mqtt-fx): commercial licensing or free and open source

## Suggested reading order

- New users: tutorial → client → troubleshooting
- Platform/backend: 5.0 → testing scenarios → troubleshooting
- Field/hardware: Windows or Linux install page → tutorial → TLS
- Choosing a tool: comparison overview → the matching competitor comparison → client

## Related pages

- [MQTT client](/mqtt-client)
- [MQTT tutorial](/guide/mqtt)
- [MQTT FAQ](/mqtt-faq)
- [Download NetAssistant](/download)
