---
title: FAQ - macOS でも使えるネットワーク診断ツール
description: "NetAssistant の FAQ。macOS・Linux 対応、NetAssist との違い、winget インストール、Linux パッケージ、TCP スティッキーパケット、負荷テストについて回答します。"
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
            "name": "定番の NetAssist ネットワーク診断ツールは macOS に対応していますか？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "名前が似た 2 つのツールを区別してください。本サイトで紹介しているクロスプラットフォームの NetAssistant は macOS・Windows・Linux に対応し、x64 と ARM64 のネイティブビルドを提供します。一方、名前が似た定番の NetAssist は Windows 版のみです。macOS や Linux で TCP/UDP をデバッグする場合は、NetAssistant をそのままお使いください。"
            }
          },
          {
            "@type": "Question",
            "name": "NetAssistant と NetAssist の違いは何ですか？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "両者は別のソフトウェアです。NetAssist はクローズドソースの Windows 専用ツール、NetAssistant は Rust で開発されたオープンソース（Apache-2.0）のクロスプラットフォームツールで、Windows/Linux/macOS の x64 と ARM64 に対応しています。スティッキーパケット対策の 4 種の TCP デコーダーや、高並列の負荷テスト（QPS と p50/p95/p99 レイテンシ）も内蔵しています。"
            }
          },
          {
            "@type": "Question",
            "name": "winget で NetAssistant をインストールするには？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "Windows では winget install SunJary.NetAssistant を実行します。アップグレードは winget upgrade SunJary.NetAssistant で行えます。"
            }
          },
          {
            "@type": "Question",
            "name": "Linux 用の deb パッケージはありますか？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "はい。Linux 向けに deb、AppImage、tar.gz パッケージを用意しています（x86_64 と aarch64）。Debian/Ubuntu 系では deb パッケージがおすすめで、sudo apt install で依存関係が自動解決されます。AppImage は Fedora/Arch などのディストリビューションに適しており（初回実行時に libfuse2 が必要）、tar.gz は GTK3 のインストールが必要です。"
            }
          },
          {
            "@type": "Question",
            "name": "NetAssistant は無料のオープンソースですか？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "はい。NetAssistant は Apache-2.0 ライセンスで公開されており、すべての機能を無料で利用できます。ソースコードは GitHub で公開しています。"
            }
          },
          {
            "@type": "Question",
            "name": "TCP のスティッキーパケットにはどう対処しますか？",
            "acceptedAnswer": {
              "@type": "Answer",
              "text": "NetAssistant には 4 種の TCP デコーダー（生データ、行区切り、長さプレフィックス、JSON）があり、プロトコル形式に応じてバイトストリームを自動で分割します。詳しくは TCP/UDP デバッグガイドをご覧ください。"
            }
          }
        ]
      }
---

# FAQ

## 定番の NetAssist ネットワーク診断ツールは macOS に対応していますか？

まず、名前が似た 2 つの製品を区別してください。末尾の「ant」だけが異なる別製品です:

- **NetAssistant**（本サイトのツール）: オープンソースでクロスプラットフォーム。**Windows・Linux・macOS に対応**し、x64/ARM64 のネイティブビルドがあるため Mac でもそのまま動作します → [ダウンロード](/ja/download)
- **NetAssist**（定番の Windows 専用ツール）: macOS 版・Linux 版はありません

Mac や Linux で TCP/UDP をデバッグする場合は、使い慣れたデバッグツールと同じ感覚で使える **NetAssistant** をそのままお使いください。詳しくは[はじめに](/en/guide/)と[比較表](/en/comparison)をご覧ください。

## NetAssistant と NetAssist の違いは何ですか？

両者は名前が似ているものの、**別のソフトウェア**です:

| | NetAssistant | NetAssist |
| ---- | ---- | ---- |
| プラットフォーム | Windows / Linux / macOS（x64 + ARM64） | Windows のみ |
| オープンソース | ✅ Apache-2.0 | ❌ クローズドソース |
| TCP デコーダー（スティッキーパケット） | 生データ / 行 / 長さプレフィックス / JSON | 限定的 |
| TCP/UDP 高並列負荷テスト | ✅ QPS + p50/p95/p99 | ❌ |

詳しくは[比較表](/en/comparison)をご覧ください。

## winget で NetAssistant をインストールするには？

Windows では、自動アップグレードにも対応する winget がおすすめです:

```bash
winget install SunJary.NetAssistant
```

アップグレードするには:

```bash
winget upgrade SunJary.NetAssistant
```

その他のインストール方法は[ダウンロードページ](/ja/download)をご覧ください。

## Linux 用の deb パッケージはありますか？

はい。**deb**、**AppImage**、**tar.gz** パッケージを用意しています（x86_64 と aarch64 の両方）:

- **deb**（推奨）: Debian/Ubuntu 系向け。`sudo apt install ./netassistant-linux-x86_64.deb` で依存関係が自動解決され、Ubuntu 22.04 以降で動作します
- **AppImage**: ポータブルですぐに動作。Fedora/Arch などのディストリビューションに最適で、初回実行時に `sudo apt install libfuse2` が必要です
- **tar.gz**: 展開して実行。GTK3（`sudo apt install libgtk-3-0`）が必要です

ダウンロードリンクは[ダウンロードページ](/ja/download#linux)にあります。

## NetAssistant は無料のオープンソースですか？

はい。NetAssistant は **Apache-2.0** ライセンスで公開されており、すべての機能を無料で利用できます。ソースコードは [GitHub](https://github.com/SunJary/NetAssistant) で公開しており、コントリビューションを歓迎します。

## TCP のスティッキーパケットにはどう対処しますか？

NetAssistant にはバイトストリームを自動で分割する 4 種の TCP デコーダーがあるため、手作業でバイト数を数える必要はありません:

1. **生データ**: 受信した順に表示
2. **行区切り**: 改行で分割
3. **長さプレフィックス**: ヘッダーの長さフィールドで分割
4. **JSON**: 完全な JSON オブジェクト単位で分割

詳しくは [TCP/UDP デバッグガイド](/en/guide/tcp-udp)をご覧ください。
