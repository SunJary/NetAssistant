---
title: MQTT 客户端 Windows - winget 安装与调试指南
description: 在 Windows 10/11 上安装 NetAssistant MQTT 客户端：winget 一条命令、安装程序或免安装 zip；支持 MQTT 3.1.1 / 5.0、TLS、订阅开关与约 20MB 内存，适合现场与远程桌面调试。
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
            "name": "Windows 上怎么安装 MQTT 客户端？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "推荐在终端执行 winget install SunJary.NetAssistant，后续用 winget upgrade 升级；也可以从下载页获取安装程序或免安装 zip。"
            }
          },
          {
            "@type": "Question",
            "name": "支持哪些 Windows 版本？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "支持 Windows 10 1709 及以上与 Windows 11，提供 x64 与 ARM64 构建。"
            }
          },
          {
            "@type": "Question",
            "name": "winget 提示找不到包怎么办？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "先更新 App Installer（应用安装程序）与 winget 源，再重试；仍找不到时直接用下载页的安装程序或免安装 zip。"
            }
          },
          {
            "@type": "Question",
            "name": "需要额外放行防火墙吗？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "客户端主动连接 Broker 属于出站流量，通常无需放行；如果公司网络限制出站端口，需要允许 1883 或 8883。"
            }
          },
          {
            "@type": "Question",
            "name": "和 Windows 网络调试助手是同一个软件吗？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "是。NetAssistant 在同一个应用里提供 TCP/UDP 客户端与服务端、MQTT 客户端、粘包解码与 TCP/UDP 压测；MQTT 暂不支持压测。"
            }
          }
        ]
      }
---

# MQTT 客户端 Windows：安装、连接与现场调试

Windows 上的 MQTT 客户端不少，但多数是基于 Electron 的专用工具：为了偶尔看几条报文，常驻几百 MB 内存并不划算。NetAssistant 是 Rust 原生的网络调试工具，MQTT 3.1.1 / 5.0 与 TCP/UDP 调试在同一个应用里，内存约 20MB，适合长期挂在远程桌面或低配现场机。

## 为什么在 Windows 上选它

- **原生桌面应用**：不是 Electron 套壳，启动快、内存低
- **协议齐全**：TCP/UDP 客户端与服务端 + MQTT 3.1.1 / 5.0
- **安装方式灵活**：winget、安装程序、免安装 zip
- **调试能力完整**：订阅独立开关、QoS 0/1/2、retain、遗嘱、自动回复、搜索与导出
- **TLS 可用**：三档证书校验，支持客户端证书与私钥做双向认证

## 系统要求

- Windows 10 1709 及以上，或 Windows 11
- x64 或 ARM64
- 出站网络可访问 Broker 的 1883（明文）或 8883（TLS）端口

## 安装

### 推荐：winget

```powershell
winget install SunJary.NetAssistant
```

升级：

```powershell
winget upgrade SunJary.NetAssistant
```

### 安装程序或免安装 zip

从[下载页](/zh/download)获取 Windows 安装程序，或下载 zip 解压后直接运行。免安装版适合没有管理员权限、需要随身携带或放在 U 盘里的场景。

首次运行时 SmartScreen 可能提示未知发布者；确认来源可信后，选择「更多信息 → 仍要运行」。公司设备若受策略限制，请按 IT 要求处理。

## 第一次连接 MQTT

1. 打开 NetAssistant，点击 `+新建`，协议选择 **MQTT**
2. 填写 Broker 地址、端口（1883 或 8883）、Client ID、用户名与密码
3. 需要加密时打开 TLS，选择证书校验方式
4. 连接成功后，在左侧订阅 `test/#` 或真实业务 topic
5. 在发送区填写 topic、QoS、retain 和 payload，点击发布

完整步骤与截图见 [MQTT 调试教程](/zh/guide/mqtt)。

## 适合 Windows 现场的场景

- **远程桌面 / 跳板机**：内存占用低，长时间挂着不拖慢机器
- **多环境切换**：连接名称区分测试、预发与生产，标签页并行打开
- **TCP/UDP 与 MQTT 混合**：网关、私有协议和 MQTT 在同一个工具里排查
- **离线安装**：zip 版本不依赖在线安装器，适合内网机器
- **配置留存**：连接配置自动保存，下次启动直接连

如果你更关心 Windows 上的 TCP/UDP 调试，可以看 [Windows 网络调试助手](/zh/windows-network-debug-tool)。

## 常见问题

### Windows 上怎么安装 MQTT 客户端？

运行 `winget install SunJary.NetAssistant`，或从下载页获取安装程序 / 免安装 zip。

### 支持哪些 Windows 版本？

Windows 10 1709 及以上与 Windows 11，提供 x64 与 ARM64 构建。

### winget 提示找不到包怎么办？

更新 App Installer 与 winget 源后重试；仍找不到就用安装程序或 zip。

### 需要额外放行防火墙吗？

客户端连接 Broker 是出站流量，通常无需放行；公司网络限制出站端口时允许 1883 / 8883。

### 和 Windows 网络调试助手是同一个软件吗？

是。MQTT 与 TCP/UDP 调试都在 NetAssistant 里。

## 相关页面

- 功能与参数：[MQTT 客户端](/zh/mqtt-client)
- 连接与订阅教程：[MQTT 调试教程](/zh/guide/mqtt)
- 连接失败排查：[MQTT 排错](/zh/mqtt-troubleshooting)
- 其他平台：[macOS 版](/zh/mqtt-client-mac)、[Linux 版](/zh/mqtt-client-linux)
- 常见问题：[MQTT 常见问题](/zh/mqtt-faq)
- [下载 Windows 版](/zh/download)
