---
title: Baixar NetAssistant - Grátis para Windows, Linux e macOS
titleTemplate: ':title'
description: "Baixe o NetAssistant para Windows, Linux e macOS em x64 e ARM64: winget, deb/AppImage/tar.gz ou compilação do código-fonte, com requisitos de sistema."
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
            "description": "Ferramenta de depuração de rede multiplataforma com cliente e servidor TCP/UDP, MQTT 3.1.1 / 5.0, decodificadores de framing, payloads hexadecimais e teste de carga TCP/UDP.",
            "url": "https://netassistant.trydo.top/pt-BR/download",
            "downloadUrl": "https://github.com/SunJary/NetAssistant/releases",
            "license": "https://www.apache.org/licenses/LICENSE-2.0",
            "offers": { "@type": "Offer", "price": "0", "priceCurrency": "USD" }
          },
          {
            "@type": "FAQPage",
            "mainEntity": [
              {
                "@type": "Question",
                "name": "O NetAssistant é gratuito?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Sim. O NetAssistant é open source sob a licença Apache-2.0, com todos os recursos gratuitos, sem compras dentro do app, assinaturas ou limites de funcionalidades."
                }
              },
              {
                "@type": "Question",
                "name": "Quais sistemas operacionais são suportados?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Windows 10 versão 1709 ou superior, macOS 10.15 ou superior e distribuições Linux como Ubuntu 22.04 ou superior. Cada plataforma oferece builds x64 e ARM64."
                }
              },
              {
                "@type": "Question",
                "name": "Como instalar no Windows?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Use o winget: execute winget install SunJary.NetAssistant em um terminal e atualize depois com winget upgrade. Você também pode usar o instalador ou um zip portátil."
                }
              },
              {
                "@type": "Question",
                "name": "Existe um build ARM64?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Sim. Windows e Linux oferecem builds aarch64, e o macOS disponibiliza um DMG universal para Intel e Apple Silicon, cobrindo Surface Pro X, Raspberry Pi 5 e Macs da série M."
                }
              },
              {
                "@type": "Question",
                "name": "O macOS diz que não é possível verificar o desenvolvedor. O que devo fazer?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Isso é normal em apps sem assinatura e não notarizados e não significa que o app esteja danificado. Clique com o botão direito no NetAssistant e escolha Abrir, ou acesse Ajustes do Sistema → Privacidade e Segurança e clique em Abrir Mesmo Assim."
                }
              }
            ]
          }
        ]
      }
---

# Download

O NetAssistant está disponível para Windows, Linux e macOS, com builds x64 e ARM64 para cada plataforma. Os links de download abaixo sempre apontam para os arquivos da [versão mais recente](https://github.com/sunjary/netassistant/releases/latest) — clique para baixar diretamente. Versões anteriores estão disponíveis em [GitHub Releases](https://github.com/sunjary/netassistant/releases).

Guias de instalação por plataforma: [Windows](/en/windows-network-debug-tool) · [macOS](/en/mac-network-debug-tool) · [Linux](/en/linux-network-debug-tool).

<DownloadHero />

## Windows

**Recomendado: instalar com winget** (com atualizações automáticas)

```bash
winget install SunJary.NetAssistant
```

Para atualizar:

```bash
winget upgrade SunJary.NetAssistant
```

**Ou use o instalador**: baixe [netassistant-windows-x86_64-setup.exe](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-windows-x86_64-setup.exe) e siga o assistente (cria atalhos no Menu Iniciar e na área de trabalho automaticamente).

**Alternativa**: baixe [netassistant-windows-x86_64.zip](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-windows-x86_64.zip), extraia e execute `netassistant.exe`.

**Dispositivos ARM64** (Surface Pro X, notebooks Snapdragon etc.): o winget e o instalador atualmente distribuem apenas x64. Em ARM64, baixe [netassistant-windows-aarch64.zip](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-windows-aarch64.zip), extraia e execute `netassistant.exe`. Esse build é gerado automaticamente e ainda não foi totalmente testado em hardware real.

## Linux

**Recomendado: pacote deb** (Debian/Ubuntu e derivados, instala as dependências automaticamente)

1. Baixe [netassistant-linux-x86_64.deb](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-x86_64.deb)
2. Instale (as dependências são resolvidas automaticamente):

```bash
sudo apt install ./netassistant-linux-x86_64.deb
```

3. Abra pelo menu de aplicativos ou execute `netassistant` em um terminal

**AppImage** (portátil, para Fedora/Arch e outras distros)

1. Baixe [netassistant-linux-x86_64.AppImage](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-x86_64.AppImage)
2. Adicione permissão de execução e rode:

```bash
chmod +x netassistant-linux-x86_64.AppImage
./netassistant-linux-x86_64.AppImage
```

**Alternativa: tar.gz** (leve)

1. Baixe [netassistant-linux-x86_64.tar.gz](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-x86_64.tar.gz)
2. Extraia e execute:

```bash
tar -xzf netassistant-linux-x86_64.tar.gz
chmod +x netassistant
./netassistant
```

**Dispositivos ARM64** (Raspberry Pi 5, servidores ARM etc.): baixe [netassistant-linux-aarch64.deb](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-aarch64.deb), [netassistant-linux-aarch64.AppImage](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-aarch64.AppImage) ou [netassistant-linux-aarch64.tar.gz](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-aarch64.tar.gz); o uso é o mesmo acima. Esse build é gerado automaticamente e ainda não foi totalmente testado em hardware real.

## macOS

**Recomendado: DMG universal** (funciona em Intel e Apple Silicon)

1. Baixe [netassistant-macos-universal.dmg](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-macos-universal.dmg), abra e arraste o NetAssistant para a pasta Aplicativos
2. Se o macOS avisar "não é possível verificar o desenvolvedor" na primeira execução (normal em apps sem assinatura e não notarizados — não é corrupção): clique com o botão direito no NetAssistant → Abrir; ou acesse Ajustes do Sistema → Privacidade e Segurança e clique em "Abrir Mesmo Assim"

**Alternativa**: baixe [netassistant-macos-universal.zip](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-macos-universal.zip) e extraia para executar diretamente (as permissões de execução são preservadas).

## Requisitos de sistema

| Plataforma | Requisitos |
| ---------- | ---------- |
| Windows | Windows 10 versão 1709 (Build 16299) ou superior, incluindo Windows 11 (x64 / ARM64). Windows Server 2019 e superior são suportados; Windows Server 2016 e anteriores não |
| Linux | Ubuntu 22.04 ou superior (x86_64 / ARM64), GPU compatível com Vulkan |
| macOS | macOS 10.15 ou superior |

## Compilando a partir do código-fonte

Para compilar uma versão personalizada ou obter o snapshot de desenvolvimento mais recente:

```bash
git clone https://github.com/sunjary/netassistant.git
cd netassistant
cargo build --release
```

Após a compilação, o executável fica no diretório `target/release`.

## Solução de erros de dependência

A maioria dos sistemas desktop já inclui as dependências necessárias e funciona imediatamente. No Linux, instale os pacotes abaixo apenas se a inicialização falhar com erro:

- O AppImage falha com erro de FUSE (por exemplo, `AppImages require FUSE to run`):

```bash
sudo apt install libfuse2
```

- O binário puro / tar.gz falha por falta da biblioteca GTK3 (por exemplo, `error while loading shared libraries: libgtk-3.so.0`):

```bash
sudo apt install libgtk-3-0
```

## FAQ

### O NetAssistant é gratuito?

Sim. O NetAssistant é open source sob a licença Apache-2.0, com todos os recursos gratuitos, sem compras dentro do app, assinaturas ou limites de funcionalidades.

### Quais sistemas operacionais são suportados?

Windows 10 versão 1709 ou superior, macOS 10.15 ou superior e distribuições Linux como Ubuntu 22.04 ou superior. Cada plataforma oferece builds x64 e ARM64.

### Como instalar no Windows?

Use o winget: execute `winget install SunJary.NetAssistant` em um terminal e atualize depois com `winget upgrade`. Você também pode usar o instalador ou um zip portátil.

### Existe um build ARM64?

Sim. Windows e Linux oferecem builds aarch64, e o macOS disponibiliza um DMG universal para Intel e Apple Silicon, cobrindo Surface Pro X, Raspberry Pi 5 e Macs da série M.

### O macOS diz que não é possível verificar o desenvolvedor. O que devo fazer?

Isso é normal em apps sem assinatura e não notarizados e não significa que o app esteja danificado. Clique com o botão direito no NetAssistant e escolha **Abrir**, ou acesse **Ajustes do Sistema → Privacidade e Segurança** e clique em **Abrir Mesmo Assim**.

Veja a [FAQ](/pt-BR/faq) para mais perguntas e o [changelog](/en/changelog) para as versões.
