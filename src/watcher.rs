use notify::{Watcher, RecursiveMode, Result as NotifyResult, Event};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use std::fs;
use crate::session::SessionMessage;
use crate::unicode_utils::truncate_str;
use crate::config::Config;

#[derive(Debug)]
pub struct SessionMessageWithFile {
    pub message: SessionMessage,
    pub file_path: PathBuf,
}

pub struct FileWatcher {
    _watcher: notify::RecommendedWatcher,
    receiver: mpsc::Receiver<SessionMessageWithFile>,
}

impl FileWatcher {
    pub fn new() -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::channel(1000);
        let tx_clone = tx.clone();
        
        let mut watcher = notify::recommended_watcher(move |res: NotifyResult<Event>| {
            if let Ok(event) = res {
                for path in event.paths {
                    if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                        if let Ok(messages) = read_jsonl_file(&path) {
                            for msg in messages {
                                let msg_with_file = SessionMessageWithFile {
                                    message: msg,
                                    file_path: path.clone(),
                                };
                                let _ = tx_clone.try_send(msg_with_file);
                            }
                        }
                    } else if path.is_dir() {
                        // 新しいプロジェクトディレクトリが作成された場合、
                        // そのディレクトリ内のjsonlファイルをスキャン
                        // 新しいディレクトリ検出は後で対応
                    }
                }
            }
        })?;

        let config = Config::load()?;
        let claude_dir = &config.claude_log_dir;

        if claude_dir.exists() {
            watcher.watch(&claude_dir, RecursiveMode::Recursive)?;
            
            // 初期読み込み - 既存のファイルをすべて読む
            let initial_tx = tx.clone();
            let claude_dir_clone = claude_dir.clone();
            tokio::spawn(async move {
                if let Err(e) = load_existing_files(&claude_dir_clone, initial_tx).await {
                    eprintln!("Error loading existing files: {}", e);
                }
            });
            
            // 定期的に新しいプロジェクトディレクトリをスキャンする
            let periodic_tx = tx.clone();
            let periodic_claude_dir = claude_dir.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
                loop {
                    interval.tick().await;
                    if let Err(e) = scan_for_new_projects(&periodic_claude_dir, &periodic_tx).await {
                        eprintln!("Error scanning for new projects: {}", e);
                    }
                }
            });
        } else {
            eprintln!("Warning: Claude projects directory not found at {:?}", claude_dir);
            eprintln!("Make sure Claude is installed and has been run at least once.");
        }

        Ok(Self {
            _watcher: watcher,
            receiver: rx,
        })
    }

    pub async fn next_message(&mut self) -> Option<SessionMessageWithFile> {
        self.receiver.recv().await
    }
}

async fn load_existing_files(
    claude_dir: &Path, 
    tx: mpsc::Sender<SessionMessageWithFile>
) -> anyhow::Result<()> {
    let entries = fs::read_dir(claude_dir)?;
    
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_dir() {
            let jsonl_entries = fs::read_dir(&path)?;
            
            // ファイルを更新日時順（新しい順）でソート
            let mut jsonl_files: Vec<_> = jsonl_entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    let path = entry.path();
                    path.extension().and_then(|s| s.to_str()) == Some("jsonl")
                })
                .filter_map(|entry| {
                    let path = entry.path();
                    let metadata = fs::metadata(&path).ok()?;
                    let modified = metadata.modified().ok()?;
                    
                    // 直近24時間以内に更新されたファイルのみを対象とする（範囲を拡張）
                    let now = std::time::SystemTime::now();
                    let twenty_four_hours_ago = now.checked_sub(std::time::Duration::from_secs(24 * 60 * 60))?;
                    
                    if modified >= twenty_four_hours_ago {
                        Some((path, modified))
                    } else {
                        None
                    }
                })
                .collect();
            
            // 更新日時で降順ソート（新しいファイルから）
            jsonl_files.sort_by(|a, b| b.1.cmp(&a.1));
            
            for (jsonl_path, _) in jsonl_files {
                // 最新の1行のみ読み込み（高速化）
                if let Ok(messages) = read_jsonl_file_tail(&jsonl_path, 1) {
                    for msg in messages {
                        let msg_with_file = SessionMessageWithFile {
                            message: msg,
                            file_path: jsonl_path.clone(),
                        };
                        let _ = tx.send(msg_with_file).await;
                    }
                }
            }
        }
    }
    
    Ok(())
}

fn read_jsonl_file(path: &Path) -> anyhow::Result<Vec<SessionMessage>> {
    read_jsonl_file_tail(path, 1) // 最新の1行のみ
}

fn read_jsonl_file_tail(path: &Path, lines: usize) -> anyhow::Result<Vec<SessionMessage>> {
    // 設定からデバッグモードを取得
    let config = Config::load().unwrap_or_else(|_| Config { 
        claude_log_dir: std::path::PathBuf::new(), 
        debug_mode: false 
    });
    let debug_mode = config.debug_mode;
    let content = fs::read_to_string(path)?;
    let mut messages = Vec::new();
    
    // 最後のN行を取得
    let lines_iter = content.lines().rev().take(lines);
    
    for line in lines_iter {
        if line.trim().is_empty() {
            continue;
        }
        
        match serde_json::from_str::<SessionMessage>(line) {
            Ok(msg) => messages.push(msg),
            Err(e) => {
                // デバッグモードでのみエラーを出力
                if debug_mode {
                    eprintln!("JSON parse error in line: {} - Error: {}", 
                        truncate_str(line, 100), e);
                }
                // 通常は静かに無視（summary、tool_resultなど、異なる構造のメッセージが多数存在）
            }
        }
    }
    
    messages.reverse(); // 時系列順に戻す
    Ok(messages)
}

// 新しいディレクトリをスキャンしてjsonlファイルを検出する関数
async fn scan_directory_for_jsonl(
    dir_path: &Path,
    tx: mpsc::Sender<SessionMessageWithFile>
) -> anyhow::Result<()> {
    if !dir_path.is_dir() {
        return Ok(());
    }
    
    let entries = fs::read_dir(dir_path)?;
    
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        
        if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
            if let Ok(messages) = read_jsonl_file_tail(&path, 1) {
                for msg in messages {
                    let msg_with_file = SessionMessageWithFile {
                        message: msg,
                        file_path: path.clone(),
                    };
                    let _ = tx.send(msg_with_file).await;
                }
            }
        }
    }
    
    Ok(())
}

// 新しいプロジェクトディレクトリをスキャンする関数
async fn scan_for_new_projects(
    claude_dir: &Path,
    tx: &mpsc::Sender<SessionMessageWithFile>
) -> anyhow::Result<()> {
    let entries = fs::read_dir(claude_dir)?;
    
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_dir() {
            // 最近作成されたディレクトリかどうかをチェック
            if let Ok(metadata) = fs::metadata(&path) {
                if let Ok(created) = metadata.created() {
                    let now = std::time::SystemTime::now();
                    let five_minutes_ago = now.checked_sub(std::time::Duration::from_secs(5 * 60));
                    
                    if let Some(five_minutes_ago) = five_minutes_ago {
                        if created >= five_minutes_ago {
                            // 新しいディレクトリ内のjsonlファイルをスキャン
                            if let Err(e) = scan_directory_for_jsonl(&path, tx.clone()).await {
                                eprintln!("Error scanning new project directory {:?}: {}", path, e);
                            }
                        }
                    }
                }
            }
        }
    }
    
    Ok(())
}