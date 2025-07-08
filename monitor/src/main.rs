use clap::Parser;

use climonitor_monitor::grpc_server::start_grpc_server;
use climonitor_monitor::live_ui::LiveUI;
use climonitor_monitor::session_manager::SessionManager;
use climonitor_monitor::transport_server::TransportMonitorServer;
use climonitor_shared::{Config, ConnectionConfig};
use std::sync::Arc;

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

    /// Use gRPC protocol instead of raw TCP/Unix socket
    #[arg(long)]
    grpc: bool,

    /// gRPC bind address (only with --grpc)
    #[arg(long, default_value = "127.0.0.1:50051")]
    bind: String,

    /// Unix socket path (default: /tmp/climonitor.sock)
    #[arg(long)]
    socket: Option<std::path::PathBuf>,

    /// Configuration file path
    #[arg(short, long)]
    config: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // 設定を読み込み（優先順位: CLI > 環境変数 > 設定ファイル > デフォルト）
    let mut config = if let Some(config_path) = &cli.config {
        // --config で指定された設定ファイルを読み込み
        Config::from_file(config_path)?
    } else if let Some((config, _path)) = Config::load_auto()? {
        // 自動検出で設定ファイルを読み込み
        config
    } else {
        // デフォルト設定を使用
        Config::default()
    };

    // 環境変数で上書き
    config.apply_env_overrides();

    // CLI引数で上書き
    if let Some(socket_path) = cli.socket {
        config.connection.unix_socket_path = Some(socket_path);
    }
    if cli.verbose {
        config.logging.verbose = true;
    }
    if let Some(log_file) = cli.log_file.clone() {
        config.logging.log_file = Some(log_file);
    }

    // 接続設定を生成
    let connection_config = config.to_connection_config();

    if cli.grpc {
        // gRPCモード：gRPC Monitor サーバーとして動作
        run_grpc_mode(cli.bind, config.logging.verbose, config.logging.log_file).await?;
    } else if cli.live {
        // ライブモード：Monitor サーバーとして動作
        run_live_mode(
            connection_config,
            config.logging.verbose,
            config.logging.log_file,
        )
        .await?;
    } else {
        // デフォルト：ライブモード
        run_live_mode(
            connection_config,
            config.logging.verbose,
            config.logging.log_file,
        )
        .await?;
    }

    Ok(())
}

/// ライブモード実行
async fn run_live_mode(
    config: ConnectionConfig,
    verbose: bool,
    log_file: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    if verbose {
        println!("🔧 Starting monitor server in verbose mode...");
        println!("🔧 Connection config: {config:?}");
        if let Some(ref log_path) = log_file {
            let log_display = log_path.display();
            println!("📝 Log file: {log_display}");
        }
    }

    // Monitor サーバー開始
    let mut server = TransportMonitorServer::new(config, verbose, log_file)?;

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

/// gRPCモード実行
async fn run_grpc_mode(
    bind_addr: String,
    verbose: bool,
    _log_file: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    if verbose {
        println!("🚀 Starting gRPC monitor server...");
        println!("🔧 Bind address: {bind_addr}");
    }

    // SessionManager作成（LiveUIに合わせてRwLockを使用）
    let session_manager = Arc::new(tokio::sync::RwLock::new(SessionManager::new()));

    // UI更新チャネル作成（broadcast channelを使用）
    let (ui_tx, ui_rx) = tokio::sync::broadcast::channel(100);

    // gRPCサーバー用に同じSessionManagerとUI更新チャネルを共有
    let grpc_session_manager = Arc::clone(&session_manager);
    let grpc_ui_tx = ui_tx.clone();
    let grpc_bind_addr = bind_addr.clone();
    let grpc_handle = tokio::spawn(async move {
        if let Err(e) = start_grpc_server(grpc_session_manager, grpc_ui_tx, &grpc_bind_addr).await {
            eprintln!("❌ gRPC server error: {e}");
        }
    });

    // LiveUI開始
    let mut live_ui = LiveUI::new(session_manager, ui_rx, verbose);

    // gRPCサーバーとUIを並行実行
    tokio::select! {
        result = grpc_handle => {
            match result {
                Ok(_) => {
                    if verbose {
                        println!("✅ gRPC server finished successfully");
                    }
                }
                Err(e) => {
                    eprintln!("❌ gRPC server task error: {e}");
                    return Err(anyhow::anyhow!("gRPC server task failed: {e}"));
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
