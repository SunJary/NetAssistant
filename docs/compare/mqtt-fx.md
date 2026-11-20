---
title: MQTT.fx 替代品 - 免费开源与商业客户端对比
description: MQTT.fx 5.x 是 Softblade 的商业客户端，提供 30 天试用；NetAssistant 是免费开源的轻量替代，覆盖 MQTT 3.1.1 / 5.0、TLS 与 TCP/UDP 调试，按场景选择。
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
            "name": "MQTT.fx 免费吗？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "当前 MQTT.fx 5.x 是 Softblade 的商业产品，提供免费试用与 Demo 会话；网上较早的 1.x 免费版属于旧版本，授权模式与当前版本不同。具体价格与试用规则以官方页面为准。"
            }
          },
          {
            "@type": "Question",
            "name": "NetAssistant 能替代 MQTT.fx 吗？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "在免费、轻量、TCP/UDP 与 MQTT 混合调试的场景可以替代；如果依赖 MQTT.fx 的完整 MQTT 5 工具链、脚本或已购授权，继续使用它更合适。"
            }
          },
          {
            "@type": "Question",
            "name": "两者最大的区别是什么？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "授权与技术栈不同：MQTT.fx 5.x 是商业 Java 客户端，NetAssistant 是 Apache-2.0 开源的 Rust 原生应用，内存约 20MB，并把 TCP/UDP 调试放在同一个应用里。"
            }
          },
          {
            "@type": "Question",
            "name": "NetAssistant 支持 MQTT 5.0 吗？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "支持基础能力：Clean Start、会话过期、接收最大值，以及连接、订阅、发布的原因码；用户属性、Will Delay、共享订阅等高级特性未提供。"
            }
          },
          {
            "@type": "Question",
            "name": "从 MQTT.fx 迁移麻烦吗？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "不麻烦。新建 MQTT 连接，填入相同的 Broker、端口与账号，按证书来源配置 TLS，再订阅原来的 topic 即可；同时连接两个客户端时 Client ID 要不同。"
            }
          }
        ]
      }
---

# MQTT.fx 替代品：免费开源、轻量与商业客户端怎么选

MQTT.fx 是很多物联网开发者最早接触的图形化 MQTT 客户端之一，当前 5.x 由 Softblade 以商业授权提供，带免费试用。NetAssistant 不试图在功能数量上对标它，而是走另一条路：免费开源、约 20MB 内存、原生桌面，并把 TCP/UDP 调试和 MQTT 放在一起。

## MQTT.fx 现在是什么

- **商业产品**：MQTT.fx 5.x 需要授权，官方提供免费试用与 Demo 会话，具体期限与价格以官网为准
- **Java 桌面客户端**：跨平台运行，功能覆盖连接配置、topic 浏览、消息编辑、QoS、retain 与 TLS
- **MQTT 5 特性**：官方提供 MQTT 5 相关能力，完整程度以官方文档与版本为准
- **老教程注意**：网上大量 1.x 教程对应的免费旧版，与当前 5.x 的授权模式不同

## NetAssistant 的不同

- **免费开源**：Apache-2.0，全部功能可用，没有试用期与许可证管理
- **Rust 原生**：不是 Java/Electron 套壳，内存约 20MB，低配现场与远程桌面更轻
- **协议范围更宽**：TCP/UDP 客户端与服务端 + MQTT 3.1.1 / 5.0
- **TLS 更细**：系统根证书 / 自定义 CA / 跳过校验三档，支持客户端证书做双向认证
- **边界明确**：不提供脚本、MQTT over WebSocket 与 5.0 高级特性，适合功能验证而不是全功能工作台

## 对比表

| 维度 | NetAssistant | MQTT.fx 5.x |
| ---- | ---- | ---- |
| 授权与费用 | Apache-2.0，免费 | 商业授权，提供试用 |
| 技术栈 | Rust 原生 | Java 桌面客户端 |
| 资源占用 | 约 20MB 内存 | 以实际运行为准 |
| 协议范围 | TCP/UDP + MQTT | MQTT |
| MQTT 版本 | 3.1.1 / 5.0（基础 5.0） | 3.1.1 / 5.0（以官方为准） |
| TLS / 双向认证 | 支持 | 支持 |
| topic 浏览 | 报文流 + topic 标签 + 搜索 | topic 浏览器 + 消息编辑 |
| 脚本与高级 5.0 | 不支持 | 以官方版本为准 |
| MQTT 压测 | 不支持 | 不在本页比较范围 |

## 什么场景继续用 MQTT.fx

- 已经购买授权，团队流程围绕它建立
- 需要更完整的 MQTT 5 属性编辑与高级工具
- 需要脚本或更复杂的消息处理能力
- 更习惯它的 topic 浏览器与消息编辑器

## 什么场景换成 NetAssistant

- 不想为偶尔调试购买授权，或需要可自由分发的开源工具
- 机器内存紧张，或需要长期挂在远程桌面
- 同一现场既有 TCP/UDP 私有协议，又有 MQTT
- 需要 mTLS、自定义 CA、MQTT 5 原因码等基础排错能力
- 只需要验证连接、订阅、发布与报文内容

## 从 MQTT.fx 迁移

1. 在 NetAssistant 新建 MQTT 连接，填入相同的 Broker、端口与账号
2. 按证书来源配置 TLS；原来用 8883 的记得打开 TLS
3. 订阅之前在 MQTT.fx 里使用的 topic 或通配符
4. 用发送区复现发布，消息区搜索关键字，必要时导出 JSON
5. 两个客户端同时连接时，Client ID 必须不同

MQTT.fx 是 Softblade GmbH 的商标，本页仅做产品对比，与 Softblade 无隶属关系。

## 常见问题

### MQTT.fx 免费吗？

当前 5.x 是商业产品，提供免费试用与 Demo；较早的 1.x 免费版属于旧版本，规则以官方为准。

### NetAssistant 能替代 MQTT.fx 吗？

免费、轻量、TCP/UDP 与 MQTT 混合调试场景可以；依赖完整 5.0 工具链、脚本或已购授权时继续用 MQTT.fx。

### 两者最大的区别是什么？

MQTT.fx 是商业 Java 客户端；NetAssistant 是免费开源的 Rust 原生应用，约 20MB，并包含 TCP/UDP 调试。

### NetAssistant 支持 MQTT 5.0 吗？

支持 Clean Start、会话过期、接收最大值与原因码；高级特性未提供。

### 从 MQTT.fx 迁移麻烦吗？

不麻烦，填入相同 Broker 与账号、配置 TLS、订阅原 topic 即可；同时连接时 Client ID 要不同。

## 相关页面

- 与 MQTTX 的差异：[MQTTX 替代品对比](/comparison-mqttx)
- 与 MQTT Explorer 的差异：[MQTT Explorer 对比](/compare/mqtt-explorer)
- 功能与参数：[MQTT 客户端](/mqtt-client)
- 常见问题汇总：[MQTT 常见问题](/mqtt-faq)
- [下载 NetAssistant](/download)
