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
- **設定ファイル対応**: TOML形式の設定ファイルで詳細設定
- **多様な通信方式**: Unix Socket、gRPC対応
- **セキュリティ対応**: IP制限によるアクセス制御
- **ロケール対応**: 日本語/英語環境に対応した時刻表示
- **クロスプラットフォーム**: Linux、Windows、macOS対応

## クイックスタート

### 基本セットアップ（Unix Socket）

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

### gRPC接続セットアップ（リモート監視）

```bash
# 監視サーバー（リモートマシン）
./target/release/climonitor --grpc --bind 0.0.0.0:50051 --live

# 作業マシン（Claudeを実行）
export CLIMONITOR_GRPC_ADDR="192.168.1.100:50051"
./target/release/climonitor-launcher claude
```

### Windows セットアップ

#### ローカルビルド（Windows）
```powershell
# 1. Rustがインストール済みであることを確認
rustc --version

# 2. ビルド
cargo build --release

# 3. PowerShell 1: 監視サーバー起動
.\target\release\climonitor.exe --live

# 4. PowerShell 2: Claude を監視付きで起動
.\target\release\climonitor-launcher.exe claude
```

#### クロスコンパイル（Linux → Windows）
```bash
# 1. Windows向けターゲットを追加
rustup target add x86_64-pc-windows-gnu

# 2. mingw-w64をインストール
sudo apt install mingw-w64

# 3. Windows向けビルド
cargo build --release --target x86_64-pc-windows-gnu

# 4. 生成されたWindows実行ファイル
ls target/x86_64-pc-windows-gnu/release/*.exe
```

#### Windows固有の注意点
- **PTY対応**: Windows ConPTY（Console Pseudoterminal）を使用
- **信号処理**: Ctrl+C、Ctrl+Zなどの信号をCLIツールに適切に転送
- **パス処理**: Windows特有のパス区切り文字やnull terminator問題に対応
- **.cmdファイル**: Claude Code、Gemini CLIの.cmdファイルも適切に実行

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

## 設定ファイル

climonitor は TOML 形式の設定ファイルで詳細な設定が可能です。

### 設定ファイルの場所（優先度順）
1. `./climonitor/config.toml` （カレントディレクトリ）
2. `~/.climonitor/config.toml` （ホームディレクトリ）
3. `~/.config/climonitor/config.toml` （XDG設定ディレクトリ）

### 基本設定例

```toml
[connection]
# 接続タイプ: "unix" または "grpc"
type = "grpc"

# gRPC接続時のバインドアドレス（デフォルト: 127.0.0.1:50051）
grpc_bind_addr = "127.0.0.1:50051"

# gRPC接続時のIP許可リスト（セキュリティ機能）
grpc_allowed_ips = ["127.0.0.1", "192.168.1.0/24", "localhost"]

# Unix socket接続時のソケットパス
# unix_socket_path = "/tmp/climonitor.sock"

[logging]
# 詳細ログを有効にするか
verbose = true

# ログファイルパス（CLIツールの出力を保存）
log_file = "~/.climonitor/climonitor.log"
```

### 設定の優先順位
1. **CLI引数** （最優先）
2. **環境変数**
3. **設定ファイル**
4. **デフォルト値** （最低優先）

### 主な環境変数
- `CLIMONITOR_GRPC_ADDR`: gRPC接続アドレス（デフォルト: 127.0.0.1:50051）
- `CLIMONITOR_SOCKET_PATH`: Unix socketパス
- `CLIMONITOR_VERBOSE`: 詳細ログ有効化
- `CLIMONITOR_LOG_FILE`: ログファイルパス

## マルチマシン構成

### リモート監視の設定

**サーバーマシン（Monitor）:**
```bash
# gRPC接続でIP制限付きサーバー起動
climonitor --grpc --bind 0.0.0.0:50051 --live
```

**クライアントマシン（Launcher）:**
```bash
# gRPC接続でClaude起動
export CLIMONITOR_GRPC_ADDR="192.168.1.100:50051"
climonitor-launcher claude
```

### セキュリティ考慮事項

- **IP許可リスト**: gRPC接続時は必ずIP制限を設定
- **ローカル優先**: 可能な限りUnix socketを使用
- **ファイアウォール**: 適切なポート制限を設定
- **接続ログ**: `--verbose` でアクセス状況を監視

## 通知システム

climonitor では状態変化時にカスタムスクリプトを実行できます。

### 設定方法

**Unix系（Linux/macOS）:**
1. **設定ディレクトリの作成**
```bash
mkdir -p ~/.climonitor
```

2. **通知スクリプトの作成**
`~/.climonitor/notify.sh` を作成します。

3. **実行権限の設定**
```bash
chmod +x ~/.climonitor/notify.sh
```

**Windows:**
1. **設定ディレクトリの作成**
```powershell
New-Item -ItemType Directory -Path "$env:USERPROFILE\.climonitor" -Force
```

2. **通知スクリプトの作成**
`$env:USERPROFILE\.climonitor\notify.ps1` を作成します。

3. **PowerShell実行ポリシーの確認**
```powershell
# 現在のポリシーを確認
Get-ExecutionPolicy -Scope CurrentUser

# 必要に応じて設定（RemoteSignedまたはUnrestricted）
Set-ExecutionPolicy RemoteSigned -Scope CurrentUser
```

### スクリプトの引数

通知スクリプトには以下の引数が渡されます：

**Unix系:**
```bash
notify.sh <event_type> <tool_name> <message> <duration>
```

**Windows:**
```powershell
notify.ps1 -EventType <event_type> -ToolName <tool_name> -Message <message> -Duration <duration>
```

- `event_type`: イベント種別（`waiting_for_input`, `error`, `completed`, `status_change`）
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

**Windows（Toast通知）:**
```powershell
# PowerShell Notification Script for climonitor
# ~/.climonitor/notify.ps1

param(
    [string]$EventType,
    [string]$ToolName, 
    [string]$Message,
    [string]$Duration
)

# ログファイルに記録
$LogFile = "$env:USERPROFILE\.climonitor\notifications.log"
$LogDir = Split-Path $LogFile -Parent
if (-not (Test-Path $LogDir)) {
    New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
}

$LogEntry = "$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') - event_type=$EventType, tool=$ToolName, message=$Message, duration=$Duration"
Add-Content -Path $LogFile -Value $LogEntry

# Windows Toast通知関数
function Show-ToastNotification {
    param([string]$Title, [string]$Body)
    
    try {
        [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null
        $Template = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02)
        $RawXml = [xml] $Template.GetXml()
        ($RawXml.toast.visual.binding.text | Where-Object {$_.id -eq "1"}).AppendChild($RawXml.CreateTextNode($Title)) | Out-Null
        ($RawXml.toast.visual.binding.text | Where-Object {$_.id -eq "2"}).AppendChild($RawXml.CreateTextNode($Body)) | Out-Null
        
        $SerializedXml = New-Object Windows.Data.Xml.Dom.XmlDocument
        $SerializedXml.LoadXml($RawXml.OuterXml)
        $Toast = [Windows.UI.Notifications.ToastNotification]::new($SerializedXml)
        $Notifier = [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier("climonitor")
        $Notifier.Show($Toast)
    }
    catch {
        Write-Host "Toast notification failed: $Title - $Body"
    }
}

# イベントタイプに応じた通知
switch ($EventType) {
    "waiting_for_input" { Show-ToastNotification -Title "$ToolName が入力待ち" -Body $Message }
    "error" { Show-ToastNotification -Title "$ToolName エラー" -Body "エラーが発生しました: $Message" }
    "completed" { Show-ToastNotification -Title "$ToolName 完了" -Body $Message }
    default { Show-ToastNotification -Title "climonitor" -Body "$ToolName: $Message" }
}
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

**Unix系（Linux/macOS）:**

1. **スクリプトが実行されない**
   - 実行権限を確認: `ls -la ~/.climonitor/notify.sh`
   - ファイルパスが正しいか確認

2. **通知が表示されない**
   - スクリプト内でログ出力を追加してテスト
   - 手動でスクリプトを実行してテスト:
     ```bash
     ~/.climonitor/notify.sh waiting_for_input claude "Allow execution? (y/n)" 30s
     ```

3. **環境変数が必要な場合**
   ```bash
   #!/bin/bash
   # 環境変数を明示的に設定
   export PATH="/usr/local/bin:$PATH"
   export HOME="/Users/yourusername"
   ```

**Windows:**

1. **PowerShell実行ポリシーエラー**
   ```powershell
   # 現在のポリシーを確認
   Get-ExecutionPolicy -Scope CurrentUser
   
   # RemoteSignedに設定
   Set-ExecutionPolicy RemoteSigned -Scope CurrentUser
   ```

2. **スクリプトが見つからない**
   - ファイルパスを確認: `Test-Path "$env:USERPROFILE\.climonitor\notify.ps1"`
   - ディレクトリ作成: `New-Item -ItemType Directory -Path "$env:USERPROFILE\.climonitor" -Force`

3. **通知が表示されない**
   - 手動でスクリプトを実行してテスト:
     ```powershell
     & "$env:USERPROFILE\.climonitor\notify.ps1" -EventType "waiting_for_input" -ToolName "claude" -Message "Allow execution? (y/n)" -Duration "30s"
     ```
   - 通知ログを確認: `Get-Content "$env:USERPROFILE\.climonitor\notifications.log"`

4. **Toast通知が動作しない**
   - Windows 10/11の通知設定を確認
   - アプリの通知許可を確認（設定 > システム > 通知とアクション）

## コマンドオプション

### climonitor (監視サーバー)
```bash
climonitor [OPTIONS]

OPTIONS:
    --live                 ライブ監視モード（デフォルト）
    --verbose              詳細ログ出力
    --grpc                 gRPC接続を使用
    --bind <ADDR>          gRPC バインドアドレス（デフォルト: 127.0.0.1:50051）
    --socket <PATH>        Unix socketパス
    --config <FILE>        設定ファイルパス
    --log-file <FILE>      ログファイルパス
    --help                 ヘルプ表示
```

### climonitor-launcher (CLIラッパー)
```bash
climonitor-launcher [OPTIONS] <TOOL>

ARGS:
    <TOOL>          起動するツール (claude | gemini)

OPTIONS:
    --verbose              詳細ログ出力
    --grpc                 gRPC接続を使用
    --connect <ADDR>       接続アドレス（gRPC: host:port, Unix: パス）
    --config <FILE>        設定ファイルパス
    --log-file <FILE>      ログファイルパス
    --help                 ヘルプ表示
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
- **Transport**: Unix Socket / gRPC 通信レイヤー（IP制限対応）
- **Config**: TOML設定ファイル管理（優先度制御）

### 主な依存関係
- **tokio** - 非同期ランタイム
- **portable-pty** - PTY（疑似端末）統合  
- **vte** - 端末パーサー
- **tonic** - gRPC実装（Rust）
- **prost** - Protocol Buffers生成
- **chrono** - 日時処理（ロケール対応）
- **serde** - JSON解析
- **toml** - TOML設定ファイル解析
- **unicode-width** - Unicode文字幅計算
- **home** - ホームディレクトリ取得
- **cidr** - IPアドレス範囲処理

### 対応プラットフォーム
- macOS
- Linux  
- Windows（WSLのみ動作確認済み）

## ライセンス

[MIT License](LICENSE)