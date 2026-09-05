# Changelog

All official release notes are published on the [GitHub Releases](https://github.com/sunjary/netassistant/releases) page.

## v1.1.0 <Badge type="tip" text="2026-08-31" />

- **Multilingual interface (i18n)**: multi-language support built on rust-i18n, with full internationalization of UI texts, dialogs and panels
- **HEX editor**: new hex editor with dual hex/text views for editing payloads
- **Stress panel**: scrollable panel and improved dialog interactions

## v1.0.1 <Badge type="tip" text="2026-08-25" />

- **Variable templates in stress configs**: insert dynamic variables into message payloads
- Fixed a stress test UI lag issue
- **Linux AppImage build**: fixed AppRun / GTK dependency issues, AppImage distribution for Linux

## v1.0.0 <Badge type="tip" text="2026-08-13" />

First stable release. Core capabilities:

- TCP/UDP client & server with IPv4/IPv6 dual stack
- Four TCP decoders (raw / line-delimited / length-prefix / JSON) solving sticky packet issues
- Chat-style message log with favorites, remarks and keyword search
- Message export (TXT/JSON/CSV) and real-time logging
- Auto-reply and periodic send
- Smart UDP broadcast reply display for IoT device discovery debugging
- Built-in TCP/UDP high-concurrency stress engine (QPS, latency percentiles, failure breakdown, CSV reports)
- Dark mode, multi-tab, multilingual interface
- HEX editor

Highlights within this release:

- **TCP/UDP stress testing**: new stress engine, config dialog and stress panel
- TCP decoder and JSON decoder optimizations
- Optimized message list rendering with lower memory usage
- Fixed issues such as abnormal app exit

## v0.9.1 <Badge type="tip" text="2026-08-07" />

- Overall UI polish: theme color adjustments and log level changes

## v0.9.0 <Badge type="tip" text="2026-08-02" />

- **Connection editing**: edit existing saved connection configs
- **JSON formatting**: JSON pretty display for message content
- Monospace font for the message area, easier to read
- Upgraded the GPUI framework version

## v0.8.0 <Badge type="tip" text="2026-07-22" />

- **Unexpected source highlighting**: when the UDP server receives replies, unexpected source addresses are highlighted to spot abnormal senders
- **Manual client addition**: manually add client addresses in UDP server mode

## v0.7.1 <Badge type="tip" text="2026-07-21" />

- Fixed the UDP client source address filtering issue so broadcast replies are received correctly

## v0.7.0 <Badge type="tip" text="2026-07-15" />

- **Message export**: export message history (TXT/JSON/CSV) with real-time logging to file

## v0.6.1 <Badge type="tip" text="2026-06-05" />

- Fixed window dragging on GNOME (switched to the TitleBar component)

## v0.6.0 <Badge type="tip" text="2026-05-20" />

- **Message favorites**: favorite frequently used messages with remarks for quick reuse

## v0.5.1 <Badge type="tip" text="2026-05-11" />

- Fixed connections not being closed properly on disconnect

## v0.5.0 <Badge type="tip" text="2026-03-15" />

- **Message list rewrite**: rebuilt the message list and scrollbar on GPUI List for smoother long-list scrolling
- Ubuntu 22 build support

## v0.4.5 <Badge type="tip" text="2026-03-10" />

- **IPv6 support**: TCP/UDP connections now support IPv6
- **Server message sending**: TCP/UDP servers can proactively send messages to clients
- **Auto-scroll option**: the message list auto-scroll is now configurable
- Current version shown in the UI

## v0.4.4 <Badge type="tip" text="2026-03-01" />

- Fixed message sending issues and improved adaptive UI layout
- Switched to `smol::channel` to avoid UI lag under heavy message load

## v0.4.3 <Badge type="tip" text="2026-02-16" />

- **Enhanced TCP decoder system**: new decoder selection dialog with multiple decoding options
- Fixed real-time message refresh and theme issues

## v0.4.2 <Badge type="tip" text="2026-02-12" />

- **Copy / clear messages**: copy a single message or clear the whole list at once
- **Hex auto-reply**: auto-reply now supports hex content
- Added a scrollbar to the message list; refactored connection networking UI and message sender
- Theme and color updates

## v0.4.1 <Badge type="tip" text="2026-01-31" />

- Fixed TCP receiver splitting a single message into multiple messages
- Fixed dark mode issues; release workflow now includes commit logs

## v0.4.0 <Badge type="tip" text="2026-01-27" />

- **Dark mode**: light/dark theme switching
- Fixed the macOS build

## v0.3.0 <Badge type="tip" text="2026-01-20" />

- Fixed connection/tab mismatch by introducing connection IDs; multi-line tabs supported

## v0.2.0 <Badge type="tip" text="2026-01-20" />

- Fixed connections not being closed when closing a tab

## v0.1.0 <Badge type="tip" text="2026-01-20" />

- First public release: TCP/UDP client & server, multi-tab, persistent connection configs, auto-reply

## Roadmap

- [x] Multilingual interface (v1.1.0)
- [ ] File data source
- [ ] SSE debugging
- [ ] More data format codecs
- [ ] WebSocket protocol
