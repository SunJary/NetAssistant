# Features

NetAssistant is a high-performance, modern **cross-platform** network debugging tool built with Rust, designed for developers and available on Windows, Linux and macOS.

## Core Features

- **Multi-protocol support**: full support for TCP/UDP client and server modes
- **IPv4/IPv6 dual stack**: supports both IPv4 and IPv6, adapting to any network environment
- **Multiple TCP decoders**: raw data, line-delimited, length-prefix and JSON decoders for different protocol formats, effectively solving TCP sticky packet issues
- **JSON formatting**: the send box offers "Prettify/Minify" buttons in text mode to format the outgoing payload; the receive area can switch between Raw/Prettified/Minified display formats globally, with non-JSON content shown as-is
- **Chat-style message log**: intuitive display of packet exchanges for easier debugging and analysis
- **Persistent configuration**: connection configs are saved automatically and restored on the next launch

![TCP decoder screenshot](../../assets/screenshots/screenshot_tcp_decoder.png)

## Message Management

- **Copy message**: quickly copy a single message to the clipboard, in either text or hex format
- **Favorite messages**: add important messages to favorites with optional remarks, and locate them quickly via keyword search
- **Real-time logging**: once enabled manually, all sent and received messages are written asynchronously to a log file in real time (flushed to disk per message), with a customizable save path; the log is flushed and closed automatically on disconnect
- **Manual export anytime**: export the current message history to TXT/JSON/CSV files at any time for archiving, sharing and further analysis
- **Smart UDP broadcast reply display**: optimized for host workstation / IoT device discovery scenarios — send a command to a broadcast address and receive replies from all devices; replies from unexpected addresses are highlighted in red so no important response is lost
- **Connection config editing**: saved connection configs can be edited directly, no need to delete and recreate

![Favorite message screenshot](../../assets/screenshots/screenshot_favorite_message.png)

## Automated Testing

- **Auto-reply**: automatic replies for testing, simulating server or client responses
- **Periodic send**: send messages on a schedule for stress testing or long-run stability testing
- **Stress testing**: built-in TCP/UDP high-concurrency stress test engine, see the [Stress Testing Guide](/en/guide/stress)

## Modern Interface

- **Dark mode**: adapts to the system theme automatically for a comfortable night-time experience
- **Multi-tab management**: manage multiple connections at once, easy to switch and compare
- **Per-client message view**: in server mode, view messages of a specific client
- **Manual client addition for UDP**: in UDP server mode, add client addresses manually to proactively send data to a specific address

![UDP manual client addition screenshot](../../assets/screenshots/screenshot_udp_add_client.png)

## Technical Highlights

### ⚡ Blazing Performance

- **Powered by Rust**: zero-cost abstractions, memory safety, no garbage collection
- **Tokio async runtime**: high-performance event loop based on epoll/kqueue, supporting million-level concurrent connections

### 🎨 Modern Interface

- **GPUI framework**: GPU-accelerated rendering and text rendering, smooth 60fps experience
- **Adaptive theme**: automatically follows the system light/dark mode
- **Responsive design**: adaptive layout that fits different screen sizes

### 🔧 Architecture

- **Standalone stress engine**: pure logic layer with zero GPUI dependency, easy to reuse and test; token-bucket-based rate limiting for precise send-rate control
- **Decoupled core logic and UI**: message handlers are independent of the UI layer; the network protocol layer and the UI layer are cleanly separated

## Benchmarks

During development, NetAssistant was stress-tested against itself using the built-in stress engine, which helped discover and optimize several performance bottlenecks (batched message updates, batched log writes, etc.):

| Metric | Result |
| ------ | ------ |
| Startup time | < 100ms |
| Memory footprint | < 20MB |
| UI responsiveness | 60fps rendering |

## Use Cases

- **Developer communication verification**: backend developers simulate hardware sending data before integrating with it; test communication logic and data formats between client and server to verify server correctness and robustness
- **Hardware device testing**: hardware engineers test sensors, controllers and other devices; embedded developers verify the network protocol stack implementation and transmission efficiency of resource-constrained systems
- **Service performance stress testing**: evaluate concurrency throughput, latency distribution and stability of your own server, and locate performance bottlenecks quickly
