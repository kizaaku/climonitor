use anyhow::Result;
use std::process::Stdio;
use tokio::process::{Child, Command};

/// Claude実行ラッパー
pub struct ClaudeWrapper {
    args: Vec<String>,
    working_dir: Option<std::path::PathBuf>,
}

impl ClaudeWrapper {
    /// 新しいClaudeWrapperを作成
    pub fn new(args: Vec<String>) -> Self {
        Self {
            args,
            working_dir: None,
        }
    }

    /// 作業ディレクトリを設定
    pub fn working_dir<P: Into<std::path::PathBuf>>(mut self, dir: P) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    /// Claudeプロセスを起動（標準出力をパイプ）
    pub async fn spawn(&self) -> Result<Child> {
        let mut cmd = Command::new("claude");
        
        // 引数を設定
        cmd.args(&self.args);
        
        // 作業ディレクトリを設定
        if let Some(ref dir) = self.working_dir {
            cmd.current_dir(dir);
        }
        
        // 標準入出力を設定
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::inherit()); // ユーザー入力はそのまま通す
        
        // プロセス起動
        let child = cmd.spawn()
            .map_err(|e| anyhow::anyhow!("Failed to start Claude: {}", e))?;
        
        Ok(child)
    }

    /// Claudeプロセスを通常通り実行（監視なし）
    pub async fn run_directly(&self) -> Result<()> {
        let mut cmd = Command::new("claude");
        
        // 引数を設定
        cmd.args(&self.args);
        
        // 作業ディレクトリを設定
        if let Some(ref dir) = self.working_dir {
            cmd.current_dir(dir);
        }
        
        // 標準入出力はそのまま通す
        cmd.stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .stdin(Stdio::inherit());
        
        // プロセス起動・待機
        let status = cmd.status().await
            .map_err(|e| anyhow::anyhow!("Failed to run Claude: {}", e))?;
        
        if !status.success() {
            return Err(anyhow::anyhow!("Claude exited with status: {:?}", status));
        }
        
        Ok(())
    }

    /// Claude引数の取得
    pub fn get_args(&self) -> &[String] {
        &self.args
    }

    /// 作業ディレクトリの取得
    pub fn get_working_dir(&self) -> Option<&std::path::PathBuf> {
        self.working_dir.as_ref()
    }

    /// プロジェクト名を推測
    pub fn guess_project_name(&self) -> Option<String> {
        // --project オプションから取得を試行
        if let Some(pos) = self.args.iter().position(|arg| arg == "--project") {
            if let Some(project) = self.args.get(pos + 1) {
                return Some(project.clone());
            }
        }

        // ファイル引数からプロジェクト名を推測
        for arg in &self.args {
            // オプション（--で始まる）をスキップ
            if arg.starts_with('-') {
                continue;
            }
            
            // ファイルパスからプロジェクト名を推測
            if let Some(path) = std::path::Path::new(arg).parent() {
                if let Some(name) = path.file_name() {
                    return Some(name.to_string_lossy().to_string());
                }
            }
        }

        // 作業ディレクトリ名を使用
        if let Some(ref dir) = self.working_dir {
            if let Some(name) = dir.file_name() {
                return Some(name.to_string_lossy().to_string());
            }
        }

        // 現在のディレクトリ名を使用
        std::env::current_dir()
            .ok()
            .and_then(|dir| dir.file_name().map(|name| name.to_string_lossy().to_string()))
    }

    /// デバッグ用: コマンドライン文字列取得
    pub fn to_command_string(&self) -> String {
        let mut cmd_parts = vec!["claude".to_string()];
        cmd_parts.extend(self.args.iter().cloned());
        cmd_parts.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_wrapper_creation() {
        let wrapper = ClaudeWrapper::new(vec!["--help".to_string()]);
        assert_eq!(wrapper.get_args(), &["--help"]);
    }

    #[test]
    fn test_project_name_guessing() {
        // --project オプション
        let wrapper = ClaudeWrapper::new(vec![
            "--project".to_string(),
            "myproject".to_string(),
        ]);
        assert_eq!(wrapper.guess_project_name(), Some("myproject".to_string()));

        // ファイルパス
        let wrapper = ClaudeWrapper::new(vec![
            "src/main.rs".to_string(),
        ]);
        assert_eq!(wrapper.guess_project_name(), Some("src".to_string()));

        // 引数なし
        let wrapper = ClaudeWrapper::new(vec![]);
        // 現在のディレクトリ名が返される（テスト環境依存）
        assert!(wrapper.guess_project_name().is_some());
    }

    #[test]
    fn test_command_string() {
        let wrapper = ClaudeWrapper::new(vec![
            "--project".to_string(),
            "test".to_string(),
            "file.txt".to_string(),
        ]);
        
        assert_eq!(
            wrapper.to_command_string(),
            "claude --project test file.txt"
        );
    }

    #[test]
    fn test_working_dir() {
        let wrapper = ClaudeWrapper::new(vec!["--help".to_string()])
            .working_dir("/tmp");
        
        assert_eq!(wrapper.working_dir, Some("/tmp".into()));
    }
}