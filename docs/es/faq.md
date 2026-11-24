---
title: "Preguntas frecuentes: ¿NetAssistant es compatible con macOS?"
description: "Preguntas frecuentes de NetAssistant: disponibilidad en macOS y Linux, diferencias con NetAssist, instalación con winget, paquetes Linux y paquetes pegados TCP."
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
            "name": "¿La clásica herramienta de depuración de red NetAssist es compatible con macOS?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Deben distinguirse dos herramientas de nombre parecido: NetAssistant (la herramienta multiplataforma documentada en este sitio) es compatible con macOS, Windows y Linux con compilaciones nativas x64 y ARM64, mientras que la clásica NetAssist, de nombre similar, solo ofrece versión para Windows. Para depurar TCP/UDP en macOS o Linux, usa NetAssistant directamente."
            }
          },
          {
            "@type": "Question",
            "name": "¿Cuál es la diferencia entre NetAssistant y NetAssist?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Son programas diferentes. NetAssist es una herramienta de código cerrado solo para Windows; NetAssistant es una herramienta multiplataforma de código abierto (Apache-2.0) creada en Rust, compatible con Windows/Linux/macOS en x64 y ARM64, con cuatro decodificadores TCP para paquetes pegados y pruebas de carga integradas de alta concurrencia (QPS y latencia p50/p95/p99)."
            }
          },
          {
            "@type": "Question",
            "name": "¿Cómo instalo NetAssistant con winget?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Ejecuta winget install SunJary.NetAssistant en Windows. Las actualizaciones posteriores están disponibles con winget upgrade SunJary.NetAssistant."
            }
          },
          {
            "@type": "Question",
            "name": "¿Hay un paquete deb para Linux?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "NetAssistant ofrece paquetes deb, AppImage y tar.gz para Linux (x86_64 y aarch64). En Debian/Ubuntu y derivados se recomienda el paquete deb: sudo apt install resuelve las dependencias automáticamente; AppImage es ideal para Fedora/Arch y otras distribuciones (requiere libfuse2 en el primer inicio); tar.gz necesita GTK3 instalado."
            }
          },
          {
            "@type": "Question",
            "name": "¿NetAssistant es gratis y de código abierto?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Sí. NetAssistant se publica bajo la licencia Apache-2.0, todas las funciones son gratuitas y el código fuente está alojado en GitHub."
            }
          },
          {
            "@type": "Question",
            "name": "¿Cómo se manejan los paquetes pegados de TCP?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "NetAssistant incluye cuatro decodificadores TCP: datos sin procesar, delimitado por líneas, prefijo de longitud y JSON, que dividen el flujo de bytes automáticamente según el formato de tu protocolo. Consulta la guía de depuración TCP/UDP para más detalles."
            }
          }
        ]
      }
---

# Preguntas frecuentes

## ¿La clásica herramienta de depuración de red NetAssist es compatible con macOS?

Primero, una aclaración rápida: son **dos productos diferentes** cuyos nombres solo difieren en el sufijo “ant”:

- **NetAssistant** (la herramienta documentada en este sitio): de código abierto y multiplataforma; **Windows, Linux y macOS son compatibles**, con compilaciones nativas x64 y ARM64, así que funciona en un Mac sin configuración adicional → [descargar](/es/download)
- **NetAssist** (la clásica herramienta solo para Windows): no tiene versión para macOS ni Linux

Así que para depurar TCP/UDP en un Mac o en Linux, usa directamente **NetAssistant**: funciona igual que los asistentes de depuración que ya conoces. Consulta [Primeros pasos](/en/guide/) y la [tabla comparativa](/en/comparison) completa.

## ¿Cuál es la diferencia entre NetAssistant y NetAssist?

Son **programas diferentes** con nombres similares:

| | NetAssistant | NetAssist |
| ---- | ---- | ---- |
| Plataformas | Windows / Linux / macOS (x64 + ARM64) | Solo Windows |
| Código abierto | ✅ Apache-2.0 | ❌ Código cerrado |
| Decodificadores TCP (paquetes pegados) | Datos sin procesar / línea / prefijo de longitud / JSON | Limitados |
| Pruebas de carga TCP/UDP de alta concurrencia | ✅ QPS + p50/p95/p99 | ❌ |

Consulta la [tabla comparativa](/en/comparison) completa.

## ¿Cómo instalo NetAssistant con winget?

En Windows, la forma recomendada es winget (con actualizaciones automáticas):

```bash
winget install SunJary.NetAssistant
```

Actualiza más adelante con:

```bash
winget upgrade SunJary.NetAssistant
```

Más opciones de instalación en la [página de descarga](/es/download).

## ¿Hay un paquete deb para Linux?

Sí. Se ofrecen paquetes **deb**, **AppImage** y **tar.gz** (tanto x86_64 como aarch64):

- **deb** (recomendado): para Debian/Ubuntu y derivados — `sudo apt install ./netassistant-linux-x86_64.deb` resuelve las dependencias automáticamente; funciona en Ubuntu 22.04 y posteriores
- **AppImage**: portable y listo para usar — ideal para Fedora/Arch y otras distribuciones; el primer inicio requiere `sudo apt install libfuse2`
- **tar.gz**: descomprime y ejecuta; requiere GTK3 (`sudo apt install libgtk-3-0`)

Enlaces de descarga en la [página de descarga](/es/download#linux).

## ¿NetAssistant es gratis y de código abierto?

Sí. NetAssistant se publica bajo la licencia **Apache-2.0** con todas las funciones gratuitas. El código fuente está en [GitHub](https://github.com/SunJary/NetAssistant): las contribuciones son bienvenidas.

## ¿Cómo se manejan los paquetes pegados de TCP?

NetAssistant incluye cuatro decodificadores TCP que dividen el flujo de bytes automáticamente, para que nunca tengas que contar bytes a mano:

1. **Datos sin procesar**: se muestran en orden de llegada
2. **Delimitado por líneas**: se divide por salto de línea
3. **Prefijo de longitud**: se divide por el campo de longitud de la cabecera
4. **JSON**: se divide por cada objeto JSON completo

Consulta la [guía de depuración TCP/UDP](/en/guide/tcp-udp) para ver su uso.
