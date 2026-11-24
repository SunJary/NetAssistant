---
title: Descargar NetAssistant para Windows, Linux y macOS
titleTemplate: ':title'
description: "Descarga NetAssistant para Windows, Linux y macOS en x64 y ARM64: winget, paquetes deb/AppImage/tar.gz o compilar desde el código fuente."
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
            "description": "Herramienta de depuración de red multiplataforma con cliente y servidor TCP/UDP, MQTT 3.1.1 / 5.0, decodificadores de tramas, cargas en hexadecimal y pruebas de carga TCP/UDP.",
            "url": "https://netassistant.trydo.top/es/download",
            "downloadUrl": "https://github.com/SunJary/NetAssistant/releases",
            "license": "https://www.apache.org/licenses/LICENSE-2.0",
            "offers": { "@type": "Offer", "price": "0", "priceCurrency": "USD" }
          },
          {
            "@type": "FAQPage",
            "mainEntity": [
              {
                "@type": "Question",
                "name": "¿NetAssistant es gratis?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Sí. NetAssistant es de código abierto bajo la licencia Apache-2.0 y todas las funciones son gratuitas, sin compras integradas, suscripciones ni límites de funciones."
                }
              },
              {
                "@type": "Question",
                "name": "¿Qué sistemas operativos son compatibles?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Windows 10 versión 1709 o posterior, macOS 10.15 o posterior y distribuciones Linux como Ubuntu 22.04 o posterior. Cada plataforma incluye compilaciones x64 y ARM64."
                }
              },
              {
                "@type": "Question",
                "name": "¿Cómo instalo en Windows?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Usa winget: ejecuta winget install SunJary.NetAssistant en una terminal y actualiza más adelante con winget upgrade. También puedes usar el instalador o un zip portable."
                }
              },
              {
                "@type": "Question",
                "name": "¿Hay una compilación para ARM64?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Sí. Windows y Linux incluyen compilaciones aarch64, y macOS ofrece un DMG universal para Intel y Apple Silicon, compatible con Surface Pro X, Raspberry Pi 5 y Macs de la serie M."
                }
              },
              {
                "@type": "Question",
                "name": "macOS dice que no puede verificar al desarrollador. ¿Qué debo hacer?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Es normal en aplicaciones sin firmar ni notarizar y no significa que la aplicación esté dañada. Haz clic derecho en NetAssistant y elige Abrir, o ve a Ajustes del Sistema → Privacidad y seguridad y haz clic en Abrir de todos modos."
                }
              }
            ]
          }
        ]
      }
---

# Descargar

NetAssistant está disponible para Windows, Linux y macOS, con compilaciones x64 y ARM64 para cada plataforma. Los enlaces de descarga siguientes apuntan siempre a los archivos de la [última versión](https://github.com/sunjary/netassistant/releases/latest); haz clic para descargar directamente. Las versiones anteriores están en [GitHub Releases](https://github.com/sunjary/netassistant/releases).

Guías de instalación por plataforma: [Windows](/en/windows-network-debug-tool) · [macOS](/en/mac-network-debug-tool) · [Linux](/en/linux-network-debug-tool).

<DownloadHero />

## Windows

**Recomendado: instalar con winget** (admite actualizaciones automáticas)

```bash
winget install SunJary.NetAssistant
```

Para actualizar:

```bash
winget upgrade SunJary.NetAssistant
```

**O usa el instalador**: descarga [netassistant-windows-x86_64-setup.exe](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-windows-x86_64-setup.exe) y sigue el asistente (crea automáticamente accesos directos en el menú Inicio y en el escritorio).

**Alternativa**: descarga [netassistant-windows-x86_64.zip](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-windows-x86_64.zip), descomprímelo y ejecuta `netassistant.exe`.

**Dispositivos ARM64** (Surface Pro X, portátiles Snapdragon, etc.): winget y el instalador actualmente solo ofrecen x64. En ARM64, descarga [netassistant-windows-aarch64.zip](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-windows-aarch64.zip), descomprímelo y ejecuta `netassistant.exe`. Esta compilación se genera automáticamente y no se ha probado por completo en hardware real.

## Linux

**Recomendado: paquete deb** (Debian/Ubuntu y derivados, instala las dependencias automáticamente)

1. Descarga [netassistant-linux-x86_64.deb](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-x86_64.deb)
2. Instálalo (las dependencias se resuelven automáticamente):

```bash
sudo apt install ./netassistant-linux-x86_64.deb
```

3. Inicia la aplicación desde el menú de aplicaciones o ejecuta `netassistant` en una terminal

**AppImage** (portable, para Fedora/Arch y otras distribuciones)

1. Descarga [netassistant-linux-x86_64.AppImage](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-x86_64.AppImage)
2. Añade permiso de ejecución y ejecútalo:

```bash
chmod +x netassistant-linux-x86_64.AppImage
./netassistant-linux-x86_64.AppImage
```

**Alternativa: tar.gz** (ligero)

1. Descarga [netassistant-linux-x86_64.tar.gz](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-x86_64.tar.gz)
2. Descomprime y ejecuta:

```bash
tar -xzf netassistant-linux-x86_64.tar.gz
chmod +x netassistant
./netassistant
```

**Dispositivos ARM64** (Raspberry Pi 5, servidores ARM, etc.): descarga [netassistant-linux-aarch64.deb](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-aarch64.deb), [netassistant-linux-aarch64.AppImage](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-aarch64.AppImage) o [netassistant-linux-aarch64.tar.gz](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-aarch64.tar.gz); su uso es igual que el anterior. Esta compilación se genera automáticamente y no se ha probado por completo en hardware real.

## macOS

**Recomendado: DMG universal** (funciona tanto en Intel como en Apple Silicon)

1. Descarga [netassistant-macos-universal.dmg](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-macos-universal.dmg), ábrelo y arrastra NetAssistant a la carpeta Aplicaciones
2. Si macOS muestra el aviso “no se puede verificar al desarrollador” en el primer inicio (es normal en aplicaciones sin firmar ni notarizar, no indica corrupción): haz clic derecho en NetAssistant → Abrir; o ve a Ajustes del Sistema → Privacidad y seguridad y haz clic en “Abrir de todos modos”

**Alternativa**: descarga [netassistant-macos-universal.zip](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-macos-universal.zip) y descomprímela para ejecutarla directamente (se conservan los permisos de ejecución).

## Requisitos del sistema

| Plataforma | Requisitos |
| -------- | ------------ |
| Windows | Windows 10 versión 1709 (Build 16299) o posterior, incluido Windows 11 (x64 / ARM64). Windows Server 2019 y posteriores son compatibles; Windows Server 2016 y anteriores no |
| Linux | Ubuntu 22.04 o posterior (x86_64 / ARM64), GPU compatible con Vulkan |
| macOS | macOS 10.15 o posterior |

## Compilar desde el código fuente

Para compilar una versión personalizada u obtener la última instantánea de desarrollo:

```bash
git clone https://github.com/sunjary/netassistant.git
cd netassistant
cargo build --release
```

Tras la compilación, el ejecutable se encuentra en el directorio `target/release`.

## Solucionar errores de dependencias

La mayoría de los sistemas de escritorio ya incluyen las dependencias necesarias y funcionan sin configuración adicional. En Linux, instala estas solo si el inicio falla con un error:

- AppImage falla con un error de FUSE (p. ej. `AppImages require FUSE to run`):

```bash
sudo apt install libfuse2
```

- El binario sin empaquetar / tar.gz falla por falta de la biblioteca GTK3 (p. ej. `error while loading shared libraries: libgtk-3.so.0`):

```bash
sudo apt install libgtk-3-0
```

## Preguntas frecuentes

### ¿NetAssistant es gratis?

Sí. NetAssistant es de código abierto bajo la licencia Apache-2.0 y todas las funciones son gratuitas, sin compras integradas, suscripciones ni límites de funciones.

### ¿Qué sistemas operativos son compatibles?

Windows 10 versión 1709 o posterior, macOS 10.15 o posterior y distribuciones Linux como Ubuntu 22.04 o posterior. Cada plataforma incluye compilaciones x64 y ARM64.

### ¿Cómo instalo en Windows?

Usa winget: ejecuta `winget install SunJary.NetAssistant` en una terminal y actualiza más adelante con `winget upgrade`. También puedes usar el instalador o un zip portable.

### ¿Hay una compilación para ARM64?

Sí. Windows y Linux incluyen compilaciones aarch64, y macOS ofrece un DMG universal para Intel y Apple Silicon, compatible con Surface Pro X, Raspberry Pi 5 y Macs de la serie M.

### macOS dice que no puede verificar al desarrollador. ¿Qué debo hacer?

Es normal en aplicaciones sin firmar ni notarizar y no significa que la aplicación esté dañada. Haz clic derecho en NetAssistant y elige **Abrir**, o ve a **Ajustes del Sistema → Privacidad y seguridad** y haz clic en **Abrir de todos modos**.

Consulta las [preguntas frecuentes](/es/faq) para más dudas y el [registro de cambios](/en/changelog) para las versiones.
