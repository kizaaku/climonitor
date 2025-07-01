// launcher_client.rs の修正箇所

use crate::state_detector::StateDetector;
use anyhow::Result;
use chrono::Utc;
use portable_pty::MasterPty;
use serde_json;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;
use tokio::task::JoinHandle;

use crate::tool_wrapper::ToolWrapper;
use ccmonitor_shared::{generate_connection_id, LauncherToMonitor, SessionStatus};

/// ターミナル状態の自動復元ガード
#[cfg(unix)]
pub struct TerminalGuard {
    fd: i32,
    original: nix::sys::termios::Termios,
    verbose: bool,
}

#[cfg(not(unix))]
pub struct TerminalGuard {
    verbose: bool,
}

/// ダミーターミナルガード（main関数で実際のガードが作成済みの場合）
pub struct DummyTerminalGuard {
    #[allow(dead_code)]
    verbose: bool,
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::fd::BorrowedFd;

            // ターミナルかどうかチェック
            if !nix::unistd::isatty(self.fd).unwrap_or(false) {
                if self.verbose {
                    eprintln!("🔓 Terminal guard dropped (non-TTY)");
                }
                return;
            }

            if self.verbose {
                eprintln!("🔓 Restoring terminal settings");
            }

            // SAFETY: fd は有効なファイルディスクリプタです
            let borrowed_fd = unsafe { BorrowedFd::borrow_raw(self.fd) };

            if let Err(e) = nix::sys::termios::tcsetattr(
                borrowed_fd,
                nix::sys::termios::SetArg::TCSANOW,
                &self.original,
            ) {
                if self.verbose {
                    eprintln!("⚠️  Failed to restore terminal: {}", e);
                }
            }
        }

        #[cfg(not(unix))]
        {
            if self.verbose {
                eprintln!("🔓 Terminal guard dropped (no-op on non-Unix)");
            }
        }
    }
}

/// Launcher クライアント
pub struct LauncherClient {
    launcher_id: String,
    socket_stream: Option<UnixStream>,
    tool_wrapper: ToolWrapper,
    project_name: Option<String>,
    session_id: String,
    verbose: bool,
    log_file: Option<PathBuf>,
}

impl LauncherClient {
    /// 新しいLauncherClientを作成
    pub async fn new(
        tool_wrapper: ToolWrapper,
        socket_path: Option<std::path::PathBuf>,
        verbose: bool,
        log_file: Option<PathBuf>,
    ) -> Result<Self> {
        let launcher_id = generate_connection_id();
        let session_id = generate_connection_id();
        let project_name = tool_wrapper.guess_project_name();

        let mut client = Self {
            launcher_id,
            socket_stream: None,
            tool_wrapper,
            project_name,
            session_id,
            verbose,
            log_file,
        };

        // Monitor サーバーに接続を試行
        client.try_connect_to_monitor(socket_path).await?;

        Ok(client)
    }

    /// Monitor サーバーへの接続を試行
    async fn try_connect_to_monitor(&mut self, socket_path: Option<PathBuf>) -> Result<()> {
        let socket_path = socket_path.unwrap_or_else(|| {
            std::env::var("CCMONITOR_SOCKET_PATH")
                .unwrap_or_else(|_| {
                    std::env::temp_dir()
                        .join("ccmonitor.sock")
                        .to_string_lossy()
                        .to_string()
                })
                .into()
        });

        // Monitor サーバーに接続（失敗しても続行）
        if self.verbose {
            eprintln!(
                "🔄 Attempting to connect to monitor server at {}",
                socket_path.display()
            );
            eprintln!("🔍 Socket path exists: {}", socket_path.exists());
        }

        match tokio::net::UnixStream::connect(&socket_path).await {
            Ok(stream) => {
                self.socket_stream = Some(stream);
                if self.verbose {
                    eprintln!(
                        "🔗 Connected to monitor server at {}",
                        socket_path.display()
                    );
                }
                // 接続メッセージは run_claude() 開始時に送信
            }
            Err(e) => {
                if self.verbose {
                    eprintln!(
                        "⚠️  Failed to connect to monitor server: {}. Running without monitoring.",
                        e
                    );
                }
            }
        }

        Ok(())
    }

    /// Monitor サーバーに接続されているかチェック
    pub fn is_connected(&self) -> bool {
        self.socket_stream.is_some()
    }

    /// 接続メッセージを送信（非同期版）
    async fn send_connect_message(&mut self) -> Result<()> {
        if let Some(ref mut stream) = self.socket_stream {
            let connect_msg = LauncherToMonitor::Connect {
                launcher_id: self.launcher_id.clone(),
                project: self.project_name.clone(),
                tool_type: match self.tool_wrapper.get_tool_type() {
                    crate::cli_tool::CliToolType::Claude => "claude".to_string(),
                    crate::cli_tool::CliToolType::Gemini => "gemini".to_string(),
                },
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
            stream.write_all(&msg_bytes).await?;
            stream.write_all(b"\n").await?;
            stream.flush().await?;

            if self.verbose {
                eprintln!("✅ Connect message sent successfully");
            }
        } else if self.verbose {
            eprintln!("⚠️  No socket connection available for sending connect message");
        }
        Ok(())
    }

    /// 切断メッセージを送信
    async fn send_disconnect_message(&mut self) -> Result<()> {
        if let Some(ref mut stream) = self.socket_stream {
            let disconnect_msg = LauncherToMonitor::Disconnect {
                launcher_id: self.launcher_id.clone(),
                timestamp: Utc::now(),
            };

            let msg_bytes = serde_json::to_vec(&disconnect_msg)?;
            stream.write_all(&msg_bytes).await?;
            stream.write_all(b"\n").await?;

            if self.verbose {
                eprintln!("📤 Sent disconnect message to monitor");
            }
        }
        Ok(())
    }

    /// Claude プロセス起動・監視（修正版）
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
                eprintln!("🔄 Running Claude without monitoring (monitor not connected)");
            }
            return self.tool_wrapper.run_directly().await;
        }

        // 接続メッセージを送信
        if let Err(e) = self.send_connect_message().await {
            if self.verbose {
                eprintln!("⚠️  Failed to send connect message: {}", e);
            }
        } else if self.verbose {
            eprintln!("✅ Connect message sent successfully");
        }

        // 初期状態メッセージを送信（detector無しなのでNoneで）
        if let Some(ref mut stream) = self.socket_stream {
            let update_msg = LauncherToMonitor::StateUpdate {
                launcher_id: self.launcher_id.clone(),
                session_id: self.session_id.clone(),
                status: SessionStatus::Idle,
                ui_execution_context: None,
                ui_above_text: None,
                timestamp: Utc::now(),
            };

            if let Ok(msg_bytes) = serde_json::to_vec(&update_msg) {
                let _ = stream.write_all(&msg_bytes).await;
                let _ = stream.write_all(b"\n").await;
            }
        }

        // ターミナルガードはmain関数で作成済み（ここでは作らない）
        let terminal_guard = DummyTerminalGuard {
            verbose: self.verbose,
        };

        // Claude プロセス起動（PTYを使用してTTY環境を提供）
        let (mut claude_process, pty_master) = self.tool_wrapper.spawn_with_pty()?;

        // PTYベースの双方向I/O開始
        let pty_handle = self
            .start_pty_bidirectional_io(pty_master, terminal_guard)
            .await?;

        if self.verbose {
            eprintln!("👀 Monitoring started for Claude process");
        }

        // Claude プロセスの終了を待つタスクを一度だけ起動
        let mut wait_task = tokio::task::spawn_blocking(move || claude_process.wait());

        // シグナルハンドリングとリサイズ処理
        let exit_status = self.wait_with_signals(&mut wait_task).await;

        // PTYタスクを終了
        pty_handle.abort();

        // 少し待機してI/Oが完了するのを待つ
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // ターミナル設定を明示的に復元（Dropでも復元されるが念のため）
        // TODO: Re-enable terminal guard restoration
        // if let Some(guard) = &terminal_guard {
        //     guard.restore();
        // }

        match exit_status {
            Ok(status) => {
                if self.verbose {
                    eprintln!("🏁 Claude process exited with status: {:?}", status);
                }
            }
            Err(e) => {
                if self.verbose {
                    eprintln!("❌ Claude execution failed: {}", e);
                }
                // エラー時でも切断メッセージを送信
                if let Err(disconnect_err) = self.send_disconnect_message().await {
                    if self.verbose {
                        eprintln!("⚠️  Failed to send disconnect message: {}", disconnect_err);
                    }
                }
                // ソケット接続を明示的に閉じる
                if let Some(stream) = self.socket_stream.take() {
                    drop(stream);
                    if self.verbose {
                        eprintln!("🔌 Socket connection closed (after error)");
                    }
                }
                return Err(e);
            }
        }

        // 切断メッセージ送信
        self.send_disconnect_message().await?;

        // ソケット接続を明示的に閉じる
        if let Some(stream) = self.socket_stream.take() {
            drop(stream);
            if self.verbose {
                eprintln!("🔌 Socket connection closed");
            }
        }

        Ok(())
    }

    /// PTY 双方向I/Oタスク開始（修正版）
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
        let handle = tokio::spawn(async move {
            Self::handle_pty_bidirectional_io(
                pty_master,
                launcher_id,
                session_id,
                verbose,
                log_file,
                tool_type,
                _terminal_guard,
            )
            .await;
        });

        Ok(handle)
    }

    /// PTY 双方向I/O処理（修正版）
    async fn handle_pty_bidirectional_io(
        pty_master: Box<dyn MasterPty + Send>,
        launcher_id: String,
        session_id: String,
        verbose: bool,
        log_file: Option<PathBuf>,
        tool_type: crate::cli_tool::CliToolType,
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
                        eprintln!("⚠️  Failed to open log file {}: {}", log_path.display(), e);
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
                    eprintln!("⚠️  Failed to get PTY writer: {}", e);
                }
                return;
            }
        };

        let pty_reader = match pty_master.try_clone_reader() {
            Ok(reader) => reader,
            Err(e) => {
                if verbose {
                    eprintln!("⚠️  Failed to get PTY reader: {}", e);
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

        // ターミナルガードはDropで自動的に復元される
    }

    /// プロセス終了とシグナルを待機（修正版）
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
                        eprintln!("🛑 Received SIGINT, letting Claude handle it...");
                    }
                    // Claudeプロセスが自身でSIGINTを処理するので、ここでは何もしない
                    // プロセスが終了するまで待機を続ける
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
                    // 現在のターミナルサイズを取得してPTYに適用
                    let new_size = crate::cli_tool::get_pty_size();
                    // Note: PTYサイズの動的変更は構造上複雑なため、
                    // 新しい接続時に正しいサイズが設定されることを確保
                    if self.verbose {
                        eprintln!("📏 New terminal size: {}x{}", new_size.cols, new_size.rows);
                    }
                    // ループ継続
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
                        eprintln!("🛑 Received Ctrl+C, letting Claude handle it...");
                    }
                    // Claudeプロセスが自身でCtrl+Cを処理するので、ここでは何もしない
                    // プロセスが終了するまで待機を続ける
                }
            }
        }
    }

    // 既存のset_raw_mode、gentle_terminal_reset、force_terminal_resetメソッドは削除
    // （TerminalGuardに機能が移行されたため）

    /// PTY出力をstdoutに転送（監視・ログ付き）
    async fn handle_pty_to_stdout_with_monitoring(
        mut pty_reader: Box<dyn std::io::Read + Send>,
        launcher_id: String,
        session_id: String,
        verbose: bool,
        mut log_writer: Option<tokio::fs::File>,
        tool_type: crate::cli_tool::CliToolType,
    ) {
        use crate::state_detector::create_state_detector;
        use ccmonitor_shared::SessionStatus;

        let mut state_detector = create_state_detector(tool_type, verbose);
        let mut last_status = SessionStatus::Idle;
        
        // ターミナルサイズ監視用
        let mut last_terminal_size = crate::cli_tool::get_pty_size();
        use std::io::Read;
        use tokio::io::AsyncWriteExt;

        let mut buffer = [0u8; 8192];
        let mut stdout = tokio::io::stdout();

        loop {
            match pty_reader.read(&mut buffer) {
                Ok(0) => {
                    if verbose {
                        eprintln!("📡 PTY reader EOF");
                    }
                    break;
                }
                Ok(n) => {
                    let data = &buffer[..n];
                    let output_str = String::from_utf8_lossy(data);

                    // 標準出力に書き込み
                    if let Err(e) = stdout.write_all(data).await {
                        if verbose {
                            eprintln!("⚠️  Failed to write to stdout: {}", e);
                        }
                        break;
                    }

                    // ログファイルに書き込み
                    if let Some(ref mut log_file) = log_writer {
                        if let Err(e) = log_file.write_all(data).await {
                            if verbose {
                                eprintln!("⚠️  Failed to write to log file: {}", e);
                            }
                        }
                    }

                    // ターミナルサイズ変更チェック
                    let current_terminal_size = crate::cli_tool::get_pty_size();
                    if current_terminal_size.rows != last_terminal_size.rows 
                        || current_terminal_size.cols != last_terminal_size.cols {
                        if verbose {
                            eprintln!("🔄 Terminal size changed: {}x{} -> {}x{}", 
                                     last_terminal_size.cols, last_terminal_size.rows,
                                     current_terminal_size.cols, current_terminal_size.rows);
                        }
                        state_detector.resize_screen_buffer(
                            current_terminal_size.rows as usize, 
                            current_terminal_size.cols as usize
                        );
                        last_terminal_size = current_terminal_size;
                    }

                    // 状態検出とモニター通知
                    if let Some(_new_state) = state_detector.process_output(&output_str) {
                        let new_status = state_detector.to_session_status();
                        if new_status != last_status {
                            if verbose {
                                eprintln!(
                                    "🔄 Status changed: {:?} -> {:?}",
                                    last_status, new_status
                                );
                            }
                            last_status = new_status.clone();

                            // モニターサーバーに状態更新を送信（ベストエフォート）
                            Self::send_status_update_async(
                                &launcher_id,
                                &session_id,
                                new_status,
                                &*state_detector,
                                verbose,
                            )
                            .await;
                        }
                    }

                    // 出力をフラッシュ
                    let _ = stdout.flush().await;
                    if let Some(ref mut log_file) = log_writer {
                        let _ = log_file.flush().await;
                    }
                }
                Err(e) => {
                    if verbose {
                        eprintln!("⚠️  PTY read error: {}", e);
                    }
                    break;
                }
            }
        }
    }

    /// Stdin入力をPTYに転送（Raw mode対応版）
    async fn handle_stdin_to_pty_simple(
        mut pty_writer: Box<dyn std::io::Write + Send>,
        verbose: bool,
    ) {
        use std::io::Write;
        use tokio::io::AsyncReadExt;

        // rawモードはmain関数で既に設定済みなので、ここでは設定しない
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
                            eprintln!("⚠️  Failed to write to PTY: {}", e);
                        }
                        break;
                    }

                    if let Err(e) = pty_writer.flush() {
                        if verbose {
                            eprintln!("⚠️  Failed to flush PTY: {}", e);
                        }
                        break;
                    }
                }
                Err(e) => {
                    if verbose {
                        eprintln!("⚠️  Stdin read error: {}", e);
                    }
                    break;
                }
            }
        }

        if verbose {
            eprintln!("📡 Stdin to PTY forwarding ended");
        }
    }

    /// 非同期でステータス更新をモニターサーバーに送信（フォールバック用）
    async fn send_status_update_async(
        launcher_id: &str,
        session_id: &str,
        status: SessionStatus,
        detector: &dyn StateDetector,
        verbose: bool,
    ) {
        // 新しい接続でステータス更新を送信（ベストエフォート）
        let socket_path = std::env::var("CCMONITOR_SOCKET_PATH").unwrap_or_else(|_| {
            std::env::temp_dir()
                .join("ccmonitor.sock")
                .to_string_lossy()
                .to_string()
        });

        match tokio::net::UnixStream::connect(&socket_path).await {
            Ok(mut stream) => {
                let update_msg = LauncherToMonitor::StateUpdate {
                    launcher_id: launcher_id.to_string(),
                    session_id: session_id.to_string(),
                    status: status.clone(),
                    ui_execution_context: detector.get_ui_execution_context(),
                    ui_above_text: detector.get_ui_above_text(),
                    timestamp: Utc::now(),
                };

                if let Ok(msg_bytes) = serde_json::to_vec(&update_msg) {
                    let _ = stream.write_all(&msg_bytes).await;
                    let _ = stream.write_all(b"\n").await;
                    let _ = stream.flush().await;

                    if verbose {
                        eprintln!("📤 Sent fallback status update: {:?}", status);
                    }
                }
            }
            Err(_) => {
                // 接続失敗は無視（ベストエフォート）
                if verbose {
                    eprintln!("⚠️  Failed to send status update (monitor not available)");
                }
            }
        }
    }
}

/// 強制的にターミナルをcooked modeに復元（エラー時の緊急用）
#[cfg(unix)]
pub fn force_restore_terminal() {
    use std::os::fd::BorrowedFd;
    use std::os::unix::io::AsRawFd;

    let stdin_fd = std::io::stdin().as_raw_fd();
    if nix::unistd::isatty(stdin_fd).unwrap_or(false) {
        let borrowed_fd = unsafe { BorrowedFd::borrow_raw(stdin_fd) };

        // 標準的なcooked mode設定を適用
        if let Ok(mut termios) = nix::sys::termios::tcgetattr(borrowed_fd) {
            // ENABLEフラグを設定（cooked mode）
            termios.local_flags |= nix::sys::termios::LocalFlags::ICANON
                | nix::sys::termios::LocalFlags::ECHO
                | nix::sys::termios::LocalFlags::ECHOE
                | nix::sys::termios::LocalFlags::ECHOK
                | nix::sys::termios::LocalFlags::ISIG;

            // INPUTフラグも修正
            termios.input_flags |=
                nix::sys::termios::InputFlags::ICRNL | nix::sys::termios::InputFlags::IXON;

            let _ = nix::sys::termios::tcsetattr(
                borrowed_fd,
                nix::sys::termios::SetArg::TCSANOW,
                &termios,
            );
        }
    }
}

#[cfg(not(unix))]
pub fn force_restore_terminal() {
    // 非Unix環境では何もしない
}

/// グローバル用のターミナルガード作成関数（main関数で使用）
#[cfg(unix)]
pub fn create_terminal_guard_global(verbose: bool) -> Result<TerminalGuard> {
    use std::os::fd::BorrowedFd;
    use std::os::unix::io::AsRawFd;

    let stdin_fd = std::io::stdin().as_raw_fd();

    // stdinがターミナルかどうかチェック
    if !nix::unistd::isatty(stdin_fd).unwrap_or(false) {
        if verbose {
            eprintln!("🔒 Terminal guard created (non-TTY mode)");
        }
        // ターミナルでない場合は何もしない（ダミーのTermiosを作成）
        let dummy_termios = unsafe { std::mem::zeroed() };
        return Ok(TerminalGuard {
            fd: stdin_fd,
            original: dummy_termios,
            verbose,
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
        eprintln!("🔒 Terminal guard created with raw mode");
    }

    Ok(TerminalGuard {
        fd: stdin_fd,
        original: original_termios,
        verbose,
    })
}

#[cfg(not(unix))]
pub fn create_terminal_guard_global(verbose: bool) -> Result<TerminalGuard> {
    // 非Unix環境では何もしない
    Ok(TerminalGuard { verbose })
}
