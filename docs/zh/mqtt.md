---
title: MQTT 调试指南 - 教程、TLS、5.0 与排错
description: NetAssistant MQTT 调试总览：从连接 Broker、订阅发布，到 TLS 配置、5.0 会话、QoS/retain/遗嘱测试与连接排错，按任务快速进入对应页面。
---

# MQTT 调试指南：从连接到排错

这个页面把 MQTT 相关内容按任务串起来。第一次用先从「快速开始」进入；已经能连上、但消息或证书有问题，直接看排错与 TLS；要选工具则看对比总览。

## 快速开始

1. [下载并安装 NetAssistant](/zh/download)，支持 Windows、macOS 与 Linux
2. 按 [MQTT 调试教程](/zh/guide/mqtt) 新建连接、订阅并发布第一条消息
3. 连不上或收不到消息时，查 [MQTT 排错](/zh/mqtt-troubleshooting)
4. 需要加密连接，按 [MQTT TLS 配置](/zh/mqtt-tls) 选择证书校验方式

## 按任务找页面

| 你现在要做什么 | 进入页面 |
| ---- | ---- |
| 了解客户端支持哪些能力 | [MQTT 客户端](/zh/mqtt-client) |
| 第一次连接 Broker、订阅与发布 | [MQTT 调试教程](/zh/guide/mqtt) |
| 配置 TLS、自签证书、双向认证 | [MQTT TLS](/zh/mqtt-tls) |
| 使用会话过期、接收最大值与原因码 | [MQTT 5.0](/zh/mqtt-5) |
| 验证 QoS、retain、遗嘱、通配符 | [MQTT 测试场景](/zh/mqtt-testing) |
| 连接被拒、订阅失败、收不到消息 | [MQTT 排错](/zh/mqtt-troubleshooting) |
| 在 Windows / macOS / Linux 安装 | [Windows](/zh/mqtt-client-windows)、[macOS](/zh/mqtt-client-mac)、[Linux](/zh/mqtt-client-linux) |
| 和 MQTTX、Explorer、MQTT.fx 对比 | [MQTT 客户端对比](/zh/compare/) |
| 快速查端口、Client ID、QoS 等问题 | [MQTT 常见问题](/zh/mqtt-faq) |

## 能力摘要

- 协议版本：MQTT 3.1.1 与 5.0，可在同一连接配置中切换
- 传输：TCP 直连，TLS 支持系统根证书、自定义 CA、跳过校验与双向认证
- 收发：QoS 0/1/2、retain、遗嘱、通配符订阅、订阅独立开关
- 5.0：Clean Start、会话过期、接收最大值、连接/订阅/发布原因码
- 调试：消息搜索、收藏、TXT / JSON / CSV 导出、自动回复、周期发送
- 边界：不支持 MQTT over WebSocket、脚本、共享订阅与 MQTT 压测

完整参数见 [MQTT 客户端](/zh/mqtt-client)。

## 平台安装

| 平台 | 安装方式 |
| ---- | ---- |
| Windows | winget、安装程序或免安装 zip，见 [Windows 版](/zh/mqtt-client-windows) |
| macOS | Universal DMG 或 zip，见 [macOS 版](/zh/mqtt-client-mac) |
| Linux | deb、AppImage、tar.gz，见 [Linux 版](/zh/mqtt-client-linux) |

## 选型与对比

- [MQTT 客户端对比总览](/zh/compare/)：MQTTX、MQTT Explorer、MQTT.fx 与 NetAssistant 的适用场景
- [MQTTX 对比](/zh/comparison-mqttx)：WebSocket、脚本与完整 5.0 的差异
- [MQTT Explorer 对比](/zh/compare/mqtt-explorer)：topic 树与报文流的选择
- [MQTT.fx 对比](/zh/compare/mqtt-fx)：商业授权与免费开源的取舍

## 建议阅读顺序

- 新用户：教程 → 客户端 → 排错
- 平台/后端：5.0 → 测试场景 → 排错
- 现场/硬件：Windows 或 Linux 安装页 → 教程 → TLS
- 选型：对比总览 → 对应竞品对比页 → 客户端

## 相关页面

- [MQTT 客户端](/zh/mqtt-client)
- [MQTT 调试教程](/zh/guide/mqtt)
- [MQTT 常见问题](/zh/mqtt-faq)
- [下载 NetAssistant](/zh/download)
