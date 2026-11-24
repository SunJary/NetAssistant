---
title: MQTT 客户端 Mac - Apple Silicon 安装与调试
description: 在 macOS 上安装 NetAssistant MQTT 客户端：Universal DMG 与 zip，兼容 Apple Silicon 与 Intel；支持 MQTT 3.1.1 / 5.0、TLS、双向认证与约 20MB 内存。
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
            "name": "macOS 上怎么安装 MQTT 客户端？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "从下载页获取 netassistant-macos-universal.dmg，打开后把 NetAssistant 拖入 Applications；也可以下载 zip 解压即用。"
            }
          },
          {
            "@type": "Question",
            "name": "支持 Apple Silicon 吗？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "支持。Universal 包同时兼容 M1/M2/M3 等 Apple Silicon 与 Intel Mac，不需要 Rosetta。"
            }
          },
          {
            "@type": "Question",
            "name": "提示无法验证开发者怎么办？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "这是未公证应用的正常提示，并非文件损坏。右键点击 NetAssistant 选择「打开」，或在「系统设置 → 隐私与安全性」中选择「仍要打开」。"
            }
          },
          {
            "@type": "Question",
            "name": "可以用 Homebrew 安装吗？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "当前提供 Universal DMG 与 zip 两种方式，暂未提供 Homebrew Cask；安装后可按常规方式启动与升级。"
            }
          },
          {
            "@type": "Question",
            "name": "Mac 版支持 MQTT TLS 吗？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "支持。证书校验可选系统根证书、自定义 CA 或跳过校验，并可填写客户端证书与私钥（PEM）完成双向认证。"
            }
          }
        ]
      }
---

# MQTT 客户端 Mac：Apple Silicon 安装与调试

macOS 上能同时做 TCP/UDP 与 MQTT 调试的原生工具并不多：老牌网络调试助手没有 Mac 版，多数 MQTT 客户端又是 Electron 应用。NetAssistant 用 Rust + GPUI 构建，提供 Universal 安装包，Apple Silicon 与 Intel 都能原生运行，内存约 20MB。

## 为什么在 Mac 上选它

- **原生而非套壳**：启动快、内存低，长期挂着不发热
- **Universal 包**：M1/M2/M3 与 Intel 共用同一个 DMG，无需 Rosetta
- **协议齐全**：TCP/UDP 客户端与服务端 + MQTT 3.1.1 / 5.0
- **TLS 完整**：系统根证书、自定义 CA、跳过校验，支持 mTLS
- **调试顺手**：订阅独立开关、QoS 0/1/2、retain、遗嘱、自动回复、搜索与导出

## 系统要求

- macOS 10.15 及以上
- Apple Silicon 或 Intel
- 可访问 Broker 的 1883（明文）或 8883（TLS）端口

## 安装

### 推荐：Universal DMG

1. 从[下载页](/zh/download)获取 `netassistant-macos-universal.dmg`
2. 打开 DMG，把 NetAssistant 拖入 Applications
3. 首次打开若提示「无法验证开发者」，右键点击 NetAssistant → 打开，或在「系统设置 → 隐私与安全性」中选择「仍要打开」

这是应用未公证时的正常现象，不需要关闭 Gatekeeper。

### 备选：zip

下载 `netassistant-macos-universal.zip`，解压后直接运行，文件会保留可执行权限。

## 第一次连接 MQTT

1. 打开 NetAssistant，点击 `+新建`，协议选择 **MQTT**
2. 填写 Broker 地址、端口、Client ID、用户名与密码
3. 需要加密时打开 TLS，按证书来源选择校验方式
4. 连接成功后订阅 `test/#` 或业务 topic
5. 在发送区填写 topic、QoS、retain 与 payload，点击发布

完整步骤见 [MQTT 调试教程](/zh/guide/mqtt)。

## 在 macOS 上配置 TLS

- 证书与私钥使用 PEM 文件路径，支持 PKCS#8、PKCS#1 与 SEC1
- 私有 CA 选择「自定义 CA」，指向 CA 的 PEM；公共 CA 用默认的系统根证书
- 双向认证需成对填写客户端证书与私钥；配置只保存路径，不保存私钥内容
- 本地联调自签证书时可临时跳过校验，生产环境不要使用

握手失败的排查见 [MQTT 排错](/zh/mqtt-troubleshooting#tls-握手失败)。

## 适合 Mac 开发者的场景

- 本地 Docker 跑 EMQX / Mosquitto，Mac 上直接连 `127.0.0.1`
- 同时开测试、预发、生产多个连接，标签页切换
- 与 TCP/UDP 私有协议一起调试，不用在两个应用之间复制粘贴
- 出差或现场用笔记本调试，低内存不拖慢其他工具

如果你更关心 macOS 上的 TCP/UDP 调试，可以看 [Mac 网络调试助手](/zh/mac-network-debug-tool)。

## 常见问题

### macOS 上怎么安装 MQTT 客户端？

下载 Universal DMG 拖入 Applications，或使用 zip 解压即用。

### 支持 Apple Silicon 吗？

支持，Universal 包原生兼容 Apple Silicon 与 Intel，无需 Rosetta。

### 提示无法验证开发者怎么办？

右键打开，或在隐私与安全性中选择「仍要打开」；这是未公证应用的正常提示。

### 可以用 Homebrew 安装吗？

当前提供 DMG 与 zip，暂未提供 Homebrew Cask。

### Mac 版支持 MQTT TLS 吗？

支持三档证书校验与双向认证。

## 相关页面

- 功能与参数：[MQTT 客户端](/zh/mqtt-client)
- 连接与订阅教程：[MQTT 调试教程](/zh/guide/mqtt)
- 连接失败排查：[MQTT 排错](/zh/mqtt-troubleshooting)
- 其他平台：[Windows 版](/zh/mqtt-client-windows)、[Linux 版](/zh/mqtt-client-linux)
- 常见问题：[MQTT 常见问题](/zh/mqtt-faq)
- [下载 macOS 版](/zh/download)
