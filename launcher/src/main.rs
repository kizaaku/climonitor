use anyhow::Result;
use clap::{Arg, Command};

// lib crate から import
use climonitor_launcher::cli_tool::{CliToolFactory, CliToolType};
use climonitor_launcher::grpc_client::GrpcLauncherClient;
use climonitor_launcher::tool_wrapper::ToolWrapper;
use climonitor_launcher::transport_client::LauncherClient;
use climonitor_shared::Config;

#[tokio::main]
async fn main() -> Result<()> {
    let matches = Command::new("climonitor-launcher")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Launch Claude Code or Gemini CLI with real-time session monitoring")
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help("Enable verbose output")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("log_file")
                .long("log-file")
                .help("Log file path to save CLI tool output")
                .value_name("FILE"),
        )
        .arg(
            Arg::new("grpc")
                .long("grpc")
                .help("Use gRPC protocol instead of raw TCP/Unix socket")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("connect")
                .long("connect")
                .help("Connection address (Unix: socket path)")
                .value_name("ADDR"),
        )
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .help("Configuration file path")
                .value_name("FILE"),
        )
        .arg(
            Arg::new("cli_args")
                .help("CLI tool and arguments (e.g., 'claude --help' or 'gemini chat')")
                .num_args(0..)
                .trailing_var_arg(true)
                .allow_hyphen_values(true),
        )
        .get_matches();

    let verbose = matches.get_flag("verbose");
    let log_file = matches
        .get_one::<String>("log_file")
        .map(std::path::PathBuf::from);
    let use_grpc = matches.get_flag("grpc");
    let connect_addr = matches.get_one::<String>("connect");
    let config_path = matches
        .get_one::<String>("config")
        .map(std::path::PathBuf::from);
    let cli_args: Vec<String> = matches
        .get_many::<String>("cli_args")
        .unwrap_or_default()
        .map(|s| s.to_string())
        .collect();

    // CLI ツールタイプを判定
    let (tool_type, tool_args) = if let Some(first_arg) = cli_args.first() {
        if let Some(cli_tool_type) = CliToolType::from_command(first_arg) {
            (cli_tool_type, cli_args[1..].to_vec())
        } else {
            // デフォルトはClaude（後方互換性）
            (CliToolType::Claude, cli_args)
        }
    } else {
        // 引数なしの場合はClaude
        (CliToolType::Claude, vec![])
    };

    if verbose {
        println!("🔧 climonitor-launcher starting...");
        println!("🛠️  Tool: {tool_type:?}");
        println!("📝 Args: {tool_args:?}");
    }

    // 設定を読み込み（優先順位: CLI > 環境変数 > 設定ファイル > デフォルト）
    let mut config = if let Some(config_path) = &config_path {
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
    if let Some(addr) = connect_addr {
        if !addr.starts_with("tcp://") {
            config.connection.unix_socket_path = Some(addr.into());
        }
    }
    if verbose {
        config.logging.verbose = true;
    }
    if let Some(log_file_path) = log_file.clone() {
        config.logging.log_file = Some(log_file_path);
    }

    // ログシステムの初期化
    config.logging.init_logging();

    // 接続設定を生成
    let connection_config = config.to_connection_config();

    if verbose {
        println!("🔧 Connection config: {connection_config:?}");
    }

    // ツールを作成
    let cli_tool = CliToolFactory::create_tool(tool_type.clone());

    // 作業ディレクトリを取得してnull terminatorを除去
    let current_dir = std::env::current_dir()?;
    let working_dir = {
        let path_str = current_dir.to_string_lossy();
        // Windows環境でのnull terminator問題を回避
        let clean_path = path_str.trim_end_matches('\0');
        std::path::PathBuf::from(clean_path)
    };

    let tool_wrapper = ToolWrapper::new(cli_tool, tool_args).working_dir(working_dir);

    if use_grpc {
        // gRPC接続
        if verbose {
            println!("🚀 Using gRPC protocol");
        }

        let grpc_client = if let Some(addr) = connect_addr {
            let endpoint = if addr.starts_with("http://") || addr.starts_with("https://") {
                addr.to_string()
            } else {
                format!("http://{addr}")
            };
            if verbose {
                println!("🔧 gRPC endpoint: {endpoint}");
            }
            let launcher_id = climonitor_shared::generate_connection_id();
            let session_id = climonitor_shared::generate_connection_id();
            GrpcLauncherClient::new_with_endpoint(launcher_id, session_id, endpoint).await?
        } else {
            GrpcLauncherClient::new(&connection_config).await?
        };

        // Connect message送信
        grpc_client
            .send_connect(
                tool_wrapper.guess_project_name(),
                tool_type,
                tool_wrapper.get_args().to_vec(),
                tool_wrapper
                    .get_working_dir()
                    .cloned()
                    .unwrap_or_else(|| std::env::current_dir().unwrap()),
            )
            .await?;

        if verbose {
            println!("🔄 Running CLI tool with gRPC monitoring...");
        }

        // gRPC用のLauncherClient作成（PTY監視付き）
        let mut launcher = LauncherClient::new_with_grpc(
            tool_wrapper,
            grpc_client,
            config.logging.verbose,
            config.logging.log_file,
        )
        .await?;

        // monitor接続時のみターミナルガード作成
        let _terminal_guard = if launcher.is_connected() {
            use climonitor_launcher::transport_client::create_terminal_guard_global;
            Some(create_terminal_guard_global(config.logging.verbose)?)
        } else {
            None
        };

        // gRPCパスでも同様のシグナルハンドリングを実装
        #[cfg(unix)]
        let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
        #[cfg(unix)]
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

        // プラットフォーム固有のシグナルハンドリング
        #[cfg(unix)]
        {
            tokio::select! {
                result = launcher.run_claude() => {
                    match result {
                        Ok(_) => {
                            if config.logging.verbose {
                                println!("✅ CLI tool finished successfully");
                            }
                        }
                        Err(e) => {
                            climonitor_shared::log_error!(climonitor_shared::LogCategory::System, "CLI tool execution failed: {e}");
                            if let Some(guard) = _terminal_guard {
                                drop(guard); // ターミナル設定を明示的に復元
                            }
                            climonitor_launcher::transport_client::force_restore_terminal(); // 強制復元
                            std::process::exit(1);
                        }
                    }
                }
                _ = sigint.recv() => {
                    if config.logging.verbose {
                        println!("\n🛑 Received SIGINT, shutting down gracefully...");
                    }
                    if let Some(guard) = _terminal_guard {
                        drop(guard); // ターミナル設定を明示的に復元
                    }
                    climonitor_launcher::transport_client::force_restore_terminal(); // 強制復元
                    std::process::exit(130); // 128 + 2 (SIGINT)
                }
                _ = sigterm.recv() => {
                    if config.logging.verbose {
                        println!("\n🛑 Received SIGTERM, shutting down gracefully...");
                    }
                    if let Some(guard) = _terminal_guard {
                        drop(guard); // ターミナル設定を明示的に復元
                    }
                    climonitor_launcher::transport_client::force_restore_terminal(); // 強制復元
                    std::process::exit(143); // 128 + 15 (SIGTERM)
                }
                _ = tokio::signal::ctrl_c() => {
                    if config.logging.verbose {
                        println!("\n🛑 Received Ctrl+C, shutting down gracefully...");
                    }
                    if let Some(guard) = _terminal_guard {
                        drop(guard); // ターミナル設定を明示的に復元
                    }
                    climonitor_launcher::transport_client::force_restore_terminal(); // 強制復元
                    std::process::exit(130); // 128 + 2 (SIGINT)
                }
            }
        }

        #[cfg(not(unix))]
        {
            tokio::select! {
                result = launcher.run_claude() => {
                    match result {
                        Ok(_) => {
                            if config.logging.verbose {
                                println!("✅ CLI tool finished successfully");
                            }
                        }
                        Err(e) => {
                            climonitor_shared::log_error!(climonitor_shared::LogCategory::System, "❌ CLI tool execution failed: {e}");
                            // Windows版では正常終了（TerminalGuardのDropが自動的に実行される）
                            std::process::exit(1);
                        }
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    if config.logging.verbose {
                        println!("\n🛑 Received Ctrl+C, shutting down gracefully...");
                    }
                    // Windows版では正常終了（TerminalGuardのDropが自動的に実行される）
                    return Ok(());
                }
            }
        }

        if config.logging.verbose {
            println!("👋 climonitor-launcher finished");
        }

        return Ok(());
    }

    // 従来のTCP/Unix socket接続
    let mut launcher = LauncherClient::new(
        tool_wrapper,
        connection_config,
        config.logging.verbose,
        config.logging.log_file,
    )
    .await?;

    // monitor接続時のみターミナルガード作成
    let _terminal_guard = if launcher.is_connected() {
        use climonitor_launcher::transport_client::create_terminal_guard_global;
        Some(create_terminal_guard_global(config.logging.verbose)?)
    } else {
        None
    };

    // クロスプラットフォーム対応のシグナルハンドリング
    #[cfg(unix)]
    let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    #[cfg(unix)]
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

    // プラットフォーム固有のシグナルハンドリング
    #[cfg(unix)]
    {
        tokio::select! {
            result = launcher.run_claude() => {
                match result {
                    Ok(_) => {
                        if config.logging.verbose {
                            println!("✅ CLI tool finished successfully");
                        }
                    }
                    Err(e) => {
                        climonitor_shared::log_error!(climonitor_shared::LogCategory::System, "❌ CLI tool execution failed: {e}");
                        if let Some(guard) = _terminal_guard {
                            drop(guard); // ターミナル設定を明示的に復元
                        }
                        climonitor_launcher::transport_client::force_restore_terminal(); // 強制復元
                        std::process::exit(1);
                    }
                }
            }
            _ = sigint.recv() => {
                if config.logging.verbose {
                    println!("\n🛑 Received SIGINT, shutting down gracefully...");
                }
                if let Some(guard) = _terminal_guard {
                    drop(guard); // ターミナル設定を明示的に復元
                }
                climonitor_launcher::transport_client::force_restore_terminal(); // 強制復元
                std::process::exit(130); // 128 + 2 (SIGINT)
            }
            _ = sigterm.recv() => {
                if config.logging.verbose {
                    println!("\n🛑 Received SIGTERM, shutting down gracefully...");
                }
                if let Some(guard) = _terminal_guard {
                    drop(guard); // ターミナル設定を明示的に復元
                }
                climonitor_launcher::transport_client::force_restore_terminal(); // 強制復元
                std::process::exit(143); // 128 + 15 (SIGTERM)
            }
            _ = tokio::signal::ctrl_c() => {
                if config.logging.verbose {
                    println!("\n🛑 Received Ctrl+C, shutting down gracefully...");
                }
                if let Some(guard) = _terminal_guard {
                    drop(guard); // ターミナル設定を明示的に復元
                }
                climonitor_launcher::transport_client::force_restore_terminal(); // 強制復元
                std::process::exit(130); // 128 + 2 (SIGINT)
            }
        }
    }

    #[cfg(not(unix))]
    {
        tokio::select! {
            result = launcher.run_claude() => {
                match result {
                    Ok(_) => {
                        if config.logging.verbose {
                            println!("✅ CLI tool finished successfully");
                        }
                    }
                    Err(e) => {
                        climonitor_shared::log_error!(climonitor_shared::LogCategory::System, "❌ CLI tool execution failed: {e}");
                        // Windows版では正常終了（TerminalGuardのDropが自動的に実行される）
                        std::process::exit(1);
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                if config.logging.verbose {
                    println!("\n🛑 Received Ctrl+C, shutting down gracefully...");
                }
                // Windows版では正常終了（TerminalGuardのDropが自動的に実行される）
                return Ok(());
            }
        }
    }

    if config.logging.verbose {
        println!("👋 climonitor-launcher finished");
    }

    Ok(())
}
