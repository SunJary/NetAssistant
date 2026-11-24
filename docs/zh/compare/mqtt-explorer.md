---
title: MQTT Explorer 替代品 - topic 树与报文流对比
description: MQTT Explorer 与 NetAssistant 的差异：前者以 topic 树、retained 清理和数值图表见长；后者以报文流、搜索导出、TCP/UDP 一体和约 20MB 内存见长。按场景选择，附迁移步骤。
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
            "name": "NetAssistant 能替代 MQTT Explorer 吗？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "在报文收发、订阅排查、搜索导出和 TCP/UDP 混合调试场景可以替代；如果你主要依赖 topic 树浏览、数值曲线和 retained 批量清理，MQTT Explorer 仍然更合适。"
            }
          },
          {
            "@type": "Question",
            "name": "NetAssistant 有 topic 树视图吗？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "没有。它按时间展示报文流，每条消息带 topic 标签，支持搜索、过滤与导出。需要按层级浏览整个 Broker 的 topic 时用 MQTT Explorer。"
            }
          },
          {
            "@type": "Question",
            "name": "怎么删除 retained 消息？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "向同一个 topic 发布空 payload 并勾选 retain，Broker 会清除该 topic 的保留消息。NetAssistant 没有针对 topic 树的批量删除按钮。"
            }
          },
          {
            "@type": "Question",
            "name": "两者都免费吗？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "NetAssistant 基于 Apache-2.0 开源，全部功能免费；MQTT Explorer 同样是开源客户端。具体许可与版本以各自项目页面为准。"
            }
          },
          {
            "@type": "Question",
            "name": "可以同时连多个 Broker 吗？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "NetAssistant 用标签页管理多个连接，可以同时打开不同 Broker 或不同账号；MQTT Explorer 也提供多连接配置，具体以官方版本为准。"
            }
          }
        ]
      }
---

# MQTT Explorer 替代品：topic 树与报文流的取舍

MQTT Explorer 把 Broker 上的 topic 画成一棵树，适合从上到下浏览整个命名空间；NetAssistant 则把消息按时间排成报文流，适合盯住某几个 topic 排查收发。两者都是免费开源客户端，差别不在「谁更强」，而在你当前是在**浏览**还是在**调试**。

## MQTT Explorer 擅长什么

- **层级 topic 树**：一眼看清 Broker 上有哪些设备、哪些子主题
- **retained 消息管理**：查看并删除保留消息，清理「僵尸 topic」
- **数值与对比**：把数值型 topic 画成曲线，查看当前消息与上一条的差异
- **Home Assistant / IoT 浏览**：社区里常用它巡检主题结构

如果你的任务是「看看这个 Broker 里都发生了什么」，topic 树确实比报文流直观。

## NetAssistant 的不同

- **报文流 + topic 标签**：每条消息带 topic，双击复制原始 topic，适合盯住 `device/+/status` 这类过滤器
- **搜索与导出**：消息可搜索、收藏，导出 TXT / JSON / CSV，便于把现场数据带回分析
- **TCP/UDP 与 MQTT 同在一个应用**：网关、私有协议和 MQTT 混合调试时不用来回切工具
- **Rust 原生、约 20MB 内存**：低配 Windows 现场、远程桌面、长时间挂着都更轻
- **TLS 更细**：系统根证书 / 自定义 CA / 跳过校验三档，支持客户端证书与私钥做双向认证
- **MQTT 5.0 基础能力**：会话过期、接收最大值、连接/订阅/发布原因码

它没有 topic 树、没有数值曲线，也不提供按树批量删除 retained 的操作。

## 对比表

| 维度 | NetAssistant | MQTT Explorer |
| ---- | ---- | ---- |
| 主视图 | 时间序报文流 + topic 标签 | 层级 topic 树 |
| 适合任务 | 盯 topic 收发、排错、导出数据 | 浏览 Broker 结构、清理 retained、看数值曲线 |
| 协议范围 | TCP/UDP 客户端与服务端 + MQTT 3.1.1 / 5.0 | MQTT |
| 多连接 | 标签页并行管理 | 多连接配置（以官方为准） |
| TLS | 三档校验 + 双向认证 | 支持 TLS |
| 消息搜索 / 导出 | 支持，TXT / JSON / CSV | 以界面浏览为主 |
| 技术栈与体积 | Rust 原生，约 20MB 内存 | 桌面图形客户端 |
| 价格 | 免费开源（Apache-2.0） | 开源免费 |

## 什么场景继续用 MQTT Explorer

- 接手一个陌生 Broker，需要先摸清 topic 结构
- 要批量清理 retained 消息、删除废弃主题
- 需要把数值 topic 画成曲线观察趋势
- 团队已经围绕 topic 树建立了巡检习惯

## 什么场景换成 NetAssistant

- 已经知道 topic，主要是收报文、查字段、复现问题
- 需要把消息导出给后端或写报告
- 现场同时有 TCP/UDP 私有协议和 MQTT
- 机器内存紧张，或通过远程桌面长期挂着客户端
- 需要 mTLS、自定义 CA、MQTT 5.0 原因码等调试能力

## 从 MQTT Explorer 迁移

1. 在 NetAssistant 新建 MQTT 连接，填入同样的 Broker、端口、Client ID 与账号
2. TLS 配置按证书来源选择校验方式；原来用 MQTTS 的记得把端口改成 8883
3. 订阅之前在 MQTT Explorer 里关注的通配符，例如 `home/#` 或 `device/+/status`
4. 用发布区复现问题，消息区搜索关键字，必要时导出 JSON
5. 需要浏览结构时再切回 MQTT Explorer，两边可以连同一个 Broker，但 Client ID 要不同

## 常见问题

### NetAssistant 能替代 MQTT Explorer 吗？

在报文收发、订阅排查、搜索导出和 TCP/UDP 混合调试场景可以；依赖 topic 树、数值曲线和 retained 批量清理时，继续用 MQTT Explorer。

### NetAssistant 有 topic 树视图吗？

没有。它按时间展示报文流，每条消息带 topic 标签，支持搜索与导出。

### 怎么删除 retained 消息？

向同一 topic 发布空 payload 并勾选 retain，Broker 会清除保留消息。

### 两者都免费吗？

NetAssistant 基于 Apache-2.0 开源免费；MQTT Explorer 同样开源，许可与版本以官方页面为准。

### 可以同时连多个 Broker 吗？

NetAssistant 用标签页管理多个连接；MQTT Explorer 也提供多连接配置。

## 相关页面

- 与 MQTTX 的差异：[MQTTX 替代品对比](/zh/comparison-mqttx)
- 功能与参数：[MQTT 客户端](/zh/mqtt-client)
- 从连接到第一条消息：[MQTT 调试教程](/zh/guide/mqtt)
- [下载 NetAssistant](/zh/download)
