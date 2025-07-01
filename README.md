# Claude Code Monitor (ccmonitor)

Claude Codeセッションのリアルタイム監視とPTY統合による高精度状態検出ツール

[![CI](https://github.com/username/ccmonitor/workflows/CI/badge.svg)](https://github.com/username/ccmonitor/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## 特徴

- ⚡ **リアルタイム監視**: PTY統合によるClaude Codeの直接監視
- 🎯 **高精度状態検出**: VTEパーサーベースの端末解析とUIボックス検出
- 📊 **クライアント・サーバー構成**: 複数セッションの同時監視
- 🖥️ **ターミナルUI**: セッション状態のリアルタイムダッシュボード
- 🔧 **マルチツール対応**: Claude CodeとGemini CLIに対応
- 🌍 **Unicode対応**: 日本語テキストと絵文字の適切な処理

## デモ

```
┌─ ccmonitor --live ─────────────────────────────────────┐
🔥 Claude Session Monitor - Live Mode
📊 Session: 2
══════════════════════════════════════════════════════════════════════
  📁 folder1:
    🔵 ✨ 完了 | 10s ago

  📁 folder2:
    🔵 🤖 完了 | 1m ago

🔄 Last update: 01:02:05 | Press Ctrl+C to exit
└────────────────────────────────────────────────────────┘
```

## クイックスタート

```bash
# ビルド
cargo build --release

# ターミナル1: Claude Codeを監視付きで起動
ccmonitor-launcher claude

# ターミナル2: リアルタイム状態表示
ccmonitor --live
```

## インストール

### Cargoからインストール

```bash
# ローカルインストール
cargo install --path .

# 実行
ccmonitor --live
ccmonitor-launcher claude
```

### バイナリを直接使用

```bash
# リリースビルド
cargo build --release

# 実行ファイルを直接使用
./target/release/ccmonitor --live
./target/release/ccmonitor-launcher claude
```

## 基本的な使い方

### リアルタイム監視（推奨）

```bash
# 基本的な監視付き起動
ccmonitor-launcher claude

# 詳細なデバッグ出力付き
ccmonitor-launcher --verbose claude

# 任意のClaude引数をサポート
ccmonitor-launcher claude --project myproject
ccmonitor-launcher claude --help
```

### 監視ダッシュボード

```bash
# リアルタイムライブ表示
ccmonitor --live

# 詳細ログ付きライブモード
ccmonitor --live --verbose

# 一回限りのスナップショット
ccmonitor --no-tui
```

### ログファイル機能

```bash
# 出力をファイルに記録
ccmonitor --live --log-file /path/to/output.log
ccmonitor-launcher --log-file /path/to/session.log claude
```

## セッション状態

| 状態 | アイコン | 説明 |
|------|----------|------|
| **接続中** | 🔗 | PTYセッションが実行中 |
| **アイドル** | 🔵 | UIボックスが表示されているが操作なし |
| **実行中** | 🔵 | "ツール"、"自動更新"、"思考中"パターン検出 |
| **入力待ち** | ⏳ | "続行しますか？"、"y/n"などの確認待ち |
| **エラー** | 🔴 | "✗"、"failed"、"Error"パターン検出 |

## アーキテクチャ

### PTY統合モニタリング

```
┌─ ccmonitor-launcher ─┐    ┌─ ccmonitor --live ─┐
│ PTY Integration      │───>│ Monitor Server     │
│ ├─ Claude Code       │    │ ├─ LiveUI          │
│ ├─ VTE Parser        │    │ ├─ SessionManager  │
│ └─ State Detection   │    │ └─ Unix Socket     │
└──────────────────────┘    └────────────────────┘
```

1. **クライアント・サーバー構成**: 中央監視サーバーと複数のランチャークライアント
2. **PTY統合**: 真の端末エミュレーションによるClaude Codeとの透明な相互作用
3. **VTEパーサー**: 完全な画面バッファ解析による正確な状態検出
4. **Unix Domain Socket**: 高速クライアント・サーバー通信
5. **リアルタイム更新**: 画面バッファ解析による即座の状態変化検出

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
ccmonitor-launcher --verbose claude # 詳細な状態検出プロセス表示
ccmonitor-launcher --verbose claude --help  # シンプルなコマンドでテスト
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
ccmonitor-launcher --verbose claude 2> debug.log

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
export CCMONITOR_SOCKET_PATH=/tmp/ccmonitor.sock

# Rustログレベル
export RUST_LOG=debug
```

## API・プログラマティック利用

### Shared ライブラリ

```rust
use ccmonitor_shared::{SessionStatus, LauncherMessage, MonitorMessage};

// セッション状態の確認
let status = SessionStatus::Busy;
println!("Status: {} ({})", status.description(), status.icon());
```

### 独自の状態検出器実装

```rust
use ccmonitor_launcher::{StateDetector, StatePatterns};

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

**ccmonitor**は、Claude Codeセッションの監視とワークフロー最適化のための強力なツールです。リアルタイム状態検出により、開発者の生産性向上をサポートします。
