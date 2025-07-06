use anyhow::Result;
use clap::{Arg, Command};

// lib crate から import
use climonitor_launcher::cli_tool::{CliToolFactory, CliToolType};
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
            Arg::new("tcp")
                .long("tcp")
                .help("Use TCP connection instead of Unix domain socket")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("connect")
                .long("connect")
                .help("Connection address (TCP: host:port, Unix: socket path)")
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
    let use_tcp = matches.get_flag("tcp");
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
    if use_tcp {
        config.connection.r#type = "tcp".to_string();
        if let Some(addr) = connect_addr {
            config.connection.tcp_bind_addr = addr.to_string();
        }
    } else if let Some(addr) = connect_addr {
        if addr.starts_with("tcp://") {
            config.connection.r#type = "tcp".to_string();
            config.connection.tcp_bind_addr = addr.strip_prefix("tcp://").unwrap().to_string();
        } else {
            config.connection.r#type = "unix".to_string();
            config.connection.unix_socket_path = Some(addr.into());
        }
    }
    if verbose {
        config.logging.verbose = true;
    }
    if let Some(log_file_path) = log_file.clone() {
        config.logging.log_file = Some(log_file_path);
    }

    // 接続設定を生成
    let connection_config = config.to_connection_config();

    if verbose {
        println!("🔧 Connection config: {connection_config:?}");
    }

    // ツールを作成
    let cli_tool = CliToolFactory::create_tool(tool_type);
    
    // 作業ディレクトリを取得してnull terminatorを除去
    let current_dir = std::env::current_dir()?;
    let working_dir = {
        let path_str = current_dir.to_string_lossy();
        // Windows環境でのnull terminator問題を回避
        let clean_path = path_str.trim_end_matches('\0');
        std::path::PathBuf::from(clean_path)
    };
    
    let tool_wrapper = ToolWrapper::new(cli_tool, tool_args).working_dir(working_dir);

    // Launcher クライアントを作成（接続は内部で自動実行）
    let mut launcher = LauncherClient::new(
        tool_wrapper,
        connection_config,
        config.logging.verbose,
        config.logging.log_file,
    )
    .await?;

    // monitor接続時のみターミナルガード作成
    #[cfg(unix)]
    let _terminal_guard = if launcher.is_connected() {
        use climonitor_launcher::transport_client::create_terminal_guard_global;
        Some(create_terminal_guard_global(config.logging.verbose)?)
    } else {
        None
    };

    // SIGINT/SIGTERM ハンドラーを設定してターミナル復元を保証
    #[cfg(unix)]
    {
        let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

        // CLI ツール プロセス実行をシグナル処理と並行して実行
        tokio::select! {
            result = launcher.run_claude() => {
                match result {
                    Ok(_) => {
                        if config.logging.verbose {
                            println!("✅ CLI tool finished successfully");
                        }
                    }
                    Err(e) => {
                        eprintln!("❌ CLI tool execution failed: {e}");
                        #[cfg(unix)]
                        {
                            if let Some(guard) = _terminal_guard {
                                drop(guard); // ターミナル設定を明示的に復元
                            }
                            climonitor_launcher::transport_client::force_restore_terminal(); // 強制復元
                        }
                        std::process::exit(1);
                    }
                }
            }
            _ = sigint.recv() => {
                if config.logging.verbose {
                    println!("\n🛑 Received SIGINT, shutting down gracefully...");
                }
                #[cfg(unix)]
                {
                    if let Some(guard) = _terminal_guard {
                        drop(guard); // ターミナル設定を明示的に復元
                    }
                    climonitor_launcher::transport_client::force_restore_terminal(); // 強制復元
                }
                std::process::exit(130); // 128 + 2 (SIGINT)
            }
            _ = sigterm.recv() => {
                if config.logging.verbose {
                    println!("\n🛑 Received SIGTERM, shutting down gracefully...");
                }
                #[cfg(unix)]
                {
                    if let Some(guard) = _terminal_guard {
                        drop(guard); // ターミナル設定を明示的に復元
                    }
                    climonitor_launcher::transport_client::force_restore_terminal(); // 強制復元
                }
                std::process::exit(143); // 128 + 15 (SIGTERM)
            }
        }
    }

    #[cfg(not(unix))]
    {
        // 非Unix環境では通常の実行
        match launcher.run_claude().await {
            Ok(_) => {
                if config.logging.verbose {
                    println!("✅ CLI tool finished successfully");
                }
            }
            Err(e) => {
                eprintln!("❌ CLI tool execution failed: {e}");
                std::process::exit(1);
            }
        }
    }

    if config.logging.verbose {
        println!("👋 climonitor-launcher finished");
    }

    Ok(())
}
