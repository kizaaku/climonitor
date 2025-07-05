use anyhow::Result;
use chrono::Utc;
use portable_pty::MasterPty;
use serde_json;
use std::path::PathBuf;
use tokio::task::JoinHandle;

use crate::tool_wrapper::ToolWrapper;
use climonitor_shared::{
    generate_connection_id, Connection, ConnectionConfig, LauncherToMonitor, SessionStatus,
    connect_client,
};

/// ダミーターミナルガード（main関数で実際のガードが作成済みの場合）
pub struct DummyTerminalGuard {
    #[allow(dead_code)]
    verbose: bool,
}

/// Transport対応 Launcher クライアント
pub struct TransportLauncherClient {
    launcher_id: String,
    connection: Option<Connection>,
    connection_config: ConnectionConfig,
    tool_wrapper: ToolWrapper,
    project_name: Option<String>,
    session_id: String,
    verbose: bool,
    log_file: Option<PathBuf>,
}

impl TransportLauncherClient {
    /// 新しいTransportLauncherClientを作成
    pub async fn new(
        tool_wrapper: ToolWrapper,
        connection_config: ConnectionConfig,
        verbose: bool,
        log_file: Option<PathBuf>,
    ) -> Result<Self> {
        let launcher_id = generate_connection_id();
        let session_id = generate_connection_id();
        let project_name = tool_wrapper.guess_project_name();

        let mut client = Self {
            launcher_id,
            connection: None,
            connection_config,
            tool_wrapper,
            project_name,
            session_id,
            verbose,
            log_file,
        };

        // Monitor サーバーに接続を試行
        client.try_connect_to_monitor().await?;

        Ok(client)
    }

    /// Monitor サーバーへの接続を試行
    async fn try_connect_to_monitor(&mut self) -> Result<()> {
        if self.verbose {
            eprintln!(
                "🔄 Attempting to connect to monitor server: {:?}",
                self.connection_config
            );
        }

        match connect_client(&self.connection_config).await {
            Ok(connection) => {
                self.connection = Some(connection);
                if self.verbose {
                    eprintln!("🔗 Connected to monitor server");
                }
            }
            Err(e) => {
                if self.verbose {
                    eprintln!(
                        "⚠️  Failed to connect to monitor server: {e}. Running without monitoring."
                    );
                }
            }
        }

        Ok(())
    }

    /// Monitor サーバーに接続されているかチェック
    pub fn is_connected(&self) -> bool {
        self.connection.is_some()
    }

    /// 接続メッセージを送信
    async fn send_connect_message(&mut self) -> Result<()> {
        if let Some(ref mut connection) = self.connection {
            let connect_msg = LauncherToMonitor::Connect {
                launcher_id: self.launcher_id.clone(),
                project: self.project_name.clone(),
                tool_type: self.tool_wrapper.get_tool_type(),
                claude_args: self.tool_wrapper.get_args().to_vec(),
                working_dir: self
                    .tool_wrapper
                    .get_working_dir()
                    .cloned()
                    .unwrap_or_else(|| std::env::current_dir().unwrap_or_default()),
                timestamp: Utc::now(),
            };

            if self.verbose {
                eprintln!(
                    "📤 Sending connect message: launcher_id={}, project={:?}",
                    self.launcher_id, self.project_name
                );
            }

            let msg_bytes = serde_json::to_vec(&connect_msg)?;
            connection.write_all(&msg_bytes).await?;
            connection.write_all(b"\n").await?;
            connection.flush().await?;

            if self.verbose {
                eprintln!("✅ Connect message sent successfully");
            }
        } else if self.verbose {
            eprintln!("⚠️  No connection available for sending connect message");
        }
        Ok(())
    }

    /// 切断メッセージを送信
    async fn send_disconnect_message(&mut self) -> Result<()> {
        if let Some(ref mut connection) = self.connection {
            let disconnect_msg = LauncherToMonitor::Disconnect {
                launcher_id: self.launcher_id.clone(),
                timestamp: Utc::now(),
            };

            let msg_bytes = serde_json::to_vec(&disconnect_msg)?;
            connection.write_all(&msg_bytes).await?;
            connection.write_all(b"\n").await?;

            if self.verbose {
                eprintln!("📤 Sent disconnect message to monitor");
            }
        }
        Ok(())
    }

    /// Claude プロセス起動・監視
    pub async fn run_claude(&mut self) -> Result<()> {
        if self.verbose {
            eprintln!(
                "🚀 Starting CLI tool: {}",
                self.tool_wrapper.to_command_string()
            );
        }

        // Monitor に接続できていない場合は単純にClaude実行
        if !self.is_connected() {
            if self.verbose {
                eprintln!("🔄 Running CLI tool without monitoring (monitor not connected)");
            }
            return self.tool_wrapper.run_directly().await;
        }

        // 接続メッセージを送信
        if let Err(e) = self.send_connect_message().await {
            if self.verbose {
                eprintln!("⚠️  Failed to send connect message: {e}");
            }
        } else if self.verbose {
            eprintln!("✅ Connect message sent successfully");
        }

        // ターミナルガードはmain関数で作成済み（ここでは作らない）
        let terminal_guard = DummyTerminalGuard {
            verbose: self.verbose,
        };

        // CLI ツール プロセス起動（PTYを使用してTTY環境を提供）
        let (mut process, pty_master) = self.tool_wrapper.spawn_with_pty()?;

        // PTYベースの双方向I/O開始
        let pty_handle = self
            .start_pty_bidirectional_io(pty_master, terminal_guard)
            .await?;

        if self.verbose {
            eprintln!("👀 Monitoring started for CLI tool process");
        }

        // CLI ツール プロセスの終了を待つタスクを一度だけ起動
        let mut wait_task = tokio::task::spawn_blocking(move || process.wait());

        // シグナルハンドリングとリサイズ処理
        let exit_status = self.wait_with_signals(&mut wait_task).await;

        // PTYタスクを終了
        pty_handle.abort();

        // 少し待機してI/Oが完了するのを待つ
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        match exit_status {
            Ok(status) => {
                if self.verbose {
                    eprintln!("🏁 CLI tool process exited with status: {status:?}");
                }
            }
            Err(e) => {
                if self.verbose {
                    eprintln!("❌ CLI tool execution failed: {e}");
                }
                // エラー時でも切断メッセージを送信
                if let Err(disconnect_err) = self.send_disconnect_message().await {
                    if self.verbose {
                        eprintln!("⚠️  Failed to send disconnect message: {disconnect_err}");
                    }
                }
                // 接続を明示的に閉じる
                if let Some(connection) = self.connection.take() {
                    drop(connection);
                    if self.verbose {
                        eprintln!("🔌 Connection closed (after error)");
                    }
                }
                return Err(e);
            }
        }

        // 切断メッセージ送信
        self.send_disconnect_message().await?;

        // 接続を明示的に閉じる
        if let Some(connection) = self.connection.take() {
            drop(connection);
            if self.verbose {
                eprintln!("🔌 Connection closed");
            }
        }

        Ok(())
    }

    /// PTY 双方向I/Oタスク開始
    async fn start_pty_bidirectional_io(
        &self,
        pty_master: Box<dyn MasterPty + Send>,
        _terminal_guard: DummyTerminalGuard,
    ) -> Result<JoinHandle<()>> {
        let launcher_id = self.launcher_id.clone();
        let session_id = self.session_id.clone();
        let verbose = self.verbose;
        let log_file = self.log_file.clone();
        let tool_type = self.tool_wrapper.get_tool_type();
        let config = self.connection_config.clone();
        
        let handle = tokio::spawn(async move {
            Self::handle_pty_bidirectional_io(
                pty_master,
                launcher_id,
                session_id,
                verbose,
                log_file,
                tool_type,
                config,
                _terminal_guard,
            )
            .await;
        });

        Ok(handle)
    }

    /// PTY 双方向I/O処理
    async fn handle_pty_bidirectional_io(
        pty_master: Box<dyn MasterPty + Send>,
        launcher_id: String,
        session_id: String,
        verbose: bool,
        log_file: Option<PathBuf>,
        tool_type: crate::cli_tool::CliToolType,
        connection_config: ConnectionConfig,
        _terminal_guard: DummyTerminalGuard,
    ) {
        // ログファイルを開く
        let log_writer = if let Some(ref log_path) = log_file {
            match tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_path)
                .await
            {
                Ok(file) => Some(file),
                Err(e) => {
                    if verbose {
                        let log_display = log_path.display();
                        eprintln!("⚠️  Failed to open log file {log_display}: {e}");
                    }
                    None
                }
            }
        } else {
            None
        };

        // PTY writer/reader を取得
        let pty_writer = match pty_master.take_writer() {
            Ok(writer) => writer,
            Err(e) => {
                if verbose {
                    eprintln!("⚠️  Failed to get PTY writer: {e}");
                }
                return;
            }
        };

        let pty_reader = match pty_master.try_clone_reader() {
            Ok(reader) => reader,
            Err(e) => {
                if verbose {
                    eprintln!("⚠️  Failed to get PTY reader: {e}");
                }
                return;
            }
        };

        // 双方向I/Oタスクを起動
        let mut pty_to_stdout = tokio::spawn(async move {
            Self::handle_pty_to_stdout_with_monitoring(
                pty_reader,
                launcher_id.clone(),
                session_id.clone(),
                verbose,
                log_writer,
                tool_type,
                connection_config,
            )
            .await;
        });

        let mut stdin_to_pty = tokio::spawn(async move {
            Self::handle_stdin_to_pty_simple(pty_writer, verbose).await;
        });

        // タスクの完了を待つ
        tokio::select! {
            _ = &mut pty_to_stdout => {
                if verbose {
                    eprintln!("📡 PTY to stdout task ended");
                }
                stdin_to_pty.abort();
            }
            _ = &mut stdin_to_pty => {
                if verbose {
                    eprintln!("📡 Stdin to PTY task ended");
                }
                pty_to_stdout.abort();
            }
        }
    }

    /// プロセス終了とシグナルを待機（Unix版）
    #[cfg(unix)]
    async fn wait_with_signals(
        &self,
        wait_task: &mut tokio::task::JoinHandle<std::io::Result<portable_pty::ExitStatus>>,
    ) -> Result<portable_pty::ExitStatus> {
        let mut sigwinch =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::window_change()).unwrap();
        let mut sigint =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()).unwrap();
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();

        loop {
            tokio::select! {
                result = &mut *wait_task => {
                    return result?.map_err(|e| anyhow::anyhow!("Process wait error: {}", e));
                }
                _ = sigint.recv() => {
                    if self.verbose {
                        eprintln!("🛑 Received SIGINT, letting CLI tool handle it...");
                    }
                }
                _ = sigterm.recv() => {
                    if self.verbose {
                        eprintln!("🛑 Received SIGTERM, shutting down gracefully...");
                    }
                    return Err(anyhow::anyhow!("Terminated by signal"));
                }
                _ = sigwinch.recv() => {
                    if self.verbose {
                        eprintln!("🔄 Terminal resized - updating PTY size...");
                    }
                    let new_size = crate::cli_tool::get_pty_size();
                    if self.verbose {
                        let cols = new_size.cols;
                        let rows = new_size.rows;
                        eprintln!("📏 New terminal size: {cols}x{rows}");
                    }
                }
            }
        }
    }

    /// プロセス終了とシグナルを待機（非Unix版）
    #[cfg(not(unix))]
    async fn wait_with_signals(
        &self,
        wait_task: &mut tokio::task::JoinHandle<std::io::Result<portable_pty::ExitStatus>>,
    ) -> Result<portable_pty::ExitStatus> {
        loop {
            tokio::select! {
                result = &mut *wait_task => {
                    return result?.map_err(|e| anyhow::anyhow!("Process wait error: {}", e));
                }
                _ = tokio::signal::ctrl_c() => {
                    if self.verbose {
                        eprintln!("🛑 Received Ctrl+C, letting CLI tool handle it...");
                    }
                }
            }
        }
    }

    /// PTY出力をstdoutに転送（監視・ログ付き）
    async fn handle_pty_to_stdout_with_monitoring(
        mut pty_reader: Box<dyn std::io::Read + Send>,
        launcher_id: String,
        session_id: String,
        verbose: bool,
        mut log_writer: Option<tokio::fs::File>,
        tool_type: crate::cli_tool::CliToolType,
        connection_config: ConnectionConfig,
    ) {
        use crate::state_detector::create_state_detector;
        use climonitor_shared::SessionStatus;

        let state_detector: std::sync::Arc<
            std::sync::Mutex<Box<dyn crate::state_detector::StateDetector + Send>>,
        > = std::sync::Arc::new(std::sync::Mutex::new(create_state_detector(
            tool_type, verbose,
        )));
        let last_notified_status = std::sync::Arc::new(std::sync::Mutex::new(SessionStatus::Idle));

        // ターミナルサイズ監視用
        let mut last_terminal_size = crate::cli_tool::get_pty_size();
        use std::io::Read;
        use tokio::io::AsyncWriteExt;

        let mut buffer = [0u8; 8192];
        let mut stdout = tokio::io::stdout();

        // 定期的な状態チェックタスクを起動
        let state_checker_task = {
            let state_detector_clone = state_detector.clone();
            let last_notified_status_clone = last_notified_status.clone();
            let launcher_id_clone = launcher_id.clone();
            let session_id_clone = session_id.clone();
            let config_clone = connection_config.clone();

            tokio::spawn(async move {
                Self::periodic_state_checker(
                    state_detector_clone,
                    last_notified_status_clone,
                    launcher_id_clone,
                    session_id_clone,
                    config_clone,
                    verbose,
                )
                .await;
            })
        };

        loop {
            match pty_reader.read(&mut buffer) {
                Ok(0) => {
                    if verbose {
                        eprintln!("📡 PTY reader EOF");
                    }
                    state_checker_task.abort();
                    break;
                }
                Ok(n) => {
                    let data = &buffer[..n];
                    let output_str = String::from_utf8_lossy(data);

                    // 標準出力に書き込み
                    if let Err(e) = stdout.write_all(data).await {
                        if verbose {
                            eprintln!("⚠️  Failed to write to stdout: {e}");
                        }
                        break;
                    }

                    // ログファイルに書き込み
                    if let Some(ref mut log_file) = log_writer {
                        if let Err(e) = log_file.write_all(data).await {
                            if verbose {
                                eprintln!("⚠️  Failed to write to log file: {e}");
                            }
                        }
                    }

                    // ターミナルサイズ変更チェック
                    let current_terminal_size = crate::cli_tool::get_pty_size();
                    if current_terminal_size.rows != last_terminal_size.rows
                        || current_terminal_size.cols != last_terminal_size.cols
                    {
                        if verbose {
                            eprintln!(
                                "🔄 Terminal size changed: {}x{} -> {}x{}",
                                last_terminal_size.cols,
                                last_terminal_size.rows,
                                current_terminal_size.cols,
                                current_terminal_size.rows
                            );
                        }
                        if let Ok(mut detector) = state_detector.lock() {
                            detector.resize_screen_buffer(
                                current_terminal_size.rows as usize,
                                current_terminal_size.cols as usize,
                            );
                        }
                        last_terminal_size = current_terminal_size;
                    }

                    // 状態検出器に出力を送信（内部状態更新のみ）
                    if let Ok(mut detector) = state_detector.lock() {
                        detector.process_output(&output_str);
                    }

                    // 出力をフラッシュ
                    let _ = stdout.flush().await;
                    if let Some(ref mut log_file) = log_writer {
                        let _ = log_file.flush().await;
                    }
                }
                Err(e) => {
                    if verbose {
                        eprintln!("⚠️  PTY read error: {e}");
                    }
                    state_checker_task.abort();
                    break;
                }
            }
        }
    }

    /// Stdin入力をPTYに転送
    async fn handle_stdin_to_pty_simple(
        mut pty_writer: Box<dyn std::io::Write + Send>,
        verbose: bool,
    ) {
        use std::io::Write;
        use tokio::io::AsyncReadExt;

        if verbose {
            eprintln!("📡 Starting stdin to PTY forwarding (raw mode already set by main)");
        }

        let mut stdin = tokio::io::stdin();
        let mut buffer = [0u8; 1024];

        loop {
            match stdin.read(&mut buffer).await {
                Ok(0) => {
                    if verbose {
                        eprintln!("📡 Stdin EOF");
                    }
                    break;
                }
                Ok(n) => {
                    let data = &buffer[..n];

                    if let Err(e) = pty_writer.write_all(data) {
                        if verbose {
                            eprintln!("⚠️  Failed to write to PTY: {e}");
                        }
                        break;
                    }

                    if let Err(e) = pty_writer.flush() {
                        if verbose {
                            eprintln!("⚠️  Failed to flush PTY: {e}");
                        }
                        break;
                    }
                }
                Err(e) => {
                    if verbose {
                        eprintln!("⚠️  Stdin read error: {e}");
                    }
                    break;
                }
            }
        }

        if verbose {
            eprintln!("📡 Stdin to PTY forwarding ended");
        }
    }

    /// 定期的な状態チェッカー（1秒ごと）
    async fn periodic_state_checker(
        state_detector: std::sync::Arc<
            std::sync::Mutex<Box<dyn crate::state_detector::StateDetector + Send>>,
        >,
        last_notified_status: std::sync::Arc<std::sync::Mutex<SessionStatus>>,
        launcher_id: String,
        session_id: String,
        connection_config: ConnectionConfig,
        verbose: bool,
    ) {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));

        loop {
            interval.tick().await;

            let current_status = {
                if let Ok(detector) = state_detector.lock() {
                    detector.current_state().clone()
                } else {
                    continue;
                }
            };

            let should_notify = {
                if let Ok(mut last_status) = last_notified_status.lock() {
                    if current_status != *last_status {
                        *last_status = current_status.clone();
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            };

            if should_notify {
                if verbose {
                    eprintln!("🔄 Periodic status update: {current_status:?}");
                }

                let ui_above_text = {
                    if let Ok(detector) = state_detector.lock() {
                        detector.get_ui_above_text()
                    } else {
                        None
                    }
                };

                Self::send_status_update_simple(
                    &launcher_id,
                    &session_id,
                    current_status,
                    ui_above_text,
                    &connection_config,
                    verbose,
                )
                .await;
            }
        }
    }

    /// 簡易状態更新送信
    async fn send_status_update_simple(
        launcher_id: &str,
        session_id: &str,
        status: SessionStatus,
        ui_above_text: Option<String>,
        connection_config: &ConnectionConfig,
        verbose: bool,
    ) {
        match connect_client(connection_config).await {
            Ok(mut connection) => {
                let update_msg = LauncherToMonitor::StateUpdate {
                    launcher_id: launcher_id.to_string(),
                    session_id: session_id.to_string(),
                    status: status.clone(),
                    ui_above_text,
                    timestamp: chrono::Utc::now(),
                };

                if let Ok(msg_bytes) = serde_json::to_vec(&update_msg) {
                    let _ = connection.write_all(&msg_bytes).await;
                    let _ = connection.write_all(b"\n").await;
                    let _ = connection.flush().await;

                    if verbose {
                        eprintln!("📤 Sent periodic status update: {status:?}");
                    }
                }
            }
            Err(_) => {
                if verbose {
                    eprintln!("⚠️  Failed to send periodic status update (monitor not available)");
                }
            }
        }
    }
}

// 公開インターフェース
pub use crate::launcher_client::{create_terminal_guard_global, force_restore_terminal, TerminalGuard};

// 新しいクライアントをLauncherClientとしてエクスポート
pub type LauncherClient = TransportLauncherClient;