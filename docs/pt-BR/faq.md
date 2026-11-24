---
title: FAQ - Depuração TCP/UDP e suporte a macOS
description: "FAQ do NetAssistant: suporte a macOS e Linux, diferenças em relação ao NetAssist, instalação via winget, pacotes Linux, TCP sticky packets e teste de carga."
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
            "name": "A ferramenta clássica de depuração de rede NetAssist suporta macOS?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "É preciso distinguir duas ferramentas com nomes parecidos: o NetAssistant (ferramenta multiplataforma documentada neste site) suporta macOS, Windows e Linux com builds nativos x64 e ARM64, enquanto o clássico NetAssist, de nome semelhante, oferece apenas versão para Windows. Para depurar TCP/UDP no macOS ou no Linux, use o NetAssistant diretamente. Veja o guia de introdução e a tabela de comparação completa."
            }
          },
          {
            "@type": "Question",
            "name": "Qual é a diferença entre NetAssistant e NetAssist?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "São softwares diferentes. O NetAssist é uma ferramenta de código fechado exclusiva para Windows; o NetAssistant é uma ferramenta open source (Apache-2.0) multiplataforma escrita em Rust, com suporte a Windows/Linux/macOS em x64 e ARM64, quatro decodificadores TCP para sticky packets e teste de carga de alta concorrência integrado (QPS e latência p50/p95/p99). Veja a tabela de comparação completa."
            }
          },
          {
            "@type": "Question",
            "name": "Como instalar o NetAssistant com winget?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Execute winget install SunJary.NetAssistant no Windows. Atualizações posteriores ficam disponíveis com winget upgrade SunJary.NetAssistant. Mais opções de instalação na página de download."
            }
          },
          {
            "@type": "Question",
            "name": "Existe pacote deb para Linux?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "O NetAssistant oferece pacotes deb, AppImage e tar.gz para Linux (x86_64 e aarch64). No Debian/Ubuntu e derivados, o pacote deb é recomendado — sudo apt install resolve as dependências automaticamente; o AppImage é indicado para Fedora/Arch e outras distros (requer libfuse2 na primeira execução); o tar.gz exige o GTK3 instalado. Os links de download estão na página de download."
            }
          },
          {
            "@type": "Question",
            "name": "O NetAssistant é gratuito e open source?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Sim. O NetAssistant é distribuído sob a licença Apache-2.0, todos os recursos são gratuitos e o código-fonte está hospedado no GitHub — contribuições são bem-vindas."
            }
          },
          {
            "@type": "Question",
            "name": "Como tratar TCP sticky packets?",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "O NetAssistant traz quatro decodificadores TCP que dividem o fluxo de bytes automaticamente, sem contar bytes manualmente: dados brutos (exibidos na ordem de chegada), delimitado por linha (divisão por quebra de linha), prefixo de comprimento (divisão pelo campo de tamanho do cabeçalho) e JSON (divisão por objeto JSON completo). Veja o guia de depuração TCP/UDP para saber como usar."
            }
          }
        ]
      }
---

# FAQ

## A ferramenta clássica de depuração de rede NetAssist suporta macOS?

É preciso distinguir duas ferramentas com nomes parecidos: o NetAssistant (ferramenta multiplataforma documentada neste site) suporta macOS, Windows e Linux com builds nativos x64 e ARM64, enquanto o clássico NetAssist, de nome semelhante, oferece apenas versão para Windows. Para depurar TCP/UDP no macOS ou no Linux, use o NetAssistant diretamente. Veja o [guia de introdução](/en/guide/) e a [tabela de comparação completa](/en/comparison).

## Qual é a diferença entre NetAssistant e NetAssist?

São softwares diferentes. O NetAssist é uma ferramenta de código fechado exclusiva para Windows; o NetAssistant é uma ferramenta open source (Apache-2.0) multiplataforma escrita em Rust, com suporte a Windows/Linux/macOS em x64 e ARM64, quatro decodificadores TCP para sticky packets e teste de carga de alta concorrência integrado (QPS e latência p50/p95/p99). Veja a [tabela de comparação completa](/en/comparison).

| | NetAssistant | NetAssist |
| ---- | ---- | ---- |
| Plataformas | Windows / Linux / macOS (x64 + ARM64) | Apenas Windows |
| Código aberto | ✅ Apache-2.0 | ❌ Código fechado |
| Decodificadores TCP (sticky packets) | Bruto / linha / prefixo de comprimento / JSON | Limitado |
| Teste de carga de alta concorrência TCP/UDP | ✅ QPS + p50/p95/p99 | ❌ |

## Como instalar o NetAssistant com winget?

Execute `winget install SunJary.NetAssistant` no Windows. Atualizações posteriores ficam disponíveis com `winget upgrade SunJary.NetAssistant`. Mais opções de instalação na [página de download](/pt-BR/download).

## Existe pacote deb para Linux?

O NetAssistant oferece pacotes deb, AppImage e tar.gz para Linux (x86_64 e aarch64). No Debian/Ubuntu e derivados, o pacote deb é recomendado — `sudo apt install` resolve as dependências automaticamente; o AppImage é indicado para Fedora/Arch e outras distros (requer `libfuse2` na primeira execução); o tar.gz exige o GTK3 instalado. Os links de download estão na [página de download](/pt-BR/download#linux).

## O NetAssistant é gratuito e open source?

Sim. O NetAssistant é distribuído sob a licença **Apache-2.0**, todos os recursos são gratuitos e o código-fonte está hospedado no [GitHub](https://github.com/SunJary/NetAssistant) — contribuições são bem-vindas.

## Como tratar TCP sticky packets?

O NetAssistant traz quatro decodificadores TCP que dividem o fluxo de bytes automaticamente, sem contar bytes manualmente: dados brutos (exibidos na ordem de chegada), delimitado por linha (divisão por quebra de linha), prefixo de comprimento (divisão pelo campo de tamanho do cabeçalho) e JSON (divisão por objeto JSON completo). Veja o [guia de depuração TCP/UDP](/en/guide/tcp-udp) para saber como usar.
