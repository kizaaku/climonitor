use anyhow::Result;
use chrono::Utc;
use portable_pty::MasterPty;
use std::path::PathBuf;
use tokio::task::JoinHandle;

use crate::tool_wrapper::ToolWrapper;
use climonitor_shared::{
    generate_connection_id, transport::MessageSender, ConnectionConfig, LauncherToMonitor,
    SessionStatus,
};

/// PTY処理に必要な設定をまとめた構造体
#[derive(Debug, Clone)]
pub struct PtyConfig {
    pub launcher_id: String,
    pub session_id: String,
    pub verbose: bool,
    pub log_file: Option<PathBuf>,
    pub tool_type: crate::cli_tool::CliToolType,
    pub connection_config: ConnectionConfig,
    pub grpc_client: Option<crate::grpc_client::GrpcLauncherClient>,
}

/// PTY監視処理用の設定構造体
struct PtyMonitoringConfig {
    launcher_id: String,
    session_id: String,
    verbose: bool,
    tool_type: crate::cli_tool::CliToolType,
    connection_config: ConnectionConfig,
    grpc_client: Option<crate::grpc_client::GrpcLauncherClient>,
}

/// ダミーターミナルガード（main関数で実際のガードが作成済みの場合）
pub struct DummyTerminalGuard {
    #[allow(dead_code)]
    verbose: bool,
}

/// Transport対応 Launcher クライアント
pub struct TransportLauncherClient {
    launcher_id: String,
    message_sender: Option<Box<dyn MessageSender>>,
    grpc_client: Option<crate::grpc_client::GrpcLauncherClient>,
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
            message_sender: None,
            grpc_client: None,
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

    /// gRPC client付きで新しいTransportLauncherClientを作成
    pub async fn new_with_grpc(
        tool_wrapper: ToolWrapper,
        grpc_client: crate::grpc_client::GrpcLauncherClient,
        verbose: bool,
        log_file: Option<PathBuf>,
    ) -> Result<Self> {
        let launcher_id = grpc_client.get_launcher_id().to_string();
        let session_id = grpc_client.get_session_id().to_string();
        let project_name = tool_wrapper.guess_project_name();

        let client = Self {
            launcher_id,
            message_sender: None, // gRPCクライアントは別途管理
            grpc_client: Some(grpc_client),
            connection_config: climonitor_shared::ConnectionConfig::default_grpc(), // ダミー
            tool_wrapper,
            project_name,
            session_id,
            verbose,
            log_file,
        };

        // Note: gRPCクライアントは既に接続済みのため、接続試行は不要

        Ok(client)
    }

    /// Monitor サーバーへの接続を試行
    async fn try_connect_to_monitor(&mut self) -> Result<()> {
        if self.verbose {
            climonitor_shared::log_info!(
                climonitor_shared::LogCategory::Transport,
                "🔄 Attempting to connect to monitor server: {:?}",
                self.connection_config
            );
        }

        match crate::transports::create_message_sender(&self.connection_config).await {
            Ok(sender) => {
                self.message_sender = Some(sender);
                if self.verbose {
                    climonitor_shared::log_info!(
                        climonitor_shared::LogCategory::Transport,
                        "🔗 Connected to monitor server"
                    );
                }
            }
            Err(e) => {
                if self.verbose {
                    climonitor_shared::log_warn!(
                        climonitor_shared::LogCategory::Transport,
                        "⚠️  Failed to connect to monitor server: {e}. Running without monitoring."
                    );
                }
            }
        }

        Ok(())
    }

    /// Monitor サーバーに接続されているかチェック
    pub fn is_connected(&self) -> bool {
        self.message_sender.is_some() || self.grpc_client.is_some()
    }

    /// 接続メッセージを送信
    async fn send_connect_message(&mut self) -> Result<()> {
        let _connect_msg = LauncherToMonitor::Connect {
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

        if let Some(ref grpc_client) = self.grpc_client {
            if self.verbose {
                climonitor_shared::log_debug!(
                    climonitor_shared::LogCategory::Grpc,
                    "📤 Sending gRPC connect message: launcher_id={}, project={:?}",
                    self.launcher_id,
                    self.project_name
                );
            }
            grpc_client
                .send_connect(
                    self.project_name.clone(),
                    self.tool_wrapper.get_tool_type(),
                    self.tool_wrapper.get_args().to_vec(),
                    self.tool_wrapper
                        .get_working_dir()
                        .cloned()
                        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default()),
                )
                .await?;
            if self.verbose {
                climonitor_shared::log_debug!(
                    climonitor_shared::LogCategory::Grpc,
                    "✅ gRPC connect message sent successfully"
                );
            }
        } else if let Some(ref sender) = self.message_sender {
            if self.verbose {
                climonitor_shared::log_debug!(
                    climonitor_shared::LogCategory::Transport,
                    "📤 Sending connect message: launcher_id={}, project={:?}",
                    self.launcher_id,
                    self.project_name
                );
            }
            sender
                .send_connect(
                    self.project_name.clone(),
                    self.tool_wrapper.get_tool_type(),
                    self.tool_wrapper.get_args().to_vec(),
                    self.tool_wrapper
                        .get_working_dir()
                        .cloned()
                        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default()),
                )
                .await?;
            if self.verbose {
                climonitor_shared::log_debug!(
                    climonitor_shared::LogCategory::Transport,
                    "✅ Connect message sent successfully"
                );
            }
        } else if self.verbose {
            climonitor_shared::log_warn!(
                climonitor_shared::LogCategory::Transport,
                "⚠️  No connection available for sending connect message"
            );
        }
        Ok(())
    }

    /// 切断メッセージを送信
    async fn send_disconnect_message(&mut self) -> Result<()> {
        if let Some(ref grpc_client) = self.grpc_client {
            grpc_client.send_disconnect().await?;
            if self.verbose {
                climonitor_shared::log_debug!(
                    climonitor_shared::LogCategory::Grpc,
                    "📤 Sent gRPC disconnect message to monitor"
                );
            }
        } else if let Some(ref sender) = self.message_sender {
            sender.send_disconnect(self.session_id.clone()).await?;
            if self.verbose {
                climonitor_shared::log_debug!(
                    climonitor_shared::LogCategory::Transport,
                    "📤 Sent disconnect message to monitor"
                );
            }
        }
        Ok(())
    }

    /// 状態更新メッセージを送信
    pub async fn send_state_update(&self, status: SessionStatus, message: String) -> Result<()> {
        if let Some(ref grpc_client) = self.grpc_client {
            if self.verbose {
                climonitor_shared::log_debug!(
                    climonitor_shared::LogCategory::Grpc,
                    "📤 Sending gRPC state update: {status:?}"
                );
            }
            grpc_client.send_state_update(status, Some(message)).await?;
        } else if let Some(ref sender) = self.message_sender {
            if self.verbose {
                climonitor_shared::log_debug!(
                    climonitor_shared::LogCategory::Transport,
                    "📤 Sending state update: {status:?}"
                );
            }
            sender
                .send_status_update(
                    self.session_id.clone(),
                    status,
                    Utc::now(),
                    self.project_name.clone(),
                )
                .await?;
        }
        Ok(())
    }

    /// コンテキスト更新メッセージを送信
    pub async fn send_context_update(&self, ui_above_text: String) -> Result<()> {
        if let Some(ref grpc_client) = self.grpc_client {
            if self.verbose {
                climonitor_shared::log_debug!(
                    climonitor_shared::LogCategory::Grpc,
                    "📤 Sending gRPC context update"
                );
            }
            grpc_client.send_context_update(Some(ui_above_text)).await?;
        } else if let Some(ref sender) = self.message_sender {
            if self.verbose {
                climonitor_shared::log_debug!(
                    climonitor_shared::LogCategory::Transport,
                    "📤 Sending context update"
                );
            }
            sender
                .send_context_update(self.session_id.clone(), ui_above_text, Utc::now())
                .await?;
        }
        Ok(())
    }

    /// Claude プロセス起動・監視
    pub async fn run_claude(&mut self) -> Result<()> {
        if self.verbose {
            climonitor_shared::log_info!(
                climonitor_shared::LogCategory::System,
                "🚀 Starting CLI tool: {}",
                self.tool_wrapper.to_command_string()
            );
        }

        // Monitor に接続できていない場合は単純にClaude実行
        if !self.is_connected() {
            if self.verbose {
                climonitor_shared::log_info!(
                    climonitor_shared::LogCategory::System,
                    "🔄 Running CLI tool without monitoring (monitor not connected)"
                );
            }
            return self.tool_wrapper.run_directly().await;
        }

        // 接続メッセージを送信
        if let Err(e) = self.send_connect_message().await {
            if self.verbose {
                climonitor_shared::log_warn!(
                    climonitor_shared::LogCategory::Transport,
                    "⚠️  Failed to send connect message: {e}"
                );
            }
        } else if self.verbose {
            climonitor_shared::log_debug!(
                climonitor_shared::LogCategory::Transport,
                "✅ Connect message sent successfully"
            );
        }

        // ターミナルガードはmain関数で作成済み（ここでは作らない）
        let terminal_guard = DummyTerminalGuard {
            verbose: self.verbose,
        };

        // CLI ツール プロセス起動（全プラットフォームでPTYを使用）
        let (mut process, io_handle) = {
            let (process, pty_master) = self.tool_wrapper.spawn_with_pty()?;
            let pty_handle = self
                .start_pty_bidirectional_io(pty_master, terminal_guard)
                .await?;
            (process, pty_handle)
        };

        if self.verbose {
            climonitor_shared::log_info!(
                climonitor_shared::LogCategory::System,
                "👀 Monitoring started for CLI tool process"
            );
        }

        // CLI ツール プロセスの終了を待つタスクを一度だけ起動
        let mut wait_task = tokio::task::spawn_blocking(move || process.wait());

        // シグナルハンドリングとリサイズ処理
        let exit_status = self.wait_with_signals(&mut wait_task).await;

        // I/Oタスクを終了
        io_handle.abort();

        // 少し待機してI/Oが完了するのを待つ
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        match exit_status {
            Ok(status) => {
                if self.verbose {
                    climonitor_shared::log_info!(
                        climonitor_shared::LogCategory::System,
                        "🏁 CLI tool process exited with status: {status:?}"
                    );
                }
            }
            Err(e) => {
                if self.verbose {
                    climonitor_shared::log_error!(
                        climonitor_shared::LogCategory::System,
                        "❌ CLI tool execution failed: {e}"
                    );
                }
                // エラー時でも切断メッセージを送信
                if let Err(disconnect_err) = self.send_disconnect_message().await {
                    if self.verbose {
                        climonitor_shared::log_warn!(
                            climonitor_shared::LogCategory::Transport,
                            "⚠️  Failed to send disconnect message: {disconnect_err}"
                        );
                    }
                }
                // 接続を明示的に閉じる
                if let Some(sender) = self.message_sender.take() {
                    drop(sender);
                    if self.verbose {
                        climonitor_shared::log_debug!(
                            climonitor_shared::LogCategory::Transport,
                            "🔌 Connection closed (after error)"
                        );
                    }
                }
                return Err(e);
            }
        }

        // 切断メッセージ送信
        self.send_disconnect_message().await?;

        // 接続を明示的に閉じる
        if let Some(sender) = self.message_sender.take() {
            drop(sender);
            if self.verbose {
                climonitor_shared::log_debug!(
                    climonitor_shared::LogCategory::Transport,
                    "🔌 Connection closed"
                );
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
        let connection_config = self.connection_config.clone();
        let grpc_client = self.grpc_client.clone();

        // PTYのリサイズ機能を有効にするため、Arc<Mutex<>>でラップ
        let pty_master_shared = std::sync::Arc::new(std::sync::Mutex::new(pty_master));

        let handle = tokio::spawn(async move {
            let config = PtyConfig {
                launcher_id,
                session_id,
                verbose,
                log_file,
                tool_type,
                connection_config,
                grpc_client,
            };
            Self::handle_pty_bidirectional_io(pty_master_shared, config, _terminal_guard).await;
        });

        Ok(handle)
    }

    /// PTY 双方向I/O処理
    async fn handle_pty_bidirectional_io(
        pty_master: std::sync::Arc<std::sync::Mutex<Box<dyn MasterPty + Send>>>,
        config: PtyConfig,
        _terminal_guard: DummyTerminalGuard,
    ) {
        // ログファイルを開く
        let log_writer = if let Some(ref log_path) = config.log_file {
            match tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_path)
                .await
            {
                Ok(file) => Some(file),
                Err(e) => {
                    if config.verbose {
                        let log_display = log_path.display();
                        climonitor_shared::log_warn!(
                            climonitor_shared::LogCategory::System,
                            "⚠️  Failed to open log file {}: {e}",
                            log_display
                        );
                    }
                    None
                }
            }
        } else {
            None
        };

        // PTY writer/reader を取得
        let (pty_writer, pty_reader) = {
            if let Ok(master) = pty_master.lock() {
                let writer = match master.take_writer() {
                    Ok(writer) => writer,
                    Err(e) => {
                        if config.verbose {
                            climonitor_shared::log_warn!(
                                climonitor_shared::LogCategory::System,
                                "⚠️  Failed to get PTY writer: {e}"
                            );
                        }
                        return;
                    }
                };

                let reader = match master.try_clone_reader() {
                    Ok(reader) => reader,
                    Err(e) => {
                        if config.verbose {
                            climonitor_shared::log_warn!(
                                climonitor_shared::LogCategory::System,
                                "⚠️  Failed to get PTY reader: {e}"
                            );
                        }
                        return;
                    }
                };

                (writer, reader)
            } else {
                if config.verbose {
                    climonitor_shared::log_warn!(
                        climonitor_shared::LogCategory::System,
                        "⚠️  Failed to lock PTY master"
                    );
                }
                return;
            }
        };

        // 設定値を事前にコピー（move クロージャで使用するため）
        let config_clone = config.clone();

        // 双方向I/Oタスクを起動
        let pty_master_for_resize = pty_master.clone();
        let monitoring_config = PtyMonitoringConfig {
            launcher_id: config_clone.launcher_id.clone(),
            session_id: config_clone.session_id.clone(),
            verbose: config_clone.verbose,
            tool_type: config_clone.tool_type,
            connection_config: config_clone.connection_config,
            grpc_client: config_clone.grpc_client.clone(),
        };
        let mut pty_to_stdout = tokio::spawn(async move {
            Self::handle_pty_to_stdout_with_monitoring(
                pty_reader,
                log_writer,
                monitoring_config,
                pty_master_for_resize,
            )
            .await;
        });

        let mut stdin_to_pty = tokio::spawn(async move {
            Self::handle_stdin_to_pty_simple(pty_writer, config.verbose).await;
        });

        // タスクの完了を待つ
        tokio::select! {
            _ = &mut pty_to_stdout => {
                if config.verbose {
                    climonitor_shared::log_debug!(climonitor_shared::LogCategory::System, "📡 PTY to stdout task ended");
                }
                stdin_to_pty.abort();
            }
            _ = &mut stdin_to_pty => {
                if config.verbose {
                    climonitor_shared::log_debug!(climonitor_shared::LogCategory::System, "📡 Stdin to PTY task ended");
                }
                pty_to_stdout.abort();
            }
        }
    }

    /// プロセス終了とシグナルを待機
    async fn wait_with_signals(
        &self,
        wait_task: &mut tokio::task::JoinHandle<std::io::Result<portable_pty::ExitStatus>>,
    ) -> Result<portable_pty::ExitStatus> {
        #[cfg(unix)]
        {
            let mut sigwinch =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::window_change())
                    .unwrap();
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
                            climonitor_shared::log_info!(climonitor_shared::LogCategory::System, "🛑 Received SIGINT, letting CLI tool handle it...");
                        }
                    }
                    _ = sigterm.recv() => {
                        if self.verbose {
                            climonitor_shared::log_info!(climonitor_shared::LogCategory::System, "🛑 Received SIGTERM, shutting down gracefully...");
                        }
                        return Err(anyhow::anyhow!("Terminated by signal"));
                    }
                    _ = sigwinch.recv() => {
                        if self.verbose {
                            climonitor_shared::log_debug!(climonitor_shared::LogCategory::System, "🔄 Terminal resized - updating PTY size...");
                        }
                        let new_size = crate::cli_tool::get_pty_size();
                        if self.verbose {
                            let cols = new_size.cols;
                            let rows = new_size.rows;
                            climonitor_shared::log_debug!(climonitor_shared::LogCategory::System, "📏 New terminal size: {cols}x{rows}");
                        }
                    }
                }
            }
        }

        #[cfg(not(unix))]
        {
            loop {
                tokio::select! {
                    result = &mut *wait_task => {
                        return result?.map_err(|e| anyhow::anyhow!("Process wait error: {}", e));
                    }
                    _ = tokio::signal::ctrl_c() => {
                        if self.verbose {
                            climonitor_shared::log_info!(climonitor_shared::LogCategory::System, "🛑 Received Ctrl+C, letting CLI tool handle it...");
                        }
                    }
                }
            }
        }
    }

    /// PTY出力をstdoutに転送（監視・ログ付き）
    async fn handle_pty_to_stdout_with_monitoring(
        mut pty_reader: Box<dyn std::io::Read + Send>,
        mut log_writer: Option<tokio::fs::File>,
        config: PtyMonitoringConfig,
        pty_master: std::sync::Arc<std::sync::Mutex<Box<dyn MasterPty + Send>>>,
    ) {
        use crate::state_detector::create_state_detector;
        use climonitor_shared::SessionStatus;

        let state_detector: std::sync::Arc<
            std::sync::Mutex<Box<dyn crate::state_detector::StateDetector + Send>>,
        > = std::sync::Arc::new(std::sync::Mutex::new(create_state_detector(
            config.tool_type,
            config.verbose,
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
            let launcher_id_clone = config.launcher_id.clone();
            let session_id_clone = config.session_id.clone();
            let config_clone = config.connection_config.clone();
            let grpc_client_clone = config.grpc_client.clone();
            let verbose = config.verbose;

            tokio::spawn(async move {
                Self::periodic_state_checker(
                    state_detector_clone,
                    last_notified_status_clone,
                    launcher_id_clone,
                    session_id_clone,
                    config_clone,
                    grpc_client_clone,
                    verbose,
                )
                .await;
            })
        };

        loop {
            match pty_reader.read(&mut buffer) {
                Ok(0) => {
                    if config.verbose {
                        climonitor_shared::log_debug!(
                            climonitor_shared::LogCategory::System,
                            "📡 PTY reader EOF"
                        );
                    }
                    state_checker_task.abort();
                    break;
                }
                Ok(n) => {
                    let data = &buffer[..n];
                    let output_str = String::from_utf8_lossy(data);

                    // 標準出力に書き込み
                    if let Err(e) = stdout.write_all(data).await {
                        if config.verbose {
                            climonitor_shared::log_warn!(
                                climonitor_shared::LogCategory::System,
                                "⚠️  Failed to write to stdout: {e}"
                            );
                        }
                        break;
                    }

                    // ログファイルに書き込み
                    if let Some(ref mut log_file) = log_writer {
                        if let Err(e) = log_file.write_all(data).await {
                            if config.verbose {
                                climonitor_shared::log_warn!(
                                    climonitor_shared::LogCategory::System,
                                    "⚠️  Failed to write to log file: {e}"
                                );
                            }
                        }
                    }

                    // ターミナルサイズ変更チェック
                    let current_terminal_size = crate::cli_tool::get_pty_size();
                    if current_terminal_size.rows != last_terminal_size.rows
                        || current_terminal_size.cols != last_terminal_size.cols
                    {
                        if config.verbose {
                            climonitor_shared::log_debug!(
                                climonitor_shared::LogCategory::System,
                                "🔄 Terminal size changed: {}x{} -> {}x{}",
                                last_terminal_size.cols,
                                last_terminal_size.rows,
                                current_terminal_size.cols,
                                current_terminal_size.rows
                            );
                        }

                        // PTYマスターのサイズを更新
                        if let Ok(master) = pty_master.lock() {
                            if let Err(e) = master.resize(current_terminal_size) {
                                if config.verbose {
                                    climonitor_shared::log_warn!(
                                        climonitor_shared::LogCategory::System,
                                        "⚠️  Failed to resize PTY: {e}"
                                    );
                                }
                            } else if config.verbose {
                                climonitor_shared::log_debug!(
                                    climonitor_shared::LogCategory::System,
                                    "✅ PTY resized successfully"
                                );
                            }
                        }

                        // 状態検出器のバッファも更新
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
                    if config.verbose {
                        climonitor_shared::log_warn!(
                            climonitor_shared::LogCategory::System,
                            "⚠️  PTY read error: {e}"
                        );
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
            climonitor_shared::log_debug!(
                climonitor_shared::LogCategory::System,
                "📡 Starting stdin to PTY forwarding (raw mode already set by main)"
            );
        }

        let mut stdin = tokio::io::stdin();
        let mut buffer = [0u8; 1024];

        loop {
            match stdin.read(&mut buffer).await {
                Ok(0) => {
                    if verbose {
                        climonitor_shared::log_debug!(
                            climonitor_shared::LogCategory::System,
                            "📡 Stdin EOF"
                        );
                    }
                    break;
                }
                Ok(n) => {
                    let data = &buffer[..n];

                    if let Err(e) = pty_writer.write_all(data) {
                        if verbose {
                            climonitor_shared::log_warn!(
                                climonitor_shared::LogCategory::System,
                                "⚠️  Failed to write to PTY: {e}"
                            );
                        }
                        break;
                    }

                    if let Err(e) = pty_writer.flush() {
                        if verbose {
                            climonitor_shared::log_warn!(
                                climonitor_shared::LogCategory::System,
                                "⚠️  Failed to flush PTY: {e}"
                            );
                        }
                        break;
                    }
                }
                Err(e) => {
                    if verbose {
                        climonitor_shared::log_warn!(
                            climonitor_shared::LogCategory::System,
                            "⚠️  Stdin read error: {e}"
                        );
                    }
                    break;
                }
            }
        }

        if verbose {
            climonitor_shared::log_debug!(
                climonitor_shared::LogCategory::System,
                "📡 Stdin to PTY forwarding ended"
            );
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
        grpc_client: Option<crate::grpc_client::GrpcLauncherClient>,
        verbose: bool,
    ) {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        let mut last_ui_context: Option<String> = None;

        loop {
            interval.tick().await;

            let (current_status, current_ui_context) = {
                if let Ok(detector) = state_detector.lock() {
                    (
                        detector.current_state().clone(),
                        detector.get_ui_above_text(),
                    )
                } else {
                    continue;
                }
            };

            let (_status_changed, should_notify_status) = {
                if let Ok(mut last_status) = last_notified_status.lock() {
                    if current_status != *last_status {
                        // Connected→Idle の直接遷移を防ぐ
                        if *last_status == SessionStatus::Connected
                            && current_status == SessionStatus::Idle
                        {
                            if verbose {
                                climonitor_shared::log_debug!(climonitor_shared::LogCategory::Session, "🔒 [STATE_TRANSITION] Blocked Connected→Idle transition, keeping Connected");
                            }
                            (false, false) // 状態変化を通知しない（Connected状態を維持）
                        } else {
                            *last_status = current_status.clone();
                            (true, true)
                        }
                    } else {
                        (false, false)
                    }
                } else {
                    (false, false)
                }
            };

            let context_changed = current_ui_context != last_ui_context;

            // 状態変化時はStateUpdate、コンテキスト変化のみの場合はContextUpdate
            if should_notify_status {
                if verbose {
                    climonitor_shared::log_debug!(
                        climonitor_shared::LogCategory::Session,
                        "🔄 Periodic status update: {current_status:?}"
                    );
                }

                if let Err(e) = Self::send_periodic_status_update(
                    current_status,
                    current_ui_context.clone(),
                    &connection_config,
                    grpc_client.as_ref(),
                    &launcher_id,
                    &session_id,
                    verbose,
                )
                .await
                {
                    if verbose {
                        climonitor_shared::log_warn!(
                            climonitor_shared::LogCategory::Transport,
                            "⚠️  Failed to send periodic status update: {e}"
                        );
                    }
                }

                last_ui_context = current_ui_context;
            } else if context_changed {
                if verbose {
                    climonitor_shared::log_debug!(
                        climonitor_shared::LogCategory::Session,
                        "🔄 Context update: {current_ui_context:?}"
                    );
                }

                if let Err(e) = Self::send_periodic_context_update(
                    current_ui_context.clone(),
                    &connection_config,
                    grpc_client.as_ref(),
                    &launcher_id,
                    &session_id,
                    verbose,
                )
                .await
                {
                    if verbose {
                        climonitor_shared::log_warn!(
                            climonitor_shared::LogCategory::Transport,
                            "⚠️  Failed to send periodic context update: {e}"
                        );
                    }
                }

                last_ui_context = current_ui_context;
            }
        }
    }

    /// 定期的な状態更新送信
    async fn send_periodic_status_update(
        status: SessionStatus,
        ui_above_text: Option<String>,
        connection_config: &ConnectionConfig,
        grpc_client: Option<&crate::grpc_client::GrpcLauncherClient>,
        _launcher_id: &str,
        session_id: &str,
        verbose: bool,
    ) -> Result<()> {
        if let Some(grpc_client) = grpc_client {
            if verbose {
                climonitor_shared::log_debug!(
                    climonitor_shared::LogCategory::Grpc,
                    "📤 Sent gRPC periodic status update: {status:?}"
                );
            }
            grpc_client.send_state_update(status, ui_above_text).await?;
        } else {
            // Create a temporary sender with the same launcher_id
            if let Ok(sender) = crate::transports::create_message_sender_with_id(
                connection_config,
                _launcher_id.to_string(),
            )
            .await
            {
                sender
                    .send_status_update(session_id.to_string(), status, Utc::now(), None)
                    .await?;

                // Send context update separately if ui_above_text is provided
                if let Some(ui_text) = ui_above_text {
                    sender
                        .send_context_update(session_id.to_string(), ui_text, Utc::now())
                        .await?;
                }
            }
        }
        Ok(())
    }

    /// 定期的なコンテキスト更新送信
    async fn send_periodic_context_update(
        ui_above_text: Option<String>,
        connection_config: &ConnectionConfig,
        grpc_client: Option<&crate::grpc_client::GrpcLauncherClient>,
        _launcher_id: &str,
        session_id: &str,
        verbose: bool,
    ) -> Result<()> {
        if let Some(grpc_client) = grpc_client {
            grpc_client.send_context_update(ui_above_text).await?;
            if verbose {
                climonitor_shared::log_debug!(
                    climonitor_shared::LogCategory::Grpc,
                    "📤 Sent gRPC context update"
                );
            }
        } else {
            // Create a temporary sender with the same launcher_id
            if let Ok(sender) = crate::transports::create_message_sender_with_id(
                connection_config,
                _launcher_id.to_string(),
            )
            .await
            {
                sender
                    .send_context_update(
                        session_id.to_string(),
                        ui_above_text.unwrap_or_default(),
                        Utc::now(),
                    )
                    .await?;
            }
        }
        Ok(())
    }

    // This method is no longer needed as we use the trait-based MessageSender API
}

// Drop実装を削除し、明示的な切断処理に依存
// （run_claude関数内で既に適切に切断メッセージが送信されている）

/// ターミナル状態の自動復元ガード（クロスプラットフォーム対応）
pub struct TerminalGuard {
    verbose: bool,
    #[cfg(unix)]
    fd: i32,
    #[cfg(unix)]
    original: nix::sys::termios::Termios,
    #[cfg(windows)]
    original_mode: Option<u32>,
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::fd::BorrowedFd;

            // ターミナルかどうかチェック
            if !nix::unistd::isatty(self.fd).unwrap_or(false) {
                if self.verbose {
                    climonitor_shared::log_debug!(
                        climonitor_shared::LogCategory::System,
                        "🔓 Terminal guard dropped (non-TTY)"
                    );
                }
                return;
            }

            if self.verbose {
                climonitor_shared::log_debug!(
                    climonitor_shared::LogCategory::System,
                    "🔓 Restoring terminal settings"
                );
            }

            // SAFETY: fd は有効なファイルディスクリプタです
            let borrowed_fd = unsafe { BorrowedFd::borrow_raw(self.fd) };

            if let Err(e) = nix::sys::termios::tcsetattr(
                borrowed_fd,
                nix::sys::termios::SetArg::TCSANOW,
                &self.original,
            ) {
                if self.verbose {
                    climonitor_shared::log_warn!(
                        climonitor_shared::LogCategory::System,
                        "⚠️  Failed to restore terminal: {e}"
                    );
                }
            }
        }

        #[cfg(windows)]
        {
            use std::ptr;
            use winapi::um::consoleapi::SetConsoleMode;
            use winapi::um::processenv::GetStdHandle;
            use winapi::um::winbase::STD_INPUT_HANDLE;

            unsafe {
                let stdin_handle = GetStdHandle(STD_INPUT_HANDLE);
                if stdin_handle != ptr::null_mut() {
                    if let Some(original_mode) = self.original_mode {
                        // 元のコンソールモードを正確に復元
                        if SetConsoleMode(stdin_handle, original_mode) != 0 {
                            if self.verbose {
                                climonitor_shared::log_debug!(
                                    climonitor_shared::LogCategory::System,
                                    "🔓 Windows console mode restored to original (0x{:x})",
                                    original_mode
                                );
                            }
                        } else if self.verbose {
                            climonitor_shared::log_warn!(
                                climonitor_shared::LogCategory::System,
                                "⚠️  Failed to restore original Windows console mode (0x{:x})",
                                original_mode
                            );
                        }
                    } else if self.verbose {
                        climonitor_shared::log_warn!(
                            climonitor_shared::LogCategory::System,
                            "⚠️  No original console mode to restore"
                        );
                    }
                }
            }
        }

        if self.verbose {
            climonitor_shared::log_debug!(
                climonitor_shared::LogCategory::System,
                "🔓 Terminal guard dropped"
            );
        }
    }
}

/// グローバル用のターミナルガード作成関数（クロスプラットフォーム対応）
pub fn create_terminal_guard_global(verbose: bool) -> anyhow::Result<TerminalGuard> {
    #[cfg(unix)]
    {
        use std::os::fd::BorrowedFd;
        use std::os::unix::io::AsRawFd;

        let stdin_fd = std::io::stdin().as_raw_fd();

        // stdinがターミナルかどうかチェック
        if !nix::unistd::isatty(stdin_fd).unwrap_or(false) {
            if verbose {
                climonitor_shared::log_debug!(
                    climonitor_shared::LogCategory::System,
                    "🔒 Terminal guard created (non-TTY mode)"
                );
            }
            // ターミナルでない場合は何もしない（ダミーのTermiosを作成）
            let dummy_termios = unsafe { std::mem::zeroed() };
            return Ok(TerminalGuard {
                verbose,
                #[cfg(unix)]
                fd: stdin_fd,
                #[cfg(unix)]
                original: dummy_termios,
                #[cfg(windows)]
                original_mode: None,
            });
        }

        // SAFETY: stdin_fd は有効なファイルディスクリプタです
        let borrowed_fd = unsafe { BorrowedFd::borrow_raw(stdin_fd) };

        let original_termios = nix::sys::termios::tcgetattr(borrowed_fd)
            .map_err(|e| anyhow::anyhow!("Failed to get terminal attributes: {}", e))?;

        // ターミナルをrawモードに設定
        let mut raw_termios = original_termios.clone();
        nix::sys::termios::cfmakeraw(&mut raw_termios);
        nix::sys::termios::tcsetattr(
            borrowed_fd,
            nix::sys::termios::SetArg::TCSANOW,
            &raw_termios,
        )
        .map_err(|e| anyhow::anyhow!("Failed to set raw mode: {}", e))?;

        if verbose {
            climonitor_shared::log_debug!(
                climonitor_shared::LogCategory::System,
                "🔒 Terminal guard created with raw mode"
            );
        }

        Ok(TerminalGuard {
            verbose,
            #[cfg(unix)]
            fd: stdin_fd,
            #[cfg(unix)]
            original: original_termios,
            #[cfg(windows)]
            original_mode: None,
        })
    }

    #[cfg(windows)]
    {
        use std::ptr;
        use winapi::um::consoleapi::{GetConsoleMode, SetConsoleMode};
        use winapi::um::processenv::GetStdHandle;
        use winapi::um::winbase::STD_INPUT_HANDLE;
        use winapi::um::wincon::{
            ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT,
            ENABLE_VIRTUAL_TERMINAL_INPUT,
        };

        unsafe {
            let stdin_handle = GetStdHandle(STD_INPUT_HANDLE);
            if stdin_handle == ptr::null_mut() {
                return Ok(TerminalGuard {
                    verbose,
                    original_mode: None,
                });
            }

            let mut original_mode = 0;
            if GetConsoleMode(stdin_handle, &mut original_mode) == 0 {
                return Ok(TerminalGuard {
                    verbose,
                    original_mode: None,
                });
            }

            // Raw modeに設定（エコーとライン入力、プロセス処理を無効化してCLIツールに信号を委譲）
            let new_mode = original_mode
                & !ENABLE_ECHO_INPUT
                & !ENABLE_LINE_INPUT
                & !ENABLE_PROCESSED_INPUT  // Ctrl+Cなどの信号をCLIツールに委譲
                | ENABLE_VIRTUAL_TERMINAL_INPUT;

            if SetConsoleMode(stdin_handle, new_mode) == 0 {
                if verbose {
                    climonitor_shared::log_warn!(
                        climonitor_shared::LogCategory::System,
                        "⚠️  Failed to set Windows console raw mode"
                    );
                }
                return Ok(TerminalGuard {
                    verbose,
                    original_mode: Some(original_mode),
                });
            }

            if verbose {
                climonitor_shared::log_debug!(
                    climonitor_shared::LogCategory::System,
                    "🔒 Windows console raw mode enabled (original: 0x{:x})",
                    original_mode
                );
            }

            Ok(TerminalGuard {
                verbose,
                original_mode: Some(original_mode),
            })
        }
    }

    #[cfg(not(any(unix, windows)))]
    {
        // 他のプラットフォーム（将来的なサポート）
        Ok(TerminalGuard { verbose })
    }
}

/// 強制的にターミナル設定を復元する関数（緊急時用）
#[cfg(unix)]
pub fn force_restore_terminal() {
    use std::os::fd::BorrowedFd;
    use std::os::unix::io::AsRawFd;

    let stdin_fd = std::io::stdin().as_raw_fd();

    // ターミナルかどうかチェック
    if !nix::unistd::isatty(stdin_fd).unwrap_or(false) {
        return;
    }

    // SAFETY: stdin_fd は有効なファイルディスクリプタです
    let borrowed_fd = unsafe { BorrowedFd::borrow_raw(stdin_fd) };

    // 現在の設定を取得して、rawモードを解除
    if let Ok(mut termios) = nix::sys::termios::tcgetattr(borrowed_fd) {
        // rawモードを解除
        termios.input_flags |=
            nix::sys::termios::InputFlags::ICRNL | nix::sys::termios::InputFlags::IXON;
        termios.output_flags |= nix::sys::termios::OutputFlags::OPOST;
        termios.local_flags |= nix::sys::termios::LocalFlags::ECHO
            | nix::sys::termios::LocalFlags::ECHONL
            | nix::sys::termios::LocalFlags::ICANON
            | nix::sys::termios::LocalFlags::ISIG
            | nix::sys::termios::LocalFlags::IEXTEN;
        termios.control_flags |= nix::sys::termios::ControlFlags::CREAD;

        let _ =
            nix::sys::termios::tcsetattr(borrowed_fd, nix::sys::termios::SetArg::TCSANOW, &termios);
    }
}

#[cfg(not(unix))]
pub fn force_restore_terminal() {
    #[cfg(windows)]
    {
        use std::ptr;
        use winapi::um::consoleapi::SetConsoleMode;
        use winapi::um::processenv::GetStdHandle;
        use winapi::um::winbase::STD_INPUT_HANDLE;
        use winapi::um::wincon::{ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT};

        unsafe {
            let stdin_handle = GetStdHandle(STD_INPUT_HANDLE);
            if stdin_handle != ptr::null_mut() {
                // 標準的なコンソールモードに強制復元
                let default_mode = ENABLE_ECHO_INPUT | ENABLE_LINE_INPUT | ENABLE_PROCESSED_INPUT;
                let _ = SetConsoleMode(stdin_handle, default_mode);
                climonitor_shared::log_debug!(
                    climonitor_shared::LogCategory::System,
                    "🔓 Force restored Windows console to default mode (0x{:x})",
                    default_mode
                );
            }
        }
    }
}

// 新しいクライアントをLauncherClientとしてエクスポート
impl Drop for TransportLauncherClient {
    fn drop(&mut self) {
        // Drop時に同期的に切断メッセージを送信することは困難なため、
        // 主にログ出力とクリーンアップに集中
        if self.verbose && (self.message_sender.is_some() || self.grpc_client.is_some()) {
            climonitor_shared::log_debug!(
                climonitor_shared::LogCategory::System,
                "📤 TransportLauncherClient dropping - connection cleanup"
            );
        }

        // 注意: 実際の切断メッセージ送信は run_claude() の終了時に行われる
        // または main関数のシグナルハンドリングで明示的に呼び出される
    }
}

pub type LauncherClient = TransportLauncherClient;
