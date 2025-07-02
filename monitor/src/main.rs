use clap::Parser;

use climonitor_monitor::live_ui::LiveUI;
use climonitor_monitor::monitor_server::MonitorServer;

#[derive(Parser)]
#[command(name = "climonitor")]
#[command(about = "Monitor CLI tool session status in real-time")]
struct Cli {
    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Live mode - start monitor server for real-time updates (default behavior)
    #[arg(long)]
    live: bool,

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
    } else {
        // デフォルト：ライブモード
        run_live_mode(cli.verbose, cli.log_file).await?;
    }

    Ok(())
}

/// ライブモード実行
async fn run_live_mode(verbose: bool, log_file: Option<std::path::PathBuf>) -> anyhow::Result<()> {
    if verbose {
        println!("🔧 Starting monitor server in verbose mode...");
        if let Some(ref log_path) = log_file {
            let log_display = log_path.display();
            println!("📝 Log file: {log_display}");
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
                    eprintln!("❌ Monitor server error: {e}");
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
                    eprintln!("❌ Live UI error: {e}");
                    return Err(e);
                }
            }
        }
    }

    Ok(())
}
