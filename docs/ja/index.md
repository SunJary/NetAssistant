---
layout: home

title: NetAssistant - TCP/UDP/MQTT ネットワーク診断ツール
titleTemplate: ':title'
description: "Rust 製の高速なネットワーク診断ツール。TCP/UDP クライアント＆サーバー、MQTT 3.1.1/5.0、4 種のデコーダー、ツールボックス、負荷テストに対応。Windows/Linux/macOS 対応。"

hero:
  name: NetAssistant
  text: 高性能クロスプラットフォーム・ネットワーク診断ツール
  tagline: Rust 製 · NetAssist と MQTTX のオープンソース代替 · TCP/UDP/MQTT クライアント＆サーバー · 複数デコーダー · ツールボックス＆TCP/UDP 負荷テスト · GPU アクセラレーション UI
  actions:
    - theme: brand
      text: 今すぐダウンロード
      link: /ja/download
    - theme: alt
      text: ユーザーガイド
      link: /en/guide/
    - theme: alt
      text: GitHub
      link: https://github.com/SunJary/NetAssistant

features:
  - icon: 🔌
    title: マルチプロトコル対応
    details: TCP/UDP のクライアント・サーバーモードと、MQTT 3.1.1 / 5.0 クライアント（TLS、サブスクリプション単位のスイッチ、QoS 0/1/2、遺言メッセージ）に完全対応。IPv4/IPv6 デュアルスタックで、あらゆるネットワーク環境に対応します。
  - icon: 🧰
    title: 内蔵ツールボックス
    details: 変換・チェックサム・ハッシュ・時刻・JSON・エンコード・乱数の 7 グループを内蔵。結果はすぐに表示され、ワンクリックでコピーできるため、外部サイトに切り替える必要はありません。
  - icon: 📦
    title: スマートデコーダー
    details: 生データ、行区切り、長さプレフィックス、JSON の 4 種類の TCP デコーダーで、スティッキーパケットの問題を効果的に解決。HEX モードでの送受信にも対応します。
  - icon: 💬
    title: チャット形式のメッセージログ
    details: パケットのやり取りをチャット形式で直感的に表示。お気に入りとメモ、キーワード検索、リアルタイムログ、TXT/JSON/CSV エクスポートを備えています。
  - icon: 🚀
    title: TCP/UDP 負荷テスト
    details: リアルタイム QPS、レイテンシのパーセンタイル（p50/p95/p99）、失敗の内訳、変数テンプレート、CSV レポート出力を備えた TCP/UDP 負荷テストエンジンを内蔵。MQTT の負荷テストには対応していません。
  - icon: 🤖
    title: 自動テスト
    details: 自動返信で対向側をシミュレートし、定期送信で長時間の安定性テストを実施。UDP ブロードキャストの返信を賢く表示して、デバイス探索を支援します。
  - icon: 🎨
    title: モダンなインターフェース
    details: GPUI フレームワークによる GPU アクセラレーション描画で、なめらかな 60fps を実現。ダークモードはシステム設定に自動追従し、マルチタブで接続を管理できます。
---

## スクリーンショット

### クライアントモード

![クライアントモードのスクリーンショット](../../assets/screenshots/en/screenshot_client.png)

### サーバーモード

![サーバーモードのスクリーンショット](../../assets/screenshots/en/screenshot_server.png)

### MQTT クライアント

![MQTT クライアントのスクリーンショット](../../assets/screenshots/en/screenshot_mqtt_client.png)

### 内蔵ツールボックス

![ツールボックス（ハッシュ）のスクリーンショット](../../assets/screenshots/en/screenshot_toolbox_hash.png)

### 負荷テスト

![負荷テストのスクリーンショット](../../assets/screenshots/en/screenshot_stress.png)

### ダークモード

![UDP サーバーのダークモードのスクリーンショット](../../assets/screenshots/en/screenshot_udp_server_dark.png)

## NetAssistant を選ぶ理由

- **圧倒的なパフォーマンス**: Rust + Tokio の非同期ランタイム。起動は 100ms 未満、メモリ使用量は 20MB 未満、数百万レベルの同時接続に対応
- **デバッグのために設計**: ネットワークアプリ開発からハードウェア・組み込みのデバッグまで、通信検証のワークフロー全体をカバー
- **クロスプラットフォーム**: Windows、Linux、macOS に完全対応。x64 と ARM64 のネイティブビルドを提供
- **オープンソースで無料**: Apache-2.0 ライセンスで公開、コントリビューション歓迎

詳しくは[機能紹介](/en/features)、使い方は[ガイド](/en/guide/)をご覧ください。NetAssist などのツールとの違いは[比較](/en/comparison)で確認できます。

## プラットフォーム別ダウンロード

- [Windows ネットワーク診断ツール](/en/windows-network-debug-tool): winget でコマンド 1 行インストール、Windows 10/11 の x64 と ARM64 に対応
- [Mac ネットワーク診断ツール](/en/mac-network-debug-tool): Apple Silicon と Intel の両方に対応したユニバーサル DMG
- [Linux ネットワーク診断ツール](/en/linux-network-debug-tool): x86_64 と ARM64 向けの deb、AppImage、tar.gz

## 人気のガイド

- [NetAssist 代替: 比較と移行](/en/compare/netassist)
- [TCP デバッグツール: 送信・受信とフレーミング](/ja/tcp-debug-tool)
- [UDP デバッグツール: コネクションレス送信と複数デバイスの返信](/ja/udp-debug-tool)
- [ソケットデバッグツール: ポート待ち受けと到達性](/en/socket-debug-tool)
- [HEX 送信: HEX ペイロードとテキスト変換](/en/hex-send)
- [Modbus TCP デバッグ: フレームと CRC16](/en/modbus-tcp)
- [TCP スティッキーパケット: 原因・4 つの解決策・デバッグ](/en/tcp-sticky-packet)
- [MQTT デバッグガイド: タスク別にページを探す](/en/mqtt)
- [MQTT クライアント比較: MQTTX、Explorer、MQTT.fx](/en/compare/)
- [MQTT クライアント: 軽量な MQTT 3.1.1 / 5.0 デバッグツール](/ja/mqtt-client)
- [MQTTX 代替: 軽量 MQTT クライアントの比較](/en/comparison-mqttx)
- [MQTT チュートリアル: 接続・サブスクライブ・パブリッシュ](/en/guide/mqtt)
- [MQTT トラブルシューティング: 接続とメッセージ欠落](/en/mqtt-troubleshooting)
- [MQTT TLS: 証明書検証と相互認証](/en/mqtt-tls)
- [MQTT 5.0: セッション有効期限と理由コード](/en/mqtt-5)
- [MQTT Explorer 代替の比較](/en/compare/mqtt-explorer)
- [MQTT.fx 代替の比較](/en/compare/mqtt-fx)
- [MQTT FAQ](/en/mqtt-faq)
- [Linux への MQTT クライアントインストール](/en/mqtt-client-linux)
- [TCP/UDP 負荷テスト: QPS と p99 レイテンシ](/en/tcp-stress-test)
- [UDP ブロードキャストによるデバイス探索: LAN 検索とトラブルシューティング](/en/udp-broadcast-device-discovery)
