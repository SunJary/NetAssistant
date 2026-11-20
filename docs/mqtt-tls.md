---
title: MQTT TLS 配置 - 证书校验、自签证书与双向认证
description: NetAssistant 的 MQTT TLS 设置：系统根证书、自定义 CA 与跳过校验三种模式，自签证书、双向认证（mTLS）、ALPN 配置与常见握手错误处理，适用于 3.1.1 与 5.0。
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
            "name": "MQTT TLS 用哪个端口？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "TLS 通常使用 8883，明文 MQTT 使用 1883。端口和 TLS 开关必须匹配，否则会在握手阶段失败。"
            }
          },
          {
            "@type": "Question",
            "name": "自签证书一定要跳过校验吗？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "不一定。更安全的做法是把自建 CA 的 PEM 填入「自定义 CA」，由客户端校验 Broker 证书链；只有在临时联调、无法拿到 CA 文件时才使用跳过校验。"
            }
          },
          {
            "@type": "Question",
            "name": "跳过证书校验安全吗？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "跳过校验后传输仍然加密，但不验证 Broker 身份，存在中间人攻击风险，只适合本地或测试环境，不要用于生产。"
            }
          },
          {
            "@type": "Question",
            "name": "支持 MQTT 双向认证吗？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "支持。填写客户端证书与私钥（PEM，支持 PKCS#8、PKCS#1 与 SEC1，需成对）即可完成 mTLS；Broker 侧需要开启 require_certificate 等对应配置。"
            }
          },
          {
            "@type": "Question",
            "name": "证书内容会保存到配置里吗？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "不会。连接配置只保存证书文件路径，不保存 PEM 内容，私钥不会写入配置文件。"
            }
          }
        ]
      }
---

# MQTT TLS：证书校验、自签证书与双向认证

公网、云 IoT 平台和多数生产 Broker 都要求加密连接。NetAssistant 的 MQTT 客户端在 3.1.1 与 5.0 下使用同一套 TLS 设置：填好端口、选择校验方式，需要时再补客户端证书即可。证书过期、主机名不匹配、证书链不受信任都会给出可读的握手错误。

## 什么时候需要 TLS

- Broker 监听 8883，或云平台明确要求 `mqtts`
- 报文经过公网、公司外部网络或不可信 Wi-Fi
- 平台要求双向认证，客户端必须出示证书
- 本地联调可以用明文 1883；一旦验证通过，上线前再切 TLS 复测

> NetAssistant 只支持 TCP 直连，不支持 MQTT over WebSocket，因此 `wss://` 地址无法连接。

## 三种证书校验方式

| 模式 | 适用场景 | 行为 |
| ---- | ---- | ---- |
| 系统根证书（默认） | Broker 使用公共 CA 签发证书 | 按操作系统信任链校验 |
| 自定义 CA | 自建 CA、私有证书链、企业内网 CA | 读取指定 PEM 文件校验证书链 |
| 跳过校验 | 临时联调、无法获取 CA 的自签证书 | 不验证身份，传输仍然加密 |

「自定义 CA」比「跳过校验」更安全：既保留加密，也能确认连到的是正确的 Broker。选择后点「选择文件」回填 PEM 路径即可。

## 配置步骤

1. 端口填 Broker 的 TLS 端口，通常是 `8883`。
2. 打开 **TLS** 开关。
3. 按证书来源选择校验方式；使用私有 CA 时填入 CA 的 PEM 文件。
4. Broker 要求双向认证时，填写客户端证书与私钥（PEM，成对）。
5. Broker 要求 ALPN 时填写协议名，多个用逗号分隔，例如 `mqtt`；留空则不下发。
6. 点击连接，握手失败时按提示检查端口、证书链与主机名。

证书路径会随连接配置保存，但 PEM 内容与私钥不会写入配置。

## 用自签证书测试

本地验证 TLS 时，可以生成一张带 SAN 的证书，避免主机名不匹配：

```bash
openssl req -x509 -newkey rsa:2048 -nodes -days 365 \
  -keyout server.key -out server.crt \
  -subj "/CN=broker.local" \
  -addext "subjectAltName=DNS:broker.local,IP:127.0.0.1"
```

Mosquitto 侧的最小 TLS 配置：

```ini
listener 8883
cafile ca.crt
certfile server.crt
keyfile server.key
```

客户端连接 `broker.local:8883`（或 `127.0.0.1:8883`），校验方式选「自定义 CA」并指向 CA 文件。使用单张自签证书时，也可以直接把 `server.crt` 当作 CA 文件。只有在拿不到任何证书文件时才用「跳过校验」。

## 双向认证（mTLS）

Broker 开启 `require_certificate true` 后，客户端必须提供由可信 CA 签发的证书：

1. 准备客户端证书、私钥，以及签发它的 CA。
2. 在连接配置中填写客户端证书与私钥路径，两者必须成对。
3. 私钥支持 PKCS#8、PKCS#1 与 SEC1 格式；加密私钥需要先解密或提供未加密版本。
4. CA 校验方式仍按 Broker 证书的来源选择，客户端证书与 Broker 证书可以来自同一 CA。

## ALPN

部分 Broker 或网关要求 TLS ALPN 协商。把协议名填入 ALPN 输入框，多个用英文逗号分隔；不确定时留空，让客户端不下发 ALPN，避免协商失败。

## 常见握手错误

| 提示 | 原因 | 处理 |
| ---- | ---- | ---- |
| certificate verify failed / unknown CA | 证书链不受信任 | 改用「自定义 CA」并填对 CA 文件 |
| hostname mismatch | 连接地址不在证书 SAN 中 | 使用证书里的域名连接，或重签证书 |
| certificate expired | 证书过期 | 更新证书；联调时可临时跳过校验 |
| Broker 要求客户端证书 | 开启了双向认证 | 补客户端证书与私钥 |
| handshake failure | 端口与 TLS 开关不匹配、ALPN 不一致 | 8883 开 TLS、1883 关 TLS，按 Broker 要求填 ALPN |

更多连接与订阅问题见 [MQTT 排错](/mqtt-troubleshooting)。

## 安全建议

- 生产环境不要使用「跳过校验」
- 私钥单独存放并限制文件权限，不要提交到仓库
- 证书到期前轮换，客户端只需更新文件路径
- 优先使用带 SAN 的证书，避免依赖 Common Name
- 双向认证时，为每台设备或每个开发者签发独立客户端证书

## 常见问题

### MQTT TLS 用哪个端口？

TLS 通常使用 8883，明文 MQTT 使用 1883。端口和 TLS 开关必须匹配，否则会在握手阶段失败。

### 自签证书一定要跳过校验吗？

不一定。把自建 CA 的 PEM 填入「自定义 CA」更安全；只有临时联调、拿不到 CA 文件时才跳过校验。

### 跳过证书校验安全吗？

传输仍然加密，但不验证 Broker 身份，存在中间人风险，只适合本地或测试环境。

### 支持 MQTT 双向认证吗？

支持。填写成对的客户端证书与私钥（PEM）即可；Broker 侧需开启对应配置。

### 证书内容会保存到配置里吗？

不会。配置只保存文件路径，不保存 PEM 内容，私钥不会写入配置文件。

## 相关页面

- TLS 参数与界面说明：[MQTT 客户端](/mqtt-client#mqtt-over-tls)
- 握手失败排查：[MQTT 排错](/mqtt-troubleshooting#tls-握手失败)
- 从连接到第一条消息：[MQTT 调试教程](/guide/mqtt)
- [下载 NetAssistant](/download)
