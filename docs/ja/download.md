---
title: Windows・Linux・macOS 版ダウンロード
titleTemplate: ':title'
description: "Windows・Linux・macOS の x64/ARM64 ビルドを用意。winget、deb/AppImage/tar.gz、ソースビルドの手順と動作環境、依存関係の対処法をまとめています。"
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
            "description": "TCP/UDP クライアント＆サーバー、MQTT 3.1.1 / 5.0、フレーミングデコーダー、HEX ペイロード、TCP/UDP 負荷テストを備えたクロスプラットフォームのネットワーク診断ツール。",
            "url": "https://netassistant.trydo.top/ja/download",
            "downloadUrl": "https://github.com/SunJary/NetAssistant/releases",
            "license": "https://www.apache.org/licenses/LICENSE-2.0",
            "offers": { "@type": "Offer", "price": "0", "priceCurrency": "USD" }
          },
          {
            "@type": "FAQPage",
            "mainEntity": [
              {
                "@type": "Question",
                "name": "NetAssistant は無料ですか？",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "はい。NetAssistant は Apache-2.0 ライセンスのオープンソースで、すべての機能を無料で利用できます。アプリ内課金やサブスクリプション、機能制限はありません。"
                }
              },
              {
                "@type": "Question",
                "name": "どのオペレーティングシステムに対応していますか？",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Windows 10 バージョン 1709 以降、macOS 10.15 以降、Ubuntu 22.04 以降などの Linux ディストリビューションに対応しています。各プラットフォームで x64 と ARM64 の両方のビルドを提供しています。"
                }
              },
              {
                "@type": "Question",
                "name": "Windows へのインストール方法は？",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "winget を使います。ターミナルで winget install SunJary.NetAssistant を実行し、後から winget upgrade でアップグレードできます。インストーラーやポータブル zip も利用できます。"
                }
              },
              {
                "@type": "Question",
                "name": "ARM64 版はありますか？",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "はい。Windows と Linux は aarch64 ビルドを提供し、macOS は Intel と Apple Silicon の両方に対応したユニバーサル DMG を提供しています。Surface Pro X、Raspberry Pi 5、M シリーズの Mac で利用できます。"
                }
              },
              {
                "@type": "Question",
                "name": "macOS で「開発元を確認できません」と表示されます。どうすればよいですか？",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "署名・公証されていないアプリでは通常の動作で、アプリが壊れているわけではありません。NetAssistant を右クリックして「開く」を選ぶか、システム設定 → プライバシーとセキュリティで「このまま開く」をクリックしてください。"
                }
              }
            ]
          }
        ]
      }
---

# ダウンロード

NetAssistant は Windows、Linux、macOS で利用でき、各プラットフォームで x64 と ARM64 の両方のビルドを提供しています。以下のダウンロードリンクは常に[最新リリース](https://github.com/sunjary/netassistant/releases/latest)のファイルを指しているため、クリックするだけでダウンロードできます。過去のバージョンは [GitHub Releases](https://github.com/sunjary/netassistant/releases) で確認できます。

プラットフォーム別のインストールガイド: [Windows](/en/windows-network-debug-tool) · [macOS](/en/mac-network-debug-tool) · [Linux](/en/linux-network-debug-tool)

<DownloadHero />

## Windows

**推奨: winget でインストール**（自動アップグレード対応）

```bash
winget install SunJary.NetAssistant
```

アップグレードするには:

```bash
winget upgrade SunJary.NetAssistant
```

**またはインストーラーを使用**: [netassistant-windows-x86_64-setup.exe](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-windows-x86_64-setup.exe) をダウンロードし、ウィザードに従ってください（スタートメニューとデスクトップのショートカットが自動で作成されます）。

**代替手段**: [netassistant-windows-x86_64.zip](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-windows-x86_64.zip) をダウンロードして展開し、`netassistant.exe` を実行します。

**ARM64 デバイス**（Surface Pro X、Snapdragon 搭載ノート PC など）: 現在、winget とインストーラーは x64 のみを提供しています。ARM64 では [netassistant-windows-aarch64.zip](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-windows-aarch64.zip) をダウンロードして展開し、`netassistant.exe` を実行してください。このビルドは自動生成されたもので、実機での十分なテストは行われていません。

## Linux

**推奨: deb パッケージ**（Debian/Ubuntu 系、依存関係を自動インストール）

1. [netassistant-linux-x86_64.deb](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-x86_64.deb) をダウンロード
2. インストール（依存関係は自動で解決されます）:

```bash
sudo apt install ./netassistant-linux-x86_64.deb
```

3. アプリケーションメニューから起動するか、ターミナルで `netassistant` を実行します

**AppImage**（ポータブル、Fedora/Arch などのディストリビューション向け）

1. [netassistant-linux-x86_64.AppImage](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-x86_64.AppImage) をダウンロード
2. 実行権限を付けて起動:

```bash
chmod +x netassistant-linux-x86_64.AppImage
./netassistant-linux-x86_64.AppImage
```

**代替手段: tar.gz**（軽量）

1. [netassistant-linux-x86_64.tar.gz](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-x86_64.tar.gz) をダウンロード
2. 展開して実行:

```bash
tar -xzf netassistant-linux-x86_64.tar.gz
chmod +x netassistant
./netassistant
```

**ARM64 デバイス**（Raspberry Pi 5、ARM サーバーなど）: [netassistant-linux-aarch64.deb](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-aarch64.deb)、[netassistant-linux-aarch64.AppImage](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-aarch64.AppImage)、[netassistant-linux-aarch64.tar.gz](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-aarch64.tar.gz) をダウンロードしてください。使い方は上記と同じです。このビルドは自動生成されたもので、実機での十分なテストは行われていません。

## macOS

**推奨: ユニバーサル DMG**（Intel と Apple Silicon の両方で動作）

1. [netassistant-macos-universal.dmg](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-macos-universal.dmg) をダウンロードして開き、NetAssistant を Applications フォルダにドラッグします
2. 初回起動時に macOS が「開発元を確認できません」と警告した場合（署名・公証されていないアプリでは通常の動作で、破損ではありません）: NetAssistant を右クリック →「開く」を選択。またはシステム設定 → プライバシーとセキュリティを開き、「このまま開く」をクリックします

**代替手段**: [netassistant-macos-universal.zip](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-macos-universal.zip) をダウンロードして展開すれば、そのまま実行できます（実行権限は保持されます）。

## 動作環境

| プラットフォーム | 要件 |
| -------- | ------------ |
| Windows | Windows 10 バージョン 1709（Build 16299）以降、Windows 11 を含む（x64 / ARM64）。Windows Server 2019 以降に対応。Windows Server 2016 以前は非対応 |
| Linux | Ubuntu 22.04 以降（x86_64 / ARM64）、Vulkan 対応 GPU |
| macOS | macOS 10.15 以降 |

## ソースからのビルド

カスタム版をビルドしたい場合や、最新の開発スナップショットを入手したい場合:

```bash
git clone https://github.com/sunjary/netassistant.git
cd netassistant
cargo build --release
```

ビルド後、実行ファイルは `target/release` ディレクトリに生成されます。

## 依存関係エラーのトラブルシューティング

ほとんどのデスクトップ環境には必要な依存関係が最初から入っており、そのまま動作します。Linux で起動時にエラーが出た場合にのみ、以下をインストールしてください:

- AppImage が FUSE エラーで失敗する場合（例: `AppImages require FUSE to run`）:

```bash
sudo apt install libfuse2
```

- バイナリ / tar.gz が GTK3 ライブラリ不足で失敗する場合（例: `error while loading shared libraries: libgtk-3.so.0`）:

```bash
sudo apt install libgtk-3-0
```

## FAQ

### NetAssistant は無料ですか？

はい。NetAssistant は Apache-2.0 ライセンスのオープンソースで、すべての機能を無料で利用できます。アプリ内課金やサブスクリプション、機能制限はありません。

### どのオペレーティングシステムに対応していますか？

Windows 10 バージョン 1709 以降、macOS 10.15 以降、Ubuntu 22.04 以降などの Linux ディストリビューションに対応しています。各プラットフォームで x64 と ARM64 の両方のビルドを提供しています。

### Windows へのインストール方法は？

winget を使います。ターミナルで winget install SunJary.NetAssistant を実行し、後から winget upgrade でアップグレードできます。インストーラーやポータブル zip も利用できます。

### ARM64 版はありますか？

はい。Windows と Linux は aarch64 ビルドを提供し、macOS は Intel と Apple Silicon の両方に対応したユニバーサル DMG を提供しています。Surface Pro X、Raspberry Pi 5、M シリーズの Mac で利用できます。

### macOS で「開発元を確認できません」と表示されます。どうすればよいですか？

署名・公証されていないアプリでは通常の動作で、アプリが壊れているわけではありません。NetAssistant を右クリックして「開く」を選ぶか、システム設定 → プライバシーとセキュリティで「このまま開く」をクリックしてください。

その他の質問は [FAQ](/ja/faq)、リリース情報は[変更履歴](/en/changelog)をご覧ください。
