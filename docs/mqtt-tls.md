---
title: "MQTT TLS Setup: Verification, Self-Signed Certs and mTLS"
description: "NetAssistant MQTT TLS settings: system roots, custom CA or skipped verification, self-signed certs, mutual TLS and ALPN, plus fixes for TLS handshake errors."
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
            "name": "Which port does MQTT TLS use?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "TLS usually uses 8883, while plain MQTT uses 1883. The port and the TLS switch must match, or the connection fails during the handshake."
            }
          },
          {
            "@type": "Question",
            "name": "Do self-signed certificates always require skipping verification?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "No. A safer option is to put the private CA's PEM into Custom CA so the client verifies the broker certificate chain; skip verification only for temporary debugging when no CA file is available."
            }
          },
          {
            "@type": "Question",
            "name": "Is skipping certificate verification safe?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Traffic is still encrypted, but the broker's identity is not verified, so man-in-the-middle attacks are possible. Use it only for local or test environments, never in production."
            }
          },
          {
            "@type": "Question",
            "name": "Does NetAssistant support mutual TLS?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Yes. Provide a client certificate and private key (PEM, PKCS#8, PKCS#1 or SEC1, as a pair) to complete mTLS; the broker must enable the matching setting such as require_certificate."
            }
          },
          {
            "@type": "Question",
            "name": "Is certificate content saved in the configuration?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "No. The connection configuration stores only certificate file paths, not PEM content, and the private key is never written to the config file."
            }
          }
        ]
      }
---

# MQTT TLS: Certificate Verification, Self-Signed Certificates and Mutual TLS

Public networks, cloud IoT platforms and most production brokers require encrypted connections. NetAssistant's MQTT client uses the same TLS settings under 3.1.1 and 5.0: set the port, choose a verification mode, and add a client certificate when needed. Expired certificates, hostname mismatches and untrusted chains all surface as readable handshake errors.

## When you need TLS

- The broker listens on 8883, or the cloud platform explicitly requires `mqtts`
- Traffic crosses the public internet, an external corporate network or untrusted Wi-Fi
- The platform requires mutual TLS, so the client must present a certificate
- Local debugging can use plaintext 1883; once it works, switch to TLS and retest before going live

> NetAssistant only supports direct TCP connections and not MQTT over WebSocket, so `wss://` addresses cannot be used.

## Three certificate verification modes

| Mode | When to use | Behavior |
| ---- | ---- | ---- |
| System roots (default) | The broker certificate is issued by a public CA | Verified against the operating system trust chain |
| Custom CA | Private CA, private certificate chain, corporate internal CA | Reads the specified PEM file to verify the chain |
| Verification skipped | Temporary debugging, self-signed certificate without a CA file | No identity check; traffic is still encrypted |

Custom CA is safer than skipped verification: it keeps encryption and still confirms you reached the right broker. After selecting it, click "Choose file" to fill in the PEM path.

## Configuration steps

1. Set the port to the broker's TLS port, usually `8883`.
2. Turn on the **TLS** switch.
3. Choose a verification mode for the certificate source; for a private CA, point to its PEM file.
4. If the broker requires mutual TLS, fill in the client certificate and private key (PEM, as a pair).
5. If the broker requires ALPN, enter the protocol names, comma-separated, for example `mqtt`; leave it empty to omit them.
6. Click connect; if the handshake fails, check the port, certificate chain and hostname as prompted.

Certificate paths are saved with the connection configuration, but the PEM content and private key are not written to it.

## Testing with a self-signed certificate

To verify TLS locally, generate a certificate with a SAN to avoid a hostname mismatch:

```bash
openssl req -x509 -newkey rsa:2048 -nodes -days 365 \
  -keyout server.key -out server.crt \
  -subj "/CN=broker.local" \
  -addext "subjectAltName=DNS:broker.local,IP:127.0.0.1"
```

A minimal TLS configuration for Mosquitto:

```ini
listener 8883
cafile ca.crt
certfile server.crt
keyfile server.key
```

Connect the client to `broker.local:8883` (or `127.0.0.1:8883`), choose Custom CA and point it at the CA file. If you use a single self-signed certificate, you can also point directly at `server.crt` as the CA file. Use "Verification skipped" only when no certificate file can be obtained.

## Mutual TLS (mTLS)

After the broker enables `require_certificate true`, the client must present a certificate signed by a trusted CA:

1. Prepare the client certificate, private key and the CA that issued it.
2. In the connection settings, fill in the client certificate and private key paths; the two must be provided as a pair.
3. Private keys in PKCS#8, PKCS#1 and SEC1 format are supported; encrypted keys must be decrypted first or supplied in an unencrypted version.
4. Choose the CA verification mode according to where the broker certificate comes from; the client and broker certificates may come from the same CA.

## ALPN

Some brokers or gateways require TLS ALPN negotiation. Enter the protocol names in the ALPN field, separated by commas; when unsure, leave it empty so the client does not send ALPN and negotiation cannot fail.

## Common handshake errors

| Message | Cause | Fix |
| ---- | ---- | ---- |
| certificate verify failed / unknown CA | The certificate chain is not trusted | Switch to Custom CA and point at the correct CA file |
| hostname mismatch | The connection address is not in the certificate SAN | Connect using the domain in the certificate, or reissue the certificate |
| certificate expired | The certificate has expired | Renew it; skip verification temporarily while debugging |
| Broker requires a client certificate | Mutual TLS is enabled | Add the client certificate and private key |
| handshake failure | Port and TLS switch do not match, or ALPN differs | Enable TLS on 8883 and disable it on 1883; set ALPN as the broker requires |

For more connection and subscription issues, see [MQTT troubleshooting](/mqtt-troubleshooting).

## Security recommendations

- Do not use "Verification skipped" in production
- Store private keys separately with restricted file permissions and never commit them to a repository
- Rotate certificates before they expire; the client only needs the updated file paths
- Prefer certificates with a SAN instead of relying on the Common Name
- With mutual TLS, issue a separate client certificate for each device or developer

## FAQ

### Which port does MQTT TLS use?

TLS usually uses 8883 and plain MQTT uses 1883. The port and the TLS switch must match, or the connection fails during the handshake.

### Do self-signed certificates always require skipping verification?

No. Putting the private CA's PEM into Custom CA is safer; skip verification only for temporary debugging when no CA file is available.

### Is skipping certificate verification safe?

Traffic is still encrypted, but the broker's identity is not verified, so a man-in-the-middle risk remains. It suits local or test environments only.

### Does NetAssistant support mutual TLS?

Yes. Provide a paired client certificate and private key (PEM); the broker side must enable the corresponding setting.

### Is certificate content saved in the configuration?

No. The configuration stores only file paths, not PEM content, and the private key is never written to the config file.

## Related pages

- TLS settings and UI reference: [MQTT client](/mqtt-client#mqtt-over-tls)
- Handshake failure troubleshooting: [MQTT troubleshooting](/mqtt-troubleshooting#tls-handshake-failures)
- From connecting to the first message: [MQTT tutorial](/guide/mqtt)
- [Download NetAssistant](/download)
