# climonitor

Claude Code と Gemini CLI のリアルタイム監視ツール

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## 使い方

```bash
# ビルド
cargo build --release

# ターミナル1: 監視サーバー起動
./target/release/climonitor --live

# ターミナル2: Claude を監視付きで起動
./target/release/climonitor-launcher claude

# ターミナル3: Gemini を監視付きで起動  
./target/release/climonitor-launcher gemini
```

## 監視画面

```
🔥 Claude Session Monitor - Live Mode
📊 Session: 2
═══════════════════════════════════════════════════════════════
  📁 project:
    🔵 🤖 実行中 | 30s ago ● コードをレビュー中...
    ⏳ ✨ 入力待ち | 2m ago ✦ Allow execution? (y/n)
    
🔄 Last update: 13:30:09 | Press Ctrl+C to exit
```

## 主な依存関係

- **tokio** - 非同期ランタイム
- **portable-pty** - PTY（疑似端末）統合
- **vte** - 端末パーサー  
- **serde** - JSON解析
- **unicode-width** - Unicode文字幅計算

## ライセンス

[MIT License](LICENSE)