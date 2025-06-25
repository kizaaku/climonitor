use anyhow::Result;
use clap::{Arg, Command};

// lib crate から import
use ccmonitor_launcher::launcher_client::LauncherClient;
use ccmonitor_launcher::claude_wrapper::ClaudeWrapper;

#[tokio::main]
async fn main() -> Result<()> {
    let matches = Command::new("ccmonitor-launcher")
        .version("0.1.0")
        .about("Launch Claude Code with real-time session monitoring")
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
                .help("Log file path to save Claude's output")
                .value_name("FILE"),
        )
        .arg(
            Arg::new("claude_args")
                .help("Arguments to pass to Claude Code")
                .num_args(0..)
                .trailing_var_arg(true)
                .allow_hyphen_values(true),
        )
        .get_matches();

    let verbose = matches.get_flag("verbose");
    let log_file = matches.get_one::<String>("log_file").map(std::path::PathBuf::from);
    let mut claude_args: Vec<String> = matches
        .get_many::<String>("claude_args")
        .unwrap_or_default()
        .map(|s| s.to_string())
        .collect();

    // 最初の引数が "claude" の場合は除去（重複を避ける）
    if claude_args.first().map(|s| s.as_str()) == Some("claude") {
        claude_args.remove(0);
    }

    if verbose {
        println!("🔧 ccmonitor-launcher starting...");
        println!("📝 Claude args: {:?}", claude_args);
    }

    // Claude wrapper を作成
    let claude_wrapper = ClaudeWrapper::new(claude_args)
        .working_dir(std::env::current_dir()?);

    // ターミナルガード作成（シグナル処理前に作成して復元を保証）
    #[cfg(unix)]
    let _terminal_guard = {
        use ccmonitor_launcher::launcher_client::create_terminal_guard_global;
        create_terminal_guard_global(verbose)?
    };

    // Launcher クライアントを作成（接続は内部で自動実行）
    let mut launcher = LauncherClient::new(
        claude_wrapper,
        None, // デフォルトソケットパスを使用
        verbose,
        log_file,
    ).await?;

    // SIGINT/SIGTERM ハンドラーを設定してターミナル復元を保証
    #[cfg(unix)]
    {
        let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        
        // Claude プロセス実行をシグナル処理と並行して実行
        tokio::select! {
            result = launcher.run_claude() => {
                match result {
                    Ok(_) => {
                        if verbose {
                            println!("✅ Claude finished successfully");
                        }
                    }
                    Err(e) => {
                        eprintln!("❌ Claude execution failed: {}", e);
                        #[cfg(unix)]
                        {
                            drop(_terminal_guard); // ターミナル設定を明示的に復元
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
                    drop(_terminal_guard); // ターミナル設定を明示的に復元
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
                    drop(_terminal_guard); // ターミナル設定を明示的に復元
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
                    println!("✅ Claude finished successfully");
                }
            }
            Err(e) => {
                eprintln!("❌ Claude execution failed: {}", e);
                #[cfg(unix)]
                {
                    drop(_terminal_guard); // ターミナル設定を明示的に復元
                    ccmonitor_launcher::launcher_client::force_restore_terminal(); // 強制復元
                }
                std::process::exit(1);
            }
        }
    }

    if verbose {
        println!("👋 ccmonitor-launcher finished");
    }

    Ok(())
}