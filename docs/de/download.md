---
title: NetAssistant herunterladen – Windows, Linux & macOS
titleTemplate: ':title'
description: "NetAssistant für Windows, Linux und macOS herunterladen: winget, deb/AppImage/tar.gz für x64 und ARM64, Build aus dem Quellcode und Systemanforderungen."
head:
  - - script
    - type: application/ld+json
    - |
      {
        "@context": "https://schema.org",
        "@graph": [
          {
            "@type": "SoftwareApplication",
            "name": "NetAssistant",
            "applicationCategory": "DeveloperApplication",
            "operatingSystem": "Windows 10+, macOS 10.15+, Linux",
            "description": "Ein plattformübergreifendes Netzwerk-Debugging-Tool mit TCP/UDP-Client und -Server, MQTT 3.1.1 / 5.0, Framing-Decodern, HEX-Nutzlasten und TCP/UDP-Lasttests.",
            "url": "https://netassistant.trydo.top/de/download",
            "downloadUrl": "https://github.com/SunJary/NetAssistant/releases",
            "license": "https://www.apache.org/licenses/LICENSE-2.0",
            "offers": { "@type": "Offer", "price": "0", "priceCurrency": "USD" }
          },
          {
            "@type": "FAQPage",
            "mainEntity": [
              {
                "@type": "Question",
                "name": "Ist NetAssistant kostenlos?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Ja. NetAssistant ist quelloffen unter der Apache-2.0-Lizenz, alle Funktionen sind kostenlos nutzbar, ohne In-App-Käufe, Abonnements oder Funktionsbeschränkungen."
                }
              },
              {
                "@type": "Question",
                "name": "Welche Betriebssysteme werden unterstützt?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Windows 10 Version 1709 oder höher, macOS 10.15 oder höher und Linux-Distributionen wie Ubuntu 22.04 oder höher. Jede Plattform bietet Builds für x64 und ARM64."
                }
              },
              {
                "@type": "Question",
                "name": "Wie installiere ich unter Windows?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Verwenden Sie winget: Führen Sie winget install SunJary.NetAssistant in einem Terminal aus und aktualisieren Sie später mit winget upgrade. Alternativ können Sie den Installer oder ein portables ZIP verwenden."
                }
              },
              {
                "@type": "Question",
                "name": "Gibt es einen ARM64-Build?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Ja. Windows und Linux bieten aarch64-Builds, und macOS bietet ein universelles DMG für Intel und Apple Silicon, das Surface Pro X, Raspberry Pi 5 und Macs mit M-Serie abdeckt."
                }
              },
              {
                "@type": "Question",
                "name": "macOS meldet, der Entwickler könne nicht überprüft werden. Was soll ich tun?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Das ist bei nicht signierten, nicht notarisierten Apps normal und bedeutet nicht, dass die App beschädigt ist. Klicken Sie mit der rechten Maustaste auf NetAssistant und wählen Sie Öffnen, oder gehen Sie zu Systemeinstellungen → Datenschutz & Sicherheit und klicken Sie auf Trotzdem öffnen."
                }
              }
            ]
          }
        ]
      }
---

# Download

NetAssistant ist für Windows, Linux und macOS verfügbar, jeweils mit x64- und ARM64-Builds. Die untenstehenden Download-Links verweisen immer auf die Dateien der [neuesten Version](https://github.com/sunjary/netassistant/releases/latest) – ein Klick genügt. Ältere Versionen finden Sie in den [GitHub Releases](https://github.com/sunjary/netassistant/releases).

Plattformspezifische Installationsanleitungen: [Windows](/en/windows-network-debug-tool) · [macOS](/en/mac-network-debug-tool) · [Linux](/en/linux-network-debug-tool).

<DownloadHero />

## Windows

**Empfohlen: Installation mit winget** (unterstützt automatische Aktualisierungen)

```bash
winget install SunJary.NetAssistant
```

Zum Aktualisieren:

```bash
winget upgrade SunJary.NetAssistant
```

**Oder den Installer verwenden**: Laden Sie [netassistant-windows-x86_64-setup.exe](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-windows-x86_64-setup.exe) herunter und folgen Sie dem Assistenten (erstellt automatisch Startmenü- und Desktop-Verknüpfungen).

**Alternative**: Laden Sie [netassistant-windows-x86_64.zip](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-windows-x86_64.zip) herunter, entpacken Sie das Archiv und starten Sie `netassistant.exe`.

**ARM64-Geräte** (Surface Pro X, Snapdragon-Laptops usw.): winget und der Installer liefern derzeit nur x64. Unter ARM64 laden Sie [netassistant-windows-aarch64.zip](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-windows-aarch64.zip) herunter, entpacken Sie das Archiv und starten Sie `netassistant.exe`. Dieser Build wird automatisch erzeugt und wurde nicht vollständig auf echter Hardware getestet.

## Linux

**Empfohlen: deb-Paket** (Debian/Ubuntu und Derivate, installiert Abhängigkeiten automatisch)

1. Laden Sie [netassistant-linux-x86_64.deb](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-x86_64.deb) herunter
2. Installieren (Abhängigkeiten werden automatisch aufgelöst):

```bash
sudo apt install ./netassistant-linux-x86_64.deb
```

3. Starten Sie die Anwendung aus dem Anwendungsmenü oder führen Sie `netassistant` im Terminal aus

**AppImage** (portabel, für Fedora/Arch und andere Distributionen)

1. Laden Sie [netassistant-linux-x86_64.AppImage](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-x86_64.AppImage) herunter
2. Ausführungsberechtigung setzen und starten:

```bash
chmod +x netassistant-linux-x86_64.AppImage
./netassistant-linux-x86_64.AppImage
```

**Alternative: tar.gz** (schlicht)

1. Laden Sie [netassistant-linux-x86_64.tar.gz](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-x86_64.tar.gz) herunter
2. Entpacken und starten:

```bash
tar -xzf netassistant-linux-x86_64.tar.gz
chmod +x netassistant
./netassistant
```

**ARM64-Geräte** (Raspberry Pi 5, ARM-Server usw.): Laden Sie [netassistant-linux-aarch64.deb](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-aarch64.deb), [netassistant-linux-aarch64.AppImage](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-aarch64.AppImage) oder [netassistant-linux-aarch64.tar.gz](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-aarch64.tar.gz) herunter; die Anwendung erfolgt wie oben beschrieben. Dieser Build wird automatisch erzeugt und wurde nicht vollständig auf echter Hardware getestet.

## macOS

**Empfohlen: universelles DMG** (läuft auf Intel und Apple Silicon)

1. Laden Sie [netassistant-macos-universal.dmg](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-macos-universal.dmg) herunter, öffnen Sie das Image und ziehen Sie NetAssistant in den Ordner „Programme“
2. Wenn macOS beim ersten Start meldet, der Entwickler könne nicht überprüft werden (normal bei nicht signierten, nicht notarisierten Apps – keine Beschädigung): Klicken Sie mit der rechten Maustaste auf NetAssistant → Öffnen; oder gehen Sie zu Systemeinstellungen → Datenschutz & Sicherheit und klicken Sie auf „Trotzdem öffnen“

**Alternative**: Laden Sie [netassistant-macos-universal.zip](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-macos-universal.zip) herunter und entpacken Sie das Archiv, um die App direkt auszuführen (Ausführungsrechte bleiben erhalten).

## Systemanforderungen

| Plattform | Anforderungen |
| -------- | ------------ |
| Windows | Windows 10 Version 1709 (Build 16299) oder höher, einschließlich Windows 11 (x64 / ARM64). Windows Server 2019 und höher werden unterstützt; Windows Server 2016 und älter nicht |
| Linux | Ubuntu 22.04 oder höher (x86_64 / ARM64), Vulkan-kompatible GPU |
| macOS | macOS 10.15 oder höher |

## Aus dem Quellcode bauen

Um eine eigene Version zu bauen oder den neuesten Entwicklungsstand zu erhalten:

```bash
git clone https://github.com/sunjary/netassistant.git
cd netassistant
cargo build --release
```

Nach dem Build befindet sich die ausführbare Datei im Verzeichnis `target/release`.

## Fehlerbehebung bei Abhängigkeiten

Die meisten Desktop-Systeme bringen die erforderlichen Abhängigkeiten bereits mit und laufen ohne weitere Einrichtung. Installieren Sie diese unter Linux nur, wenn der Start mit einer Fehlermeldung fehlschlägt:

- AppImage schlägt mit einem FUSE-Fehler fehl (z. B. `AppImages require FUSE to run`):

```bash
sudo apt install libfuse2
```

- Nackte Binärdatei / tar.gz schlägt wegen einer fehlenden GTK3-Bibliothek fehl (z. B. `error while loading shared libraries: libgtk-3.so.0`):

```bash
sudo apt install libgtk-3-0
```

## FAQ

### Ist NetAssistant kostenlos?

Ja. NetAssistant ist quelloffen unter der Apache-2.0-Lizenz, alle Funktionen sind kostenlos nutzbar, ohne In-App-Käufe, Abonnements oder Funktionsbeschränkungen.

### Welche Betriebssysteme werden unterstützt?

Windows 10 Version 1709 oder höher, macOS 10.15 oder höher und Linux-Distributionen wie Ubuntu 22.04 oder höher. Jede Plattform bietet Builds für x64 und ARM64.

### Wie installiere ich unter Windows?

Verwenden Sie winget: Führen Sie `winget install SunJary.NetAssistant` in einem Terminal aus und aktualisieren Sie später mit `winget upgrade`. Alternativ können Sie den Installer oder ein portables ZIP verwenden.

### Gibt es einen ARM64-Build?

Ja. Windows und Linux bieten aarch64-Builds, und macOS bietet ein universelles DMG für Intel und Apple Silicon, das Surface Pro X, Raspberry Pi 5 und Macs mit M-Serie abdeckt.

### macOS meldet, der Entwickler könne nicht überprüft werden. Was soll ich tun?

Das ist bei nicht signierten, nicht notarisierten Apps normal und bedeutet nicht, dass die App beschädigt ist. Klicken Sie mit der rechten Maustaste auf NetAssistant und wählen Sie **Öffnen**, oder gehen Sie zu **Systemeinstellungen → Datenschutz & Sicherheit** und klicken Sie auf **Trotzdem öffnen**.

Weitere Fragen finden Sie im [FAQ](/de/faq), Releases im [Changelog](/en/changelog).
