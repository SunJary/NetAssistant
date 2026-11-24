---
title: FAQ – Unterstützt NetAssistant macOS?
description: "NetAssistant FAQ: Verfügbarkeit für macOS und Linux, Unterschied zu NetAssist, winget-Installation, Linux-Pakete, TCP-Sticky-Packets und TCP/UDP-Lasttests."
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
            "name": "Unterstützt der klassische NetAssist-Netzwerkassistent macOS?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Zwei ähnlich benannte Tools sind zu unterscheiden: NetAssistant (das auf dieser Website dokumentierte plattformübergreifende Tool) unterstützt macOS, Windows und Linux mit nativen Builds für x64 und ARM64, während der ähnlich benannte klassische NetAssist nur eine Windows-Version bietet. Für TCP/UDP-Debugging unter macOS oder Linux verwenden Sie direkt NetAssistant."
            }
          },
          {
            "@type": "Question",
            "name": "Worin unterscheidet sich NetAssistant von NetAssist?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Es sind zwei verschiedene Programme. NetAssist ist ein quellgeschlossenes Windows-only-Tool; NetAssistant ist ein quelloffenes (Apache-2.0) plattformübergreifendes Tool, entwickelt mit Rust, das Windows/Linux/macOS auf x64 und ARM64 unterstützt, mit vier TCP-Decodern für Sticky Packets und integrierten Lasttests mit hoher Parallelität (QPS und p50/p95/p99-Latenz)."
            }
          },
          {
            "@type": "Question",
            "name": "Wie installiere ich NetAssistant mit winget?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Führen Sie winget install SunJary.NetAssistant unter Windows aus. Spätere Aktualisierungen sind über winget upgrade SunJary.NetAssistant möglich."
            }
          },
          {
            "@type": "Question",
            "name": "Gibt es ein deb-Paket für Linux?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "NetAssistant bietet deb-, AppImage- und tar.gz-Pakete für Linux (x86_64 und aarch64). Unter Debian/Ubuntu und Derivaten wird das deb-Paket empfohlen – sudo apt install löst Abhängigkeiten automatisch auf; AppImage eignet sich für Fedora/Arch und andere Distributionen (beim ersten Start ist libfuse2 erforderlich); tar.gz erfordert installiertes GTK3."
            }
          },
          {
            "@type": "Question",
            "name": "Ist NetAssistant kostenlos und quelloffen?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Ja. NetAssistant ist unter der Apache-2.0-Lizenz veröffentlicht, alle Funktionen sind kostenlos, und der Quellcode wird auf GitHub gehostet."
            }
          },
          {
            "@type": "Question",
            "name": "Wie gehe ich mit TCP-Sticky-Packets um?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "NetAssistant bietet vier TCP-Decoder: Rohdaten, zeilengetrennt, Längenpräfix und JSON, die den Byte-Stream automatisch gemäß Ihrem Protokollformat aufteilen. Weitere Details finden Sie in der TCP/UDP-Debugging-Anleitung."
            }
          }
        ]
      }
---

# FAQ

## Unterstützt der klassische NetAssist-Netzwerkassistent macOS?

Zunächst eine kurze Abgrenzung – es handelt sich um **zwei verschiedene Produkte**, deren Namen sich nur im abschließenden „ant“ unterscheiden:

- **NetAssistant** (das auf dieser Website dokumentierte Tool): quelloffen und plattformübergreifend – **Windows, Linux und macOS werden vollständig unterstützt**, mit nativen Builds für x64 und ARM64, läuft also auf einem Mac ohne weitere Schritte → [Download](/de/download)
- **NetAssist** (das klassische Windows-only-Tool): keine macOS- oder Linux-Version

Für TCP/UDP-Debugging unter macOS oder Linux verwenden Sie daher einfach **NetAssistant** – es funktioniert genauso wie die Debugging-Assistenten, die Sie bereits kennen. Siehe [Erste Schritte](/en/guide/) und die vollständige [Vergleichstabelle](/en/comparison).

## Worin unterscheidet sich NetAssistant von NetAssist?

Es sind **zwei verschiedene Programme** mit ähnlichen Namen:

| | NetAssistant | NetAssist |
| ---- | ---- | ---- |
| Plattformen | Windows / Linux / macOS (x64 + ARM64) | Nur Windows |
| Open Source | ✅ Apache-2.0 | ❌ Closed Source |
| TCP-Decoder (Sticky Packets) | Roh / Zeile / Längenpräfix / JSON | Eingeschränkt |
| TCP/UDP-Lasttests mit hoher Parallelität | ✅ QPS + p50/p95/p99 | ❌ |

Siehe die vollständige [Vergleichstabelle](/en/comparison).

## Wie installiere ich NetAssistant mit winget?

Unter Windows ist winget der empfohlene Weg (mit automatischen Aktualisierungen):

```bash
winget install SunJary.NetAssistant
```

Später aktualisieren mit:

```bash
winget upgrade SunJary.NetAssistant
```

Weitere Installationsoptionen finden Sie auf der [Download-Seite](/de/download).

## Gibt es ein deb-Paket für Linux?

Ja. Es werden **deb**-, **AppImage**- und **tar.gz**-Pakete bereitgestellt (sowohl für x86_64 als auch aarch64):

- **deb** (empfohlen): für Debian/Ubuntu und Derivate – `sudo apt install ./netassistant-linux-x86_64.deb` löst Abhängigkeiten automatisch auf; funktioniert ab Ubuntu 22.04
- **AppImage**: portabel und sofort einsatzbereit – ideal für Fedora/Arch und andere Distributionen; beim ersten Start ist `sudo apt install libfuse2` erforderlich
- **tar.gz**: entpacken und starten, erfordert GTK3 (`sudo apt install libgtk-3-0`)

Download-Links auf der [Download-Seite](/de/download#linux).

## Ist NetAssistant kostenlos und quelloffen?

Ja. NetAssistant ist unter der **Apache-2.0**-Lizenz veröffentlicht, alle Funktionen sind kostenlos. Der Quellcode ist auf [GitHub](https://github.com/SunJary/NetAssistant) verfügbar – Beiträge sind willkommen.

## Wie gehe ich mit TCP-Sticky-Packets um?

NetAssistant bietet vier TCP-Decoder, die den Byte-Stream automatisch aufteilen, sodass Sie nie Bytes von Hand zählen müssen:

1. **Rohdaten**: angezeigt in der Reihenfolge des Eintreffens
2. **Zeilengetrennt**: Aufteilung an Zeilenumbrüchen
3. **Längenpräfix**: Aufteilung anhand des Längenfelds im Header
4. **JSON**: Aufteilung pro vollständigem JSON-Objekt

Siehe die [TCP/UDP-Debugging-Anleitung](/en/guide/tcp-udp) zur Verwendung.
