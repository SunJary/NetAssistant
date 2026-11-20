# NetAssistant - TCP/UDP/MQTT 网络调试助手

<div align="center">

**Rust 构建的高性能跨平台网络调试工具：TCP/UDP 客户端与服务端、MQTT 3.1.1/5.0、内置压测与校验工具箱**

[![Rust](https://img.shields.io/badge/Rust-2024-orange.svg)](https://www.rust-lang.org)
[![Release](https://img.shields.io/github/v/release/SunJary/NetAssistant)](https://github.com/SunJary/NetAssistant/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/SunJary/NetAssistant/total)](https://github.com/SunJary/NetAssistant/releases)
[![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20macOS%20%7C%20Linux-blue)](https://github.com/SunJary/NetAssistant/releases)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

**免费开源 · 约 20MB · 无需注册**

[⬇️ Windows](https://github.com/SunJary/NetAssistant/releases/latest/download/netassistant-windows-x86_64-setup.exe) ·
[⬇️ macOS](https://github.com/SunJary/NetAssistant/releases/latest/download/netassistant-macos-universal.dmg) ·
[⬇️ Linux](https://github.com/SunJary/NetAssistant/releases/latest/download/netassistant-linux-x86_64.deb) ·
[📖 在线文档](https://netassistant.trydo.top/)

[English](README-en.md) | 中文

</div>

![NetAssistant 主界面](assets/screenshots/zh/screenshot_client.png)

## 为什么选 NetAssistant

- **一套工具覆盖三种协议**：TCP/UDP 客户端与服务端、MQTT 3.1.1/5.0，不用在多个工具之间来回切换。
- **轻量且快**：Rust + GPUI，启动 < 100ms，内存占用 < 20MB，消息列表保持 60fps。
- **调试闭环完整**：文本/Hex 收发、四种 TCP 解码器、回复规则、周期发送、内置压测与 CSV 报告。
- **跨平台、开源免费**：Windows 10+、macOS 10.15+、Linux（GTK3），Apache-2.0，无账号、无云上传。

## 核心功能

- **TCP/UDP 调试**：客户端与服务端、IPv4/IPv6、多标签、UDP 广播与多设备回复、本地地址绑定。
- **报文处理**：文本 ⇄ Hex、原始/行/长度前缀/JSON 四种解码器、JSON 美化、搜索、收藏、TXT/JSON/CSV 导出、实时日志。
- **自动化**：周期发送、逐行发送、定时任务、条件回复规则，以及 `${seq}`、`${timestamp}`、`${uuid}` 等变量模板。
- **压力测试**：TCP/UDP Ping-Pong 与吞吐模式、短/长连接、QPS、p50/p95/p99、失败分类、CSV 报告。
- **MQTT 客户端**：3.1.1/5.0、QoS 0/1/2、TLS 与双向认证、遗嘱消息、订阅恢复、断线自动重连。
- **内置工具箱**：转换、校验（XOR/Sum8/LRC/CRC16/CRC32）、哈希、时间戳、JSON、Base64/URL 编解码。

完整说明见 [功能特性](https://netassistant.trydo.top/features)。

## 快速开始

| 平台 | 推荐安装方式 |
| ---- | ---- |
| Windows | `winget install SunJary.NetAssistant`，或下载安装程序 |
| macOS | 下载 DMG，打开后拖入 Applications |
| Linux | 下载 deb 后执行 `sudo apt install ./netassistant-*.deb` |

直接下载：[Windows 安装包](https://github.com/SunJary/NetAssistant/releases/latest/download/netassistant-windows-x86_64-setup.exe) · [macOS DMG](https://github.com/SunJary/NetAssistant/releases/latest/download/netassistant-macos-universal.dmg) · [Linux deb](https://github.com/SunJary/NetAssistant/releases/latest/download/netassistant-linux-x86_64.deb) · [全部版本](https://github.com/SunJary/NetAssistant/releases)

系统要求：Windows 10 1709+（不含 Windows Server 2016 及更早）、macOS 10.15+、Linux 需 GTK3（Ubuntu 22.04+）。

五步上手：新建连接 → 连接或启动 → 选择文本/Hex → 发送消息 → 按需配置解码器与回复规则。详细步骤见 [TCP/UDP 调试教程](https://netassistant.trydo.top/guide/tcp-udp)。

## 界面预览

![服务端模式](assets/screenshots/zh/screenshot_server.png)

![MQTT 客户端](assets/screenshots/zh/screenshot_mqtt_client.png)

![压力测试结果](assets/screenshots/zh/screenshot_stress.png)

<details>
<summary>查看更多截图（17 张）</summary>

![新建连接](assets/screenshots/zh/screenshot_new_client.png)

![IPv6 支持](assets/screenshots/zh/screenshot_ipv6.png)

![TCP 解码器](assets/screenshots/zh/screenshot_tcp_decoder.png)

![十六进制模式](assets/screenshots/zh/screenshot_hex.png)

![ASCII 与 Hex 互转](assets/screenshots/zh/screenshot_text_hex_convert.png)

![收藏消息](assets/screenshots/zh/screenshot_favorite_message.png)

![消息搜索](assets/screenshots/zh/screenshot_search.png)

![回复规则](assets/screenshots/zh/screenshot_reply_rule.png)

![MQTT 新建连接](assets/screenshots/zh/screenshot_mqtt_new_client.png)

![工具箱 · 哈希](assets/screenshots/zh/screenshot_toolbox_hash.png)

![工具箱 · 时间](assets/screenshots/zh/screenshot_toolbox_time.png)

![文件数据源](assets/screenshots/zh/screenshot_file_source.png)

![UDP 手动添加客户端](assets/screenshots/zh/screenshot_udp_add_client.png)

![压测配置（暗黑模式）](assets/screenshots/zh/screenshot_udp_stress_dark.png)

![UDP 客户端暗黑模式](assets/screenshots/zh/screenshot_udp_client_dark.png)

![UDP 服务端暗黑模式](assets/screenshots/zh/screenshot_udp_server_dark.png)

![折叠模式](assets/screenshots/zh/screenshot_collapsed.png)

</details>

## 典型场景

- **TCP/UDP 联调**：模拟硬件或服务端，验证 Hex 报文、粘包解码与广播发现。→ [TCP 调试工具](https://netassistant.trydo.top/tcp-debug-tool)
- **MQTT 联调**：连接 Broker，验证 QoS、TLS、遗嘱与订阅恢复。→ [MQTT 客户端](https://netassistant.trydo.top/mqtt-client)
- **服务压测**：对自研服务做并发与延迟测试，导出 CSV 报告。→ [压力测试](https://netassistant.trydo.top/tcp-stress-test)

## 常见问题

**Q：和 NetAssist 有什么区别？**  
A：NetAssistant 跨平台、开源免费，并内置 MQTT 与压测；逐项差异见 [网络调试工具对比](https://netassistant.trydo.top/comparison)。

**Q：支持串口吗？**  
A：不支持。NetAssistant 只做网络调试（TCP/UDP/MQTT），串口调试请使用串口助手。

**Q：支持哪些操作系统？**  
A：Windows 10 1709+、macOS 10.15+、Linux（需要 GTK3，如 Ubuntu 22.04+）。

**Q：MQTT 支持到什么程度？**  
A：支持 3.1.1/5.0、QoS 0/1/2、TLS 与双向认证、遗嘱、订阅管理与自动重连；不支持 MQTT over WebSocket、共享订阅、用户属性与 MQTT 压测。

**Q：TCP 粘包怎么处理？**  
A：内置原始数据、行分隔、长度前缀、JSON 四种解码器；原理与选型见 [TCP 粘包与拆包](https://netassistant.trydo.top/tcp-sticky-packet)。

**Q：压测支持哪些协议？**  
A：仅 TCP/UDP，支持 Ping-Pong 与吞吐模式，可导出 CSV；MQTT 暂不支持压测。

**Q：数据会上传吗？**  
A：不会。配置与日志都保存在本地，工具不使用账号体系，也没有云端同步。

**Q：快捷键有哪些？**  
A：`Ctrl+Enter` 发送、`Ctrl+F` 搜索、`Ctrl+N` 新建连接、`Ctrl+W` 关闭标签页、`Ctrl+1..9` 切换标签页；macOS 使用 `Cmd`。

更多问题见 [FAQ](https://netassistant.trydo.top/faq)。

<details>
<summary>完整功能清单</summary>

**连接与协议**

- TCP/UDP 客户端与服务端，IPv4/IPv6 双栈，本地地址与端口绑定
- MQTT 3.1.1/5.0：QoS 0/1/2、Clean Session、KeepAlive、遗嘱消息、会话过期、原因码
- MQTT TLS：系统根证书 / 自定义 CA / 跳过校验，支持客户端证书与私钥双向认证
- 多标签页、连接重命名、复制连接（MQTT 自动生成新 Client ID）

**报文与消息**

- 文本/Hex 双模式，切换时按 UTF-8 自动互转；不可打印字节以 `\xNN` 转义
- HEX 编辑器支持十六进制与文本双视图
- 原始、行分隔、长度前缀、JSON 四种 TCP 解码器
- 发送与接收均支持 JSON 美化/压缩
- 消息搜索、收藏与备注、双击复制源地址或 topic
- 导出 TXT/JSON/CSV；实时日志写入文件，支持自定义路径
- 消息条数上限（默认 10000，`0` 不限制），控制长时间调试的内存占用
- 文件数据源：UTF-8 / GBK / ANSI，上限 1 MiB

**自动化与压测**

- 周期发送、逐行发送、定时任务（心跳），支持暂停/继续/停止
- 回复规则：字节包含、掩码、前缀+长度、偏移、正则、来源 CIDR、校验值等条件，可嵌套
- 变量模板：`${seq}`、`${worker_id}`、`${counter}`、`${timestamp}`、`${uuid}`、`${random:min:max}`，以及 `${rx.*}` 接收帧变量
- 压测：Ping-Pong / 吞吐模式、短/长连接、QPS、活跃连接、收发字节、p50/p95/p99、失败分类、CSV 导出

**界面与效率**

- 亮色/暗色主题自动跟随系统
- 中英文界面切换
- 快捷键：发送、搜索、标签页切换、新建连接、聚焦输入框
- 客户端消息过滤、UDP 手动添加客户端、折叠模式

</details>

<details>
<summary>技术栈与性能</summary>

- **语言与 UI**：Rust 2024、[GPUI](https://github.com/zed-industries/zed)、[gpui-kit](https://crates.io/crates/gpui-kit)
- **网络与异步**：[Tokio](https://tokio.rs/)、[rumqttc](https://crates.io/crates/rumqttc)
- **性能**：启动 < 100ms、内存占用 < 20MB、消息列表 60fps
- **架构**：解码器、回复规则、发送任务与压测引擎均与 UI 解耦，便于复用和测试

</details>

<details>
<summary>路线图</summary>

- [x] MQTT 客户端与 MQTT 5.0 / TLS（v1.5.0）
- [x] 内置工具箱（v1.4.0）
- [x] 文件数据源、多语言、十六进制编辑器
- [ ] SSE 调试
- [ ] WebSocket 协议
- [ ] 更多数据格式的编解码

</details>

## 贡献与构建

欢迎提交 Issue 或 Pull Request。从源码构建：

```bash
git clone https://github.com/SunJary/NetAssistant.git
cd NetAssistant
cargo build --release
```

构建产物位于 `target/release`。文档站点使用 VitePress：`npm install && npm run docs:build`。

## 许可证

本项目采用 Apache License 2.0，详见 [LICENSE](LICENSE)。

## 联系方式

- 官网：[https://netassistant.trydo.top/](https://netassistant.trydo.top/)
- 项目主页：[https://github.com/SunJary/NetAssistant](https://github.com/SunJary/NetAssistant)
- 问题反馈：[https://github.com/SunJary/NetAssistant/issues](https://github.com/SunJary/NetAssistant/issues)

## 致谢

感谢 [GPUI](https://github.com/zed-industries/zed)、[gpui-kit](https://crates.io/crates/gpui-kit)、[Tokio](https://tokio.rs/)、[Rust](https://www.rust-lang.org/) 等开源项目。

---

<div align="center">

**如果这个项目对你有帮助，请给它一个 ⭐️**

Made with ❤️ by Rust Community

</div>
