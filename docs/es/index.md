---
layout: home

title: NetAssistant - Herramienta de depuración de red TCP/UDP/MQTT
titleTemplate: ':title'
description: "Herramienta de depuración de red en Rust con cliente y servidor TCP/UDP, MQTT 3.1.1 / 5.0, decodificadores y pruebas de carga para Windows, Linux y macOS."

hero:
  name: NetAssistant
  text: Herramienta de depuración de red multiplataforma de alto rendimiento
  tagline: Desarrollada en Rust · Alternativa open source a NetAssist y MQTTX · Cliente y servidor TCP/UDP/MQTT · Varios decodificadores · Caja de herramientas integrada y pruebas de carga TCP/UDP · Interfaz acelerada por GPU
  actions:
    - theme: brand
      text: Descargar ahora
      link: /es/download
    - theme: alt
      text: Guía de usuario
      link: /en/guide/
    - theme: alt
      text: GitHub
      link: https://github.com/SunJary/NetAssistant

features:
  - icon: 🔌
    title: Compatibilidad con múltiples protocolos
    details: Compatibilidad total con los modos cliente y servidor TCP/UDP, además de un cliente MQTT 3.1.1 / 5.0 (TLS, conmutadores por suscripción, QoS 0/1/2, mensajes de última voluntad), con pila dual IPv4/IPv6 para cualquier entorno de red.
  - icon: 🧰
    title: Caja de herramientas integrada
    details: Siete grupos de uso diario — conversión, checksum, hash, tiempo, JSON, codificación y aleatorio — con resultados instantáneos y copia con un clic, sin necesidad de acudir a sitios externos.
  - icon: 📦
    title: Decodificadores inteligentes
    details: Cuatro decodificadores TCP — datos sin procesar, delimitado por líneas, prefijo de longitud y JSON — que resuelven de forma eficaz los paquetes pegados; envío y recepción en modo hexadecimal.
  - icon: 💬
    title: Registro de mensajes tipo chat
    details: Visualización intuitiva del intercambio de paquetes, con favoritos y notas, búsqueda por palabra clave, registro en tiempo real y exportación a TXT/JSON/CSV.
  - icon: 🚀
    title: Pruebas de carga TCP/UDP
    details: Motor de carga TCP/UDP integrado con QPS en tiempo real, percentiles de latencia (p50/p95/p99), desglose de fallos, plantillas con variables y exportación de informes CSV; las pruebas de carga MQTT no son compatibles.
  - icon: 🤖
    title: Pruebas automatizadas
    details: Respuesta automática para simular al interlocutor, envío periódico para pruebas de estabilidad de larga duración y visualización inteligente de respuestas de difusión UDP para el descubrimiento de dispositivos.
  - icon: 🎨
    title: Interfaz moderna
    details: Renderizado acelerado por GPU con el framework GPUI a 60 fps fluidos; el modo oscuro sigue automáticamente al sistema, con gestión de conexiones en pestañas.
---

## Capturas de pantalla

### Modo cliente

![Captura de pantalla del modo cliente](../../assets/screenshots/en/screenshot_client.png)

### Modo servidor

![Captura de pantalla del modo servidor](../../assets/screenshots/en/screenshot_server.png)

### Cliente MQTT

![Captura de pantalla del cliente MQTT](../../assets/screenshots/en/screenshot_mqtt_client.png)

### Caja de herramientas integrada

![Captura de pantalla de la herramienta de hash](../../assets/screenshots/en/screenshot_toolbox_hash.png)

### Pruebas de carga

![Captura de pantalla de las pruebas de carga](../../assets/screenshots/en/screenshot_stress.png)

### Modo oscuro

![Captura de pantalla del servidor UDP en modo oscuro](../../assets/screenshots/en/screenshot_udp_server_dark.png)

## Por qué NetAssistant

- **Rendimiento excepcional**: Rust + runtime asíncrono Tokio, arranque < 100ms, consumo de memoria < 20MB, conexiones concurrentes a escala de millones
- **Diseñada para depurar**: desde el desarrollo de aplicaciones de red hasta la depuración de hardware y sistemas embebidos, cubre todo el flujo de verificación de comunicaciones
- **Multiplataforma**: compatibilidad total con Windows, Linux y macOS, con compilaciones nativas para x64 y ARM64
- **Código abierto y gratuita**: publicada bajo la licencia Apache-2.0, contribuciones bienvenidas

Para más detalles, consulta [Funciones](/en/features); para empezar, lee la [Guía](/en/guide/). Para ver cómo se compara con NetAssist y herramientas similares, consulta la [Comparación](/en/comparison).

## Descargar por plataforma

- [Herramienta de depuración de red para Windows](/en/windows-network-debug-tool): instalación con winget en un solo comando, Windows 10/11 en x64 y ARM64
- [Herramienta de depuración de red para Mac](/en/mac-network-debug-tool): DMG universal para Apple Silicon e Intel
- [Herramienta de depuración de red para Linux](/en/linux-network-debug-tool): deb, AppImage y tar.gz para x86_64 y ARM64

## Guías populares

- [Alternativa a NetAssist: comparación y migración](/en/compare/netassist)
- [Herramienta de depuración TCP: envío, recepción y delimitación de tramas](/en/tcp-debug-tool)
- [Herramienta de depuración UDP: envío sin conexión y respuestas de múltiples dispositivos](/en/udp-debug-tool)
- [Herramienta de depuración de sockets: escucha de puertos y accesibilidad](/en/socket-debug-tool)
- [Envío hexadecimal: cargas HEX y conversión de texto](/en/hex-send)
- [Depuración Modbus TCP: tramas y CRC16](/en/modbus-tcp)
- [Paquetes pegados en TCP: causas, cuatro soluciones y depuración](/en/tcp-sticky-packet)
- [Guía de depuración MQTT: encuentra páginas por tarea](/en/mqtt)
- [Comparación de clientes MQTT: MQTTX, Explorer y MQTT.fx](/en/compare/)
- [Cliente MQTT: una herramienta ligera de depuración MQTT 3.1.1 / 5.0](/en/mqtt-client)
- [Alternativa a MQTTX: comparación de clientes MQTT ligeros](/en/comparison-mqttx)
- [Tutorial de MQTT: conectar, suscribirse y publicar](/en/guide/mqtt)
- [Solución de problemas de MQTT: conexiones y mensajes perdidos](/en/mqtt-troubleshooting)
- [MQTT TLS: verificación de certificados y autenticación mutua](/en/mqtt-tls)
- [MQTT 5.0: caducidad de sesión y códigos de motivo](/en/mqtt-5)
- [Comparación con la alternativa MQTT Explorer](/en/compare/mqtt-explorer)
- [Comparación con la alternativa MQTT.fx](/en/compare/mqtt-fx)
- [Preguntas frecuentes sobre MQTT](/en/mqtt-faq)
- [Instalación del cliente MQTT en Linux](/en/mqtt-client-linux)
- [Pruebas de carga TCP/UDP: QPS y latencia p99](/en/tcp-stress-test)
- [Descubrimiento de dispositivos por difusión UDP: búsqueda en LAN y solución de problemas](/en/udp-broadcast-device-discovery)
