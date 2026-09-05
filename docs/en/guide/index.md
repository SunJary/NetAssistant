# Getting Started

## System Requirements

- **Windows**: 10 or later
- **Linux**: GTK3 library required (e.g. Ubuntu 22.04 or later)
- **macOS**: 10.15 or later

## Installation

### Windows

**Recommended: install with winget**

```bash
winget install SunJary.NetAssistant
```

To upgrade later, simply run:

```bash
winget upgrade SunJary.NetAssistant
```

**Alternative**: download the latest version from the [GitHub Release](https://github.com/sunjary/netassistant/releases) page.

### Linux

1. Download the latest Linux archive from the [GitHub Release](https://github.com/sunjary/netassistant/releases) page (AppImage recommended, works out of the box)
2. Extract the archive:

```bash
tar -xzf netassistant-linux-x64.tar.gz
```

3. Run the executable:

```bash
./netassistant
```

The AppImage requires libfuse2 on first run: `sudo apt install libfuse2`.

### macOS

1. Download the latest macOS archive from the [GitHub Release](https://github.com/sunjary/netassistant/releases) page
2. Extract the archive and drag NetAssistant into the Applications folder
3. Right-click the app and choose "Open" to run it (required on first launch)

See the [Download page](/en/download) for more details.

## Your First Debug Session: Three Steps

1. **Create a connection**: click the `[+ New]` button in the left panel, choose the connection type (client/server) and protocol (TCP/UDP), then fill in the address and port. After creation, you can configure the TCP decoder type on the connection detail page.
2. **Start the connection**: for a client connection, click `[Connect]`; for a server connection, click `[Start]`.
3. **Send a message**: choose the send mode (text or hex) above the input box at the bottom, type your content, then click `[Send]` or press Enter.

Next steps:

- Running into sticky packet issues while debugging TCP? → Read [TCP/UDP Debugging](/en/guide/tcp-udp)
- Need to simulate peer responses, send periodically, or run stress tests? → Read [Stress Testing](/en/guide/stress)
