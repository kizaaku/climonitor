# CLAUDE.md

このファイルは、Claude Code (claude.ai/code) がこのリポジトリで作業する際のガイドを提供します。

## 🚀 クイックスタート

```bash
# 1. プロジェクトをビルド
cargo build --release

# 2. ターミナル1: Claude Codeを監視付きで起動
climonitor-launcher claude

# 3. ターミナル2: リアルタイム状態表示
climonitor --live
```

## 📋 よく使うコマンド

### ビルドと実行
```bash
# リリースビルド
cargo build --release

# 開発実行（monitor serverを起動）
cargo run --bin climonitor -- --live

# launcherの実行
cargo run --bin climonitor-launcher -- claude
cargo run --bin climonitor-launcher -- gemini
```

### デバッグ
```bash
# 詳細ログ付きでClaude起動
climonitor-launcher --verbose claude

# 詳細ログ付きでGemini起動
climonitor-launcher --verbose gemini

# monitor server詳細ログ
climonitor --live --verbose
```

### テスト
```bash
# 全テスト実行
cargo test

# 統合テスト実行
cargo test --test integration_state_detection
cargo test --test integration_protocol_basic

# コード品質チェック
cargo fmt
cargo clippy --all-targets --all-features
```

## 🎯 何をするツール？

**climonitor** は Claude Code と Gemini CLI の**実行状態をリアルタイム監視**するツールです：

- **状態表示**: 現在実行中/待機中/エラーなどの状態
- **実行コンテキスト**: 「● マージ完了！」「✦ Got it.」などの実行内容
- **複数セッション**: 複数のCLIツールを同時監視
- **リアルタイム**: 即座に状態変化を検出

## 📊 監視画面の見方

```
🔥 Claude Session Monitor - Live Mode
📊 Session: 2
═══════════════════════════════════════════════════════════════
  📁 climonitor:
    🔵 🤖 実行中 | 30s ago ● マージ完了！mainブランチが3コミット...
    ⏳ ✨ 入力待ち | 2m ago ✦ Allow execution? (y/n)
    
🔄 Last update: 13:30:09 | Press Ctrl+C to exit
```

### 状態アイコンの意味
- 🔗 **接続中**: プロセス実行中
- 🔵 **実行中/アイドル**: 作業中または待機中
- ⏳ **入力待ち**: ユーザーの確認待ち
- 🔴 **エラー**: エラーが発生

### ツールアイコン
- 🤖 **Claude Code**: Claude セッション
- ✨ **Gemini CLI**: Gemini セッション

## 🔧 アーキテクチャ概要

### 2つのバイナリ
1. **`climonitor`**: 監視サーバー（状態表示）
2. **`climonitor-launcher`**: CLIツールラッパー（状態検出）

### データフロー
```
┌─ climonitor-launcher claude ─┐    ┌─ climonitor --live ─┐
│ PTY + Claude Code            │───>│ Monitor Dashboard   │
│ ├─ 画面出力をキャプチャ      │    │ ├─ セッション一覧   │
│ ├─ 状態検出（●, esc to...）  │    │ ├─ 状態表示         │
│ └─ Unix Socket送信           │    │ └─ リアルタイム更新 │
└──────────────────────────────┘    └─────────────────────┘
```

## 🐛 トラブルシューティング

### よくある問題

**1. 「monitor not available」エラー**
```bash
# monitor serverが起動していない
climonitor --live

# 別ターミナルでlauncherを起動
climonitor-launcher claude
```

**2. 状態が更新されない**
```bash
# 詳細ログで原因を確認
climonitor-launcher --verbose claude 2> debug.log
tail -f debug.log
```

**3. Unicode文字化け**
```bash
# 環境変数を設定
export LANG=ja_JP.UTF-8
export LC_ALL=ja_JP.UTF-8
```

### デバッグログの見方
- `📺 [SCREEN]`: 画面バッファの状態
- `📦 [UI_BOX]`: UIボックス検出
- `🎯 [STATE_CHANGE]`: 状態変化
- `🔍 [CLAUDE_STATE]` / `🔍 [GEMINI_STATE]`: ツール固有の状態検出

## 🧪 開発時の注意点

### コードを変更する場合
```bash
# 必ずビルドして動作確認
cargo build --release

# フォーマットとLintを実行
cargo fmt
cargo clippy --all-targets --all-features

# テストを実行
cargo test
```

### 新しい状態検出パターンを追加する場合
1. `launcher/src/screen_claude_detector.rs` （Claude用）
2. `launcher/src/screen_gemini_detector.rs` （Gemini用）
3. テストケースを `launcher/tests/integration_state_detection.rs` に追加

### プロトコル変更の場合
1. `shared/src/protocol.rs` を更新
2. monitor と launcher の両方を更新
3. 互換性テストを実行

## 📚 関連ドキュメント

- **README.md**: プロジェクト概要と詳細仕様
- **docs/code-structure.md**: コード構造とファイル依存関係
- **docs/state-detectors.md**: 状態検出ロジックの詳細

## 🎯 Claude Code向けアドバイス

このプロジェクトで作業する際：

1. **必ずテストを実行**: `cargo test` で動作確認
2. **ログファイルを活用**: `--verbose` オプションで詳細ログを確認
3. **実際に使ってテスト**: climonitorを起動してClaude Codeの動作を確認
4. **Unicode安全**: 日本語やemoji処理では文字境界に注意
5. **状態検出優先**: 新機能よりも既存の状態検出精度を重視

---

**開発目標**: Claude CodeとGemini CLIの実行状況を分かりやすく監視し、開発者の作業効率を向上させること