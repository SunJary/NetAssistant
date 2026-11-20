---
title: MQTT 客户端 Linux - deb / AppImage 安装与调试
description: 在 Linux 上安装 NetAssistant MQTT 客户端：deb、AppImage、tar.gz 三种方式，支持 Ubuntu 22.04+、x86_64 与 ARM64；含 Vulkan 依赖、TLS 配置与首次连接步骤。
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
            "name": "Linux 上怎么安装 MQTT 客户端？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Debian/Ubuntu 推荐下载 deb 包并用 sudo apt install ./netassistant-linux-x86_64.deb 安装；其他发行版可用 AppImage 或 tar.gz，赋予执行权限后直接运行。"
            }
          },
          {
            "@type": "Question",
            "name": "启动提示 Vulkan 相关错误怎么办？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "NetAssistant 需要 Vulkan 兼容 GPU 与驱动。Ubuntu 可安装 mesa-vulkan-drivers，并在虚拟机或远程环境中确认已启用 3D 加速；没有 Vulkan 的环境无法启动图形界面。"
            }
          },
          {
            "@type": "Question",
            "name": "支持树莓派等 ARM64 设备吗？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "提供 linux-aarch64 的 deb、AppImage 与 tar.gz。该架构为自动构建产物，未经真机完整测试，建议先在目标设备上验证再用于生产。"
            }
          },
          {
            "@type": "Question",
            "name": "AppImage 无法运行怎么办？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "先执行 chmod +x 添加执行权限；若仍失败，检查是否缺少 FUSE，或改用 tar.gz 版本运行解压后的可执行文件。"
            }
          },
          {
            "@type": "Question",
            "name": "Linux 版支持 MQTT over WebSocket 吗？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "不支持。Linux 版与其他平台一致，只通过 TCP 直连 Broker；WebSocket 接入请使用 MQTTX 等客户端。"
            }
          }
        ]
      }
---

# MQTT 客户端 Linux：安装、连接与调试

Linux 上的 MQTT 调试工具要么是命令行，要么是基于 Electron 的图形客户端。NetAssistant 是 Rust + GPUI 构建的原生应用，提供 deb、AppImage、tar.gz 三种安装方式，把 MQTT 3.1.1 / 5.0 与 TCP/UDP 调试放在同一个界面里。

## 为什么在 Linux 上选它

- **原生应用**：不是 Electron 套壳，启动快、内存约 20MB
- **安装方式齐全**：deb 自动处理依赖，AppImage 免安装，tar.gz 适合精简环境
- **x86_64 与 ARM64**：覆盖常见服务器、开发机与树莓派 5
- **协议齐全**：TCP/UDP 客户端与服务端 + MQTT 3.1.1 / 5.0
- **TLS 完整**：系统根证书、自定义 CA、跳过校验，支持双向认证

## 系统要求

| 项目 | 要求 |
| ---- | ---- |
| 发行版 | Ubuntu 22.04 及以上及衍生版；其他发行版建议 AppImage |
| 架构 | x86_64 或 ARM64（aarch64） |
| 图形 | Vulkan 兼容 GPU 与驱动 |
| 网络 | 可访问 Broker 的 1883（明文）或 8883（TLS）端口 |

NetAssistant 需要 Vulkan 渲染界面。Ubuntu 上可安装 `mesa-vulkan-drivers`；虚拟机、WSL 或远程桌面环境需确认已启用 3D 加速，否则无法启动图形界面。

## 安装

### 推荐：deb 包

```bash
sudo apt install ./netassistant-linux-x86_64.deb
```

安装后从应用菜单启动，或在终端运行 `netassistant`。deb 会自动解析依赖。

### AppImage（免安装）

```bash
chmod +x netassistant-linux-x86_64.AppImage
./netassistant-linux-x86_64.AppImage
```

适合 Fedora、Arch 等非 Debian 系发行版。如果系统缺少 FUSE，改用 tar.gz。

### tar.gz（精简环境）

```bash
tar -xzf netassistant-linux-x86_64.tar.gz
chmod +x netassistant
./netassistant
```

### ARM64

树莓派 5、ARM 服务器等设备下载 `linux-aarch64` 对应的 deb、AppImage 或 tar.gz，用法相同。ARM64 构建为自动产物，未经真机完整测试，建议先在目标设备验证。

## 第一次连接 MQTT

1. 打开 NetAssistant，点击 `+新建`，协议选择 **MQTT**
2. 填写 Broker 地址、端口、Client ID、用户名与密码
3. 需要加密时打开 TLS，按证书来源选择校验方式
4. 连接成功后订阅 `test/#` 或业务 topic
5. 在发送区填写 topic、QoS、retain 与 payload，点击发布

完整步骤见 [MQTT 调试教程](/guide/mqtt)。

## 在 Linux 上配置 TLS

- 证书与私钥使用 PEM 文件路径，支持 PKCS#8、PKCS#1 与 SEC1
- 私有 CA 选择「自定义 CA」，指向 CA 的 PEM；公共 CA 使用默认系统根证书
- 双向认证需成对填写客户端证书与私钥；配置只保存路径
- 本地联调自签证书时可临时跳过校验，生产环境不要使用

握手失败排查见 [MQTT 排错](/mqtt-troubleshooting#tls-握手失败)。

## 适合 Linux 的场景

- 开发机本地 Docker 跑 EMQX / Mosquitto，直接连 `127.0.0.1`
- 服务器桌面环境上排查设备上报，低内存不拖慢系统
- 树莓派接串口设备后，在同一台机器上抓 MQTT 与 TCP/UDP
- 需要免安装 AppImage，放在 U 盘或共享目录直接运行

如果你更关心 Linux 上的 TCP/UDP 调试，可以看 [Linux 网络调试助手](/linux-network-debug-tool)。

## 常见问题

### Linux 上怎么安装 MQTT 客户端？

Ubuntu/Debian 用 deb 包；其他发行版用 AppImage 或 tar.gz，赋予执行权限后运行。

### 启动提示 Vulkan 相关错误怎么办？

安装 `mesa-vulkan-drivers`，并确认虚拟机或远程环境已启用 3D 加速；无 Vulkan 环境无法启动图形界面。

### 支持树莓派等 ARM64 设备吗？

提供 aarch64 的 deb、AppImage 与 tar.gz，但为自动构建产物，建议先验证。

### AppImage 无法运行怎么办？

先 `chmod +x`，检查 FUSE；仍失败时改用 tar.gz。

### Linux 版支持 MQTT over WebSocket 吗？

不支持，只通过 TCP 直连 Broker。

## 相关页面

- 功能与参数：[MQTT 客户端](/mqtt-client)
- 连接与订阅教程：[MQTT 调试教程](/guide/mqtt)
- 连接失败排查：[MQTT 排错](/mqtt-troubleshooting)
- 其他平台：[Windows 版](/mqtt-client-windows)、[macOS 版](/mqtt-client-mac)
- [下载 Linux 版](/download)
