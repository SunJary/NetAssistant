---
layout: home

hero:
  name: NetAssistant
  text: 高性能跨平台网络调试工具
  tagline: 基于 Rust 构建 · TCP/UDP 客户端与服务端 · 多种解码器 · 内置压力测试 · GPU 加速界面
  actions:
    - theme: brand
      text: 立即下载
      link: /download
    - theme: alt
      text: 使用指南
      link: /guide/
    - theme: alt
      text: GitHub
      link: https://github.com/SunJary/NetAssistant

features:
  - icon: 🔌
    title: 多协议支持
    details: 完整支持 TCP/UDP 客户端与服务端模式，IPv4/IPv6 双栈，适应各种网络环境。
  - icon: 📦
    title: 智能解码器
    details: 原始数据、行分隔、长度前缀、JSON 四种 TCP 解码器，有效解决粘包问题；收发内容支持十六进制模式。
  - icon: 💬
    title: 聊天式报文记录
    details: 直观展示报文交互过程，支持收藏与备注、关键字搜索、实时日志与 TXT/JSON/CSV 导出。
  - icon: 🚀
    title: 高并发压力测试
    details: 内置压测引擎，实时展示 QPS、延迟分位数（p50/p95/p99）、失败原因分类，支持变量模板与 CSV 报告导出。
  - icon: 🤖
    title: 自动化测试
    details: 自动回复模拟对端响应，周期发送用于长稳测试，UDP 广播回复智能展示助力设备发现调试。
  - icon: 🎨
    title: 现代化界面
    details: GPUI 框架 GPU 加速渲染，60fps 流畅体验；暗黑模式自动跟随系统，多标签页管理多连接。
---

## 界面预览

### 客户端模式

![客户端模式截图](../assets/screenshots/screenshot_client.png)

### 服务端模式

![服务端模式截图](../assets/screenshots/screenshot_server.png)

### 压力测试

![压力测试截图](../assets/screenshots/screenshot_stress.png)

### 暗黑模式

![UDP 服务端暗黑模式截图](../assets/screenshots/screenshot_udp_server_dark.png)

## 为什么选择 NetAssistant

- **极速性能**：Rust + Tokio 异步运行时，启动 < 100ms，内存占用 < 20MB，百万级并发连接能力
- **专为调试而生**：从网络应用开发到硬件、嵌入式调试，覆盖通信验证全流程
- **跨平台**：Windows、Linux、macOS 全平台支持
- **开源免费**：基于 Apache-2.0 许可证，欢迎参与贡献

更多功能细节请查看 [功能特性](/features)，上手请阅读 [使用指南](/guide/)。
