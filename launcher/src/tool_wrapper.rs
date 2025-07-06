use crate::cli_tool::{get_pty_size, setup_common_pty_environment, CliTool};
use anyhow::Result;
use portable_pty::CommandBuilder;
use std::process::Stdio;
use tokio::process::{Child, Command};

/// CLI ツール実行ラッパー（汎用）
pub struct ToolWrapper {
    tool: Box<dyn CliTool>,
    args: Vec<String>,
    working_dir: Option<std::path::PathBuf>,
}

impl ToolWrapper {
    /// 新しいToolWrapperを作成
    pub fn new(tool: Box<dyn CliTool>, args: Vec<String>) -> Self {
        Self {
            tool,
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

    /// CLI ツール プロセスを起動（従来のパイプベース）
    pub async fn spawn(&self) -> Result<Child> {
        let mut cmd = if cfg!(windows) {
            // Windows環境では.cmdファイルを実行するためにcmd.exeを使用
            // フルパスを指定してコマンドを実行
            let mut cmd = Command::new("cmd");
            cmd.args(["/C"]);
            // コマンド名と引数を一つの文字列として渡す
            let full_command = format!("{} {}", self.tool.command_name(), self.args.join(" "));
            cmd.arg(full_command);
            cmd
        } else {
            let mut cmd = Command::new(self.tool.command_name());
            cmd.args(&self.args);
            cmd
        };

        if let Some(working_dir) = &self.working_dir {
            // Windows環境でのnull terminator問題を回避
            let path_str = working_dir.to_string_lossy();
            let clean_path = path_str.trim_end_matches('\0');
            cmd.current_dir(clean_path);
        }

        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::piped());

        let child = cmd.spawn()?;
        Ok(child)
    }

    /// CLI ツール プロセスをPTYで起動（TTY環境を提供）
    pub fn spawn_with_pty(
        &self,
    ) -> Result<(
        Box<dyn portable_pty::Child + Send + Sync>,
        Box<dyn portable_pty::MasterPty + Send>,
    )> {
        let pty_system = crate::cli_tool::create_optimized_pty_system();

        // PTYペアを作成
        let pty_pair = pty_system.openpty(get_pty_size())?;

        // コマンドを構築
        let mut cmd = if cfg!(windows) {
            // Windows環境では.cmdファイルを実行するためにcmd.exeを使用
            let mut cmd = CommandBuilder::new("cmd");
            cmd.args(["/C"]);
            // コマンド名と引数を一つの文字列として渡す
            let full_command = format!("{} {}", self.tool.command_name(), self.args.join(" "));
            cmd.arg(full_command);
            cmd
        } else {
            let mut cmd = CommandBuilder::new(self.tool.command_name());
            cmd.args(&self.args);
            cmd
        };

        // 作業ディレクトリを設定（指定がない場合は現在のディレクトリ）
        let working_dir = self.working_dir.clone().unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
        });

        // Windows環境でのnull terminator問題を回避
        let path_str = working_dir.to_string_lossy();
        let clean_path = path_str.trim_end_matches('\0');
        cmd.cwd(clean_path);

        // 共通環境変数を設定
        setup_common_pty_environment(&mut cmd);

        // ツール固有の環境変数を設定
        self.tool.setup_environment(&mut cmd);

        // PTYスレーブでプロセスを起動
        let child = pty_pair.slave.spawn_command(cmd)?;

        Ok((child, pty_pair.master))
    }

    /// CLI ツール を直接実行（パススルー）
    pub async fn run_directly(&self) -> Result<()> {
        let mut cmd = if cfg!(windows) {
            // Windows環境では.cmdファイルを実行するためにcmd.exeを使用
            let mut cmd = Command::new("cmd");
            cmd.args(["/C"]);
            // コマンド名と引数を一つの文字列として渡す
            let full_command = format!("{} {}", self.tool.command_name(), self.args.join(" "));
            cmd.arg(full_command);
            cmd
        } else {
            let mut cmd = Command::new(self.tool.command_name());
            cmd.args(&self.args);
            cmd
        };

        if let Some(working_dir) = &self.working_dir {
            cmd.current_dir(working_dir);
        }

        cmd.stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        let status = cmd.status().await?;

        if !status.success() {
            return Err(anyhow::anyhow!(
                "{} exited with status: {}",
                self.tool.command_name(),
                status
            ));
        }

        Ok(())
    }

    /// プロジェクト名を推測
    pub fn guess_project_name(&self) -> Option<String> {
        let working_dir = self
            .working_dir
            .as_deref()
            .unwrap_or_else(|| std::path::Path::new("."));

        self.tool.guess_project_name(&self.args, working_dir)
    }

    /// コマンド文字列を生成（デバッグ用）
    pub fn to_command_string(&self) -> String {
        self.tool.to_command_string(&self.args)
    }

    /// ツールの参照を取得
    pub fn get_tool(&self) -> &dyn CliTool {
        self.tool.as_ref()
    }

    /// ツールタイプを取得
    pub fn get_tool_type(&self) -> crate::cli_tool::CliToolType {
        match self.tool.command_name() {
            "claude" => crate::cli_tool::CliToolType::Claude,
            "gemini" => crate::cli_tool::CliToolType::Gemini,
            _ => crate::cli_tool::CliToolType::Claude, // デフォルト
        }
    }
}
