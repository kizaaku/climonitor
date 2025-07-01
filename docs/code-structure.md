# climonitor コードストラクチャ

## ファイル構成と責務

### launcher/ (climonitor-launcher)

#### main.rs
- **責務**: CLI引数解析、メインエントリーポイント
- **主要関数**: `main()`
- **依存関係**:
  - `launcher_client.rs` - `LauncherClient`の作成・実行
  - `cli_tool.rs` - ツール型判定とPTYサイズ取得

#### launcher_client.rs  
- **責務**: サーバー接続、セッション管理、PTY統合
- **主要関数**: 
  - `LauncherClient::new()` - クライアント初期化
  - `run()` - メインループ実行
- **依存関係**:
  - `tool_wrapper.rs` - CLIツールの起動
  - `state_detector.rs` - 状態検出器の作成
  - `session_state.rs` - セッション状態の管理
  - `climonitor_shared::protocol` - サーバー通信

#### tool_wrapper.rs
- **責務**: CLIツールのPTY起動、I/O処理
- **主要関数**:
  - `ToolWrapper::new()` - ツールラッパー作成
  - `spawn()` - PTYプロセス起動
- **依存関係**:
  - `claude_tool.rs` - Claude Code統合
  - `gemini_tool.rs` - Gemini CLI統合
  - `cli_tool.rs` - 共通CLIツール機能
  - `portable_pty` - PTY機能

#### state_detector.rs
- **責務**: 状態検出器のファクトリーパターン、trait定義
- **主要関数**:
  - `create_state_detector()` - ツール別検出器作成
- **trait**: `StateDetector`
- **依存関係**:
  - `screen_claude_detector.rs` - Claude専用検出器
  - `screen_gemini_detector.rs` - Gemini専用検出器

#### screen_claude_detector.rs
- **責務**: Claude固有の状態検出ロジック
- **主要関数**:
  - `ScreenClaudeStateDetector::new()` - 検出器初期化
  - `detect_claude_completion_state()` - "esc to interrupt"検出
- **検出パターン**: `"esc to interrupt"`, `"Do you want"`, `"proceed?"`
- **依存関係**:
  - `screen_buffer.rs` - 画面バッファ管理
  - `session_state.rs` - 状態列挙型

#### screen_gemini_detector.rs  
- **責務**: Gemini固有の状態検出ロジック
- **主要関数**:
  - `ScreenGeminiStateDetector::new()` - 検出器初期化
  - `detect_gemini_state()` - Gemini状態検出
- **検出パターン**: `"(esc to cancel"`, `">"プロンプト`, `"Allow execution?"`
- **依存関係**:
  - `screen_buffer.rs` - 画面バッファ管理
  - `session_state.rs` - 状態列挙型

#### screen_buffer.rs
- **責務**: VTEパーサー統合、画面バッファ管理、UI box検出
- **主要関数**:
  - `ScreenBuffer::new()` - バッファ初期化
  - `process_data()` - ANSI sequence処理
  - `find_ui_boxes()` - UI box検出
- **依存関係**:
  - `vte` クレート - VTEパーサー

#### session_state.rs
- **責務**: セッション状態の定義と変換
- **enum**: `SessionState` (Idle, Busy, WaitingForInput, Error, Connected)
- **主要関数**: `to_session_status()` - プロトコル形式への変換

#### claude_tool.rs
- **責務**: Claude Code固有の起動ロジック
- **主要関数**:
  - `ClaudeTool::new()` - Claude設定
  - `command_name()`, `get_project_name()` - メタデータ取得

#### gemini_tool.rs
- **責務**: Gemini CLI固有の起動ロジック  
- **主要関数**:
  - `GeminiTool::new()` - Gemini設定
  - `command_name()`, `get_project_name()` - メタデータ取得

#### cli_tool.rs
- **責務**: CLIツール共通機能、PTYサイズ取得
- **enum**: `CliToolType` (Claude, Gemini)
- **主要関数**: 
  - `get_pty_size()` - ターミナルサイズ取得
  - 型変換functions

### monitor/ (climonitor)

#### main.rs
- **責務**: CLI引数解析、monitor起動
- **主要関数**: `main()`
- **依存関係**:
  - `monitor_server.rs` - サーバー起動
  - `live_ui.rs` - UIモード起動

#### monitor_server.rs
- **責務**: Unix Domain Socketサーバー、クライアント管理
- **主要関数**:
  - `MonitorServer::new()` - サーバー初期化
  - `run()` - メインループ
- **依存関係**:
  - `session_manager.rs` - セッション状態管理
  - `live_ui.rs` - UI更新
  - `climonitor_shared::protocol` - 通信プロトコル

#### session_manager.rs
- **責務**: セッション状態の集約管理、統計情報
- **主要関数**:
  - `SessionManager::new()` - マネージャー初期化
  - `update_session()` - セッション情報更新
- **データ構造**: `HashMap<String, SessionInfo>` - セッション管理

#### live_ui.rs
- **責務**: リアルタイムターミナルUI、セッション表示
- **主要関数**:
  - `LiveUI::new()` - UI初期化
  - `render()` - 画面描画
- **依存関係**:
  - `ratatui` - ターミナルUI
  - `unicode_utils.rs` - Unicode処理

#### unicode_utils.rs
- **責務**: Unicode安全な文字列処理
- **主要関数**:
  - `truncate_unicode_aware()` - Unicode対応切り詰め
  - `calculate_display_width()` - 表示幅計算

### shared/ (climonitor-shared)

#### protocol.rs
- **責務**: クライアント・サーバー間通信プロトコル
- **構造体**:
  - `LauncherMessage` - launcher→monitor
  - `MonitorMessage` - monitor→launcher  
  - `SessionStatus` - 状態列挙型
- **依存関係**: `serde` - JSON serialization

## 呼び出し関係

### launcher起動フロー
```
main.rs
├─ cli_tool::get_pty_size()
├─ launcher_client::LauncherClient::new()
└─ launcher_client::run()
   ├─ tool_wrapper::ToolWrapper::new()
   │  ├─ claude_tool::ClaudeTool::new()
   │  └─ gemini_tool::GeminiTool::new()
   ├─ state_detector::create_state_detector()
   │  ├─ screen_claude_detector::ScreenClaudeStateDetector::new()
   │  └─ screen_gemini_detector::ScreenGeminiStateDetector::new()
   └─ tool_wrapper::spawn()
```

### 状態検出フロー
```
tool_wrapper::spawn()
├─ pty output → state_detector::process_output()
├─ screen_buffer::process_data()
├─ screen_buffer::find_ui_boxes()
├─ screen_*_detector::detect_*_state()
└─ launcher_client → server (protocol::LauncherMessage)
```

### monitor表示フロー
```
monitor_server::run()
├─ socket accept → protocol::LauncherMessage
├─ session_manager::update_session()
├─ live_ui::render()
│  ├─ unicode_utils::truncate_unicode_aware()
│  └─ ratatui rendering
└─ protocol::MonitorMessage response
```

## 主要データフロー

1. **PTY Output** → `screen_buffer` → VTE parser → screen grid
2. **Screen Grid** → `state_detector` → pattern matching → `SessionState`  
3. **SessionState** → `protocol` → Unix Socket → monitor server
4. **Monitor Server** → `session_manager` → aggregated state → `live_ui`

## 設定・環境変数

- `CLIMONITOR_SOCKET_PATH` - ソケットパス (protocol.rs)
- `ANTHROPIC_LOG=debug` - Claude詳細ログ
- `RUST_LOG` - Rustログレベル