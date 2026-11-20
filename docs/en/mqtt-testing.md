---
title: MQTT Testing Tool - QoS, Retain, Last Will and Wildcards
description: An MQTT testing tool for QoS 0/1/2, retain and clearing, last will, wildcard subscriptions, ACL checks and auto-reply, with a clear note that MQTT stress testing is not supported.
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
            "name": "How do I test an MQTT last will message?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Configure the will topic, payload, QoS and retain under Additional settings, connect, then subscribe to that topic from another client and force-kill the process or drop the network to break the connection abnormally. Clicking Disconnect is a normal disconnect and does not trigger the will."
            }
          },
          {
            "@type": "Question",
            "name": "How do I clear a retained message?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Publish an empty payload to the same topic with retain enabled, and the broker removes the retained message for that topic; new subscribers will no longer receive it."
            }
          },
          {
            "@type": "Question",
            "name": "Why does QoS 1 deliver duplicate messages?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "QoS 1 is at-least-once semantics, so retransmission or reconnects may deliver a message twice. Deduplicate in the application by message ID or business key; only QoS 2 guarantees exactly once."
            }
          },
          {
            "@type": "Question",
            "name": "Why do I receive no messages with a wildcard subscription?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Common causes are a topic that does not match the filter's levels, `#` not placed last, `+` not filling an entire level, or subscribing to a `$` system topic without an explicit topic subscription."
            }
          },
          {
            "@type": "Question",
            "name": "Can I stress test MQTT?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "NetAssistant's stress testing applies to TCP/UDP only and does not provide MQTT load testing. For MQTT concurrency and throughput tests, use dedicated tools such as MQTTX Bench or emqtt_bench."
            }
          }
        ]
      }
---

# MQTT Testing Tool: Verify QoS, Retain, Last Will and Wildcards

What you really need to confirm during integration is rarely "can it connect", but "will messages get lost, will they duplicate, can they be recovered after going offline, and are permissions enforced". The scenarios below can all be done with two NetAssistant connections (one subscribing, one publishing), without extra scripts.

## Before you start

- A working broker; local Mosquitto is enough. For setup, see [the MQTT tutorial](/en/guide/mqtt#run-a-local-broker)
- Two MQTT connections: one subscribing to the target topic, one publishing
- To observe offline behavior, use a fixed client ID and watch Clean Session / Clean Start

## Verifying QoS 0/1/2

| QoS | Semantics | What to watch |
| ---- | ---- | ---- |
| 0 | At most once | Fastest to send; messages may be lost when the network is unstable, with no broker acknowledgement |
| 1 | At least once | Duplicates are possible; disconnecting and reconnecting is a good way to watch redelivery and duplicates |
| 2 | Exactly once | Highest overhead; should not duplicate under normal conditions |

Test method: subscribe to `test/qos/#`, publish the same payload at QoS 0/1/2, and compare message counts and timing. Duplicates at QoS 1 are behavior the protocol allows, not a client bug; deduplicate on the business side when needed.

Under 5.0, a rejected publish shows the reason code in the notification, for example `0x87` not authorized or `0x97` quota exceeded; under 3.1.1 you only see a local send failure and need to check the broker logs.

## Retain testing

1. Publish a message to `test/retain` with **retain** enabled
2. Open a new connection and subscribe to `test/retain`; the last retained message should arrive immediately
3. Publish an empty payload with retain enabled to clear the retained message for that topic
4. Subscribe again from a new connection and confirm nothing is received

Note: retain is broker behavior. Some brokers or cloud platforms disable it, and a publish may return `0x9A` (retain not supported).

## Last will testing

1. In the connection's "Additional settings", enable the last will and fill in topic, payload, QoS and retain
2. Subscribe to the will topic from another connection
3. After connecting, force-kill the NetAssistant process, or drop the network and wait for the keep-alive timeout
4. The subscriber should receive the will message

Clicking "Disconnect" is a normal disconnect and does not trigger the will. NetAssistant does not support 5.0 Will Delay; the will is published as soon as the broker detects the abnormal disconnect. If it does not arrive, check the publish permission for the will topic and the subscription filter.

## Wildcards and system topics

| Filter | Matches | Does not match |
| ---- | ---- | ---- |
| `device/+/status` | `device/1/status` | `device/1/2/status`, `device/status` |
| `device/#` | `device`, `device/1/status` | System topics starting with `$` |
| `$SYS/#` | `$SYS/broker/uptime` and similar | Ordinary business topics |

`#` must be placed last, and `+` must fill an entire level. Shared subscriptions `$share/group/topic` are not supported, and subscribing fails.

## ACL and permission testing

Use a restricted account to verify connect, subscribe and publish separately:

- Connection refused: 5.0 returns `0x87`, 3.1.1 returns code 5
- Subscription refused: 5.0 SUBACK `0x87`; the subscription switch turns off automatically with a message
- Publish refused: 5.0 PUBACK `0x87`; the top-right notification shows the topic and the reason

An ACL that allows subscribe but not publish leaves the subscriber receiving nothing; when troubleshooting, first confirm that the publisher succeeds.

## Offline messages and sessions

- 3.1.1: turn off Clean Session, use a fixed client ID, subscribe at QoS 1, disconnect, publish, then reconnect and watch the redelivery
- 5.0: turn off Clean Start, set session expiry to a positive value (for example 300 seconds), and follow the same steps
- With Clean Session / Clean Start on, the session is cleared on disconnect and offline messages are not redelivered

Brokers have their own limits on offline queue length and expiry policy; the result depends on the broker configuration.

## Auto-reply and periodic send

- **Auto-reply**: the response is published back to the topic the message arrived on, handy for simulating a device answering platform commands
- **Periodic send**: sends on the current publish topic on an interval, handy for simulating heartbeats or periodic reporting

Both reuse the existing debugging features and add no scripting. When testing auto-reply, avoid a loop where the reply matches the rule again; narrow the rule or lower the frequency if needed.

## Stress testing is not supported

MQTT stress testing is not supported: the stress engine applies to TCP/UDP only, and MQTT mode offers no connection-count, throughput or latency load testing. For MQTT load testing, use MQTTX Bench, emqtt_bench or similar tools; this tool is better suited to functional verification and observing single-connection behavior.

## FAQ

### How do I test an MQTT last will message?

Configure the will, subscribe from another connection, then force-kill the process or drop the network; a manual disconnect does not trigger it.

### How do I clear a retained message?

Publish an empty payload to the same topic with retain enabled.

### Why does QoS 1 deliver duplicate messages?

QoS 1 is at-least-once semantics; retransmission or reconnects may duplicate, so deduplicate in the application.

### Why do I receive no messages with a wildcard subscription?

Check level matching, the position of `#`, whether `+` fills a whole level, and whether `$` system topics are subscribed explicitly.

### Can I stress test MQTT?

No. Stress testing applies to TCP/UDP only; for MQTT load testing use MQTTX Bench or emqtt_bench.

## Related pages

- Parameters and UI: [MQTT client](/en/mqtt-client)
- Failure causes and reason codes: [MQTT troubleshooting](/en/mqtt-troubleshooting)
- 5.0 sessions and reason codes: [MQTT 5.0 client](/en/mqtt-5)
- [Download NetAssistant](/en/download)
