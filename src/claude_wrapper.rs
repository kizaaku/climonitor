use anyhow::Result;
use std::process::Stdio;
use tokio::process::{Child, Command};
use portable_pty::{native_pty_system, PtySize, CommandBuilder};
use terminal_size::{Width, Height, terminal_size};

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

    /// ワーキングディレクトリを設定
    pub fn working_dir<P: Into<std::path::PathBuf>>(mut self, dir: P) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    /// 引数を取得
    pub fn get_args(&self) -> &[String] {
        &self.args
    }

    /// ワーキングディレクトリを取得
    pub fn get_working_dir(&self) -> Option<&std::path::PathBuf> {
        self.working_dir.as_ref()
    }

    /// Claude プロセスを起動（従来のパイプベース）
    pub async fn spawn(&self) -> Result<Child> {
        let mut cmd = Command::new("claude");
        cmd.args(&self.args);
        
        if let Some(working_dir) = &self.working_dir {
            cmd.current_dir(working_dir);
        }
        
        cmd.stdout(Stdio::piped())
           .stderr(Stdio::piped())
           .stdin(Stdio::inherit());
        
        let child = cmd.spawn()?;
        Ok(child)
    }

    /// Claude プロセスをPTYで起動（TTY環境を提供）
    pub fn spawn_with_pty(&self) -> Result<(Box<dyn portable_pty::Child + Send + Sync>, Box<dyn portable_pty::MasterPty + Send>)> {
        let pty_system = native_pty_system();
        
        // 実際の端末サイズを取得
        let (cols, rows) = if let Some((Width(w), Height(h))) = terminal_size() {
            (w, h)
        } else {
            (80, 24) // フォールバック
        };
        
        // PTYペアを作成
        let pty_pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        
        // Claudeコマンドを構築
        let mut cmd = CommandBuilder::new("claude");
        cmd.args(&self.args);
        
        // 作業ディレクトリを設定（指定がない場合は現在のディレクトリ）
        let working_dir = self.working_dir.as_ref()
            .map(|p| p.clone())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")));
        cmd.cwd(working_dir);
        
        // PTYスレーブでClaudeを起動
        let child = pty_pair.slave.spawn_command(cmd)?;
        
        Ok((child, pty_pair.master))
    }

    /// Claude を直接実行（パススルー）
    pub async fn run_directly(&self) -> Result<()> {
        let mut cmd = Command::new("claude");
        cmd.args(&self.args);
        
        if let Some(working_dir) = &self.working_dir {
            cmd.current_dir(working_dir);
        }
        
        cmd.stdin(Stdio::inherit())
           .stdout(Stdio::inherit())
           .stderr(Stdio::inherit());
        
        let status = cmd.status().await?;
        
        if !status.success() {
            return Err(anyhow::anyhow!("Claude exited with status: {}", status));
        }
        
        Ok(())
    }

    /// プロジェクト名を推測
    pub fn guess_project_name(&self) -> Option<String> {
        // --project 引数から取得を試行
        if let Some(project_idx) = self.args.iter().position(|arg| arg == "--project") {
            if let Some(project_name) = self.args.get(project_idx + 1) {
                return Some(project_name.clone());
            }
        }
        
        // 作業ディレクトリ名から推測
        if let Some(working_dir) = &self.working_dir {
            if let Some(dir_name) = working_dir.file_name() {
                if let Some(name_str) = dir_name.to_str() {
                    return Some(name_str.to_string());
                }
            }
        }
        
        // 現在のディレクトリ名から推測
        if let Ok(current_dir) = std::env::current_dir() {
            if let Some(dir_name) = current_dir.file_name() {
                if let Some(name_str) = dir_name.to_str() {
                    return Some(name_str.to_string());
                }
            }
        }
        
        None
    }

    /// コマンド文字列を生成（デバッグ用）
    pub fn to_command_string(&self) -> String {
        let mut cmd = vec!["claude".to_string()];
        cmd.extend(self.args.clone());
        cmd.join(" ")
    }
}