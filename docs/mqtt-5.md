---
title: MQTT 5.0 客户端 - 会话过期、原因码与 3.1.1 差异
description: NetAssistant 的 MQTT 5.0 支持范围：Clean Start、会话过期、接收最大值与连接/订阅/发布原因码，对比 3.1.1 的差异，并说明用户属性、Will Delay、共享订阅等未提供的高级特性。
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
            "name": "NetAssistant 支持完整的 MQTT 5.0 吗？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "覆盖基础能力：协议版本切换、Clean Start、会话过期、接收最大值，以及连接、订阅、发布与断开时的原因码。用户属性、Will Delay、订阅标识符、共享订阅等高级特性暂未提供。"
            }
          },
          {
            "@type": "Question",
            "name": "5.0 和 3.1.1 应该选哪个？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Broker 支持 5.0 且需要会话过期、更具体的原因码时选 5.0；老设备、老 Broker 或只需要基础订阅发布时，3.1.1 更省事，行为也更简单。"
            }
          },
          {
            "@type": "Question",
            "name": "会话过期应该填多少？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "填 0 表示断开即过期；填大于 0 的秒数时，Broker 会在该时间内保留会话与离线消息。做离线消息测试时，需要 Clean Start 关闭并设置正数过期时间。"
            }
          },
          {
            "@type": "Question",
            "name": "接收最大值有什么用？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "它限定 Broker 可同时向客户端投递的 QoS 1/2 报文数量，避免设备或客户端被大量在途消息压垮。留空则不下发该属性，由 Broker 使用默认值。"
            }
          },
          {
            "@type": "Question",
            "name": "5.0 的订阅失败原因码在哪里看？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "订阅开关打开后会等待 Broker 确认，失败时通知与日志会显示 SUBACK 原因码，例如 0x87 未授权、0x8F 过滤器非法、0x97 配额超限。"
            }
          }
        ]
      }
---

# MQTT 5.0 客户端：会话、原因码与 3.1.1 差异

MQTT 5.0 不是「换个大版本号」：它把连接被拒、订阅失败、发布被拒的原因写进了协议，还增加了会话过期与流控属性。NetAssistant 的 5.0 实现聚焦调试最常用的部分——看得懂失败原因、控得住离线会话，同时不牺牲轻量。

## 5.0 能解决什么问题

- **失败原因可读**：Broker 拒绝连接、订阅或发布时返回具体原因码，不再只有「失败」两个字
- **会话可控**：用会话过期决定断开后 Broker 保留多久的会话与离线消息
- **流控可调**：用接收最大值限制同时投递的 QoS 1/2 报文数量
- **版本可切换**：同一份连接配置在 3.1.1 与 5.0 之间切换，便于对比 Broker 行为

## 支持的能力

| 能力 | NetAssistant 5.0 | 说明 |
| ---- | ---- | ---- |
| Clean Start | 支持 | 沿用连接表单的 Clean Session 开关；Client ID 为空时强制 clean start |
| 会话过期 | 支持 | 单位秒，留空不下发；0 表示断开即过期 |
| 接收最大值 | 支持 | 1–65535，留空不下发 |
| 连接原因码 | 支持 | CONNACK 被拒时显示原因码与 Broker 说明 |
| 订阅原因码 | 支持 | SUBACK 失败时显示具体原因码 |
| 发布原因码 | 支持 | QoS 1/2 的 PUBACK / PUBREC 失败原因 |
| 断开原因 | 支持 | Broker 主动 DISCONNECT 时显示原因 |
| QoS / retain / 遗嘱 | 支持 | 行为与 3.1.1 一致 |

## 未提供的高级特性

当前版本不提供以下能力，需要时请使用 MQTTX 或其他完整实现：

- 用户属性（User Properties）
- Will Delay（遗嘱延迟）
- 订阅标识符（Subscription Identifier）
- 共享订阅（Shared Subscription，`$share/...`）
- 其他 5.0 高级特性（如主题别名等）

## 与 3.1.1 的差异

| 维度 | MQTT 3.1.1 | MQTT 5.0 |
| ---- | ---- | ---- |
| 连接被拒 | CONNACK 返回码 1–5 | 原因码，如 `0x86` 认证失败、`0x87` 未授权 |
| 订阅失败 | 只有拒绝结果 | SUBACK 原因码，可区分权限、过滤器、配额 |
| 发布失败 | PUBACK 不带原因码 | 可携带原因码，能定位 ACL 与 topic 问题 |
| 会话过期 | 无此属性 | 可设置 0 或正数秒 |
| 接收最大值 | 无此属性 | 1–65535，可选 |
| Broker 主动断开 | 通常直接断开 | 可带原因码与说明 |

## Clean Start 与会话过期

- **Clean Start 开启**：每次连接都新建会话，Broker 丢弃旧会话与离线消息
- **Clean Start 关闭 + 会话过期 0**：连接期间保留状态，断开即清理
- **Clean Start 关闭 + 会话过期 > 0**：Broker 在过期前保留订阅与离线消息，重连后可继续收

做离线消息测试时，先关闭 Clean Session（即关闭 Clean Start），把会话过期设成 300 秒，订阅 QoS 1 并断开，再向该 topic 发布消息，重连后观察是否补发。Broker 对离线队列的实现和上限不同，结果以 Broker 行为为准。

Client ID 为空时，5.0 会强制 clean start，此时会话过期不会生效；需要持久会话时请填写固定且唯一的 Client ID。

## 接收最大值怎么填

接收最大值告诉 Broker「我最多同时处理多少条在途 QoS 1/2 消息」。默认留空即可；在低配设备、弱网或需要限制突发流量时再设置：

- 设备内存小：填 10–50，降低同时投递量
- 正常桌面调试：留空，使用 Broker 默认值
- 排查「消息堆积」：临时填小值，观察 Broker 是否按窗口投递

## 原因码速查

| 原因码 | 含义 | 常见场景 |
| ---- | ---- | ---- |
| `0x84` | 不支持的协议版本 | Broker 不支持 5.0 |
| `0x85` | Client ID 无效 | 为空、超长或含非法字符 |
| `0x86` | 用户名或密码错误 | 凭据不正确 |
| `0x87` | 未授权 | ACL 不允许连接、订阅或发布 |
| `0x88` | Broker 不可用 | 维护、过载 |
| `0x89` | Broker 忙 | 重试或限流 |
| `0x8A` | 客户端被封禁 | 联系管理员 |
| `0x8F` | topic 过滤器非法 | 通配符写法错误 |
| `0x90` | topic 名称非法 | 发布 topic 为空或含通配符 |
| `0x97` | 配额超限 | 订阅数或在途消息过多 |
| `0x99` | payload 格式非法 | Broker 声明了格式校验 |
| `0x9F` | 连接频率超限 | 重连过快 |

完整排查步骤见 [MQTT 排错](/mqtt-troubleshooting)。

## 什么时候选 5.0

- 需要区分配权失败、topic 非法、配额超限等具体原因
- 需要测试离线消息与会话过期
- 需要限制 Broker 的并发投递量
- Broker 与云平台明确支持 5.0

只需要基础订阅、发布与 TLS 联调时，3.1.1 也完全够用，而且日志更简单、兼容性更广。

## 常见问题

### NetAssistant 支持完整的 MQTT 5.0 吗？

覆盖基础能力：版本切换、Clean Start、会话过期、接收最大值与原因码；用户属性、Will Delay、订阅标识符、共享订阅等高级特性暂未提供。

### 5.0 和 3.1.1 应该选哪个？

需要会话过期与具体原因码时选 5.0；老设备、老 Broker 或只做基础调试时选 3.1.1。

### 会话过期应该填多少？

0 表示断开即过期；做离线消息测试时关闭 Clean Start，填一个正数秒数。

### 接收最大值有什么用？

限制 Broker 同时投递的 QoS 1/2 报文数量；留空使用 Broker 默认值。

### 5.0 的订阅失败原因码在哪里看？

订阅开关等待 SUBACK 后，失败通知与日志会显示原因码，例如 `0x87`、`0x8F`、`0x97`。

## 相关页面

- 连接与订阅参数：[MQTT 客户端](/mqtt-client#mqtt-5-0-可选)
- 订阅失败与连接被拒排查：[MQTT 排错](/mqtt-troubleshooting)
- 从零开始连接：[MQTT 调试教程](/guide/mqtt)
- [下载 NetAssistant](/download)
