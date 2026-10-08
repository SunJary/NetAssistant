---
title: 常见问题 FAQ - 网络调试助手支持 Mac 吗？
description: NetAssistant 网络调试助手常见问题：是否支持 macOS 与 Linux、与网络调试助手（NetAssist）的区别、winget 安装命令、Linux deb/AppImage 下载、TCP 粘包处理与高并发压测。
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
            "name": "网络调试助手（NetAssist）支持 macOS 吗？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "需要区分两个名字相近的软件：NetAssistant（本站介绍的跨平台网络调试工具）支持 macOS、Windows 和 Linux，x64 与 ARM64 均有原生构建；而名字相近的老牌工具 NetAssist 仅提供 Windows 版本。想在 Mac 或 Linux 上做 TCP/UDP 调试，可以直接使用 NetAssistant。"
            }
          },
          {
            "@type": "Question",
            "name": "NetAssistant 和网络调试助手（NetAssist）有什么区别？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "两者是不同的软件。NetAssist 是仅限 Windows 的闭源工具；NetAssistant 是基于 Rust 的开源（Apache-2.0）跨平台工具，支持 Windows/Linux/macOS 的 x64 与 ARM64，内置四种 TCP 解码器解决粘包问题，并提供高并发压力测试（QPS 与 p50/p95/p99 延迟）。详细对比见同类工具对比页面。"
            }
          },
          {
            "@type": "Question",
            "name": "如何用 winget 安装 NetAssistant？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "在 Windows 上执行 winget install SunJary.NetAssistant 即可安装，后续可通过 winget upgrade SunJary.NetAssistant 自动升级。"
            }
          },
          {
            "@type": "Question",
            "name": "Linux 下有 deb 安装包吗？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "提供 deb、AppImage 与 tar.gz 三种格式（x86_64 与 aarch64）。Debian/Ubuntu 及衍生版推荐 deb 包，sudo apt install 自动解决依赖；AppImage 适合 Fedora/Arch 等其他发行版，首次运行需安装 libfuse2；tar.gz 需自行安装 GTK3 依赖。"
            }
          },
          {
            "@type": "Question",
            "name": "NetAssistant 是免费开源的吗？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "是的。NetAssistant 基于 Apache-2.0 许可证开源，全部功能免费使用，源码托管在 GitHub。"
            }
          },
          {
            "@type": "Question",
            "name": "如何解决 TCP 粘包问题？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "NetAssistant 内置四种 TCP 解码器：原始数据、行分隔、长度前缀和 JSON，可按协议格式自动分帧，无需手动数字节长度。参见使用指南中的 TCP/UDP 调试章节。"
            }
          }
        ]
      }
---

# 常见问题（FAQ）

## 网络调试助手（NetAssist）支持 macOS 吗？

先区分一下：这是两个**名字相近但完全不同的软件**，拼写只差结尾的 ant 三个字母——

- **NetAssistant**（本页介绍的这款）：开源跨平台工具，**Windows、Linux、macOS 全部支持**，x64 与 ARM64 均有原生构建，Mac 上可以直接用 → [前往下载](/download)
- **NetAssist**（野人家园的老牌工具）：仅提供 Windows 版本，没有 macOS 与 Linux 版

所以，如果你想在 Mac 或 Linux 上做 TCP/UDP 调试，直接用 **NetAssistant** 即可，操作方式与常用调试助手一致，上手见[快速上手](/guide/)。更详细的差异对比见[同类工具对比](/comparison)。

## NetAssistant 和网络调试助手（NetAssist）有什么区别？

两者是**不同的软件**，只是名字相近：

| | NetAssistant | 网络调试助手（NetAssist） |
| ---- | ---- | ---- |
| 平台 | Windows / Linux / macOS（x64 + ARM64） | 仅 Windows |
| 开源 | ✅ Apache-2.0 | ❌ 闭源 |
| TCP 解码器（粘包处理） | 原始 / 行 / 长度前缀 / JSON | 有限 |
| 高并发压测 | ✅ QPS + p50/p95/p99 | ❌ |

完整的对比表见[同类工具对比](/comparison)。

## 如何用 winget 安装 NetAssistant？

Windows 用户推荐使用 winget 安装（支持自动升级）：

```bash
winget install SunJary.NetAssistant
```

升级命令：

```bash
winget upgrade SunJary.NetAssistant
```

更多安装方式见[下载页面](/download)。

## Linux 下有 deb 安装包吗？

有。提供 **deb**、**AppImage** 与 **tar.gz** 三种格式（x86_64 与 aarch64 均有构建）：

- **deb**（推荐）：Debian/Ubuntu 及衍生版使用，`sudo apt install ./netassistant-linux-x86_64.deb` 自动解决依赖，Ubuntu 22.04 及以上均可安装
- **AppImage**：免安装开箱即用，适合 Fedora/Arch 等其他发行版；首次运行需 `sudo apt install libfuse2`
- **tar.gz**：解压即用，需自行安装 GTK3 依赖（`sudo apt install libgtk-3-0`）

下载地址见[下载页面](/download#linux)。

## NetAssistant 是免费开源的吗？

是的。NetAssistant 基于 **Apache-2.0** 许可证开源，全部功能免费使用，源码托管在 [GitHub](https://github.com/SunJary/NetAssistant)，欢迎参与贡献。

## 如何解决 TCP 粘包问题？

NetAssistant 内置四种 TCP 解码器，可按协议格式自动分帧，不用再对着字节流手动数长度：

1. **原始数据**：按到达顺序原样展示
2. **行分隔**：按换行符拆分
3. **长度前缀**：按包头长度字段拆分
4. **JSON**：按完整 JSON 对象拆分

切换入口与用法参见[使用指南](/guide/tcp-udp)。
