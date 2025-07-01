# CLI Tool Monitor (climonitor)

Claude CodeとGemini CLIセッションのリアルタイム監視とPTY統合による高精度状態検出ツール

[![CI](https://github.com/kizaaku/climonitor/workflows/CI/badge.svg)](https://github.com/kizaaku/climonitor/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## 特徴

- ⚡ **リアルタイム監視**: PTY統合によるCLIツールの直接監視
- 🎯 **高精度状態検出**: VTEパーサーベースの端末解析とUIボックス検出
- 📊 **クライアント・サーバー構成**: 複数セッションの同時監視
- 🖥️ **ターミナルUI**: セッション状態のリアルタイムダッシュボード
- 🔧 **マルチツール対応**: Claude CodeとGemini CLIに対応
- 🤖 **専用状態検出**: ツール別に最適化された独立した状態検出器
- 🌍 **Unicode対応**: 日本語テキストと絵文字の適切な処理

## デモ

```
┌─ climonitor --live ────────────────────────────────────┐
🔥 CLI Tool Monitor - Live Mode
📊 Session: 2
══════════════════════════════════════════════════════════════════════
  📁 folder1:
    🔵 ✨ 完了 | 10s ago   [Gemini]

  📁 folder2:
    🔵 🤖 完了 | 1m ago    [Claude]

🔄 Last update: 01:02:05 | Press Ctrl+C to exit
└────────────────────────────────────────────────────────┘
```

## クイックスタート

```bash
# ビルド
cargo build --release

# ターミナル1: Claude Codeを監視付きで起動
climonitor-launcher claude

# ターミナル2: リアルタイム状態表示
climonitor --live
```

## インストール

### Cargoからインストール

```bash
# ローカルインストール
cargo install --path .

# 実行
climonitor --live
climonitor-launcher claude
```

### バイナリを直接使用

```bash
# リリースビルド
cargo build --release

# 実行ファイルを直接使用
./target/release/climonitor --live
./target/release/climonitor-launcher claude
```

## 基本的な使い方

### リアルタイム監視（推奨）

```bash
# 基本的な監視付き起動
climonitor-launcher claude
climonitor-launcher gemini

# 詳細なデバッグ出力付き
climonitor-launcher --verbose claude
climonitor-launcher --verbose gemini

# 任意のツール引数をサポート
climonitor-launcher claude --project myproject
climonitor-launcher gemini --project myproject
climonitor-launcher claude --help
```

### 監視ダッシュボード

```bash
# リアルタイムライブ表示
climonitor --live

# 詳細ログ付きライブモード
climonitor --live --verbose

# 一回限りのスナップショット
climonitor --no-tui
```

### ログファイル機能

```bash
# 出力をファイルに記録
climonitor --live --log-file /path/to/output.log
climonitor-launcher --log-file /path/to/session.log claude
climonitor-launcher --log-file /path/to/session.log gemini
```

## セッション状態

| 状態 | アイコン | 説明 |
|------|----------|------|
| **接続中** | 🔗 | PTYセッションが実行中 |
| **アイドル** | 🔵 | UIボックスが表示されているが操作なし、`>`プロンプト表示（Gemini） |
| **実行中** | 🔵 | "esc to interrupt"（Claude）、"(esc to cancel"（Gemini）パターン検出 |
| **入力待ち** | ⏳ | "続行しますか？"、"Allow execution?"、"y/n"などの確認待ち |
| **エラー** | 🔴 | "✗"、"failed"、"Error"パターン検出 |

## アーキテクチャ

### PTY統合モニタリング

```
┌─ climonitor-launcher ─┐    ┌─ climonitor --live ─┐
│ PTY Integration       │───>│ Monitor Server      │
│ ├─ Claude Code        │    │ ├─ LiveUI           │
│ ├─ Gemini CLI         │    │ ├─ SessionManager   │
│ ├─ VTE Parser         │    │ └─ Unix Socket      │
│ └─ State Detection    │    │                     │
│   ├─ ClaudeDetector   │    │                     │
│   └─ GeminiDetector   │    │                     │
└───────────────────────┘    └─────────────────────┘
```

1. **クライアント・サーバー構成**: 中央監視サーバーと複数のランチャークライアント
2. **PTY統合**: 真の端末エミュレーションによるCLIツールとの透明な相互作用
3. **独立型状態検出器**: ツール別に最適化された専用検出ロジック
4. **VTEパーサー**: 完全な画面バッファ解析による正確な状態検出
5. **Unix Domain Socket**: 高速クライアント・サーバー通信
6. **リアルタイム更新**: 画面バッファ解析による即座の状態変化検出

### 独立型状態検出器アーキテクチャ

各CLIツール用に最適化された専用の状態検出器を実装：

#### Claude状態検出器 (`ScreenClaudeStateDetector`)
- **主要パターン**: `"esc to interrupt"` による実行状態の高精度検出
- **完了検出**: `"esc to interrupt"` の出現・消失による状態遷移
- **承認プロンプト**: `"Do you want"`, `"May I"`, `"proceed?"` パターン
- **実行コンテキスト**: `"⏺ 実行中"` 表示による詳細情報

#### Gemini状態検出器 (`ScreenGeminiStateDetector`)
- **主要パターン**: `"(esc to cancel"` による処理中状態検出
- **アイドル検出**: `">"` で始まるプロンプト表示
- **承認プロンプト**: `"Allow execution?"`, `"waiting for user confirmation"`
- **統計表示**: セッション終了後の `"Cumulative Stats"` 検出

#### 共通機能
- **完全独立**: 各検出器が`ScreenBuffer`を直接管理
- **ScreenBuffer統合**: VTEパーサーによる画面状態の完全解析
- **UI Box解析**: ╭╮╰╯ Unicode罫線要素の自動検出と内容抽出
- **エラー処理**: 統一されたエラーパターン検出

### VTEパーサーによる画面バッファ状態検出

- **画面バッファ管理**: 80x24端末グリッドの完全なVTEパーサーサポート
- **UIボックス検出**: ╭╮╰╯ Unicode罫線描画要素の自動検出
- **コンテキスト解析**: UIボックス上部の行から実行コンテキストを抽出
- **マルチツール対応**: Claude (🤖) とGemini (✨) CLIツールの識別
- **リアルタイム更新**: 画面バッファ解析による即座の状態変化

### PTY+1列バッファアーキテクチャ

UI箱重複問題の解決のため、以下の技術的解決策を実装：

- **内部バッファ**: PTY列数+1（例：70列PTYに対し71列バッファ）
- **外部表示**: 元のPTYサイズ（例：70列）
- **UIボックス検出**: PTY表示範囲に限定

これにより、ink.jsライブラリの期待する動作とVTEパーサーの処理を整合させ、UIボックスの重複描画問題を根本的に解決。

## 開発とテスト

### ビルドコマンド

```bash
# プロジェクトのビルド
cargo build --release

# 開発実行
cargo run                           # ライブモードで実行
cargo run -- --no-tui              # 非対話スナップショットモード
cargo run -- --verbose             # デバッグ用詳細出力

# 状態検出のデバッグ（人的テスト）
climonitor-launcher --verbose claude # 詳細な状態検出プロセス表示
climonitor-launcher --verbose gemini # Gemini状態検出のテスト
climonitor-launcher --verbose claude --help  # シンプルなコマンドでテスト
```

### テストの実行

```bash
# 全テストの実行
cargo test

# 詳細出力付きテスト
cargo test --verbose

# 特定のテストを実行
cargo test claude_state_detector
```

### コード品質チェック

```bash
# フォーマット
cargo fmt

# 静的解析
cargo clippy

# フォーマットチェック
cargo fmt --check

# Clippyエラーを警告として扱う
cargo clippy -- -D warnings
```

### デバッグとトラブルシューティング

```bash
# デバッグログをファイルに保存
climonitor-launcher --verbose claude 2> debug.log
climonitor-launcher --verbose gemini 2> debug.log

# ログ内容を確認
tail -f debug.log

# 特定パターンを検索
grep "UI_BOX" debug.log
grep "STATE_CHANGE" debug.log
grep "SCREEN" debug.log
```

**ログの見方:**

- `📺 [SCREEN]`: 現在の画面バッファ状態
- `📦 [UI_BOX]`: UIボックス検出と内容抽出
- `🔍 [STATE]`: 状態検出解析
- `🎯 [STATE_CHANGE]`: 実際の状態遷移
- `📊 [CONTEXT]`: 実行コンテキスト抽出

## 環境設定

```bash
# Claude Codeで詳細ログを有効化（分析のため推奨）
export ANTHROPIC_LOG=debug

# カスタムソケットパス（オプション）
export CLIMONITOR_SOCKET_PATH=/tmp/climonitor.sock

# Rustログレベル
export RUST_LOG=debug
```

## API・プログラマティック利用

### Shared ライブラリ

```rust
use climonitor_shared::{SessionStatus, LauncherMessage, MonitorMessage};

// セッション状態の確認
let status = SessionStatus::Busy;
println!("Status: {} ({})", status.description(), status.icon());
```

### 独自の状態検出器実装

```rust
use climonitor_launcher::{StateDetector, ScreenClaudeStateDetector, ScreenGeminiStateDetector};

struct CustomStateDetector {
    patterns: StatePatterns,
}

impl StateDetector for CustomStateDetector {
    fn process_output(&mut self, output: &str) -> Option<SessionState> {
        // カスタム状態検出ロジック
    }
}
```

## CI/CD

GitHub Actionsを使用した継続的統合：

- **テスト**: 全テストケースの実行
- **フォーマット**: `cargo fmt --check`
- **Clippy**: `cargo clippy -- -D warnings`
- **ビルド**: Ubuntu、Windows、macOSでのクロスプラットフォームビルド

## 依存関係

| クレート | 用途 |
|----------|------|
| `tokio` | 非同期ランタイム |
| `ratatui` | ターミナルUI |
| `crossterm` | クロスプラットフォーム端末制御 |
| `portable-pty` | PTY（疑似端末）統合 |
| `vte` | VTE（Virtual Terminal Emulator）パーサー |
| `serde` | JSON解析 |
| `regex` | パターンマッチング |
| `unicode-width` | Unicode文字幅計算 |
| `unicode-segmentation` | Unicodeテキスト分割 |

## ライセンス

[MIT License](LICENSE)

---

**climonitor**は、Claude CodeとGemini CLIセッションの監視とワークフロー最適化のための強力なツールです。独立型状態検出器とリアルタイム監視により、開発者の生産性向上をサポートします。
