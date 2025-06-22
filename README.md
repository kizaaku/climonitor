# Claude Session Monitor

Claude セッションファイルを監視し、リアルタイムでセッション状態を表示する軽量CLIツール。

## 特徴

- 🚀 **超高速**: Rust製で起動時間 ~1ms、メモリ使用量 ~2MB
- 📊 **リアルタイム監視**: Claude セッションファイルの変更を即座に検知
- 🎯 **状態分類**: 作業中/入力待ち/エラー/アイドルを自動判定
- 📁 **プロジェクト別表示**: 作業ディレクトリごとにグループ化
- ⌨️ **インタラクティブ**: キーボード操作対応
- 🖥️ **2つのモード**: TUI（リアルタイム）+ 非対話（スナップショット）

## セッション状態

- 🟢 **作業中**: Claude がツールを実行中、またはツール結果待ち
- 🟡 **入力待ち**: Claude が応答完了、ユーザー入力待ち状態  
- 🔴 **エラー**: ツール実行エラー、またはユーザーによる操作中断
- ⚪ **アイドル**: 5分以上更新がないセッション

## インストール

```bash
# ビルド
cargo build --release

# インストール（オプション）
cargo install --path .
```

## 使用方法

### TUIモード（推奨）
```bash
# リアルタイム監視ダッシュボード
./target/release/ccmonitor

# 特定プロジェクトのみ監視
./target/release/ccmonitor --project network-management
```

### 非対話モード
```bash
# 現在の状態を表示して終了
./target/release/ccmonitor --no-tui

# 詳細出力
./target/release/ccmonitor --no-tui --verbose

# 特定プロジェクトのみ
./target/release/ccmonitor --no-tui --project ccmonitor
```

## キーボード操作（TUIモード）

- `q` / `Esc`: 終了
- `r`: 手動更新

## 注意事項

- **TUIモード**: 通常のターミナルで実行してください
- **非対話モード**: どの環境でも動作します（Claude Code含む）
- Claude Code環境では `--no-tui` オプションを使用してください

## 動作原理

Claude は `~/.claude/projects/` 配下にプロジェクト別のJSONLファイルでセッションログを保存します。このツールは：

1. ファイルシステムウォッチャーでJSONLファイルの変更を監視
2. 最新メッセージを解析してセッション状態を判定
3. TUIで美しいダッシュボードを表示

## 依存関係

- `ratatui`: ターミナルUI
- `crossterm`: クロスプラットフォーム端末制御
- `tokio`: 非同期ランタイム
- `notify`: ファイルシステム監視
- `serde`: JSON解析