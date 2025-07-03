# climonitor コードストラクチャ

## プロジェクト概要

climonitorは、Claude CodeとGemini CLIのリアルタイム監視を行うRustプロジェクトです。クライアント・サーバー構成により、複数のCLIセッションを同時に監視できます。

## ディレクトリ構成

```
climonitor/
├── shared/           # 共通ライブラリ（プロトコル定義）
├── launcher/         # climonitor-launcher（CLIラッパー）
├── monitor/          # climonitor（監視サーバー）
├── docs/             # 技術ドキュメント
└── CLAUDE.md         # Claude Code向けガイド
```

## shared/ (climonitor-shared)

### src/protocol.rs
- **責務**: クライアント・サーバー間通信プロトコル定義
- **主要型**:
  - `LauncherToMonitor` - launcher → monitor メッセージ
  - `MonitorToLauncher` - monitor → launcher メッセージ（将来拡張用）
  - `SessionStatus` - セッション状態（Connected, Idle, Busy, WaitingInput, Completed, Error）

### src/cli_tool.rs
- **責務**: CLIツール種別定義
- **主要型**: `CliToolType` (Claude, Gemini)

## launcher/ (climonitor-launcher)

### src/main.rs
- **責務**: CLI引数解析、メインエントリーポイント
- **主要関数**: `main()` - 引数に基づいてLauncherClientを起動

### src/launcher_client.rs
- **責務**: monitor server接続、セッション管理、PTY統合
- **主要構造体**: `LauncherClient`
- **主要関数**:
  - `new()` - クライアント初期化
  - `run_claude()` - Claudeセッション実行
  - `start_pty_bidirectional_io()` - PTY I/O処理開始
  - `send_state_update()` - 状態更新送信（永続接続）
  - `send_status_update_persistent()` - 状態更新送信（新規接続）

### src/tool_wrapper.rs
- **責務**: 複数CLIツールの統一インターフェース
- **主要構造体**: `ToolWrapper`
- **主要関数**:
  - `new()` - ツール種別に応じたラッパー作成
  - `spawn_with_pty()` - PTYでプロセス起動
  - `run_directly()` - monitor接続なしで直接実行

### src/claude_tool.rs / src/gemini_tool.rs
- **責務**: 各CLIツール固有の起動ロジック
- **主要構造体**: `ClaudeTool`, `GeminiTool`
- **主要関数**: `spawn_with_pty()` - PTYでツール起動

### src/state_detector.rs
- **責務**: 状態検出器のファクトリーパターン、trait定義
- **trait**: `StateDetector`
- **主要関数**: `create_state_detector()` - ツール別検出器作成

### src/screen_claude_detector.rs
- **責務**: Claude固有の状態検出ロジック
- **主要構造体**: `ScreenClaudeStateDetector`
- **検出パターン**:
  - `"esc to interrupt"` - 実行中状態
  - `"Do you want"`, `"proceed?"` - 入力待ち状態
  - `"◯ IDE connected"` - アイドル状態
  - `●` マーカー - 実行コンテキスト抽出

### src/screen_gemini_detector.rs
- **責務**: Gemini固有の状態検出ロジック
- **主要構造体**: `ScreenGeminiStateDetector`
- **検出パターン**:
  - `"(esc to cancel"` - 実行中状態
  - `"Waiting for user confirmation"` - 入力待ち状態
  - `">"` - アイドル状態
  - `✦` マーカー - 実行コンテキスト抽出

### src/screen_buffer.rs
- **責務**: VTEパーサーによる端末画面バッファ管理
- **主要構造体**: `ScreenBuffer`
- **主要機能**:
  - ANSI escape sequence処理
  - UIボックス検出（╭╮╰╯）
  - PTY+1列バッファ（UIボックス重複問題解決）

### src/cli_tool.rs
- **責務**: PTYサイズ取得などの共通ユーティリティ
- **主要関数**: `get_pty_size()` - 端末サイズ取得

## monitor/ (climonitor)

### src/main.rs
- **責務**: CLI引数解析、monitor server起動
- **主要関数**: `main()` - MonitorServerを起動

### src/monitor_server.rs
- **責務**: Unix Domain Socket server、メッセージ処理
- **主要構造体**: `MonitorServer`
- **主要関数**:
  - `run()` - サーバーメインループ
  - `handle_launcher_message()` - launcherメッセージ処理

### src/session_manager.rs
- **責務**: セッション状態管理、launcher情報管理
- **主要構造体**: `SessionManager`, `Session`, `LauncherInfo`
- **主要関数**:
  - `register_launcher()` - launcher登録
  - `update_session_status()` - セッション状態更新
  - `remove_launcher()` - launcher削除時のクリーンアップ

### src/live_ui.rs
- **責務**: リアルタイムUI表示、セッション一覧表示
- **主要関数**:
  - `display_sessions()` - セッション一覧表示
  - `truncate_str()` - 長いテキストの切り詰め

### src/unicode_utils.rs
- **責務**: Unicode安全なテキスト処理
- **主要関数**:
  - `truncate_str()` - grapheme cluster考慮のテキスト切り詰め
  - `display_width()` - 表示幅計算

## データフロー

### 1. 起動フロー
```
1. climonitor --live → MonitorServer起動 → Unix Socket待機
2. climonitor-launcher claude → LauncherClient起動 → Socket接続
3. LauncherClient → Claude起動（PTY） → 状態検出開始
```

### 2. 状態検出フロー
```
Claude出力 → PTY → ScreenBuffer → StateDetector → SessionStatus
                                                        ↓
monitor ← Unix Socket ← LauncherToMonitor::StateUpdate ←┘
```

### 3. 表示フロー
```
SessionManager → セッション状態管理 → LiveUI → ターミナル表示
```

## 重要な設計パターン

### 1. 独立型状態検出器
- 各ツール（Claude/Gemini）専用の検出器
- 完全に独立したScreenBuffer
- ツール固有パターンに最適化

### 2. クライアント・サーバー分離
- launcher: PTY統合 + 状態検出
- monitor: 状態管理 + UI表示
- Unix Domain Socket通信

### 3. PTY+1列バッファ
- UIボックス重複問題の解決
- ink.js期待動作とVTEパーサーの整合

### 4. エラーハンドリング
- launcher切断時の自動クリーンアップ
- 接続失敗時のフォールバック
- Unicode安全なテキスト処理

## テスト構成

### launcher/tests/
- `integration_state_detection.rs` - 状態検出テスト
- `integration_tool_wrapper.rs` - ツールラッパーテスト

### monitor/tests/
- `integration_protocol_basic.rs` - プロトコル基本テスト
- `integration_session_management.rs` - セッション管理テスト
- `integration_regression_detection.rs` - 回帰テスト

## 依存関係グラフ

```
climonitor-monitor
├── climonitor-shared (protocol)
├── tokio (async runtime)
├── ratatui (terminal UI)
└── unicode-width, unicode-segmentation

climonitor-launcher  
├── climonitor-shared (protocol)
├── portable-pty (PTY integration)
├── vte (terminal parser)
└── tokio (async runtime)

climonitor-shared
├── serde (serialization)
├── chrono (timestamps)
└── standard library
```

---

このコードストラクチャにより、climonitorは高精度な状態検出と安定したリアルタイム監視を実現しています。