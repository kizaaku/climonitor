// launcher_client.rs の修正箇所

use anyhow::Result;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;
use tokio::task::JoinHandle;
use portable_pty::MasterPty;
use serde_json;
use chrono::Utc;


use crate::claude_wrapper::ClaudeWrapper;
use crate::protocol::{
    LauncherToMonitor, SessionStatus, generate_connection_id
};
use crate::session_state::SessionStateDetector;

/// Launcher クライアント
pub struct LauncherClient {
    launcher_id: String,
    socket_stream: Option<UnixStream>,
    claude_wrapper: ClaudeWrapper,
    project_name: Option<String>,
    session_id: String,
    verbose: bool,
    log_file: Option<PathBuf>,
}

impl LauncherClient {
    /// 新しいLauncherClientを作成
    pub fn new(
        claude_wrapper: ClaudeWrapper,
        socket_path: Option<std::path::PathBuf>,
        verbose: bool,
        log_file: Option<PathBuf>,
    ) -> Result<Self> {
        let launcher_id = generate_connection_id();
        let session_id = generate_connection_id();
        let project_name = claude_wrapper.guess_project_name();

        let mut client = Self {
            launcher_id,
            socket_stream: None,
            claude_wrapper,
            project_name,
            session_id,
            verbose,
            log_file,
        };

        // Monitor サーバーに接続を試行
        client.try_connect_to_monitor(socket_path)?;

        Ok(client)
    }

    /// Monitor サーバーへの接続を試行
    fn try_connect_to_monitor(&mut self, socket_path: Option<PathBuf>) -> Result<()> {
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
            eprintln!("🔄 Attempting to connect to monitor server at {}", socket_path.display());
            eprintln!("🔍 Socket path exists: {}", socket_path.exists());
        }
        
        match std::os::unix::net::UnixStream::connect(&socket_path) {
            Ok(stream) => {
                // NonBlockingに設定
                stream.set_nonblocking(true)?;
                self.socket_stream = Some(tokio::net::UnixStream::from_std(stream)?);
                if self.verbose {
                    eprintln!("🔗 Connected to monitor server at {}", socket_path.display());
                }
                // 接続メッセージは run_claude() 開始時に送信
            }
            Err(e) => {
                if self.verbose {
                    eprintln!("⚠️  Failed to connect to monitor server: {}. Running without monitoring.", e);
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
                claude_args: self.claude_wrapper.get_args().to_vec(),
                working_dir: self.claude_wrapper.get_working_dir().cloned().unwrap_or_else(|| std::env::current_dir().unwrap_or_default()),
                timestamp: Utc::now(),
            };
            
            let msg_bytes = serde_json::to_vec(&connect_msg)?;
            stream.write_all(&msg_bytes).await?;
            stream.write_all(b"\n").await?;
            stream.flush().await?;
            
            if self.verbose {
                eprintln!("📤 Sent connect message to monitor");
            }
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
            eprintln!("🚀 Starting Claude: {}", self.claude_wrapper.to_command_string());
        }

        // Monitor に接続できていない場合は単純にClaude実行
        if !self.is_connected() {
            if self.verbose {
                eprintln!("🔄 Running Claude without monitoring (monitor not connected)");
            }
            return self.claude_wrapper.run_directly().await;
        }

        // 接続メッセージを送信
        if let Err(e) = self.send_connect_message().await {
            if self.verbose {
                eprintln!("⚠️  Failed to send connect message: {}", e);
            }
        } else if self.verbose {
            eprintln!("✅ Connect message sent successfully");
        }

        // 初期状態メッセージを送信
        Self::send_status_update_async(&self.launcher_id, &self.session_id, SessionStatus::Idle, self.verbose).await;

        // ターミナルガードを作成（スコープを抜ける際に自動的に復元される）
        // TODO: Re-enable terminal guard once import issue is resolved
        let terminal_guard: Option<()> = None;
        
        // Claude プロセス起動（PTYを使用してTTY環境を提供）
        let (mut claude_process, pty_master) = self.claude_wrapper.spawn_with_pty()?;
        
        // PTYベースの双方向I/O開始
        let pty_handle = self.start_pty_bidirectional_io(pty_master, terminal_guard.clone()).await?;

        if self.verbose {
            eprintln!("👀 Monitoring started for Claude process");
        }

        // Claude プロセスの終了を待つタスクを一度だけ起動
        let mut wait_task = tokio::task::spawn_blocking(move || claude_process.wait());
        
        // シグナルハンドリングとリサイズ処理
        let exit_status = self.wait_with_signals(&mut wait_task, terminal_guard.clone()).await;

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
        terminal_guard: Option<()>
    ) -> Result<JoinHandle<()>> {
        let launcher_id = self.launcher_id.clone();
        let session_id = self.session_id.clone();
        let verbose = self.verbose;
        let log_file = self.log_file.clone();

        let handle = tokio::spawn(async move {
            Self::handle_pty_bidirectional_io(
                pty_master,
                launcher_id,
                session_id,
                verbose,
                log_file,
                terminal_guard,
            ).await;
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
        _terminal_guard: Option<()>,
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
            ).await;
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
        terminal_guard: Option<()>
    ) -> Result<portable_pty::ExitStatus> {
        let mut sigwinch = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::window_change()).unwrap();
        
        loop {
            tokio::select! {
                result = &mut *wait_task => {
                    return result?.map_err(|e| anyhow::anyhow!("Process wait error: {}", e));
                }
                _ = tokio::signal::ctrl_c() => {
                    if self.verbose {
                        eprintln!("🛑 Received Ctrl+C, shutting down gracefully...");
                    }
                    // ターミナルを復元してから終了
                    // TODO: Re-enable terminal guard restoration  
                    // if let Some(guard) = &terminal_guard {
                    //     guard.restore();
                    // }
                    return Err(anyhow::anyhow!("Interrupted by user"));
                }
                _ = sigwinch.recv() => {
                    if self.verbose {
                        eprintln!("🔄 Terminal resized - reapplying settings...");
                    }
                    // rawモード設定を再適用
                    // TODO: Re-enable terminal guard reapply
                    // #[cfg(unix)]
                    // if let Some(guard) = &terminal_guard {
                    //     guard.reapply_raw_mode();
                    // }
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
        _terminal_guard: Option<()>
    ) -> Result<portable_pty::ExitStatus> {
        tokio::select! {
            result = &mut *wait_task => {
                result?.map_err(|e| anyhow::anyhow!("Process wait error: {}", e))
            }
            _ = tokio::signal::ctrl_c() => {
                if self.verbose {
                    eprintln!("🛑 Received Ctrl+C, shutting down gracefully...");
                }
                Err(anyhow::anyhow!("Interrupted by user"))
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
    ) {
        use crate::session_state::SessionStateDetector;
        use crate::protocol::SessionStatus;
        
        let mut state_detector = SessionStateDetector::new(verbose);
        let mut last_status = SessionStatus::Idle;
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
                    
                    // 状態検出とモニター通知
                    if let Some(_new_state) = state_detector.process_output(&output_str) {
                        let new_status = state_detector.to_session_status();
                        if new_status != last_status {
                            if verbose {
                                eprintln!("🔄 Status changed: {:?} -> {:?}", last_status, new_status);
                            }
                            last_status = new_status.clone();
                            
                            // モニターサーバーに状態更新を送信（ベストエフォート）
                            Self::send_status_update_async(&launcher_id, &session_id, new_status, verbose).await;
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

    /// Stdin入力をPTYに転送（シンプル版）
    async fn handle_stdin_to_pty_simple(
        mut pty_writer: Box<dyn std::io::Write + Send>,
        verbose: bool,
    ) {
        use std::io::Write;
        use tokio::io::AsyncReadExt;
        
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
    }

    /// 非同期でステータス更新をモニターサーバーに送信
    async fn send_status_update_async(
        launcher_id: &str,
        session_id: &str,
        status: SessionStatus,
        verbose: bool,
    ) {
        // 新しい接続でステータス更新を送信（ベストエフォート）
        let socket_path = std::env::var("CCMONITOR_SOCKET_PATH")
            .unwrap_or_else(|_| {
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
                    timestamp: Utc::now(),
                };
                
                if let Ok(msg_bytes) = serde_json::to_vec(&update_msg) {
                    let _ = stream.write_all(&msg_bytes).await;
                    let _ = stream.write_all(b"\n").await;
                    
                    if verbose {
                        eprintln!("📤 Sent status update: {:?}", status);
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