---
layout: home

title: NetAssistant - Ferramenta de depuração de rede TCP/UDP/MQTT
titleTemplate: ':title'
description: "Ferramenta de depuração de rede em Rust com cliente e servidor TCP/UDP, MQTT 3.1.1 / 5.0, decodificadores e teste de carga para Windows, Linux e macOS."

hero:
  name: NetAssistant
  text: Ferramenta de depuração de rede multiplataforma de alto desempenho
  tagline: Feito em Rust · Alternativa open source ao NetAssist e ao MQTTX · Cliente e servidor TCP/UDP/MQTT · Múltiplos decodificadores · Caixa de ferramentas integrada e teste de carga TCP/UDP · Interface acelerada por GPU
  actions:
    - theme: brand
      text: Baixar agora
      link: /pt-BR/download
    - theme: alt
      text: Guia do usuário
      link: /en/guide/
    - theme: alt
      text: GitHub
      link: https://github.com/SunJary/NetAssistant

features:
  - icon: 🔌
    title: Suporte a múltiplos protocolos
    details: Suporte completo aos modos cliente e servidor TCP/UDP e a um cliente MQTT 3.1.1 / 5.0 (TLS, chaves por assinatura, QoS 0/1/2, mensagens de última vontade), com pilha dupla IPv4/IPv6 para qualquer ambiente de rede.
  - icon: 🧰
    title: Caixa de ferramentas integrada
    details: Sete grupos do dia a dia — conversão, checksum, hash, tempo, JSON, codificação e aleatório — com resultados instantâneos e cópia em um clique, sem precisar abrir sites externos.
  - icon: 📦
    title: Decodificadores inteligentes
    details: Quatro decodificadores TCP — dados brutos, delimitado por linha, prefixo de comprimento e JSON — que resolvem de forma eficaz os problemas de sticky packet; envio e recebimento em modo hexadecimal.
  - icon: 💬
    title: Log de mensagens estilo chat
    details: Exibição intuitiva das trocas de pacotes, com favoritos e observações, busca por palavra-chave, log em tempo real e exportação em TXT/JSON/CSV.
  - icon: 🚀
    title: Teste de carga TCP/UDP
    details: Motor de carga TCP/UDP integrado com QPS em tempo real, percentis de latência (p50/p95/p99), detalhamento de falhas, templates com variáveis e exportação de relatório em CSV; teste de carga MQTT não é suportado.
  - icon: 🤖
    title: Testes automatizados
    details: Resposta automática para simular o par, envio periódico para testes de estabilidade de longa duração e exibição inteligente de respostas de broadcast UDP para descoberta de dispositivos.
  - icon: 🎨
    title: Interface moderna
    details: Renderização acelerada por GPU com o framework GPUI a 60fps estáveis; o modo escuro acompanha o sistema automaticamente, com gerenciamento de conexões em múltiplas abas.
---

## Capturas de tela

### Modo cliente

![Captura de tela do modo cliente](../../assets/screenshots/en/screenshot_client.png)

### Modo servidor

![Captura de tela do modo servidor](../../assets/screenshots/en/screenshot_server.png)

### Cliente MQTT

![Captura de tela do cliente MQTT](../../assets/screenshots/en/screenshot_mqtt_client.png)

### Caixa de ferramentas integrada

![Captura de tela do hash na caixa de ferramentas](../../assets/screenshots/en/screenshot_toolbox_hash.png)

### Teste de carga

![Captura de tela do teste de carga](../../assets/screenshots/en/screenshot_stress.png)

### Modo escuro

![Captura de tela do servidor UDP no modo escuro](../../assets/screenshots/en/screenshot_udp_server_dark.png)

## Por que NetAssistant

- **Desempenho excepcional**: runtime assíncrono Rust + Tokio, inicialização < 100ms, consumo de memória < 20MB, conexões simultâneas na casa dos milhões
- **Feito para depuração**: do desenvolvimento de aplicações de rede à depuração de hardware e sistemas embarcados, cobrindo todo o fluxo de validação de comunicação
- **Multiplataforma**: suporte completo a Windows, Linux e macOS, com builds nativos para x64 e ARM64
- **Open source e gratuito**: distribuído sob a licença Apache-2.0, contribuições são bem-vindas

Para mais detalhes, veja [Recursos](/en/features); para começar, leia o [Guia](/en/guide/). Para comparar com o NetAssist e ferramentas semelhantes, consulte a [Comparação](/en/comparison).

## Download por plataforma

- [Ferramenta de depuração de rede para Windows](/en/windows-network-debug-tool): instalação em um único comando com winget, Windows 10/11 em x64 e ARM64
- [Ferramenta de depuração de rede para Mac](/en/mac-network-debug-tool): DMG universal para Apple Silicon e Intel
- [Ferramenta de depuração de rede para Linux](/en/linux-network-debug-tool): deb, AppImage e tar.gz para x86_64 e ARM64

## Guias populares

- [Alternativa ao NetAssist: comparação e migração](/en/compare/netassist)
- [Ferramenta de depuração TCP: envio, recebimento e framing](/en/tcp-debug-tool)
- [Ferramenta de depuração UDP: envio sem conexão e respostas de vários dispositivos](/en/udp-debug-tool)
- [Ferramenta de depuração de socket: escuta de porta e verificação de alcance](/en/socket-debug-tool)
- [Envio em hexadecimal: payloads HEX e conversão para texto](/en/hex-send)
- [Depuração Modbus TCP: frames e CRC16](/en/modbus-tcp)
- [Sticky packets TCP: causas, quatro soluções e depuração](/en/tcp-sticky-packet)
- [Guia de depuração MQTT: encontre páginas por tarefa](/en/mqtt)
- [Comparativo de clientes MQTT: MQTTX, Explorer e MQTT.fx](/en/compare/)
- [Cliente MQTT: ferramenta leve de depuração MQTT 3.1.1 / 5.0](/en/mqtt-client)
- [Alternativa ao MQTTX: comparativo de clientes MQTT leves](/en/comparison-mqttx)
- [Tutorial MQTT: conectar, assinar e publicar](/en/guide/mqtt)
- [Solução de problemas MQTT: conexões e mensagens ausentes](/en/mqtt-troubleshooting)
- [MQTT TLS: verificação de certificado e autenticação mútua](/en/mqtt-tls)
- [MQTT 5.0: expiração de sessão e códigos de motivo](/en/mqtt-5)
- [Comparativo da alternativa MQTT Explorer](/en/compare/mqtt-explorer)
- [Comparativo da alternativa MQTT.fx](/en/compare/mqtt-fx)
- [FAQ MQTT](/en/mqtt-faq)
- [Instalação do cliente MQTT no Linux](/en/mqtt-client-linux)
- [Teste de carga TCP/UDP: QPS e latência p99](/en/tcp-stress-test)
- [Descoberta de dispositivos via broadcast UDP: busca na LAN e solução de problemas](/en/udp-broadcast-device-discovery)
