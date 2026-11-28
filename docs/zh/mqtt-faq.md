---
title: MQTT 常见问题 - 客户端选择、端口、TLS 与订阅
description: MQTT 常见问题汇总：客户端怎么选、默认端口、Client ID、3.1.1 与 5.0、TLS、WebSocket、订阅收不到消息、retain、QoS 重复、遗嘱与压测支持情况。
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
            "name": "MQTT 客户端哪个好？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "需要 WebSocket、脚本与完整 5.0 选 MQTTX；需要 Broker 全量 topic 树、retained 值与清理选 MQTT Explorer；已购商业授权选 MQTT.fx；需要免费、轻量并把 TCP/UDP 与 MQTT 放在一起调试，选 NetAssistant。"
            }
          },
          {
            "@type": "Question",
            "name": "MQTT 默认端口是多少？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "明文 MQTT 默认 1883，TLS 默认 8883，WebSocket 常见 8083/8084。NetAssistant 只支持 TCP 直连，不支持 WebSocket。"
            }
          },
          {
            "@type": "Question",
            "name": "Client ID 可以重复吗？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "同一 Broker 上必须唯一。重复时后连接的客户端会把前一个踢下线；NetAssistant 复制连接时会自动生成新 Client ID。"
            }
          },
          {
            "@type": "Question",
            "name": "MQTT 3.1.1 和 5.0 怎么选？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "老设备、老 Broker 或只做基础收发时用 3.1.1；需要会话过期、接收最大值或更具体的失败原因码时用 5.0。NetAssistant 默认 5.0。"
            }
          },
          {
            "@type": "Question",
            "name": "MQTT 怎么配置 TLS？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "端口填 8883 并打开 TLS；公共 CA 用系统根证书，私有 CA 用自定义 CA，自签证书联调可临时跳过校验。需要双向认证时再填客户端证书与私钥。"
            }
          },
          {
            "@type": "Question",
            "name": "订阅成功但收不到消息怎么办？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "检查订阅开关、topic 是否匹配过滤器、retain、QoS、发布端 ACL 与会话设置，最后用 mosquitto_sub 或另一个客户端交叉验证。"
            }
          },
          {
            "@type": "Question",
            "name": "retain 消息怎么清除？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "向同一 topic 发布空 payload 并勾选 retain，Broker 会删除保留消息。"
            }
          },
          {
            "@type": "Question",
            "name": "QoS 1 为什么会重复？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "QoS 1 是至少一次语义，重传或重连可能重复，需要应用层去重；QoS 2 才保证仅一次。"
            }
          },
          {
            "@type": "Question",
            "name": "遗嘱消息什么时候触发？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "只在异常断开时由 Broker 发布，例如断网或 KeepAlive 超时；主动断开不会触发。"
            }
          },
          {
            "@type": "Question",
            "name": "MQTT 支持压测吗？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "NetAssistant 暂不支持 MQTT 压测，压测引擎仅用于 TCP/UDP；MQTT 压测请使用 MQTTX Bench 或 emqtt_bench。"
            }
          }
        ]
      }
---

# MQTT 常见问题：客户端选择、连接与订阅

这里汇总选型、连接、TLS 与订阅发布中最常遇到的问题，每条给出结论和进一步阅读的入口。具体操作步骤在对应专题页展开，本页不重复长教程。

## 客户端怎么选

### MQTT 客户端是什么？

MQTT 客户端是连接 Broker、订阅 topic、发布和查看报文的工具。设备联调时用它验证上报与下发，平台开发时用它复现问题。

### MQTT 客户端哪个好？

| 需求 | 推荐 |
| ---- | ---- |
| WebSocket、脚本、完整 5.0 | MQTTX |
| Broker 全量 topic 树、retained 清理、数值曲线 | MQTT Explorer |
| 已购商业授权、完整 5.0 工具链 | MQTT.fx |
| 免费开源、轻量、报文流 + 会话主题树、TCP/UDP 与 MQTT 一体 | NetAssistant |

没有一款工具覆盖所有场景。日常只收发报文，轻量客户端更省资源；NetAssistant 的「主题」视图可按精确 topic 或前缀过滤报文流，但要摸清 Broker 的 retained 结构，MQTT Explorer 仍然更直接。

### NetAssistant 免费吗？

免费且开源，基于 Apache-2.0，没有试用期与许可证管理。

### 支持哪些平台？

Windows 10/11、Ubuntu 22.04 及以上、macOS 10.15 及以上，覆盖 x64 与 ARM64。

## 连接问题

### MQTT 默认端口是多少？

明文 1883，TLS 8883，WebSocket 常见 8083/8084。NetAssistant 只支持 TCP 直连，`ws://` 与 `wss://` 地址无法连接。

### Client ID 可以重复吗？

同一 Broker 上必须唯一。重复时后连接的会把前一个踢下线；复制连接会自动生成新 ID。

### MQTT 3.1.1 和 5.0 怎么选？

老设备、老 Broker 或只做基础收发用 3.1.1；需要会话过期、接收最大值与具体原因码用 5.0。NetAssistant 默认 5.0。

### MQTT 怎么配置 TLS？

端口填 8883 并打开 TLS，按证书来源选择系统根证书、自定义 CA 或跳过校验；需要双向认证时补客户端证书与私钥。详见 [MQTT TLS 配置](/zh/mqtt-tls)。

### 连接被拒绝怎么办？

先确认端口连通、地址与 TLS 开关匹配、Client ID 唯一、账号有权限，再按 CONNACK 返回码或 5.0 原因码定位。详见 [MQTT 排错](/zh/mqtt-troubleshooting#连接被拒绝或一直连不上)。

## 订阅与发布

### 订阅成功但收不到消息怎么办？

依次检查订阅开关、topic 是否匹配过滤器、retain、QoS、发布端 ACL 与会话设置，最后用 `mosquitto_sub` 交叉验证。

### retain 消息怎么清除？

向同一 topic 发布空 payload 并勾选 retain，Broker 会删除保留消息。

### QoS 1 为什么会重复？

QoS 1 是至少一次语义，重传或重连可能重复；需要应用层去重，QoS 2 才保证仅一次。

### 遗嘱消息什么时候触发？

只在异常断开时触发，例如断网、进程被杀或 KeepAlive 超时；主动断开不会触发。

### 离线消息怎么收？

使用固定 Client ID 并保持持久会话：3.1.1 关闭 Clean Session，5.0 关闭 Clean Start 且会话过期大于 0，Broker 才会保留离线消息。

### MQTT 支持压测吗？

暂不支持。NetAssistant 的压测引擎只用于 TCP/UDP；MQTT 压测请用 MQTTX Bench 或 emqtt_bench。

## 下一步

- 从零连接 Broker：[MQTT 调试教程](/zh/guide/mqtt)
- 完整能力与参数：[MQTT 客户端](/zh/mqtt-client)
- TLS 与双向认证：[MQTT TLS](/zh/mqtt-tls)
- 5.0 会话与原因码：[MQTT 5.0](/zh/mqtt-5)
- 测试场景：[QoS、retain 与遗嘱验证](/zh/mqtt-testing)
- 平台安装：[Windows 版](/zh/mqtt-client-windows)、[macOS 版](/zh/mqtt-client-mac)、[Linux 版](/zh/mqtt-client-linux)
- 与 MQTT.fx 的差异：[MQTT.fx 对比](/zh/compare/mqtt-fx)
- 全部选型对比：[MQTT 客户端对比总览](/zh/compare/)
- [下载 NetAssistant](/zh/download)
