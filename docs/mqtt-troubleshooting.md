---
title: MQTT 排错 - 连接失败、订阅收不到消息与 TLS 报错
description: 按现象排查 MQTT 问题：连接被拒、认证失败、订阅收不到消息、TLS 握手失败、中文乱码、QoS 1 重复与断线重连；每条给出原因、原因码与操作步骤。
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
            "name": "MQTT 连接被拒绝最常见的原因是什么？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "最常见的是地址或端口不对、TLS 开关与端口不匹配、Client ID 重复、用户名密码错误和 ACL 未授权。3.1.1 的 CONNACK 返回码和 5.0 的原因码能直接区分这几类问题。"
            }
          },
          {
            "@type": "Question",
            "name": "订阅成功但收不到消息怎么办？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "先确认订阅开关已开启、发布 topic 能被通配符过滤器匹配，再检查 retained、QoS、发布端 ACL 与会话设置，最后用另一个客户端或 mosquitto_sub 交叉验证。"
            }
          },
          {
            "@type": "Question",
            "name": "MQTT TLS 证书错误怎么处理？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "证书链不受信任时改用自定义 CA；主机名不匹配时使用证书中的 SAN 地址；证书过期需要更换；Broker 要求双向认证时补客户端证书与私钥。仅本地联调可临时跳过校验。"
            }
          },
          {
            "@type": "Question",
            "name": "MQTT 5.0 的原因码在哪里看？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "连接被拒、订阅失败和 Broker 主动断开时，NetAssistant 会在连接状态或右上角通知中显示原因码与 broker 说明；3.1.1 只有 CONNACK 返回码，订阅失败通常只能看到拒绝结果。"
            }
          },
          {
            "@type": "Question",
            "name": "为什么 QoS 1 会收到重复消息？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "QoS 1 的语义是至少一次，网络重传或重连后可能重复投递，这是协议允许的行为，需要应用按消息标识或业务键去重。只有 QoS 2 才保证仅一次。"
            }
          }
        ]
      }
---

# MQTT 排错：连接失败、订阅收不到消息与 TLS 报错

MQTT 的问题大多能归到几类：网络不通、认证或权限不对、topic 不匹配、TLS 配置不一致。下面按现象逐项排查，每条都给出原因和可执行的操作；不确定从哪里开始时，先做第一步的三个检查。

## 先做三个快速检查

1. **端口通不通**：Windows 用 `Test-NetConnection broker.example.com -Port 1883`，macOS / Linux 用 `nc -vz broker.example.com 1883`。超时通常是网络、VPN、防火墙或云安全组问题。
2. **地址、端口、TLS 是否配对**：1883 对应明文，8883 对应 TLS；端口用 8883 却没开 TLS，或者 1883 开了 TLS，都会握手失败。NetAssistant 不支持 MQTT over WebSocket，`ws://` 或 `wss://` 地址无法直连。
3. **Client ID 与凭据是否正确**：同一 Broker 上 Client ID 必须唯一，重复会把前一个连接踢下线；用户名密码错误或被 ACL 拒绝会直接体现在 CONNACK 返回码里。

> 只有你的机器连不上、别人能连上时，优先检查公司网络、代理、VPN 与云平台 IP 白名单。

## 连接被拒绝或一直连不上

| 现象 / 返回码 | 常见原因 | 处理 |
| ---- | ---- | ---- |
| Connection refused | Broker 未启动、端口填错、防火墙拦截 | 确认 Broker 进程与监听端口，换 127.0.0.1 或内网地址验证 |
| 连接超时 | 网络不通、VPN、安全组、IP 白名单 | 先用上面的端口探测，再查云平台访问规则 |
| 3.1.1 返回码 1 / 5.0 `0x84` | Broker 不支持所选协议版本 | 把协议版本从 5.0 切到 3.1.1，或升级 Broker |
| 3.1.1 返回码 2 / 5.0 `0x85` | Client ID 为空、过长、含非法字符或重复 | 换一个唯一 Client ID；用「复制连接」时确认已生成新 ID |
| 3.1.1 返回码 3 / 5.0 `0x88` | Broker 不可用、过载或维护中 | 看 Broker 日志与资源占用，稍后重试 |
| 3.1.1 返回码 4 / 5.0 `0x86` | 用户名或密码错误 | 核对凭据，注意多余空格与特殊字符 |
| 3.1.1 返回码 5 / 5.0 `0x87` | 未授权，ACL 不允许该用户连接 | 在 Broker 或云平台补充 ACL / 权限策略 |
| 5.0 `0x89` / `0x8A` / `0x9F` | Broker 忙、客户端被封禁、连接频率超限 | 降低重连频率，联系 Broker 管理员解封 |

5.0 的原因码会直接显示在连接状态或通知里；3.1.1 的信息有限，需要结合 Broker 日志定位。连接失败后不要连续快速重连，部分 Broker 会按频率封禁。

## TLS 握手失败

| 提示 | 原因 | 处理 |
| ---- | ---- | ---- |
| certificate verify failed / unknown CA | 证书链不受信任，常见于自建 CA | 选择「自定义 CA」并填入 CA 的 PEM 路径 |
| hostname mismatch | 连接地址不在证书 SAN 中 | 用证书里的域名连接，或为 Broker 换签包含该地址的证书 |
| certificate expired | 证书过期 | 更新 Broker 证书；联调阶段可临时跳过校验 |
| Broker 要求客户端证书 | 开启了双向认证 | 填写成对的客户端证书与私钥（PEM） |
| handshake failure / 无响应 | 端口与 TLS 开关不匹配，或 ALPN 要求不一致 | 8883 开启 TLS、1883 关闭 TLS；按 Broker 要求填写 ALPN |

「跳过校验」只跳过身份验证，传输仍然加密，适合自签证书联调，不建议用于生产环境。

## 订阅失败

订阅开关打开后会等待 Broker 确认（SUBACK），失败或 10 秒无响应会自动关闭该条并提示原因：

- **5.0 `0x87` 未授权**：ACL 不允许订阅该 topic，检查 Broker 或云平台权限
- **5.0 `0x8F` topic 过滤器非法**：通配符写法错误，`#` 必须在末尾，`+` 只能占一整层
- **5.0 `0x97` 配额超限**：订阅数量超过 Broker 限制，清理不用的订阅
- **3.1.1 被拒绝**：通常只能看到失败结果，去 Broker 日志确认 ACL
- **10 秒无响应**：Broker 卡顿、网络中断或连接已失效，先确认连接状态

NetAssistant 不支持共享订阅（`$share/...`）与 5.0 订阅标识符，这类 topic 会订阅失败。以 `$` 开头的系统 topic 需要显式订阅，`#` 不会匹配。

## 订阅成功但收不到消息

按顺序检查：

1. **订阅开关是否开启**：关闭状态会持久化，重启或重连后保持关闭。
2. **topic 是否真的匹配**：`sensor/1/temp` 能被 `sensor/+/temp` 匹配，但不匹配 `sensor/temp`；多层用 `sensor/#`。
3. **是否先发布后订阅**：普通消息不会补发，只有发布时设置了 retain，新订阅者才会立即收到最后一条。
4. **retain 是否符合预期**：发布端没勾 retain，Broker 不会保留；发送空 payload 可以清除 retained 消息。
5. **QoS 与会话**：QoS 0 在网络抖动时可能丢；离线消息只有在持久会话下才会保留（3.1.1 关闭 Clean Session，或 5.0 关闭 Clean Start 且会话过期大于 0）。
6. **ACL 是否只放开了订阅**：部分 Broker 会分别限制订阅与发布权限，发布被拒时订阅端自然收不到。
7. **Broker 侧规则**：桥接、topic 重写、云平台物模型转换都可能改变 topic。

交叉验证的方法：用另一个客户端或 `mosquitto_sub -h <broker> -t 'test/#' -v` 订阅同一 topic，再从 NetAssistant 发布。如果命令行能收到，问题在 NetAssistant 的订阅配置；如果也收不到，问题在 Broker、发布端或 ACL。

## 发布失败

MQTT 5 下 QoS 1/2 被 Broker 拒收时，通知会带具体原因码：

- `0x87` 未授权：发布 ACL 不允许该 topic
- `0x90` topic 非法：名称为空、含通配符或超长
- `0x97` 配额超限：在途消息或队列达到上限
- `0x99` payload 格式非法：Broker 声明了 payload 格式但内容不符
- `0x9A` / `0x9B`：Broker 不支持 retain 或该 QoS

3.1.1 的 PUBACK 不携带原因码，只能提示本地发送失败，需要结合 Broker 日志判断。QoS 0 没有回执，只有本地投递失败才会提示。高频发布同一 topic 时，相同通知 3 秒内只弹一次，但消息日志仍逐条记录，排查时以日志为准。

## 中文乱码

MQTT 的 payload 本质是字节流。NetAssistant 文本模式按 UTF-8 解码，如果发送端用 GBK 等编码，中文就会乱码：

- 接收端切到 Hex 模式看原始字节，再按发送端编码离线解码
- 发送端统一改用 UTF-8，JSON 报文确认序列化编码
- topic 按协议要求必须是 UTF-8；topic 乱码说明发布端编码就有问题

## QoS 1 重复消息

QoS 1 是「至少一次」，网络重传或断线重连后可能重复投递，这是协议允许的行为。需要去重时，在业务侧按消息 ID、时间戳加业务键或幂等表处理；对重复零容忍的场景改用 QoS 2，但开销更大。

## 重连后订阅消失或状态不对

- 只恢复处于开启状态的订阅，手动关闭的订阅不会自己打开
- 持久会话下 Broker 仍保留订阅，离线时取消会提示先连接
- Clean Session / Clean Start 为 true 时，会话与离线消息会被清空
- 复制连接会生成新 Client ID，旧连接的状态不会带过来

## 遗嘱消息没收到

遗嘱消息只在异常断开时由 Broker 发布，例如网络中断、KeepAlive 超时；主动点击断开属于正常断开，不会触发遗嘱。另外确认遗嘱 topic 的发布权限、订阅过滤器是否匹配；NetAssistant 不支持 5.0 的 Will Delay。

## 还是解决不了

提交问题前收集这些信息，能显著缩短定位时间：

- Broker 地址、端口、协议版本，以及是否启用 TLS
- 完整错误提示或原因码（截图即可）
- Client ID、用户名（不要贴密码）与订阅 topic / 发布 topic
- 发布时的 QoS 与 retain 设置
- Broker 侧日志，以及 `mosquitto_sub` 等命令行客户端的对比结果

可以到 [GitHub Issues](https://github.com/SunJary/NetAssistant/issues) 反馈，注意隐去密码、私钥与内网地址。

## 常见问题

### MQTT 连接被拒绝最常见的原因是什么？

地址或端口不对、TLS 开关与端口不匹配、Client ID 重复、用户名密码错误和 ACL 未授权。3.1.1 的 CONNACK 返回码和 5.0 的原因码能直接区分这几类问题。

### 订阅成功但收不到消息怎么办？

确认订阅开关已开启、topic 能被过滤器匹配，再检查 retained、QoS、ACL 与会话设置，最后用 `mosquitto_sub` 交叉验证。

### MQTT TLS 证书错误怎么处理？

证书链不受信任时改用自定义 CA；主机名不匹配时使用证书 SAN 中的地址；过期证书需要更换；双向认证要补客户端证书与私钥。

### MQTT 5.0 的原因码在哪里看？

连接被拒、订阅失败与 Broker 主动断开时，状态区或右上角通知会显示原因码与 broker 说明；3.1.1 只有 CONNACK 返回码。

### 为什么 QoS 1 会收到重复消息？

QoS 1 是至少一次语义，重传或重连后可能重复，需要应用层去重；QoS 2 才保证仅一次。

## 相关页面

- 从零开始连接与订阅：[MQTT 调试教程](/guide/mqtt)
- 功能与参数说明：[MQTT 客户端](/mqtt-client)
- 和 MQTTX 的差异：[MQTTX 替代品对比](/comparison-mqttx)
- 验证 QoS、retain 与遗嘱行为：[MQTT 测试场景](/mqtt-testing)
- [下载 NetAssistant](/download)
