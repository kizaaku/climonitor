# climonitor

Claude Code と Gemini CLI のリアルタイム監視ツール

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## 概要

climonitor は Claude Code と Gemini CLI の実行状態をリアルタイムで監視し、状態変化を通知するツールです。複数のCLIセッションを同時に監視し、プロジェクトごとに整理して表示します。

### 主な機能
- **リアルタイム監視**: Claude Code / Gemini CLI の実行状態を即座に検出
- **複数セッション対応**: 複数のCLIツールを同時監視
- **プロジェクト別表示**: ディレクトリごとにセッションをグループ化
- **状態変化通知**: カスタマイズ可能な通知システム
- **ロケール対応**: 日本語/英語環境に対応した時刻表示

## クイックスタート

```bash
# 1. ビルド
cargo build --release

# 2. ターミナル1: 監視サーバー起動
./target/release/climonitor --live

# 3. ターミナル2: Claude を監視付きで起動
./target/release/climonitor-launcher claude

# 4. ターミナル3: Gemini を監視付きで起動（オプション）
./target/release/climonitor-launcher gemini
```

## 監視画面

```
🔥 Claude Session Monitor - Live Mode
📊 Launchers: 2
═══════════════════════════════════════════════════════════════
  📁 climonitor:
    🔵 🤖 実行中 | 30秒前 ● コードをレビュー中...
    ⏳ ✨ 入力待ち | 2分前 ✦ Allow execution? (y/n)
    
🔄 Last update: 13:30:09 | Press Ctrl+C to exit
```

### 状態アイコンの説明
- **🔵 実行中/アイドル**: ツールが動作中または待機中
- **⏳ 入力待ち**: ユーザーの入力を待機中  
- **🔴 エラー**: エラーが発生
- **🔗 接続済み**: ランチャーが接続済みだがセッション開始前

### ツールアイコン
- **🤖 Claude Code**: Claude セッション
- **✨ Gemini CLI**: Gemini セッション

## 通知システム

climonitor では状態変化時にカスタムスクリプトを実行できます。

### 設定方法

1. **設定ディレクトリの作成**
```bash
mkdir -p ~/.config/climonitor
```

2. **通知スクリプトの作成**
`~/.config/climonitor/notify.sh` を作成します。

3. **実行権限の設定**
```bash
chmod +x ~/.config/climonitor/notify.sh
```

### スクリプトの引数

通知スクリプトには以下の引数が渡されます：

```bash
notify.sh <event_type> <tool_name> <message> <duration>
```

- `event_type`: イベント種別（`waiting`, `error`, `completed`）
- `tool_name`: ツール名（`claude` または `gemini`）
- `message`: メッセージ内容
- `duration`: 実行時間（例：`30s`）

### 通知スクリプトの例

**macOS（通知センター）:**
```bash
#!/bin/bash
# ~/.config/climonitor/notify.sh

tool_name="$1"
duration="$2"
status="$3"
ui_text="$4"

case "$status" in
    "waiting_for_input")
        osascript -e "display notification \"$ui_text\" with title \"$tool_name が入力待ち\""
        ;;
    "error")
        osascript -e "display notification \"エラーが発生しました\" with title \"$tool_name エラー\""
        ;;
esac
```

**Linux（notify-send）:**
```bash
#!/bin/bash
# ~/.config/climonitor/notify.sh

tool_name="$1"
duration="$2"
status="$3"
ui_text="$4"

case "$status" in
    "waiting_for_input")
        notify-send "$tool_name" "入力待ち: $ui_text"
        ;;
    "error")
        notify-send "$tool_name" "エラーが発生しました"
        ;;
esac
```

**Slack通知（webhook）:**
```bash
#!/bin/bash
# ~/.config/climonitor/notify.sh

tool_name="$1"
duration="$2"
status="$3"
ui_text="$4"

WEBHOOK_URL="https://hooks.slack.com/services/YOUR/WEBHOOK/URL"

case "$status" in
    "waiting_for_input")
        curl -X POST -H 'Content-type: application/json' \
             --data "{\"text\":\"🤖 $tool_name が入力待ち: $ui_text\"}" \
             "$WEBHOOK_URL"
        ;;
esac
```

### 通知のトラブルシューティング

1. **スクリプトが実行されない**
   - 実行権限を確認: `ls -la ~/.config/climonitor/notify.sh`
   - ファイルパスが正しいか確認

2. **通知が表示されない**
   - スクリプト内でログ出力を追加してテスト
   - 手動でスクリプトを実行してテスト:
     ```bash
     ~/.config/climonitor/notify.sh waiting claude "Allow execution? (y/n)" 30
     ```

3. **環境変数が必要な場合**
   ```bash
   #!/bin/bash
   # 環境変数を明示的に設定
   export PATH="/usr/local/bin:$PATH"
   export HOME="/Users/yourusername"
   ```

## コマンドオプション

### climonitor (監視サーバー)
```bash
climonitor [OPTIONS]

OPTIONS:
    --live          ライブ監視モード（デフォルト）
    --verbose       詳細ログ出力
    --help          ヘルプ表示
```

### climonitor-launcher (CLIラッパー)
```bash
climonitor-launcher [OPTIONS] <TOOL>

ARGS:
    <TOOL>          起動するツール (claude | gemini)

OPTIONS:
    --verbose       詳細ログ出力
    --help          ヘルプ表示
```

## 開発・デバッグ

### ビルドとテスト
```bash
# 開発ビルド
cargo build

# テスト実行
cargo test

# フォーマットとLint
cargo fmt
cargo clippy --all-targets --all-features
```

### 詳細ログの確認
```bash
# launcher の詳細ログ
climonitor-launcher --verbose claude 2> launcher.log

# monitor の詳細ログ  
RUST_LOG=debug climonitor --live --verbose 2> monitor.log
```

## 技術仕様

### アーキテクチャ
- **Monitor**: セッション状態を管理し、ライブUIを提供
- **Launcher**: CLIツールをPTYでラップし、状態検出を実行
- **Protocol**: Monitor と Launcher 間の通信プロトコル

### 主な依存関係
- **tokio** - 非同期ランタイム
- **portable-pty** - PTY（疑似端末）統合  
- **vte** - 端末パーサー
- **chrono** - 日時処理（ロケール対応）
- **serde** - JSON解析
- **unicode-width** - Unicode文字幅計算

### 対応プラットフォーム
- macOS
- Linux  
- Windows（WSLのみ動作確認済み）

## ライセンス

[MIT License](LICENSE)