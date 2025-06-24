use anyhow::Result;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::task::JoinHandle;
use portable_pty::MasterPty;

use crate::ansi_utils::clean_for_logging;

use crate::claude_wrapper::ClaudeWrapper;
use crate::monitor_server::MonitorServer;
use crate::process_monitor::ProcessMonitor;
use crate::protocol::{
    LauncherToMonitor, generate_connection_id
};
use crate::standard_analyzer::StandardAnalyzer;

/// Launcher クライアント
pub struct LauncherClient {
    launcher_id: String,
    socket_stream: Option<UnixStream>,
    claude_wrapper: ClaudeWrapper,
    process_monitor: ProcessMonitor,
    output_analyzer: StandardAnalyzer,
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
            process_monitor: ProcessMonitor::new(),
            output_analyzer: StandardAnalyzer::new(),
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

        // PTYベースの双方向I/O開始
        let pty_handle = self.start_pty_bidirectional_io(pty_master).await?;

        // プロセス監視開始
        let process_handle = self.start_process_monitoring().await;

        if self.verbose {
            println!("👀 Monitoring started for Claude process");
        }

        // Claude プロセスの終了を待つ（portable_pty::Child なので tokio::task::spawn_blocking を使用）
        let exit_status = tokio::task::spawn_blocking(move || {
            claude_process.wait()
        }).await??;

        if self.verbose {
            println!("🏁 Claude process exited with status: {:?}", exit_status);
        }

        // 監視タスクを終了
        pty_handle.abort();
        process_handle.abort();

        // 切断メッセージ送信
        self.send_disconnect_message().await?;

        Ok(())
    }

    /// stdout 監視タスク開始
    async fn start_stdout_monitoring(&self, claude_process: &mut tokio::process::Child) -> Result<JoinHandle<()>> {
        let stdout = claude_process.stdout.take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture stdout"))?;

        let launcher_id = self.launcher_id.clone();
        let session_id = self.session_id.clone();
        let verbose = self.verbose;
        let log_file = self.log_file.clone();

        let handle = tokio::spawn(async move {
            Self::monitor_output_stream(
                stdout,
                launcher_id,
                session_id,
                "stdout".to_string(),
                verbose,
                log_file,
            ).await;
        });

        Ok(handle)
    }

    /// stderr 監視タスク開始
    async fn start_stderr_monitoring(&self, claude_process: &mut tokio::process::Child) -> Result<JoinHandle<()>> {
        let stderr = claude_process.stderr.take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture stderr"))?;

        let launcher_id = self.launcher_id.clone();
        let session_id = self.session_id.clone();
        let verbose = self.verbose;
        let log_file = self.log_file.clone();

        let handle = tokio::spawn(async move {
            Self::monitor_output_stream(
                stderr,
                launcher_id,
                session_id,
                "stderr".to_string(),
                verbose,
                log_file,
            ).await;
        });

        Ok(handle)
    }

    /// 出力ストリーム監視
    async fn monitor_output_stream(
        stream: impl tokio::io::AsyncRead + Unpin,
        _launcher_id: String,
        _session_id: String,
        stream_name: String,
        verbose: bool,
        log_file: Option<PathBuf>,
    ) {
        let mut reader = BufReader::new(stream);
        let mut buffer = String::new();
        let mut analyzer = StandardAnalyzer::new();

        // ログファイルを開く（stdout のみ）
        let mut log_writer = if stream_name == "stdout" {
            if let Some(ref log_path) = log_file {
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
            }
        } else {
            None
        };

        loop {
            buffer.clear();
            
            match reader.read_line(&mut buffer).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let line = buffer.trim();
                    
                    // ユーザーには通常通り出力表示
                    match stream_name.as_str() {
                        "stdout" => println!("{}", line),
                        "stderr" => eprintln!("{}", line),
                        _ => {}
                    }

                    if verbose {
                        println!("📝 [{}] {}", stream_name, line);
                    }

                    // ログファイルに書き込み（stdout のみ、ANSI エスケープシーケンスをクリーンアップ）
                    if let Some(ref mut writer) = log_writer {
                        let clean_line = clean_for_logging(line);
                        let log_line = format!("{}\n", clean_line);
                        if let Err(e) = writer.write_all(log_line.as_bytes()).await {
                            if verbose {
                                eprintln!("⚠️  Failed to write to log file: {}", e);
                            }
                        } else {
                            // フラッシュして確実に書き込み
                            let _ = writer.flush().await;
                        }
                    }

                    // 出力を解析
                    let _analysis = analyzer.analyze_output(line, &stream_name);
                    
                    // TODO: Monitor に送信
                    // このセクションは後で実装
                }
                Err(e) => {
                    if verbose {
                        eprintln!("📡 Read error from {}: {}", stream_name, e);
                    }
                    break;
                }
            }
        }
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
        let analyzer = StandardAnalyzer::new();

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

        // 双方向I/Oタスクを起動
        let pty_to_stdout = tokio::spawn(async move {
            Self::handle_pty_to_stdout(
                pty_reader,
                verbose,
                log_writer,
                analyzer,
            ).await;
        });

        let stdin_to_pty = tokio::spawn(async move {
            Self::handle_stdin_to_pty(pty_writer, verbose).await;
        });

        // どちらかのタスクが終了したら両方終了
        tokio::select! {
            _ = pty_to_stdout => {
                if verbose {
                    println!("📡 PTY to stdout task ended");
                }
            }
            _ = stdin_to_pty => {
                if verbose {
                    println!("📡 Stdin to PTY task ended");
                }
            }
        }
    }

    /// PTY → stdout + log 転送処理
    async fn handle_pty_to_stdout(
        pty_reader: Box<dyn std::io::Read + Send>,
        verbose: bool,
        mut log_writer: Option<tokio::fs::File>,
        mut analyzer: StandardAnalyzer,
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
                    let output = String::from_utf8_lossy(&buffer_data[..n]);
                    
                    // ユーザーには通常通り出力表示
                    print!("{}", output);
                    use std::io::Write;
                    std::io::stdout().flush().unwrap();

                    if verbose {
                        println!("📝 [pty→stdout] {}", output.trim());
                    }

                    // ログファイルに書き込み（クリーンアップ済み）
                    if let Some(ref mut writer) = log_writer {
                        let clean_output = clean_for_logging(&output);
                        if let Err(e) = writer.write_all(clean_output.as_bytes()).await {
                            if verbose {
                                eprintln!("⚠️  Failed to write to log file: {}", e);
                            }
                        } else {
                            // フラッシュして確実に書き込み
                            let _ = writer.flush().await;
                        }
                    }

                    // 出力を解析
                    let _analysis = analyzer.analyze_output(&output, "pty");
                    
                    // TODO: Monitor に送信
                    // このセクションは後で実装
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

    /// stdin → PTY 転送処理
    async fn handle_stdin_to_pty(
        pty_writer: Box<dyn std::io::Write + Send>,
        verbose: bool,
    ) {
        use tokio::io::{AsyncBufReadExt, BufReader};
        use std::sync::{Arc, Mutex};
        
        let pty_writer = Arc::new(Mutex::new(pty_writer));
        let stdin = tokio::io::stdin();
        let mut reader = BufReader::new(stdin);
        let mut input_buffer = Vec::new();

        loop {
            input_buffer.clear();
            
            match reader.read_until(b'\n', &mut input_buffer).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let input = String::from_utf8_lossy(&input_buffer);
                    
                    if verbose {
                        println!("📝 [stdin→pty] {}", input.trim());
                    }

                    // PTYに書き込み
                    let result = tokio::task::spawn_blocking({
                        let pty_writer = pty_writer.clone();
                        let input = input.to_string();
                        move || {
                            use std::io::Write;
                            let mut writer = pty_writer.lock().unwrap();
                            writer.write_all(input.as_bytes())?;
                            writer.flush()?;
                            Ok::<(), std::io::Error>(())
                        }
                    }).await;

                    if let Err(e) = result {
                        if verbose {
                            eprintln!("📡 Spawn blocking error for stdin write: {}", e);
                        }
                        break;
                    } else if let Err(e) = result.unwrap() {
                        if verbose {
                            eprintln!("📡 Write error to PTY: {}", e);
                        }
                        break;
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

    /// プロセス監視タスク開始
    async fn start_process_monitoring(&self) -> JoinHandle<()> {
        let _launcher_id = self.launcher_id.clone();
        let verbose = self.verbose;
        let mut process_monitor = ProcessMonitor::new();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));

            loop {
                interval.tick().await;
                
                let process_info = process_monitor.get_process_info().await;
                
                if verbose {
                    println!("📊 Process: CPU {:.1}%, Memory {}MB, Children {}",
                        process_info.cpu_percent,
                        process_info.memory_mb,
                        process_info.child_count
                    );
                }

                // TODO: Monitor に送信
                // このセクションは後で実装
            }
        })
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