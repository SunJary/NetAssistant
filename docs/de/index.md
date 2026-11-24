---
layout: home

title: NetAssistant - TCP/UDP/MQTT-Netzwerk-Debugging-Tool
titleTemplate: ':title'
description: "Rust-Netzwerk-Debugging-Tool mit TCP/UDP-Client und -Server, MQTT 3.1.1/5.0, vier Decodern, Toolbox und Lasttests für Windows, Linux und macOS."

hero:
  name: NetAssistant
  text: Hochleistungsfähiges, plattformübergreifendes Netzwerk-Debugging-Tool
  tagline: In Rust entwickelt · Open-Source-Alternative zu NetAssist und MQTTX · TCP/UDP/MQTT-Client & -Server · Mehrere Decoder · Integrierte Toolbox & TCP/UDP-Lasttests · GPU-beschleunigte Oberfläche
  actions:
    - theme: brand
      text: Jetzt herunterladen
      link: /de/download
    - theme: alt
      text: Benutzerhandbuch
      link: /en/guide/
    - theme: alt
      text: GitHub
      link: https://github.com/SunJary/NetAssistant

features:
  - icon: 🔌
    title: Multi-Protokoll-Unterstützung
    details: Volle Unterstützung für TCP/UDP-Client- und -Server-Modus sowie einen MQTT-3.1.1-/5.0-Client (TLS, Schalter pro Abonnement, QoS 0/1/2, Last-Will-Nachrichten) – mit IPv4/IPv6-Dual-Stack für jede Netzwerkumgebung.
  - icon: 🧰
    title: Integrierte Toolbox
    details: Sieben praktische Gruppen – Konvertieren, Prüfsumme, Hash, Zeit, JSON, Kodierung und Zufall – mit sofortigen Ergebnissen und Ein-Klick-Kopieren, ohne auf externe Websites wechseln zu müssen.
  - icon: 📦
    title: Intelligente Decoder
    details: Vier TCP-Decoder – Rohdaten, zeilengetrennt, Längenpräfix und JSON – lösen Sticky-Packet-Probleme effektiv; Senden und Empfangen im HEX-Modus.
  - icon: 💬
    title: Chat-artiges Nachrichtenprotokoll
    details: Übersichtliche Anzeige des Paketaustauschs mit Favoriten und Anmerkungen, Stichwortsuche, Echtzeit-Logging und Export nach TXT/JSON/CSV.
  - icon: 🚀
    title: TCP/UDP-Lasttests
    details: Integrierte TCP/UDP-Last-Engine mit Echtzeit-QPS, Latenz-Perzentilen (p50/p95/p99), Fehleraufschlüsselung, variablen Vorlagen und CSV-Berichtsexport; MQTT-Lasttests werden nicht unterstützt.
  - icon: 🤖
    title: Automatisierte Tests
    details: Automatische Antworten zur Simulation der Gegenstelle, periodisches Senden für Langzeit-Stabilitätstests und intelligente Anzeige von UDP-Broadcast-Antworten zur Geräteerkennung.
  - icon: 🎨
    title: Moderne Oberfläche
    details: GPU-beschleunigtes Rendering mit dem GPUI-Framework bei flüssigen 60 fps; der Dark Mode folgt automatisch dem System, mit Verbindungsverwaltung über mehrere Tabs.
---

## Screenshots

### Client-Modus

![Screenshot des Client-Modus](../../assets/screenshots/en/screenshot_client.png)

### Server-Modus

![Screenshot des Server-Modus](../../assets/screenshots/en/screenshot_server.png)

### MQTT-Client

![Screenshot des MQTT-Clients](../../assets/screenshots/en/screenshot_mqtt_client.png)

### Integrierte Toolbox

![Screenshot der Toolbox mit Hash-Funktion](../../assets/screenshots/en/screenshot_toolbox_hash.png)

### Lasttests

![Screenshot der Lasttests](../../assets/screenshots/en/screenshot_stress.png)

### Dark Mode

![Screenshot des UDP-Servers im Dark Mode](../../assets/screenshots/en/screenshot_udp_server_dark.png)

## Warum NetAssistant

- **Rasante Performance**: Rust + Tokio-Async-Runtime, Start < 100 ms, Speicherbedarf < 20 MB, Millionen gleichzeitiger Verbindungen
- **Für Debugging gemacht**: von der Entwicklung von Netzwerkanwendungen bis zum Hardware- und Embedded-Debugging – der gesamte Workflow zur Kommunikationsprüfung
- **Plattformübergreifend**: volle Unterstützung für Windows, Linux und macOS, mit nativen Builds für x64 und ARM64
- **Open Source und kostenlos**: veröffentlicht unter der Apache-2.0-Lizenz, Beiträge willkommen

Weitere Details finden Sie unter [Funktionen](/en/features); für den Einstieg lesen Sie die [Anleitung](/en/guide/). Wie NetAssistant im Vergleich zu NetAssist und ähnlichen Tools abschneidet, zeigt der [Vergleich](/en/comparison).

## Download nach Plattform

- [Windows-Netzwerk-Debugging-Tool](/en/windows-network-debug-tool): Installation mit einem winget-Befehl, Windows 10/11 für x64 und ARM64
- [Mac-Netzwerk-Debugging-Tool](/en/mac-network-debug-tool): universelles DMG für Apple Silicon und Intel
- [Linux-Netzwerk-Debugging-Tool](/en/linux-network-debug-tool): deb, AppImage und tar.gz für x86_64 und ARM64

## Beliebte Anleitungen

- [NetAssist-Alternative: Vergleich und Migration](/en/compare/netassist)
- [TCP-Debug-Tool: Senden, Empfangen und Framing](/en/tcp-debug-tool)
- [UDP-Debug-Tool: verbindungsloses Senden und Antworten mehrerer Geräte](/en/udp-debug-tool)
- [Socket-Debug-Tool: Port-Listening und Erreichbarkeit](/en/socket-debug-tool)
- [HEX-Senden: HEX-Nutzlasten und Textkonvertierung](/en/hex-send)
- [Modbus-TCP-Debugging: Frames und CRC16](/en/modbus-tcp)
- [TCP-Sticky-Packets: Ursachen, vier Lösungen und Debugging](/en/tcp-sticky-packet)
- [MQTT-Debugging-Guide: Seiten nach Aufgabe finden](/en/mqtt)
- [MQTT-Client-Vergleich: MQTTX, Explorer und MQTT.fx](/en/compare/)
- [MQTT-Client: ein schlankes Debugging-Tool für MQTT 3.1.1 / 5.0](/en/mqtt-client)
- [MQTTX-Alternative: Vergleich schlanker MQTT-Clients](/en/comparison-mqttx)
- [MQTT-Tutorial: Verbinden, Abonnieren und Veröffentlichen](/en/guide/mqtt)
- [MQTT-Fehlerbehebung: Verbindungen und fehlende Nachrichten](/en/mqtt-troubleshooting)
- [MQTT TLS: Zertifikatsprüfung und gegenseitige Authentifizierung](/en/mqtt-tls)
- [MQTT 5.0: Sitzungsablauf und Reason Codes](/en/mqtt-5)
- [MQTT-Explorer-Alternative im Vergleich](/en/compare/mqtt-explorer)
- [MQTT.fx-Alternative im Vergleich](/en/compare/mqtt-fx)
- [MQTT-FAQ](/en/mqtt-faq)
- [MQTT-Client unter Linux installieren](/en/mqtt-client-linux)
- [TCP/UDP-Lasttests: QPS und p99-Latenz](/en/tcp-stress-test)
- [UDP-Broadcast-Geräteerkennung: LAN-Suche und Fehlerbehebung](/en/udp-broadcast-device-discovery)
