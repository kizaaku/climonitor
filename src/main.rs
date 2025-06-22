use clap::Parser;
use std::io::IsTerminal;

mod app;
mod config;
mod session;
mod ui;
mod unicode_utils;
mod watcher;

use app::App;

#[derive(Parser)]
#[command(name = "ccmonitor")]
#[command(about = "Monitor Claude session status in real-time")]
struct Cli {
    /// Filter by project name
    #[arg(short, long)]
    project: Option<String>,
    
    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
    
    /// Non-interactive mode (print status and exit)
    #[arg(long)]
    no_tui: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    
    let mut app = App::new(cli.project, cli.verbose).await?;
    
    if cli.no_tui {
        app.print_status().await?;
    } else {
        // 環境チェック：TUIが利用可能か確認
        if is_tty_available() {
            // TUIモードでエラーをキャッチして表示
            if let Err(e) = app.run().await {
                if cli.verbose {
                    eprintln!("TUI Error: {}", e);
                }
                eprintln!("TUI not available, using non-interactive mode...");
                app.print_status().await?;
            }
        } else {
            if cli.verbose {
                eprintln!("TTY not detected, using non-interactive mode");
            }
            app.print_status().await?;
        }
    }
    
    Ok(())
}

fn is_tty_available() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}