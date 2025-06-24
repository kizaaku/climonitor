use anyhow::Result;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::task::JoinHandle;

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

        // 対話モード検出（引数なしまたは--printなし）
        let is_interactive = self.is_interactive_mode();
        if is_interactive {
            if self.verbose {
                if self.log_file.is_some() {
                    println!("🔄 Interactive mode detected, running with log-only monitoring");
                } else {
                    println!("🔄 Interactive mode detected, running without monitoring");
                }
            }
            
            // ログファイルが設定されている場合は軽量監視モードで実行
            if self.log_file.is_some() {
                return self.run_claude_with_log_only().await;
            } else {
                return self.claude_wrapper.run_directly().await;
            }
        }

        // Claude プロセス起動（非対話モードのみ監視）
        let mut claude_process = self.claude_wrapper.spawn().await?;
        self.process_monitor.set_process(&claude_process);

        // 標準出力・エラー出力の監視開始
        let stdout_handle = self.start_stdout_monitoring(&mut claude_process).await?;
        let stderr_handle = self.start_stderr_monitoring(&mut claude_process).await?;

        // プロセス監視開始
        let process_handle = self.start_process_monitoring().await;

        if self.verbose {
            println!("👀 Monitoring started for Claude process");
        }

        // Claude プロセスの終了を待つ
        let exit_status = claude_process.wait().await?;

        if self.verbose {
            println!("🏁 Claude process exited with status: {:?}", exit_status);
        }

        // 監視タスクを終了
        stdout_handle.abort();
        stderr_handle.abort();
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

                    // ログファイルに書き込み（stdout のみ）
                    if let Some(ref mut writer) = log_writer {
                        let log_line = format!("{}\n", line);
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

    /// 対話モード検出
    fn is_interactive_mode(&self) -> bool {
        let args = self.claude_wrapper.get_args();
        
        // 引数なし = 対話モード
        if args.is_empty() {
            return true;
        }
        
        // --printオプションがない = 対話モード
        !args.contains(&"--print".to_string())
    }

    /// scriptコマンドを使ったインタラクティブClaude実行（ログ付き）
    async fn run_claude_with_log_only(&mut self) -> Result<()> {
        if self.verbose {
            println!("🚀 Starting Claude with script logging: {}", self.claude_wrapper.to_command_string());
        }

        use tokio::process::Command;

        // ログファイルパスが設定されている場合
        if let Some(ref log_path) = self.log_file {
            // claude の引数を構築
            let claude_args = self.claude_wrapper.get_args();
            let mut full_args = vec!["claude".to_string()];
            full_args.extend(claude_args.iter().cloned());

            // script コマンドでClaude実行をログ記録
            // -q: quiet mode (no startup/done messages)
            // -a: append to log file
            let script_command = format!("script -q -a {} {}", 
                log_path.to_string_lossy(),
                full_args.join(" ")
            );

            if self.verbose {
                println!("📝 Running: sh -c '{}'", script_command);
            }

            // シェル経由でコマンド実行
            let mut cmd = Command::new("sh");
            cmd.arg("-c").arg(&script_command);
            
            if let Some(dir) = self.claude_wrapper.get_working_dir() {
                cmd.current_dir(dir);
            }

            // 標準入出力はそのまま通す（インタラクティブ性を保持）
            cmd.stdout(std::process::Stdio::inherit())
                .stderr(std::process::Stdio::inherit())
                .stdin(std::process::Stdio::inherit());

            // プロセス実行・待機
            let exit_status = cmd.status().await
                .map_err(|e| anyhow::anyhow!("Failed to run Claude with script: {}", e))?;

            if self.verbose {
                println!("🏁 Claude script process exited with status: {:?}", exit_status);
            }
        } else {
            // ログファイル未設定の場合は通常実行
            return self.claude_wrapper.run_directly().await;
        }

        Ok(())
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