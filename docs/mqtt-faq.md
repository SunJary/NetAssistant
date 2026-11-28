---
title: MQTT FAQ - Client, Ports, TLS and Subscription Answers
description: "MQTT client FAQ: choosing a client, the MQTT default port (1883 or 8883), client IDs, TLS, retain, QoS, and why you get no messages after subscribing."
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
            "name": "Which MQTT client is best?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Choose MQTTX when you need WebSocket, scripting and full 5.0 features; MQTT Explorer when you need a broker-wide topic tree with retained values and cleanup; MQTT.fx when you already hold a commercial license; NetAssistant when you want a free, lightweight client that keeps TCP/UDP and MQTT debugging in one app."
            }
          },
          {
            "@type": "Question",
            "name": "What is the MQTT default port?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Plain MQTT uses 1883 by default and TLS uses 8883; WebSocket commonly uses 8083/8084. NetAssistant only connects over TCP, so WebSocket is not supported."
            }
          },
          {
            "@type": "Question",
            "name": "Can a client ID be reused?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "A client ID must be unique on the same broker. When it is reused, the newer connection kicks the previous client offline; duplicating a connection in NetAssistant generates a new client ID automatically."
            }
          },
          {
            "@type": "Question",
            "name": "MQTT 3.1.1 or 5.0?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Use 3.1.1 for older devices, older brokers or basic publish and subscribe; use 5.0 when you need session expiry, receive maximum or more specific failure reason codes. NetAssistant defaults to 5.0."
            }
          },
          {
            "@type": "Question",
            "name": "How do I configure TLS for MQTT?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Set the port to 8883 and enable TLS; use system roots for a public CA, a custom CA for a private one, and skipped verification only for temporary self-signed testing. Add a client certificate and private key when mutual TLS is required."
            }
          },
          {
            "@type": "Question",
            "name": "Why do I get no messages after subscribing?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Check the subscription switch, whether the topic matches the filter, retain, QoS, the publish-side ACL and session settings, then cross-check with mosquitto_sub or another client."
            }
          },
          {
            "@type": "Question",
            "name": "How do I clear a retained message?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Publish an empty payload to the same topic with retain enabled and the broker deletes the retained message."
            }
          },
          {
            "@type": "Question",
            "name": "Why are QoS 1 messages duplicated?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "QoS 1 is at-least-once, so retransmission or reconnects can deliver duplicates and the application must deduplicate; only QoS 2 guarantees exactly once."
            }
          },
          {
            "@type": "Question",
            "name": "When is the last-will message sent?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Only on an abnormal disconnect such as a network drop or a keep-alive timeout, when the broker publishes it; an intentional disconnect does not trigger it."
            }
          },
          {
            "@type": "Question",
            "name": "Does NetAssistant support MQTT stress testing?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "MQTT stress testing is not supported yet; the stress-testing engine only covers TCP/UDP. Use MQTTX Bench or emqtt_bench for MQTT load tests."
            }
          }
        ]
      }
---

# MQTT FAQ: Client Choice, Connections and Subscriptions

This page collects the questions that come up most often when choosing a client, connecting, and subscribing or publishing, with a short answer and a pointer to the page that covers it in depth. Step-by-step instructions live on the topic pages; this page does not repeat long tutorials.

## Choosing a client

### What is an MQTT client?

An MQTT client is a tool that connects to a broker, subscribes to topics, publishes messages and shows the traffic. Use it to verify device reporting and platform commands during device integration, or to reproduce an issue during platform development.

### Which MQTT client is best?

| Requirement | Recommendation |
| ---- | ---- |
| WebSocket, scripting, full 5.0 | MQTTX |
| Broker-wide topic tree, retained cleanup, numeric charts | MQTT Explorer |
| A commercial license already purchased, full 5.0 toolchain | MQTT.fx |
| Free and open source, lightweight, message stream + session topic tree, TCP/UDP and MQTT in one app | NetAssistant |

No single tool covers every scenario. For everyday publish and subscribe, a lightweight client saves resources; NetAssistant's Topics view filters the stream by exact topic or prefix, while mapping a broker's retained structure is still more direct in MQTT Explorer.

### Is NetAssistant free?

Yes, free and open source under Apache-2.0, with no trial period and no license management.

### Which platforms are supported?

Windows 10/11, Ubuntu 22.04 and later, and macOS 10.15 and later, covering both x64 and ARM64.

## Connection questions

### What is the MQTT default port?

Plain MQTT uses 1883, TLS uses 8883 and WebSocket commonly uses 8083/8084. NetAssistant only supports direct TCP connections, so `ws://` and `wss://` addresses cannot connect.

### Can a client ID be reused?

No: a client ID must be unique on the same broker. When it is reused, the newer connection kicks the previous client offline; duplicating a connection generates a new ID automatically.

### MQTT 3.1.1 or 5.0?

Use 3.1.1 for older devices, older brokers or basic publish and subscribe; use 5.0 when you need session expiry, receive maximum and more specific reason codes. NetAssistant defaults to 5.0.

### How do I configure TLS for MQTT?

Set the port to 8883 and enable TLS, then choose system roots, a custom CA or skipped verification depending on where the certificate comes from; add a client certificate and private key when mutual TLS is required. See [MQTT TLS configuration](/mqtt-tls).

### What if the connection is refused?

First confirm the port is reachable, the address and TLS switch match, the client ID is unique and the account has permission, then use the CONNACK return code or the 5.0 reason code to pinpoint the cause. See [MQTT troubleshooting](/mqtt-troubleshooting#connection-refused-or-never-connects).

## Subscribing and publishing

### Why do I get no messages after subscribing?

An MQTT subscribe can succeed and still deliver nothing. Check in order: the subscription switch, whether the publish topic matches the filter, retain, QoS, the publish-side ACL and session settings, then cross-check with `mosquitto_sub` or another client.

### How do I clear a retained message?

Publish an empty payload to the same topic with retain enabled; the broker then deletes the retained message.

### Why are QoS 1 messages duplicated?

QoS 1 is at-least-once, so retransmission or reconnects can deliver duplicates and the application has to deduplicate; only QoS 2 guarantees exactly once.

### When is the last-will message sent?

Only on an abnormal disconnect, for example a network drop, a killed process or a keep-alive timeout; an intentional disconnect does not trigger it.

### How do I receive offline messages?

Use a fixed client ID and keep a persistent session: turn Clean Session off for 3.1.1, or turn Clean Start off with a session expiry greater than 0 for 5.0, and the broker keeps the offline messages.

### Does NetAssistant support MQTT stress testing?

Not yet. NetAssistant's stress-testing engine only covers TCP/UDP; use MQTTX Bench or `emqtt_bench` for MQTT load tests.

## Next steps

- Connect to a broker from scratch: [MQTT tutorial](/guide/mqtt)
- Full capabilities and settings: [MQTT client](/mqtt-client)
- TLS and mutual authentication: [MQTT TLS](/mqtt-tls)
- 5.0 sessions and reason codes: [MQTT 5.0](/mqtt-5)
- Test scenarios: [QoS, retain and last-will checks](/mqtt-testing)
- Platform installs: [Windows](/mqtt-client-windows), [macOS](/mqtt-client-mac), [Linux](/mqtt-client-linux)
- Differences from MQTT.fx: [MQTT.fx comparison](/compare/mqtt-fx)
- All choices: [MQTT client comparison overview](/compare/)
- [Download NetAssistant](/download)
