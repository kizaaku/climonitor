use notify::{Watcher, RecursiveMode, Result as NotifyResult, Event};
use std::path::Path;
use tokio::sync::mpsc;
use std::fs;
use crate::session::SessionMessage;
use crate::unicode_utils::truncate_str;
use crate::config::Config;

pub struct FileWatcher {
    _watcher: notify::RecommendedWatcher,
    receiver: mpsc::Receiver<SessionMessage>,
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
                                let _ = tx_clone.try_send(msg);
                            }
                        }
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
        } else {
            eprintln!("Warning: Claude projects directory not found at {:?}", claude_dir);
            eprintln!("Make sure Claude is installed and has been run at least once.");
        }

        Ok(Self {
            _watcher: watcher,
            receiver: rx,
        })
    }

    pub async fn next_message(&mut self) -> Option<SessionMessage> {
        self.receiver.recv().await
    }
}

async fn load_existing_files(
    claude_dir: &Path, 
    tx: mpsc::Sender<SessionMessage>
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
                    
                    // 直近5時間以内に更新されたファイルのみを対象とする
                    let now = std::time::SystemTime::now();
                    let five_hours_ago = now.checked_sub(std::time::Duration::from_secs(5 * 60 * 60))?;
                    
                    if modified >= five_hours_ago {
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
                        let _ = tx.send(msg).await;
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