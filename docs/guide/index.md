# 快速上手

## 系统要求

- **Windows**：10 或更高版本
- **Linux**：需要 GTK3 库（如 Ubuntu 22.04 及以上版本）
- **macOS**：10.15 或更高版本

## 安装

### Windows

**推荐方法：使用 winget 安装**

```bash
winget install SunJary.NetAssistant
```

后续升级只需运行：

```bash
winget upgrade SunJary.NetAssistant
```

**备选方法**：从 [GitHub Release](https://github.com/sunjary/netassistant/releases) 页面下载最新版本。

### Linux

1. 从 [GitHub Release](https://github.com/sunjary/netassistant/releases) 页面下载最新版本的 Linux 压缩包（推荐 AppImage，开箱即用）
2. 解压安装包：

```bash
tar -xzf netassistant-linux-x64.tar.gz
```

3. 运行可执行文件：

```bash
./netassistant
```

AppImage 方式首次运行需安装 libfuse2：`sudo apt install libfuse2`。

### macOS

1. 从 [GitHub Release](https://github.com/sunjary/netassistant/releases) 页面下载最新版本的 macOS 压缩包
2. 解压安装包，将 NetAssistant 拖放到 Applications 文件夹
3. 右键点击应用程序，选择「打开」运行（首次运行需要此操作）

更多细节见[下载页](/download)。

## 第一次调试：三步跑通

1. **创建连接**：点击左侧面板的 `[+新建]` 按钮，选择连接类型（客户端/服务端）、协议（TCP/UDP），填写地址和端口。创建完成后可在连接详情页配置 TCP 解码器类型。
2. **启动连接**：客户端连接点击 `[连接]` 按钮；服务端连接点击 `[启动]` 按钮。
3. **发送消息**：在底部输入框上方选择消息发送模式（文本或十六进制），输入内容后点击 `[发送]` 按钮或按 Enter 键发送。

接下来可以：

- 调试 TCP 协议时遇到粘包问题？→ 阅读 [TCP/UDP 调试](/guide/tcp-udp)
- 需要模拟对端响应、周期发送或压测？→ 阅读 [压力测试](/guide/stress)
