use anyhow::Result;
use clap::{Arg, Command};

// lib crate から import
use ccmonitor_launcher::cli_tool::{CliToolFactory, CliToolType};
use ccmonitor_launcher::launcher_client::LauncherClient;
use ccmonitor_launcher::tool_wrapper::ToolWrapper;

#[tokio::main]
async fn main() -> Result<()> {
    let matches = Command::new("ccmonitor-launcher")
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
        println!("🔧 ccmonitor-launcher starting...");
        println!("🛠️  Tool: {:?}", tool_type);
        println!("📝 Args: {:?}", tool_args);
    }

    // ツールを作成
    let cli_tool = CliToolFactory::create_tool(tool_type);
    let tool_wrapper = ToolWrapper::new(cli_tool, tool_args).working_dir(std::env::current_dir()?);

    // Launcher クライアントを作成（接続は内部で自動実行）
    let mut launcher = LauncherClient::new(
        tool_wrapper,
        None, // デフォルトソケットパスを使用
        verbose,
        log_file,
    )
    .await?;

    // monitor接続時のみターミナルガード作成
    #[cfg(unix)]
    let _terminal_guard = if launcher.is_connected() {
        use ccmonitor_launcher::launcher_client::create_terminal_guard_global;
        Some(create_terminal_guard_global(verbose)?)
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
                        if verbose {
                            println!("✅ CLI tool finished successfully");
                        }
                    }
                    Err(e) => {
                        eprintln!("❌ CLI tool execution failed: {}", e);
                        #[cfg(unix)]
                        {
                            if let Some(guard) = _terminal_guard {
                                drop(guard); // ターミナル設定を明示的に復元
                            }
                            ccmonitor_launcher::launcher_client::force_restore_terminal(); // 強制復元
                        }
                        std::process::exit(1);
                    }
                }
            }
            _ = sigint.recv() => {
                if verbose {
                    println!("\n🛑 Received SIGINT, shutting down gracefully...");
                }
                #[cfg(unix)]
                {
                    if let Some(guard) = _terminal_guard {
                        drop(guard); // ターミナル設定を明示的に復元
                    }
                    ccmonitor_launcher::launcher_client::force_restore_terminal(); // 強制復元
                }
                std::process::exit(130); // 128 + 2 (SIGINT)
            }
            _ = sigterm.recv() => {
                if verbose {
                    println!("\n🛑 Received SIGTERM, shutting down gracefully...");
                }
                #[cfg(unix)]
                {
                    if let Some(guard) = _terminal_guard {
                        drop(guard); // ターミナル設定を明示的に復元
                    }
                    ccmonitor_launcher::launcher_client::force_restore_terminal(); // 強制復元
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
                if verbose {
                    println!("✅ CLI tool finished successfully");
                }
            }
            Err(e) => {
                eprintln!("❌ CLI tool execution failed: {}", e);
                std::process::exit(1);
            }
        }
    }

    if verbose {
        println!("👋 ccmonitor-launcher finished");
    }

    Ok(())
}
