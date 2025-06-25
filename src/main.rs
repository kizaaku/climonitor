use clap::Parser;

mod claude_wrapper;
mod launcher_client;
mod live_ui;
mod monitor_server;
mod protocol;
mod session_manager;
mod unicode_utils;

use monitor_server::MonitorServer;
use live_ui::{LiveUI, print_snapshot};

#[derive(Parser)]
#[command(name = "ccmonitor")]
#[command(about = "Monitor Claude session status in real-time")]
struct Cli {
    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
    
    /// Live mode - connect to ccmonitor-launcher for real-time updates
    #[arg(long)]
    live: bool,
    
    /// Non-interactive mode (print status and exit)
    #[arg(long)]
    no_tui: bool,
    
    /// Log file path to save Claude's standard output
    #[arg(long)]
    log_file: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    
    if cli.live {
        // ライブモード：Monitor サーバーとして動作
        run_live_mode(cli.verbose, cli.log_file).await?;
    } else if cli.no_tui {
        // 非対話モード：一度だけ状態表示
        run_snapshot_mode(cli.verbose).await?;
    } else {
        // デフォルト：ライブモード
        println!("💡 Starting in live mode. Use --no-tui for snapshot mode.");
        run_live_mode(cli.verbose, cli.log_file).await?;
    }
    
    Ok(())
}

/// ライブモード実行
async fn run_live_mode(verbose: bool, log_file: Option<std::path::PathBuf>) -> anyhow::Result<()> {
    if verbose {
        println!("🔧 Starting monitor server in verbose mode...");
        if let Some(ref log_path) = log_file {
            println!("📝 Log file: {}", log_path.display());
        }
    }

    // Monitor サーバー開始
    let mut server = MonitorServer::new(verbose, log_file)?;
    server.start().await?;

    // UI更新チャネル取得
    let update_receiver = server.subscribe_ui_updates();
    let session_manager = server.get_session_manager();

    // LiveUI開始
    let mut live_ui = LiveUI::new(session_manager, update_receiver, verbose);
    
    // サーバーとUIを並行実行
    tokio::select! {
        result = server.run() => {
            match result {
                Ok(_) => {
                    if verbose {
                        println!("✅ Monitor server finished successfully");
                    }
                }
                Err(e) => {
                    eprintln!("❌ Monitor server error: {}", e);
                    return Err(e);
                }
            }
        }
        
        result = live_ui.run() => {
            match result {
                Ok(_) => {
                    if verbose {
                        println!("✅ Live UI finished successfully");
                    }
                }
                Err(e) => {
                    eprintln!("❌ Live UI error: {}", e);
                    return Err(e);
                }
            }
        }
    }

    Ok(())
}

/// スナップショットモード実行
async fn run_snapshot_mode(verbose: bool) -> anyhow::Result<()> {
    if verbose {
        println!("📸 Running in snapshot mode...");
    }

    // Monitor サーバーに接続を試行
    match try_connect_to_monitor().await {
        Ok(session_manager) => {
            // 接続成功：現在の状態を表示
            print_snapshot(session_manager, verbose).await;
        }
        Err(_) => {
            // 接続失敗：Monitor が起動していない
            println!("📊 Claude Session Monitor - Snapshot");
            println!("{}", "═".repeat(50));
            println!("⚠️  Monitor server not running");
            println!("💡 Start the monitor server with:");
            println!("   ccmonitor --live");
            println!();
            println!("💡 Then start launchers with:");
            println!("   ccmonitor-launcher claude");
        }
    }

    Ok(())
}

/// Monitor サーバーへの接続試行
async fn try_connect_to_monitor() -> anyhow::Result<std::sync::Arc<tokio::sync::RwLock<session_manager::SessionManager>>> {
    use tokio::net::UnixStream;
    use tokio::time::{timeout, Duration};

    let socket_path = MonitorServer::get_client_socket_path()?;
    
    // 接続タイムアウト: 2秒
    let _stream = timeout(Duration::from_secs(2), UnixStream::connect(socket_path)).await??;
    
    // TODO: 実際のセッション情報取得
    // 現在は空のSessionManagerを返す（デモ用）
    let session_manager = std::sync::Arc::new(tokio::sync::RwLock::new(session_manager::SessionManager::new()));
    Ok(session_manager)
}


