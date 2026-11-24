---
title: MQTT 测试工具 - QoS、retain、遗嘱与通配符验证
description: 用 NetAssistant 验证 MQTT 常见行为：QoS 0/1/2、retain 与清除、遗嘱消息、通配符订阅、ACL 权限与自动回复；并明确说明 MQTT 暂不支持压测。
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
            "name": "怎么测试 MQTT 遗嘱消息？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "在连接设置的「更多设置」里配置遗嘱 topic、payload、QoS 与 retain，连接后用另一个客户端订阅该 topic，再强制结束进程或断网让连接异常断开。主动点击断开属于正常断开，不会触发遗嘱。"
            }
          },
          {
            "@type": "Question",
            "name": "retain 消息怎么清除？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "向同一个 topic 发布空 payload 并勾选 retain，Broker 会删除该 topic 的保留消息；之后新订阅者不会再收到。"
            }
          },
          {
            "@type": "Question",
            "name": "QoS 1 为什么会收到重复消息？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "QoS 1 是至少一次语义，重传或重连可能导致重复投递，需要应用层按消息 ID 或业务键去重；QoS 2 才保证仅一次。"
            }
          },
          {
            "@type": "Question",
            "name": "通配符订阅为什么收不到消息？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "常见原因是 topic 与过滤器层级不匹配、`#` 没有放在末尾、`+` 没有占满一整层，或者订阅的是以 `$` 开头的系统 topic 却没用显式主题订阅。"
            }
          },
          {
            "@type": "Question",
            "name": "可以对 MQTT 做压力测试吗？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "NetAssistant 的压测功能只用于 TCP/UDP，不提供 MQTT 压测。MQTT 并发与吞吐测试请使用 MQTTX Bench、emqtt_bench 等专用工具。"
            }
          }
        ]
      }
---

# MQTT 测试工具：QoS、retain、遗嘱与通配符验证

联调时真正要确认的往往不是「能不能连上」，而是「消息会不会丢、会不会重复、离线后能不能补、权限有没有生效」。下面这些场景都可以用 NetAssistant 的两个连接（一个订阅、一个发布）完成，不需要额外脚本。

## 测试前准备

- 一个可用的 Broker，本地 Mosquitto 即可；配置方法见 [MQTT 调试教程](/zh/guide/mqtt#本地起一个-broker-测试)
- 两条 MQTT 连接：一条订阅目标 topic，一条发布
- 需要观察离线行为时，使用固定 Client ID，并留意 Clean Session / Clean Start

## QoS 0/1/2 验证

| QoS | 语义 | 观察点 |
| ---- | ---- | ---- |
| 0 | 至多一次 | 发送最快，网络抖动时可能直接丢失，没有 Broker 回执 |
| 1 | 至少一次 | 可能重复；断开重连后适合观察补发与重复 |
| 2 | 仅一次 | 开销最大，正常情况下不应重复 |

测试方法：订阅 `test/qos/#`，分别用 QoS 0/1/2 发布同一 payload，对比消息数量与时间。QoS 1 出现重复是协议允许的行为，不是客户端 bug；需要去重时在业务侧处理。

5.0 下如果发布被拒，通知会显示原因码，例如 `0x87` 未授权、`0x97` 配额超限；3.1.1 只能看到本地发送失败，需要查 Broker 日志。

## retain 测试

1. 发布一条消息到 `test/retain`，勾选 **retain**
2. 新开一条连接并订阅 `test/retain`，应立即收到最后一条保留消息
3. 发布空 payload 并勾选 retain，清除该 topic 的保留消息
4. 再次新订阅，确认不再收到

注意：retain 是 Broker 行为，部分 Broker 或云平台会关闭该能力，发布时可能返回 `0x9A`（不支持 retain）。

## 遗嘱消息测试

1. 在连接配置的「更多设置」中启用遗嘱，填写 topic、payload、QoS 与 retain
2. 用另一条连接订阅遗嘱 topic
3. 连接后强制结束 NetAssistant 进程，或断开网络等待 KeepAlive 超时
4. 订阅端应收到遗嘱消息

主动点击「断开」属于正常断开，不会触发遗嘱。NetAssistant 不支持 5.0 的 Will Delay，遗嘱会在 Broker 检测到异常断开后立即发布。如果没收到，检查遗嘱 topic 的发布权限与订阅过滤器。

## 通配符与系统 topic

| 过滤器 | 能匹配 | 不能匹配 |
| ---- | ---- | ---- |
| `device/+/status` | `device/1/status` | `device/1/2/status`、`device/status` |
| `device/#` | `device`、`device/1/status` | 以 `$` 开头的系统 topic |
| `$SYS/#` | `$SYS/broker/uptime` 等 | 普通业务 topic |

`#` 必须放在末尾，`+` 必须占满一整层。共享订阅 `$share/group/topic` 当前不支持，订阅会失败。

## ACL 与权限测试

用受限账号分别验证连接、订阅、发布：

- 连接被拒：5.0 返回 `0x87`，3.1.1 返回码 5
- 订阅被拒：5.0 SUBACK `0x87`，订阅开关自动关闭并提示
- 发布被拒：5.0 PUBACK `0x87`，右上角通知带 topic 与原因

只允许订阅、不允许发布的 ACL 会让订阅端一直收不到消息，排查时先确认发布端是否成功。

## 离线消息与会话

- 3.1.1：关闭 Clean Session，固定 Client ID，QoS 1 订阅后断开，再发布，重连观察补发
- 5.0：关闭 Clean Start，会话过期设为正数（如 300 秒），其余步骤相同
- Clean Session / Clean Start 开启时，断开即清理会话，不会补发离线消息

Broker 对离线队列长度和过期策略有各自限制，结果以 Broker 配置为准。

## 自动回复与周期发送

- **自动回复**：应答会发回收到消息的 topic，适合模拟设备响应平台指令
- **周期发送**：按当前发布 topic 定时发送，适合模拟心跳或周期上报

两者都复用现有调试能力，不额外引入脚本。测试自动回复时避免「回复又被规则匹配」造成循环，必要时收窄规则或降低频率。

## 暂不支持压测

MQTT 暂不支持压测：压测引擎只适用于 TCP/UDP，MQTT 模式不提供连接数、吞吐和时延压测。需要 MQTT 压测时使用 MQTTX Bench、emqtt_bench 等工具，本工具更适合做功能验证与单连接行为观察。

## 常见问题

### 怎么测试 MQTT 遗嘱消息？

配置遗嘱后用另一条连接订阅，再强制结束进程或断网；主动断开不会触发。

### retain 消息怎么清除？

向同一 topic 发布空 payload 并勾选 retain。

### QoS 1 为什么会收到重复消息？

QoS 1 是至少一次语义，重传或重连可能重复，需要应用层去重。

### 通配符订阅为什么收不到消息？

检查层级匹配、`#` 位置、`+` 是否占满整层，以及 `$` 系统 topic 是否显式订阅。

### 可以对 MQTT 做压力测试吗？

不可以，压测仅用于 TCP/UDP；MQTT 压测请用 MQTTX Bench 或 emqtt_bench。

## 相关页面

- 参数与界面说明：[MQTT 客户端](/zh/mqtt-client)
- 失败原因与原因码：[MQTT 排错](/zh/mqtt-troubleshooting)
- 5.0 会话与原因码：[MQTT 5.0 客户端](/zh/mqtt-5)
- [下载 NetAssistant](/zh/download)
