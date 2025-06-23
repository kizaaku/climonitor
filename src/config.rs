use std::path::PathBuf;
use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub claude_log_dir: PathBuf,
    pub debug_mode: bool,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        // .env.localファイルを読み込み
        if let Ok(env_path) = std::env::current_dir() {
            let env_file = env_path.join(".env.local");
            if env_file.exists() {
                if let Err(e) = dotenvy::from_path(&env_file) {
                    eprintln!("Warning: Failed to load .env.local: {}", e);
                }
            }
        }

        // ログディレクトリの設定
        let claude_log_dir = if let Ok(custom_dir) = env::var("CLAUDE_LOG_DIR") {
            PathBuf::from(custom_dir)
        } else {
            // デフォルトのClaudeプロジェクトディレクトリ
            let home_dir = dirs::home_dir()
                .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
            home_dir.join(".claude").join("projects")
        };

        // デバッグモードの設定
        let debug_mode = env::var("CCMONITOR_DEBUG")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false);

        Ok(Config {
            claude_log_dir,
            debug_mode,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::tempdir;

    #[test]
    fn test_default_config() {
        // テスト用環境変数をクリア
        env::remove_var("CLAUDE_LOG_DIR");
        env::remove_var("CCMONITOR_DEBUG");
        
        let config = Config::load().unwrap();
        
        // デフォルトパスの確認
        assert!(config.claude_log_dir.ends_with(".claude/projects"));
        assert!(!config.debug_mode);
    }

    #[test]
    fn test_custom_log_dir() {
        let temp_dir = tempdir().unwrap();
        let custom_path = temp_dir.path().to_str().unwrap();
        
        // テスト開始時にクリア
        env::remove_var("CCMONITOR_DEBUG");
        env::set_var("CLAUDE_LOG_DIR", custom_path);
        
        let config = Config::load().unwrap();
        
        assert_eq!(config.claude_log_dir, PathBuf::from(custom_path));
        
        // クリーンアップ
        env::remove_var("CLAUDE_LOG_DIR");
    }

    #[test]
    fn test_debug_mode_true() {
        // テスト開始時にクリア
        env::remove_var("CLAUDE_LOG_DIR");
        env::remove_var("CCMONITOR_DEBUG");
        
        env::set_var("CCMONITOR_DEBUG", "1");
        
        let config = Config::load().unwrap();
        
        assert!(config.debug_mode, "Expected debug_mode to be true when CCMONITOR_DEBUG=1");
        
        // クリーンアップ
        env::remove_var("CCMONITOR_DEBUG");
    }

    #[test]
    fn test_debug_mode_false() {
        // テスト開始時にクリア
        env::remove_var("CLAUDE_LOG_DIR");
        env::remove_var("CCMONITOR_DEBUG");
        
        env::set_var("CCMONITOR_DEBUG", "0");
        
        let config = Config::load().unwrap();
        
        assert!(!config.debug_mode);
        
        // クリーンアップ
        env::remove_var("CCMONITOR_DEBUG");
    }

    #[test] 
    fn test_debug_mode_case_insensitive() {
        // テスト開始時にクリア
        env::remove_var("CLAUDE_LOG_DIR");
        env::remove_var("CCMONITOR_DEBUG");
        
        env::set_var("CCMONITOR_DEBUG", "TRUE");
        
        let config = Config::load().unwrap();
        
        assert!(config.debug_mode);
        
        // クリーンアップ
        env::remove_var("CCMONITOR_DEBUG");
    }
}