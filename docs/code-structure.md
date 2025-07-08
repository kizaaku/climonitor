# climonitor コードストラクチャ

## プロジェクト概要

climonitorは、Claude CodeとGemini CLIのリアルタイム監視を行うRustプロジェクトです。クライアント・サーバー構成により、複数のCLIセッションを同時に監視できます。

## ディレクトリ構成

```
climonitor/
├── shared/           # 共通ライブラリ（プロトコル定義、gRPC）
├── launcher/         # climonitor-launcher（CLIラッパー）
├── monitor/          # climonitor（監視サーバー）
├── proto/            # gRPC Protocol Buffers定義
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

### src/config.rs
- **責務**: TOML設定ファイル管理、設定優先度制御
- **主要構造体**:
  - `Config` - メイン設定構造体
  - `ConnectionSettings` - 接続設定（TCP/Unix, IP許可リスト）
  - `LoggingSettings` - ログ設定
- **主要関数**:
  - `from_file()` - 設定ファイル読み込み
  - `load_auto()` - 自動検出で設定読み込み
  - `apply_env_overrides()` - 環境変数での上書き
  - `to_connection_config()` - TransportConfig変換

### src/transport.rs
- **責務**: 通信レイヤー抽象化、gRPC/Unix Socket統合
- **主要型**:
  - `ConnectionConfig` - 接続設定（gRPC/Unix）
  - `Connection` - 統一接続インターフェース
- **主要関数**:
  - `connect_client()` - クライアント接続
  - `listen_server()` - サーバーリッスン
  - `is_ip_allowed()` - IP許可リスト検証

### src/message_conversion.rs
- **責務**: gRPCメッセージ変換ユーティリティ
- **主要関数**:
  - `to_grpc_launcher_message()` - プロトコル → gRPC変換
  - `from_grpc_launcher_message()` - gRPC → プロトコル変換
  - `to_grpc_session_status()` - SessionStatus変換

### src/climonitor.rs
- **責務**: gRPC Protocol Buffers生成コード
- **内容**: `proto/monitor.proto`から自動生成
- **主要型**: `LauncherMessage`, `ContextUpdate`, `StatusUpdate`

### build.rs
- **責務**: Protocol Buffers コンパイル設定
- **機能**: `proto/monitor.proto` → Rustコード生成

### src/cli_tool.rs
- **責務**: CLIツール種別定義
- **主要型**: `CliToolType` (Claude, Gemini)

## launcher/ (climonitor-launcher)

### src/main.rs
- **責務**: CLI引数解析、メインエントリーポイント
- **主要関数**: `main()` - 引数に基づいてLauncherClientを起動

### src/transport_client.rs
- **責務**: transport layer クライアント、gRPC/Unix Socket統合
- **主要構造体**: `TransportLauncherClient`
- **主要関数**:
  - `new()` - Unix Socket クライアント初期化
  - `new_with_grpc()` - gRPC クライアント初期化
  - `run_claude()` - Claudeセッション実行
  - `start_pty_bidirectional_io()` - PTY I/O処理開始
  - `send_unix_message()` - Unix Socket メッセージ送信

### src/grpc_client.rs
- **責務**: gRPC専用クライアント実装
- **主要構造体**: `GrpcLauncherClient`
- **主要関数**:
  - `new()` - gRPC接続初期化
  - `send_launcher_message()` - gRPCメッセージ送信
  - `get_launcher_id()` - launcher ID取得
  - `get_session_id()` - session ID取得

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

### src/transport_server.rs
- **責務**: transport layer サーバー、gRPC/Unix Socket統合
- **主要構造体**: `TransportMonitorServer`
- **主要関数**:
  - `run()` - サーバーメインループ
  - `handle_launcher_message()` - launcherメッセージ処理

### src/grpc_server.rs
- **責務**: gRPC専用サーバー実装
- **主要構造体**: `MonitorService`
- **主要機能**:
  - `CliMonitorService` trait実装
  - IP許可リスト検証
  - SessionManager統合

### src/session_manager.rs
- **責務**: セッション状態管理、launcher情報管理
- **主要構造体**: `SessionManager`, `Session`, `LauncherInfo`
- **主要関数**:
  - `register_launcher()` - launcher登録
  - `update_session_status()` - セッション状態更新
  - `get_launchers_by_project()` - プロジェクト別launcher取得
  - `remove_launcher()` - launcher削除時のクリーンアップ

### src/live_ui.rs
- **責務**: リアルタイムUI表示、launcher-based表示システム
- **主要構造体**: `LiveUI`
- **主要関数**:
  - `run()` - ライブUI表示ループ
  - `render_sessions()` - launcher-based セッション表示
  - `format_duration_since()` - ロケール対応時間表示

### src/notification.rs
- **責務**: 状態変化通知システム
- **主要関数**:
  - `send_notification_if_needed()` - 通知スクリプト実行
  - 対応スクリプト: `~/.config/climonitor/notify.sh`

### src/unicode_utils.rs
- **責務**: Unicode安全なテキスト処理
- **主要関数**:
  - `truncate_str()` - grapheme cluster考慮のテキスト切り詰め
  - `display_width()` - 表示幅計算

## proto/ (Protocol Buffers定義)

### monitor.proto
- **責務**: gRPC サービスとメッセージ定義
- **サービス**: `CliMonitorService`
- **メッセージ型**:
  - `LauncherMessage` - launcher → monitor通信
  - `ContextUpdate` - 実行コンテキスト更新
  - `StatusUpdate` - セッション状態更新
  - `Empty` - 空レスポンス

## データフロー

### 1. 設定読み込みフロー
```
CLI引数 → 環境変数 → 設定ファイル自動検出 → デフォルト値
              ↓
        Config構造体 → ConnectionConfig → transport layer
```

### 2. 起動フロー
```
1. climonitor --live → 設定読み込み → TransportMonitorServer起動 → gRPC/Unix Socket待機
2. climonitor-launcher claude → 設定読み込み → TransportLauncherClient起動 → 接続（IP制限チェック）
3. TransportLauncherClient → Claude起動（PTY） → 状態検出開始
```

### 3. 状態検出フロー（gRPC）
```
Claude出力 → PTY → ScreenBuffer → StateDetector → SessionStatus
                                                        ↓
monitor ← gRPC ← GrpcLauncherClient ← message_conversion ←┘
```

### 4. 状態検出フロー（Unix Socket）
```
Claude出力 → PTY → ScreenBuffer → StateDetector → SessionStatus
                                                        ↓
monitor ← Unix Socket ← LauncherToMonitor::StateUpdate ←┘
```

### 5. 表示フロー
```
SessionManager → launcher-based表示 → LiveUI → ターミナル表示
```

### 6. 通知フロー
```
状態変化 → notification::send_notification_if_needed() → ~/.config/climonitor/notify.sh
```

## 重要な設計パターン

### 1. 独立型状態検出器
- 各ツール（Claude/Gemini）専用の検出器
- 完全に独立したScreenBuffer
- ツール固有パターンに最適化

### 2. クライアント・サーバー分離
- launcher: PTY統合 + 状態検出
- monitor: 状態管理 + UI表示 + 通知
- 通信レイヤー: gRPC（IP制限付き）/Unix Domain Socket

### 3. 設定システム
- TOML設定ファイル（複数候補パス自動検出）
- 設定優先度: CLI > 環境変数 > 設定ファイル > デフォルト
- 接続設定: gRPC（セキュリティ）/Unix（ローカル）

### 4. gRPC統合
- Protocol Buffers定義による型安全通信
- 共有メッセージ変換ユーティリティ
- 重複コード削減とパフォーマンス向上

### 5. Launcher-based表示システム
- セッション-based からlauncher-basedに移行
- 接続済みlauncherを常に表示
- セッション状態の有無を適切に処理

### 6. PTY+1列バッファ
- UIボックス重複問題の解決
- ink.js期待動作とVTEパーサーの整合

### 7. ロケール対応
- 日本語/英語環境での時刻表示
- Unicode安全なテキスト処理

### 8. エラーハンドリング
- launcher切断時の自動クリーンアップ
- 接続失敗時のフォールバック
- IP許可リスト違反時の接続拒否
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
├── climonitor-shared (protocol, config, transport, gRPC)
├── tokio (async runtime + gRPC server)
├── tonic (gRPC implementation)
├── ratatui (terminal UI)
└── unicode-width, unicode-segmentation

climonitor-launcher  
├── climonitor-shared (protocol, config, transport, gRPC)
├── portable-pty (PTY integration)
├── vte (terminal parser)
├── tokio (async runtime + gRPC client)
└── tonic (gRPC implementation)

climonitor-shared
├── tonic (gRPC framework)
├── prost (Protocol Buffers)
├── serde (serialization)
├── toml (configuration parsing)
├── chrono (timestamps)
├── home (home directory detection)
├── cidr (IP range processing)
└── standard library
```

---

このコードストラクチャにより、climonitorは高精度な状態検出と安定したリアルタイム監視を実現しています。