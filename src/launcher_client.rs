use anyhow::Result;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;
use tokio::task::JoinHandle;
use portable_pty::MasterPty;

use crate::claude_wrapper::ClaudeWrapper;
use crate::monitor_server::MonitorServer;
use crate::protocol::{
    LauncherToMonitor, generate_connection_id
};

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
    pub fn new(claude_args: Vec<String>, verbose: bool) -> Self {
        let launcher_id = generate_connection_id();
        let session_id = format!("session-{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
        let claude_wrapper = ClaudeWrapper::new(claude_args);
        let project_name = claude_wrapper.guess_project_name();

        Self {
            launcher_id,
            socket_stream: None,
            claude_wrapper,
            project_name,
            session_id,
            verbose,
            log_file: None,
        }
    }

    /// ログファイルを設定
    pub fn set_log_file(&mut self, log_file: Option<PathBuf>) {
        self.log_file = log_file;
    }

    /// Monitor サーバーに接続
    pub async fn connect_to_monitor(&mut self) -> Result<()> {
        let socket_path = MonitorServer::get_client_socket_path()?;
        
        let stream = UnixStream::connect(&socket_path).await
            .map_err(|e| anyhow::anyhow!("Failed to connect to monitor: {}", e))?;

        self.socket_stream = Some(stream);

        if self.verbose {
            println!("✅ Connected to monitor server");
        }

        // 接続メッセージ送信
        self.send_connect_message().await?;

        // Monitor からのメッセージを受信してログファイル設定を取得
        self.receive_initial_config().await?;

        Ok(())
    }

    /// 接続メッセージ送信
    async fn send_connect_message(&mut self) -> Result<()> {
        let working_dir = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"));

        let message = LauncherToMonitor::Connect {
            launcher_id: self.launcher_id.clone(),
            project: self.project_name.clone(),
            claude_args: self.claude_wrapper.get_args().to_vec(),
            working_dir,
            timestamp: chrono::Utc::now(),
        };

        self.send_message(message).await?;

        if self.verbose {
            println!("📡 Sent connection message to monitor");
        }

        Ok(())
    }

    /// Monitor からの初期設定を受信
    async fn receive_initial_config(&mut self) -> Result<()> {
        use crate::protocol::MonitorToLauncher;
        use tokio::io::{AsyncBufReadExt, BufReader};

        if let Some(ref mut stream) = self.socket_stream {
            let mut reader = BufReader::new(stream);
            let mut buffer = String::new();
            
            // タイムアウト付きでメッセージを受信
            match tokio::time::timeout(
                tokio::time::Duration::from_secs(2),
                reader.read_line(&mut buffer)
            ).await {
                Ok(Ok(n)) if n > 0 => {
                    if let Ok(message) = serde_json::from_str::<MonitorToLauncher>(&buffer.trim()) {
                        match message {
                            MonitorToLauncher::SetLogFile { log_file_path } => {
                                self.log_file = log_file_path;
                                if self.verbose {
                                    if let Some(ref path) = self.log_file {
                                        println!("📝 Log file configured: {}", path.display());
                                    }
                                }
                            }
                            _ => {
                                // 他のメッセージは無視
                            }
                        }
                    }
                }
                _ => {
                    // タイムアウトまたは他のエラー：ログファイル設定なしとして続行
                    if self.verbose {
                        println!("⏰ No log file configuration received");
                    }
                }
            }
        }

        Ok(())
    }

    /// Claude プロセス起動・監視
    pub async fn run_claude(&mut self) -> Result<()> {
        if self.verbose {
            println!("🚀 Starting Claude: {}", self.claude_wrapper.to_command_string());
        }

        // Monitor に接続できていない場合は単純にClaude実行
        if !self.is_connected() {
            if self.verbose {
                println!("🔄 Running Claude without monitoring (monitor not connected)");
            }
            return self.claude_wrapper.run_directly().await;
        }

        // Claude プロセス起動（PTYを使用してTTY環境を提供）
        let (mut claude_process, pty_master) = self.claude_wrapper.spawn_with_pty()?;
        // Note: std::process::Child は set_process に渡せないため、プロセス監視は省略

        // PTYベースの双方向I/O開始（ターミナル設定も含む）
        let pty_handle = self.start_pty_bidirectional_io(pty_master).await?;


        if self.verbose {
            println!("👀 Monitoring started for Claude process");
        }

        // Claude プロセスの終了を待つタスクを一度だけ起動
        let mut wait_task = tokio::task::spawn_blocking(move || claude_process.wait());
        
        // シグナルハンドリングとリサイズ処理も含める
        let exit_status = self.wait_with_signals(&mut wait_task).await;

        // ターミナル設定を確実に復元（エラーでも実行）
        if self.verbose {
            println!("🔧 Ensuring terminal restoration...");
        }
        Self::force_terminal_reset(self.verbose);

        // 監視タスクを終了
        pty_handle.abort();

        match exit_status {
            Ok(status) => {
                if self.verbose {
                    println!("🏁 Claude process exited with status: {:?}", status);
                }
            }
            Err(e) => {
                if self.verbose {
                    println!("❌ Claude execution failed: {}", e);
                }
                return Err(e);
            }
        }

        // 切断メッセージ送信
        self.send_disconnect_message().await?;

        Ok(())
    }


    /// PTY 双方向I/Oタスク開始
    async fn start_pty_bidirectional_io(&self, pty_master: Box<dyn MasterPty + Send>) -> Result<JoinHandle<()>> {
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
            ).await;
        });

        Ok(handle)
    }

    /// PTY 双方向I/O処理（stdin → PTY, PTY → stdout + log）
    async fn handle_pty_bidirectional_io(
        pty_master: Box<dyn MasterPty + Send>,
        _launcher_id: String,
        _session_id: String,
        verbose: bool,
        log_file: Option<PathBuf>,
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

        // PTY writer を一度だけ取得（take_writer は一度しか呼べない）
        let pty_writer = match pty_master.take_writer() {
            Ok(writer) => writer,
            Err(e) => {
                if verbose {
                    eprintln!("⚠️  Failed to get PTY writer: {}", e);
                }
                return;
            }
        };

        // PTY reader を取得
        let pty_reader = match pty_master.try_clone_reader() {
            Ok(reader) => reader,
            Err(e) => {
                if verbose {
                    eprintln!("⚠️  Failed to get PTY reader: {}", e);
                }
                return;
            }
        };

        // ターミナルをRAWモードに設定
        use std::io::IsTerminal;
        if std::io::stdin().is_terminal() {
            if verbose {
                println!("📝 [terminal] Setting raw mode...");
            }
            Self::set_raw_mode(verbose);
        } else {
            if verbose {
                println!("⚠️ [terminal] Stdin is not a terminal, skipping raw mode");
            }
        }

        // 双方向I/Oタスクを起動
        let mut pty_to_stdout = tokio::spawn(async move {
            Self::handle_pty_to_stdout(
                pty_reader,
                verbose,
                log_writer,
            ).await;
        });

        let mut stdin_to_pty = tokio::spawn(async move {
            Self::handle_stdin_to_pty_simple(pty_writer, verbose).await;
        });
        
        tokio::select! {
            _ = &mut pty_to_stdout => {
                if verbose {
                    println!("📡 PTY to stdout task ended");
                }
                stdin_to_pty.abort();
            }
            _ = &mut stdin_to_pty => {
                if verbose {
                    println!("📡 Stdin to PTY task ended");
                }
                pty_to_stdout.abort();
            }
        }

        // ターミナル設定の復元はrun_claudeで行う
    }

    /// PTY → stdout + log 転送処理
    async fn handle_pty_to_stdout(
        pty_reader: Box<dyn std::io::Read + Send>,
        verbose: bool,
        mut log_writer: Option<tokio::fs::File>,
    ) {
        use std::sync::{Arc, Mutex};
        let pty_reader = Arc::new(Mutex::new(pty_reader));
        
        loop {
            let mut buffer = [0u8; 4096];
            let result = tokio::task::spawn_blocking({
                let pty_reader = pty_reader.clone();
                move || {
                    use std::io::Read;
                    let mut reader = pty_reader.lock().unwrap();
                    let bytes_read = reader.read(&mut buffer)?;
                    Ok::<(Vec<u8>, usize), std::io::Error>((buffer.to_vec(), bytes_read))
                }
            }).await;

            match result {
                Ok(Ok((_buffer_data, 0))) => break, // EOF
                Ok(Ok((buffer_data, n))) => {
                    // バイナリデータをそのまま標準出力に書き込む（UTF-8変換しない）
                    use std::io::Write;
                    std::io::stdout().write_all(&buffer_data[..n]).unwrap();
                    std::io::stdout().flush().unwrap();
                    
                    // ログ記録用にのみUTF-8変換を行う
                    let output = String::from_utf8_lossy(&buffer_data[..n]);

                    if verbose {
                        println!("📝 [pty→stdout] {}", output.trim());
                    }

                    // ログファイルに書き込み
                    if let Some(ref mut writer) = log_writer {
                        if let Err(e) = writer.write_all(output.as_bytes()).await {
                            if verbose {
                                eprintln!("⚠️  Failed to write to log file: {}", e);
                            }
                        } else {
                            // フラッシュして確実に書き込み
                            let _ = writer.flush().await;
                        }
                    }
                }
                Ok(Err(e)) => {
                    if verbose {
                        eprintln!("📡 Read error from PTY: {}", e);
                    }
                    break;
                }
                Err(e) => {
                    if verbose {
                        eprintln!("📡 Spawn blocking error: {}", e);
                    }
                    break;
                }
            }
        }
    }

    /// stdin → PTY 転送処理（シンプル版）
    async fn handle_stdin_to_pty_simple(
        pty_writer: Box<dyn std::io::Write + Send>,
        verbose: bool,
    ) {
        use tokio::io::AsyncReadExt;
        use std::sync::{Arc, Mutex};
        
        let pty_writer = Arc::new(Mutex::new(pty_writer));
        let mut stdin = tokio::io::stdin();
        let mut buffer = [0u8; 1024];

        if verbose {
            println!("📝 [stdin→pty] Starting simplified input reading (pass-through mode)");
        }

        loop {
            match stdin.read(&mut buffer).await {
                Ok(0) => {
                    if verbose {
                        println!("📝 [stdin→pty] EOF received");
                    }
                    break;
                }
                Ok(n) => {
                    // すべてのバイトをそのまま通す（VTフィルタリングのみ）
                    let filtered_data: Vec<u8> = buffer[..n].iter()
                        .filter(|&&byte| byte != 11) // VT (0x0B) のみフィルタ
                        .copied()
                        .collect();
                    
                    
                    if !filtered_data.is_empty() {
                        if let Err(e) = Self::write_bytes_to_pty(&pty_writer, &filtered_data, verbose).await {
                            if verbose {
                                eprintln!("📡 Error writing to PTY: {}", e);
                            }
                            break;
                        }
                    }
                }
                Err(e) => {
                    if verbose {
                        eprintln!("📡 Read error from stdin: {}", e);
                    }
                    break;
                }
            }
        }
    }

    /// バイナリデータを直接PTYに書き込む
    async fn write_bytes_to_pty(
        pty_writer: &Arc<Mutex<Box<dyn std::io::Write + Send>>>,
        data: &[u8],
        verbose: bool,
    ) -> Result<()> {
        if verbose {
            let display_data = String::from_utf8_lossy(data);
            let display_input = display_data.replace('\n', "\\n").replace('\r', "\\r");
            println!("📝 [stdin→pty] \"{}\" (bytes: {:?})", display_input, data);
        }

        let result = tokio::task::spawn_blocking({
            let pty_writer = pty_writer.clone();
            let data = data.to_vec();
            move || {
                use std::io::Write;
                let mut writer = pty_writer.lock().unwrap();
                writer.write_all(&data)?;
                writer.flush()?;
                Ok::<(), std::io::Error>(())
            }
        }).await;

        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => {
                if verbose {
                    eprintln!("📡 Write error to PTY: {}", e);
                }
                Err(anyhow::anyhow!("PTY write error: {}", e))
            }
            Err(e) => {
                if verbose {
                    eprintln!("📡 Spawn blocking error for stdin write: {}", e);
                }
                Err(anyhow::anyhow!("Spawn blocking error: {}", e))
            }
        }
    }




    /// 切断メッセージ送信
    async fn send_disconnect_message(&mut self) -> Result<()> {
        let message = LauncherToMonitor::Disconnect {
            launcher_id: self.launcher_id.clone(),
            timestamp: chrono::Utc::now(),
        };

        self.send_message(message).await?;

        if self.verbose {
            println!("📴 Sent disconnect message to monitor");
        }

        Ok(())
    }

    /// メッセージ送信
    async fn send_message(&mut self, message: LauncherToMonitor) -> Result<()> {
        let stream = self.socket_stream.as_mut()
            .ok_or_else(|| anyhow::anyhow!("Not connected to monitor"))?;

        let json_data = serde_json::to_string(&message)?;
        let data_with_newline = format!("{}\n", json_data);

        stream.write_all(data_with_newline.as_bytes()).await?;
        stream.flush().await?;

        Ok(())
    }

    /// 接続状態確認
    pub fn is_connected(&self) -> bool {
        self.socket_stream.is_some()
    }

    /// Launcher 情報取得
    pub fn get_info(&self) -> LauncherInfo {
        LauncherInfo {
            id: self.launcher_id.clone(),
            project: self.project_name.clone(),
            claude_args: self.claude_wrapper.get_args().to_vec(),
            session_id: self.session_id.clone(),
        }
    }

    /// ターミナルをRAWモードに設定
    #[cfg(unix)]
    fn set_raw_mode(verbose: bool) -> Option<libc::termios> {
        unsafe {
            let mut termios: libc::termios = std::mem::zeroed();
            
            if libc::tcgetattr(libc::STDIN_FILENO, &mut termios) == 0 {
                let original_termios = termios;
                
                // RAWモード設定: 入力の即座処理とエコー無効化
                termios.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ECHONL);
                termios.c_iflag &= !(libc::ICRNL | libc::INLCR | libc::IXON | libc::IXOFF);
                termios.c_oflag &= !libc::OPOST;
                termios.c_cc[libc::VMIN] = 1;  // 最小読み取り文字数
                termios.c_cc[libc::VTIME] = 0; // タイムアウト無効
                
                if libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &termios) == 0 {
                    if verbose {
                        println!("📝 [terminal] Set to raw mode successfully");
                        // 設定を確認
                        let mut check_termios: libc::termios = std::mem::zeroed();
                        if libc::tcgetattr(libc::STDIN_FILENO, &mut check_termios) == 0 {
                            println!("📝 [terminal] Current c_lflag: {:x}", check_termios.c_lflag);
                            println!("📝 [terminal] ICANON disabled: {}", (check_termios.c_lflag & libc::ICANON) == 0);
                            println!("📝 [terminal] ECHO disabled: {}", (check_termios.c_lflag & libc::ECHO) == 0);
                        }
                    }
                    Some(original_termios)
                } else {
                    if verbose {
                        eprintln!("⚠️  Failed to set terminal raw mode");
                    }
                    None
                }
            } else {
                if verbose {
                    eprintln!("⚠️  Failed to get terminal attributes");
                }
                None
            }
        }
    }

    /// ターミナルをRAWモードに設定（非Unix環境用）
    #[cfg(not(unix))]
    fn set_raw_mode(verbose: bool) -> Option<()> {
        if verbose {
            println!("📝 [terminal] Raw mode not supported on this platform");
        }
        None
    }

    /// 強制的にターミナル設定をリセット
    #[cfg(unix)]
    pub fn force_terminal_reset(verbose: bool) {
        unsafe {
            let mut termios: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(libc::STDIN_FILENO, &mut termios) == 0 {
                // 標準的な設定を強制適用
                termios.c_lflag |= libc::ICANON | libc::ECHO | libc::ECHONL | libc::ISIG;
                termios.c_iflag |= libc::ICRNL;
                termios.c_oflag |= libc::OPOST;
                termios.c_cc[libc::VMIN] = 1;
                termios.c_cc[libc::VTIME] = 0;
                
                if libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, &termios) == 0 {
                    if verbose {
                        println!("📝 [terminal] Force reset successful");
                    }
                } else if verbose {
                    eprintln!("⚠️  Force reset failed");
                }
            }
        }
        
        // エスケープシーケンスによるリセットも試行
        print!("\x1bc"); // Full reset
        use std::io::Write;
        let _ = std::io::stdout().flush();
    }

    /// 強制的にターミナル設定をリセット（非Unix環境用）
    #[cfg(not(unix))]
    pub fn force_terminal_reset(_verbose: bool) {
        // 非Unix環境では何もしない
    }
    
    /// プロセス終了とシグナルを待機
    #[cfg(unix)]
    async fn wait_with_signals(&self, wait_task: &mut tokio::task::JoinHandle<std::io::Result<portable_pty::ExitStatus>>) -> Result<portable_pty::ExitStatus> {
        let mut sigwinch = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::window_change()).unwrap();
        
        loop {
            tokio::select! {
                result = &mut *wait_task => {
                    return result?.map_err(|e| anyhow::anyhow!("Process wait error: {}", e));
                }
                _ = tokio::signal::ctrl_c() => {
                    if self.verbose {
                        println!("🛑 Received Ctrl+C, shutting down gracefully...");
                    }
                    return Err(anyhow::anyhow!("Interrupted by user"));
                }
                _ = sigwinch.recv() => {
                    if self.verbose {
                        println!("🔄 Terminal resized - reapplying settings...");
                    }
                    // rawモード設定を再適用
                    Self::set_raw_mode(self.verbose);
                    // ループ継続
                }
            }
        }
    }
    
    /// プロセス終了とシグナルを待機（非Unix環境用）
    #[cfg(not(unix))]
    async fn wait_with_signals(&self, wait_task: &mut tokio::task::JoinHandle<std::io::Result<portable_pty::ExitStatus>>) -> Result<portable_pty::ExitStatus> {
        tokio::select! {
            result = &mut *wait_task => {
                result?.map_err(|e| anyhow::anyhow!("Process wait error: {}", e))
            }
            _ = tokio::signal::ctrl_c() => {
                if self.verbose {
                    println!("🛑 Received Ctrl+C, shutting down gracefully...");
                }
                Err(anyhow::anyhow!("Interrupted by user"))
            }
        }
    }
}

/// Launcher 情報
#[derive(Debug, Clone)]
pub struct LauncherInfo {
    pub id: String,
    pub project: Option<String>,
    pub claude_args: Vec<String>,
    pub session_id: String,
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_launcher_client_creation() {
        let client = LauncherClient::new(vec!["--help".to_string()], false);
        assert!(!client.is_connected());
        assert!(client.launcher_id.starts_with("launcher-"));
    }

    #[test]
    fn test_launcher_info() {
        let client = LauncherClient::new(vec!["--project".to_string(), "test".to_string()], false);
        let info = client.get_info();
        
        assert_eq!(info.claude_args, vec!["--project", "test"]);
        assert_eq!(info.project, Some("test".to_string()));
    }
}