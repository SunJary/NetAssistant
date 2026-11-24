---
title: "MQTT 5.0 Client: Session Expiry, Reason Codes vs 3.1.1"
description: "What NetAssistant covers in MQTT 5.0: clean start, session expiry, receive maximum and reason codes, how it differs from 3.1.1, and what is missing."
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
            "name": "Does NetAssistant support the full MQTT 5.0 specification?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "It covers the basics: protocol version switching, clean start, session expiry, receive maximum, and reason codes on connect, subscribe, publish and disconnect. Advanced features such as user properties, Will Delay, subscription identifiers and shared subscriptions are not provided."
            }
          },
          {
            "@type": "Question",
            "name": "Should I choose 5.0 or 3.1.1?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Choose 5.0 when the broker supports it and you need session expiry or more specific reason codes; choose 3.1.1 for older devices and brokers or plain publish/subscribe, where behavior is simpler."
            }
          },
          {
            "@type": "Question",
            "name": "What value should session expiry have?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "0 expires the session on disconnect; a positive number of seconds makes the broker keep the session and offline messages for that long. For offline message testing, turn clean start off and set a positive expiry."
            }
          },
          {
            "@type": "Question",
            "name": "What is receive maximum for?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "It limits how many QoS 1/2 messages the broker may deliver concurrently, keeping devices or clients from being overwhelmed by in-flight messages. Leave it empty to omit the property and let the broker use its default."
            }
          },
          {
            "@type": "Question",
            "name": "Where can I see the MQTT 5.0 subscribe failure reason code?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "The subscription switch waits for broker confirmation; on failure, notifications and logs show the SUBACK reason code, such as 0x87 not authorized, 0x8F invalid filter or 0x97 quota exceeded."
            }
          }
        ]
      }
---

# MQTT 5.0 Client: Sessions, Reason Codes and 3.1.1 Differences

MQTT 5.0 is not just a bigger version number: it writes the reasons for refused connections, failed subscriptions and rejected publishes into the protocol, and adds session expiry and flow-control properties. NetAssistant's 5.0 implementation focuses on what debugging needs most — readable failure reasons and controllable offline sessions — without sacrificing its lightweight footprint.

## What 5.0 solves

- **Readable failure reasons**: the broker returns a specific reason code when it refuses a connection, subscription or publish, instead of a bare "failed"
- **Controllable sessions**: session expiry decides how long the broker keeps the session and offline messages after a disconnect
- **Tunable flow control**: receive maximum caps how many QoS 1/2 messages may be in flight at once
- **Switchable versions**: toggle the same connection configuration between 3.1.1 and 5.0 to compare broker behavior

## Supported capabilities

| Capability | NetAssistant 5.0 | Notes |
| ---- | ---- | ---- |
| Clean Start | Supported | Reuses the Clean Session switch in the connection form; clean start is forced when Client ID is empty |
| Session expiry | Supported | In seconds; omitted when empty; 0 expires on disconnect |
| Receive maximum | Supported | 1–65535; omitted when empty |
| Connect reason codes | Supported | Shows the reason code and broker reason string when CONNACK is refused |
| Subscribe reason codes | Supported | Shows the exact reason code when SUBACK fails |
| Publish reason codes | Supported | Failure reasons in PUBACK / PUBREC for QoS 1/2 |
| Disconnect reasons | Supported | Shows the reason when the broker sends DISCONNECT |
| QoS / retain / last will | Supported | Same behavior as 3.1.1 |

## Advanced features not provided

The current version does not offer the following; use MQTTX or another complete implementation when you need them:

- User properties
- Will Delay
- Subscription identifiers
- Shared subscriptions (`$share/...`)
- Other advanced 5.0 features such as topic aliases

## Differences from 3.1.1

| Dimension | MQTT 3.1.1 | MQTT 5.0 |
| ---- | ---- | ---- |
| Connection refused | CONNACK return code 1–5 | Reason code, e.g. `0x86` bad credentials, `0x87` not authorized |
| Subscription failure | Rejection only | SUBACK reason code that distinguishes permissions, filter and quota |
| Publish failure | PUBACK carries no reason code | May carry a reason code to pinpoint ACL and topic issues |
| Session expiry | Not available | 0 or a positive number of seconds |
| Receive maximum | Not available | 1–65535, optional |
| Broker-initiated disconnect | Usually a plain disconnect | May carry a reason code and reason string |

## Clean Start and session expiry

- **Clean Start on**: every connection creates a new session, and the broker discards the old session and its offline messages
- **Clean Start off + session expiry 0**: state is kept while connected and cleared on disconnect
- **Clean Start off + session expiry > 0**: the broker keeps subscriptions and offline messages until expiry, so a reconnect resumes delivery

To test offline messages, first turn Clean Session (that is, Clean Start) off, set session expiry to 300 seconds, subscribe at QoS 1 and disconnect, then publish to that topic and reconnect to see whether the messages are delivered. Brokers differ in how they implement and cap offline queues, so the broker's behavior is the final word.

When Client ID is empty, 5.0 forces clean start and session expiry has no effect; for a persistent session, fill in a fixed, unique Client ID.

## How to set receive maximum

Receive maximum tells the broker "how many in-flight QoS 1/2 messages I can handle at once". Leaving it empty is the default; set it on low-spec devices, weak networks or when you need to limit bursts:

- Small device memory: set 10–50 to lower the concurrent delivery volume
- Normal desktop debugging: leave it empty and use the broker default
- Investigating message pile-ups: temporarily set a small value and watch whether the broker delivers within that window

## Reason code quick reference

| Reason code | Meaning | Typical scenario |
| ---- | ---- | ---- |
| `0x84` | Unsupported protocol version | The broker does not support 5.0 |
| `0x85` | Client ID not valid | Empty, too long or containing invalid characters |
| `0x86` | Bad username or password | Incorrect credentials |
| `0x87` | Not authorized | The ACL does not allow connecting, subscribing or publishing |
| `0x88` | Server unavailable | Maintenance or overload |
| `0x89` | Server busy | Retry or throttle |
| `0x8A` | Banned | Contact the administrator |
| `0x8F` | Topic filter invalid | Malformed wildcard syntax |
| `0x90` | Topic name invalid | The publish topic is empty or contains wildcards |
| `0x97` | Quota exceeded | Too many subscriptions or in-flight messages |
| `0x99` | Payload format invalid | The broker declared payload validation |
| `0x9F` | Connection rate exceeded | Reconnecting too quickly |

For full troubleshooting steps, see [MQTT troubleshooting](/mqtt-troubleshooting).

## When to choose 5.0

- You need to distinguish authorization failures, invalid topics, quota limits and other specific causes
- You need to test offline messages and session expiry
- You need to limit the broker's concurrent delivery volume
- The broker and cloud platform explicitly support 5.0

For basic subscribe, publish and TLS debugging, 3.1.1 is entirely sufficient, with simpler logs and broader compatibility.

## FAQ

### Does NetAssistant support the full MQTT 5.0 specification?

It covers the basics: version switching, clean start, session expiry, receive maximum and reason codes; advanced features such as user properties, Will Delay, subscription identifiers and shared subscriptions are not provided.

### Should I choose 5.0 or 3.1.1?

Choose 5.0 for session expiry and specific reason codes; choose 3.1.1 for older devices and brokers or basic debugging.

### What value should session expiry have?

0 expires on disconnect; for offline message testing, turn Clean Start off and enter a positive number of seconds.

### What is receive maximum for?

It limits how many QoS 1/2 messages the broker delivers at the same time; leave it empty to use the broker default.

### Where can I see the MQTT 5.0 subscribe failure reason code?

After the subscription switch waits for SUBACK, failure notifications and logs show the reason code, for example `0x87`, `0x8F` or `0x97`.

## Related pages

- Connection and subscription settings: [MQTT client](/mqtt-client#mqtt-5-0-optional)
- Subscription failures and refused connections: [MQTT troubleshooting](/mqtt-troubleshooting)
- Connecting from scratch: [MQTT tutorial](/guide/mqtt)
- [Download NetAssistant](/download)
