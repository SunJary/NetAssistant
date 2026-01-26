# NetAssistant

<div align="center">

**A high-performance, modern network debugging tool built with Rust**

[![Rust](https://img.shields.io/badge/Rust-2024-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

English | [中文](README.md)

</div>

---

## Introduction

NetAssistant is a high-performance, modern network debugging tool built with Rust. It provides an intuitive interface for testing and debugging network communications, supporting TCP/UDP client and server modes.

## ✨ Features

- **Multi-protocol support**: TCP/UDP client and server modes
- **Chat-style message logging**: Intuitive display of message interactions
- **Configuration persistence**: Automatically saves connection configurations
- **Auto-reply functionality**: Supports test auto-replies
- **Periodic send functionality**: Supports timed periodic message sending
- **Multi-tab management**: Manage multiple connections simultaneously
- **Client message viewing**: Select specific clients to view their messages

## 🎯 Use Cases

- ✅ **IoT device integration testing**: Test communication with various IoT devices and verify device responses and data formats
- ✅ **Network application development debugging**: Quickly test communication logic and verify data transfer formats during network application development
- ✅ **Embedded device communication verification**: Verify the correctness of network communication protocol implementations in embedded systems

## 📸 Interface Preview

### Client Mode
![Client Screenshot](assets/screenshots/screenshot_client.png)

### Server Mode
![Server Screenshot](assets/screenshots/screenshot_server.png)

## 🚀 Quick Start

### Prerequisites

- Rust 1.70 or higher
- Windows 10/11 or Linux

### Installation

#### Recommended Method: Install via winget
**Advantages**: Supports automatic upgrades, easier installation and management
1. First install winget (built-in on Windows 10 1809+ or Windows 11, or refer to [Microsoft official documentation](https://learn.microsoft.com/en-us/windows/package-manager/winget/) for installation methods)
2. Open Command Prompt or PowerShell and run:
   ```bash
   winget install SunJary.NetAssistant
   ```
3. To upgrade later, simply run:
   ```bash
   winget upgrade SunJary.NetAssistant
   ```

#### Alternative Method: Download from GitHub Release
Please visit the [GitHub Release page](https://github.com/sunjary/netassistant/releases) to download the latest version.

### Running

After downloading, extract the package and run the executable file.

## 💡 Usage

1. **Create Connection**
   - Click the `[+New]` button in the left panel
   - Select connection type (Client/Server)
   - Select protocol (TCP/UDP)
   - Fill in address and port

2. **Connect to Server**
   - For client connections, click the `[Connect]` button
   - For server connections, click the `[Start]` button

3. **Send Messages**
   - Enter message content in the bottom input box
   - Click the `[Send]` button or press Enter to send

4. **Periodic Send**
   - Enable periodic send functionality in the connection tab
   - Set send interval (milliseconds)
   - Click the `[Send]` button to start periodic sending
   - Uncheck periodic send to stop the sending task

5. **Auto-reply**
   - Enable auto-reply functionality in the connection tab
   - Set auto-reply content
   - Auto-reply when receiving messages

6. **Manage Connections**
   - Use tabs to switch between different connections
   - Click the `×` on the tab to close the connection
   - Right-click on the connection to delete saved configuration

7. **Client Message Viewing**
   - In server mode, the left panel displays the list of connected clients
   - Click a single client address to select it, and the right message list will only show messages from that client
   - Click the selected client again to deselect and restore all messages
   - Server replies to the client will also be included in the viewing results

## 🎯 Technical Highlights

### ⚡ Extreme Performance

- **Rust-powered**: Built with Rust for maximum performance and security
  - Zero-cost abstractions, compile-time optimizations
  - Memory safety guarantees, no garbage collection
  - Modern concurrency model

- **Tokio async runtime**: Efficient async I/O operations
  - High-performance event loop based on epoll/kqueue
  - Non-blocking I/O, maximizes system resource utilization
  - Lightweight task scheduling, supports millions of concurrent connections

### 🎨 Modern Interface

- **GPUI framework**: Cutting-edge GPU-accelerated UI
  - GPU-based rendering, fully utilizing hardware acceleration
  - Hardware-accelerated text rendering
  - Smooth 60fps experience

- **Smooth animations**: 60fps rendering for smooth user experience
  - Smooth transition animations
  - Responsive interaction feedback
  - High-frame-rate message scrolling

- **Responsive design**: Adaptive layout for different screen sizes
  - Flexible window size adjustment
  - Adaptive message display
  - Optimized space utilization

### 🔧 Core Features

- **Real-time message monitoring**: Instant message display and auto-scroll
  - Millisecond-level message response
  - Auto-scroll to latest messages
  - Message timestamps accurate to milliseconds

- **Connection management**: Supports multiple simultaneous connections
  - Multi-tab interface
  - Independent connection state management
  - Convenient connection switching

## 🛠️ Technology Stack

### Core Frameworks
- [GPUI](https://github.com/zed-industries/zed) - GPU-accelerated UI framework
  - High-performance GPU rendering
  - Modern component model
  - Responsive state management

- [gpui-component](https://github.com/longbridge/gpui-component) - Modern UI component library
  - Rich UI components
  - Unified design language
  - Easy to customize and extend

### Network and Async
- [Tokio](https://tokio.rs/) - Network async runtime
  - High-performance async I/O
  - Rich network protocol support
  - Mature production-ready solution

### Data Processing
- [Serde](https://serde.rs/) - Data persistence serialization framework
  - Efficient serialization/deserialization
  - Supports multiple data formats
  - Zero-cost abstractions

- [UUID](https://docs.rs/uuid/) - Unique identifier generation
  - Standard UUID v4 implementation
  - Used for connection and message identification

## 📊 Performance Metrics

- **Startup time**: < 100ms
  - Quick startup, no waiting
  - Instant response to user operations

- **Message throughput**: 10,000+ messages/second
  - High-concurrency message processing
  - Low-latency message transmission

- **Memory usage**: < 50MB (idle state)
  - Lightweight resource usage
  - Efficient memory management

- **UI response**: 60fps rendering
  - Smooth user experience
  - Lag-free interactions

## 🏗️ Project Structure

```
netassistant/
├── src/                    # Source code directory
│   ├── main.rs           # Application entry: initialize logging, create app instance, start main window
│   ├── app.rs            # Main application logic: manage connections, handle network events, state management
│   ├── config/           # Configuration management: connection config definition, storage and loading
│   │   ├── connection.rs # Connection config and type definitions
│   │   ├── mod.rs        # Configuration module export
│   │   └── storage.rs    # Configuration persistence storage
│   ├── message.rs        # Message processing: define message structure, handle message direction and type
│   ├── ui/               # UI components: build user interface and handle user interaction
│   │   ├── main_window.rs      # Main window component
│   │   ├── connection_panel.rs # Connection panel: display and manage connections
│   │   ├── connection_tab.rs   # Connection tab: each tab corresponds to one connection
│   │   ├── tab_container.rs    # Tab container: manage multiple tabs
│   │   ├── mod.rs              # UI module export
│   │   └── dialog/             # Dialog components
│   │       ├── mod.rs          # Dialog module export
│   │       └── new_connection.rs # New connection dialog
│   └── utils/            # Utility functions: common tools and helper functions
│       ├── hex.rs        # Hexadecimal data processing
│       └── mod.rs        # Utility module export
├── assets/               # Resource files: icons and screenshots
│   ├── icon/             # Icon files
│   └── screenshots/      # Application screenshots
├── .cargo/               # Cargo configuration: Rust build tool configuration
│   └── config.toml       # Cargo configuration file
├── .github/              # GitHub configuration: CI/CD workflows
│   └── workflows/        # Workflow configurations
│       └── release.yml   # Release workflow
├── Cargo.toml            # Project configuration: dependency management and project metadata
├── Cargo.lock            # Dependency lock file: fix dependency versions
├── README.md             # Project documentation: Chinese description
├── README-en.md          # English documentation: English description
├── build.rs              # Build script: custom build logic
└── .gitignore            # Git ignore file: specify files and directories to be ignored by Git
```

## 🔮 Future Plans

- [ ] Support WebSocket protocol
- [ ] Add message filtering and search functionality
- [ ] Add clear history messages functionality

## 🤝 Contribution

Welcome to contribute code, report issues, or suggest improvements!

1. Fork this repository
2. Create a feature branch (`git checkout -b feature/AmazingFeature`)
3. Commit changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

## 📝 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 📮 Contact

- Project homepage: [https://github.com/sunjary/netassistant](https://github.com/sunjary/netassistant)
- Issue feedback: [https://github.com/sunjary/netassistant/issues](https://github.com/sunjary/netassistant/issues)

## 🙏 Acknowledgments

Thanks to the following open-source projects:

- [GPUI](https://github.com/zed-industries/zed)
- [gpui-component](https://github.com/longbridge/gpui-component)
- [Tokio](https://tokio.rs/)
- [Rust](https://www.rust-lang.org/)

---

<div align="center">

**If this project helps you, please give it a ⭐️**

Made with ❤️ by Rust Community

</div>
