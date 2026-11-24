---
title: MQTT Troubleshooting - Connection, Subscription and TLS Errors
description: "Diagnose MQTT problems by symptom: connection refused, authentication failure, no messages after subscribing, TLS handshake errors, garbled text, duplicate QoS 1 messages and reconnect issues, with reason codes and fixes."
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
            "name": "What is the most common reason an MQTT connection is refused?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Wrong host or port, a TLS mismatch, a duplicate client ID, bad credentials and missing ACL permissions are the usual causes. 3.1.1 CONNACK return codes and 5.0 reason codes tell these cases apart."
            }
          },
          {
            "@type": "Question",
            "name": "Why do I receive no messages after subscribing?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Check that the subscription switch is on, the publish topic matches the topic filter, and retained, QoS, publish ACL and session settings are correct. Cross-check with another client or mosquitto_sub."
            }
          },
          {
            "@type": "Question",
            "name": "How do I fix MQTT TLS certificate errors?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Use a custom CA when the chain is untrusted, connect with a hostname from the certificate SAN when it mismatches, renew expired certificates, and add a client certificate and key when the broker requires mutual TLS. Skipping verification is for local testing only."
            }
          },
          {
            "@type": "Question",
            "name": "Where can I see MQTT 5.0 reason codes?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Rejected connections, failed subscriptions and broker-initiated disconnects show the reason code and broker text in the connection state or the top-right notification. 3.1.1 only provides CONNACK return codes."
            }
          },
          {
            "@type": "Question",
            "name": "Why does QoS 1 deliver duplicate messages?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "QoS 1 is at-least-once: retransmission or reconnects may deliver a message twice, which the protocol allows. Deduplicate in the application by message ID or business key; only QoS 2 guarantees exactly once."
            }
          }
        ]
      }
---

# MQTT Troubleshooting: Connections, Missing Messages and TLS Errors

Most MQTT failures fall into a few buckets: the network is unreachable, credentials or permissions are wrong, the topic does not match, or TLS settings disagree. Work through the symptoms below — each section lists the cause and the action. If you do not know where to start, run the three checks first.

## Three quick checks

1. **Is the port reachable?** On Windows run `Test-NetConnection broker.example.com -Port 1883`; on macOS or Linux run `nc -vz broker.example.com 1883`. Timeouts usually mean network, VPN, firewall or cloud security-group problems.
2. **Do host, port and TLS match?** 1883 is plain MQTT and 8883 is TLS. Connecting to 8883 without TLS, or enabling TLS on 1883, fails the handshake. MQTT over WebSocket is not supported, so `ws://` and `wss://` endpoints cannot connect directly.
3. **Are the client ID and credentials correct?** Client IDs must be unique per broker; reusing one kicks the previous connection offline. Bad credentials or ACL denial show up in the CONNACK return code.

> If only your machine cannot connect, check corporate networks, proxies, VPNs and cloud IP allowlists first.

## Connection refused or never connects

| Symptom / code | Likely cause | Action |
| ---- | ---- | ---- |
| Connection refused | Broker not running, wrong port, firewall | Verify the broker process and listening port; test 127.0.0.1 or an internal address |
| Timeout | Network, VPN, security group, IP allowlist | Run the port check above, then review cloud access rules |
| 3.1.1 code 1 / 5.0 `0x84` | Broker does not support the selected protocol version | Switch from 5.0 to 3.1.1, or upgrade the broker |
| 3.1.1 code 2 / 5.0 `0x85` | Client ID empty, too long, invalid or duplicated | Use a unique client ID; duplicating a connection should generate a new one |
| 3.1.1 code 3 / 5.0 `0x88` | Broker unavailable, overloaded or under maintenance | Check broker logs and resources, retry later |
| 3.1.1 code 4 / 5.0 `0x86` | Bad username or password | Recheck credentials, including whitespace and special characters |
| 3.1.1 code 5 / 5.0 `0x87` | Not authorized; ACL blocks this user | Add the required ACL or permission policy in the broker or cloud platform |
| 5.0 `0x89` / `0x8A` / `0x9F` | Server busy, client banned, connection rate exceeded | Slow down reconnects and ask the broker administrator to unban the client |

5.0 reason codes appear in the connection state or notification. 3.1.1 is less specific, so check broker logs. Avoid rapid reconnect loops; some brokers ban clients that exceed the connection rate.

## TLS handshake failures

| Message | Cause | Action |
| ---- | ---- | ---- |
| certificate verify failed / unknown CA | Untrusted chain, common with a private CA | Choose **Custom CA** and point to the CA PEM file |
| hostname mismatch | The connected host is not in the certificate SAN | Connect with a name from the certificate, or reissue it for that address |
| certificate expired | Expired certificate | Renew the broker certificate; skip verification temporarily for local testing |
| Broker requires a client certificate | Mutual TLS is enabled | Provide a client certificate and private key (PEM) as a pair |
| handshake failure / no response | Port and TLS mismatch, or ALPN requirements | Enable TLS on 8883 and disable it on 1883; fill in ALPN if the broker requires it |

**Skip verification** skips identity checks only; traffic is still encrypted. It is meant for self-signed development certificates, not production.

## Subscription failures

After you turn on a subscription switch, NetAssistant waits for the broker to confirm it (SUBACK). A failure or a 10-second timeout turns that entry off with a reason:

- **5.0 `0x87` not authorized**: the ACL does not allow this topic; check the broker or cloud permission policy
- **5.0 `0x8F` invalid topic filter**: wildcard syntax is wrong; `#` must be last and `+` must occupy a whole level
- **5.0 `0x97` quota exceeded**: too many subscriptions; remove unused entries
- **3.1.1 rejected**: usually only a failure result; check broker logs for the ACL decision
- **No response for 10 seconds**: broker stalled, network dropped or the connection is gone; verify connection state first

Shared subscriptions (`$share/...`) and 5.0 subscription identifiers are not supported, so those topics fail to subscribe. System topics starting with `$` must be subscribed explicitly; `#` does not match them.

## Subscribed but no messages arrive

Check in this order:

1. **Is the subscription switch on?** The off state is persisted and survives restarts and reconnects.
2. **Does the topic actually match?** `sensor/1/temp` matches `sensor/+/temp` but not `sensor/temp`; use `sensor/#` for multiple levels.
3. **Was the message published before you subscribed?** Ordinary messages are not replayed; only a retained publish is delivered to new subscribers.
4. **Is retain what you expect?** Without retain the broker keeps nothing; publishing an empty payload clears a retained message.
5. **QoS and sessions**: QoS 0 may be lost during network hiccups; offline messages require a persistent session (Clean Session off in 3.1.1, or Clean Start off with a positive session expiry in 5.0).
6. **Does the ACL only allow subscribing?** Some brokers restrict publish and subscribe separately, so a rejected publish means nothing reaches subscribers.
7. **Broker-side rules**: bridging, topic rewriting and cloud platform model transforms can change the topic.

Cross-check with another client or `mosquitto_sub -h <broker> -t 'test/#' -v` while publishing from NetAssistant. If the CLI receives it, the local subscription settings are the problem; if not, look at the broker, publisher or ACL.

## Publish failures

Under MQTT 5, a QoS 1/2 publish rejected by the broker reports a reason code:

- `0x87` not authorized: the publish ACL blocks this topic
- `0x90` invalid topic: empty, contains wildcards or is too long
- `0x97` quota exceeded: in-flight messages or queue limit reached
- `0x99` payload format invalid: the broker declared a payload format and the content does not match
- `0x9A` / `0x9B`: the broker does not support retain or that QoS

3.1.1 PUBACK carries no reason code, so only local send failures are reported; check broker logs. QoS 0 has no acknowledgement at all, so only local delivery failures are reported. When publishing frequently to the same topic, identical notifications are shown once per 3 seconds, but the message log records every entry — use the log when debugging.

## Garbled non-ASCII text

MQTT payloads are byte streams. NetAssistant's text mode decodes as UTF-8, so text sent in GBK or another encoding appears garbled:

- Switch the receive view to hex and decode the bytes with the sender's encoding
- Standardize the sender on UTF-8, and verify JSON serialization encoding
- Topics must be UTF-8 per the protocol; a garbled topic means the publisher is already wrong

## Duplicate QoS 1 messages

QoS 1 is at-least-once: retransmission or reconnect may deliver a message twice, which the protocol allows. Deduplicate in the application by message ID, timestamp plus business key, or an idempotency table. Use QoS 2 when duplicates are unacceptable, at a higher cost.

## Subscriptions disappear or look wrong after reconnect

- Only subscriptions that were switched on are restored; manually disabled entries stay off
- With a persistent session, the broker still holds the subscription, so an offline unsubscribe asks you to connect first
- Clean Session / Clean Start set to true clears the session and offline messages
- Duplicating a connection generates a new client ID, so the previous session state does not carry over

## Last-will message not received

The will is published by the broker only on an abnormal disconnect, such as a network drop or keep-alive timeout. Clicking disconnect is a clean disconnect and does not trigger it. Also verify publish permission for the will topic and that a subscription filter matches it. MQTT 5.0 Will Delay is not supported.

## Still stuck

Collect the following before asking for help:

- Broker host, port, protocol version and whether TLS is enabled
- The full error message or reason code (a screenshot is fine)
- Client ID, username (never the password), subscription topic and publish topic
- QoS and retain used when publishing
- Broker logs and a cross-check with `mosquitto_sub` or another client

Open an issue on [GitHub Issues](https://github.com/SunJary/NetAssistant/issues) and redact passwords, private keys and internal addresses.

## FAQ

### What is the most common reason an MQTT connection is refused?

Wrong host or port, a TLS mismatch, a duplicate client ID, bad credentials or a missing ACL. 3.1.1 CONNACK return codes and 5.0 reason codes distinguish these cases.

### Why do I receive no messages after subscribing?

Confirm the switch is on and the topic matches the filter, then check retained, QoS, ACL and session settings. Cross-check with `mosquitto_sub`.

### How do I fix MQTT TLS certificate errors?

Use a custom CA for untrusted chains, a certificate SAN hostname for mismatches, renew expired certificates, and add a client certificate plus key for mutual TLS.

### Where can I see MQTT 5.0 reason codes?

Rejected connections, failed subscriptions and broker disconnects show them in the connection state or the top-right notification; 3.1.1 only provides CONNACK return codes.

### Why does QoS 1 deliver duplicate messages?

QoS 1 is at-least-once, so retransmission or reconnects may duplicate. Deduplicate in the application; QoS 2 provides exactly-once delivery.

## Related pages

- Start from the beginning: [MQTT tutorial](/guide/mqtt)
- Capabilities and settings: [MQTT client](/mqtt-client)
- Differences from MQTTX: [MQTTX alternative comparison](/comparison-mqttx)
- Verify QoS, retain and last-will behavior: [MQTT testing scenarios](/mqtt-testing)
- [Download NetAssistant](/download)
