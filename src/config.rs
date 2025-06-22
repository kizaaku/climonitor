use std::path::PathBuf;

pub struct Config {
    pub claude_log_dir: PathBuf,
    pub debug_mode: bool,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        // .env.localから環境変数を読み込み
        if let Err(_) = dotenv::from_filename(".env.local") {
            // .env.localが存在しない場合は通常の.envも試行
            let _ = dotenv::dotenv();
        }
        
        let claude_log_dir = get_claude_log_directory()?;
        let debug_mode = std::env::var("CCMONITOR_DEBUG").is_ok();
        
        Ok(Self {
            claude_log_dir,
            debug_mode,
        })
    }
}

fn get_claude_log_directory() -> anyhow::Result<PathBuf> {
    // 環境変数からログディレクトリを取得
    if let Ok(log_dir) = std::env::var("CLAUDE_LOG_DIR") {
        let path = PathBuf::from(log_dir);
        if path.exists() {
            return Ok(path);
        } else {
            eprintln!("Warning: CLAUDE_LOG_DIR specified but directory does not exist: {:?}", path);
        }
    }
    
    // フォールバック: デフォルトのClaudeディレクトリ
    let default_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?
        .join(".claude/projects");
    
    Ok(default_dir)
}