// screen_buffer.rs - VTE based screen buffer for accurate state detection

use vte::{Params, Parser, Perform};

/// 端末の一文字を表す構造体
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub char: char,
    pub fg_color: Option<u8>,
    pub bg_color: Option<u8>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            char: ' ',
            fg_color: None,
            bg_color: None,
            bold: false,
            italic: false,
            underline: false,
        }
    }
}

/// スクリーンバッファ - 実際の端末画面を表現
pub struct ScreenBuffer {
    /// グリッド（行×列）
    grid: Vec<Vec<Cell>>,
    /// 現在のカーソル位置
    cursor_row: usize,
    cursor_col: usize,
    /// 画面サイズ
    rows: usize,
    cols: usize,
    /// 現在の文字属性
    current_fg: Option<u8>,
    current_bg: Option<u8>,
    current_bold: bool,
    current_italic: bool,
    current_underline: bool,
    /// VTE Parser
    parser: Parser,
    /// デバッグモード
    verbose: bool,
}

impl ScreenBuffer {
    pub fn new(rows: usize, cols: usize, verbose: bool) -> Self {
        let mut grid = Vec::with_capacity(rows);
        for _ in 0..rows {
            grid.push(vec![Cell::default(); cols]);
        }

        Self {
            grid,
            cursor_row: 0,
            cursor_col: 0,
            rows,
            cols,
            current_fg: None,
            current_bg: None,
            current_bold: false,
            current_italic: false,
            current_underline: false,
            parser: Parser::new(),
            verbose,
        }
    }

    /// PTY出力を処理してスクリーンバッファを更新
    pub fn process_data(&mut self, data: &[u8]) {
        // VTE advanceを呼ぶためにScreenBufferを一時的に借用できるよう分離
        let mut parser = std::mem::replace(&mut self.parser, Parser::new());
        for &byte in data {
            parser.advance(self, byte);
        }
        self.parser = parser;
    }

    /// 現在の画面内容を文字列の配列として取得
    pub fn get_screen_lines(&self) -> Vec<String> {
        self.grid.iter().map(|row| {
            row.iter().map(|cell| cell.char).collect()
        }).collect()
    }

    /// UI boxを検出
    pub fn find_ui_boxes(&self) -> Vec<UIBox> {
        let mut boxes = Vec::new();
        let lines = self.get_screen_lines();
        
        for (row_idx, line) in lines.iter().enumerate() {
            if line.trim_start().starts_with('╭') && !line.contains('�') {
                if let Some(ui_box) = self.parse_ui_box_at(&lines, row_idx) {
                    boxes.push(ui_box);
                }
            }
        }
        
        boxes
    }

    /// 指定位置からUI boxを解析
    fn parse_ui_box_at(&self, lines: &[String], start_row: usize) -> Option<UIBox> {
        if start_row >= lines.len() {
            return None;
        }

        let mut content_lines = Vec::new();
        let mut end_row = None;
        
        // ╰で終わる行を探す
        for (idx, line) in lines.iter().enumerate().skip(start_row + 1) {
            if line.trim_start().starts_with('╰') {
                end_row = Some(idx);
                break;
            }
            // ボックス内のコンテンツ（│で始まる行）
            if line.trim_start().starts_with('│') {
                let content = line.trim_start_matches('│').trim_end_matches('│').trim();
                if !content.is_empty() {
                    content_lines.push(content.to_string());
                }
            }
        }

        if let Some(end) = end_row {
            // ボックス上の行を取得（実行コンテキスト）
            let mut above_lines = Vec::new();
            if start_row > 0 {
                for i in 0..start_row {
                    let line = &lines[i];
                    if !line.trim().is_empty() {
                        above_lines.push(line.clone());
                    }
                }
            }

            // ボックス下の行を取得（ステータス）
            let mut below_lines = Vec::new();
            for i in (end + 1)..(end + 4).min(lines.len()) {
                if i < lines.len() && !lines[i].trim().is_empty() {
                    below_lines.push(lines[i].clone());
                }
            }

            Some(UIBox {
                start_row,
                end_row: end,
                content_lines,
                above_lines,
                below_lines,
            })
        } else {
            None
        }
    }

    /// カーソル位置を安全に設定
    fn set_cursor(&mut self, row: usize, col: usize) {
        self.cursor_row = row.min(self.rows.saturating_sub(1));
        self.cursor_col = col.min(self.cols.saturating_sub(1));
    }

    /// 文字を現在のカーソル位置に挿入
    fn insert_char(&mut self, ch: char) {
        if self.cursor_row < self.rows && self.cursor_col < self.cols {
            self.grid[self.cursor_row][self.cursor_col] = Cell {
                char: ch,
                fg_color: self.current_fg,
                bg_color: self.current_bg,
                bold: self.current_bold,
                italic: self.current_italic,
                underline: self.current_underline,
            };
            
            // カーソルを右に移動
            self.cursor_col += 1;
            if self.cursor_col >= self.cols {
                self.cursor_col = 0;
                self.cursor_row += 1;
                if self.cursor_row >= self.rows {
                    self.cursor_row = self.rows - 1;
                    // スクロール処理は簡略化
                }
            }
        }
    }

    /// 画面をクリア
    fn clear_screen(&mut self) {
        for row in &mut self.grid {
            for cell in row {
                *cell = Cell::default();
            }
        }
        self.cursor_row = 0;
        self.cursor_col = 0;
    }
}

/// UI boxの情報
#[derive(Debug, Clone)]
pub struct UIBox {
    pub start_row: usize,
    pub end_row: usize,
    pub content_lines: Vec<String>,
    pub above_lines: Vec<String>,    // ボックス上の行（実行コンテキスト）
    pub below_lines: Vec<String>,    // ボックス下の行（ステータス）
}

/// VTE Performトレイトの実装
impl Perform for ScreenBuffer {
    /// 通常の文字の印刷
    fn print(&mut self, c: char) {
        self.insert_char(c);
    }

    /// 実行文字（制御文字）の処理
    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => {
                // 改行：カーソルを次の行の先頭に移動
                self.cursor_row += 1;
                if self.cursor_row >= self.rows {
                    self.cursor_row = self.rows - 1;
                    // スクロール処理（簡略化）
                }
            }
            b'\r' => {
                // キャリッジリターン：カーソルを行の先頭に移動
                self.cursor_col = 0;
            }
            b'\t' => {
                // タブ：8の倍数位置に移動
                self.cursor_col = ((self.cursor_col / 8) + 1) * 8;
                if self.cursor_col >= self.cols {
                    self.cursor_col = self.cols - 1;
                }
            }
            b'\x08' => {
                // バックスペース
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                }
            }
            _ => {
                if self.verbose {
                    eprintln!("Unhandled execute: 0x{:02x}", byte);
                }
            }
        }
    }

    /// フック開始
    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _c: char) {
        // 今回は使用しない
    }

    /// 文字列挿入
    fn put(&mut self, _byte: u8) {
        // 今回は使用しない
    }

    /// フック終了
    fn unhook(&mut self) {
        // 今回は使用しない
    }

    /// OSCコマンド開始
    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {
        // 今回は使用しない
    }

    /// CSI（Control Sequence Introducer）ディスパッチ
    fn csi_dispatch(&mut self, params: &Params, _intermediates: &[u8], _ignore: bool, c: char) {
        match c {
            'H' | 'f' => {
                // カーソル位置設定
                let row = params.iter().next().unwrap_or(&[1])[0] as usize;
                let col = params.iter().nth(1).unwrap_or(&[1])[0] as usize;
                self.set_cursor(row.saturating_sub(1), col.saturating_sub(1));
            }
            'A' => {
                // カーソル上移動
                let count = params.iter().next().unwrap_or(&[1])[0] as usize;
                self.cursor_row = self.cursor_row.saturating_sub(count);
            }
            'B' => {
                // カーソル下移動
                let count = params.iter().next().unwrap_or(&[1])[0] as usize;
                self.cursor_row = (self.cursor_row + count).min(self.rows - 1);
            }
            'C' => {
                // カーソル右移動
                let count = params.iter().next().unwrap_or(&[1])[0] as usize;
                self.cursor_col = (self.cursor_col + count).min(self.cols - 1);
            }
            'D' => {
                // カーソル左移動
                let count = params.iter().next().unwrap_or(&[1])[0] as usize;
                self.cursor_col = self.cursor_col.saturating_sub(count);
            }
            'J' => {
                // 画面消去
                let mode = params.iter().next().unwrap_or(&[0])[0];
                if mode == 2 {
                    self.clear_screen();
                }
            }
            'K' => {
                // 行消去
                let mode = params.iter().next().unwrap_or(&[0])[0];
                if mode == 0 && self.cursor_row < self.rows {
                    // カーソル位置から行末まで消去
                    for col in self.cursor_col..self.cols {
                        if col < self.grid[self.cursor_row].len() {
                            self.grid[self.cursor_row][col] = Cell::default();
                        }
                    }
                }
            }
            'm' => {
                // SGR（Select Graphic Rendition）- 文字属性設定
                for param in params.iter() {
                    if let Some(&value) = param.get(0) {
                        match value {
                            0 => {
                                // リセット
                                self.current_fg = None;
                                self.current_bg = None;
                                self.current_bold = false;
                                self.current_italic = false;
                                self.current_underline = false;
                            }
                            1 => self.current_bold = true,
                            3 => self.current_italic = true,
                            4 => self.current_underline = true,
                            22 => self.current_bold = false,
                            23 => self.current_italic = false,
                            24 => self.current_underline = false,
                            2 => {}, // Dim/faint - 無視
                            30..=37 => self.current_fg = Some(value as u8 - 30),
                            38 => {
                                // 拡張前景色 - 複雑なので簡略化
                                // 次のパラメータは無視
                            },
                            39 => self.current_fg = None, // デフォルト前景色
                            40..=47 => self.current_bg = Some(value as u8 - 40),
                            49 => self.current_bg = None, // デフォルト背景色
                            90..=97 => self.current_fg = Some(value as u8 - 90 + 8), // 明るい前景色
                            100..=107 => self.current_bg = Some(value as u8 - 100 + 8), // 明るい背景色
                            _ => {
                                // 他のSGRパラメータは無視（verboseログも削除）
                            }
                        }
                    }
                }
            }
            'G' => {
                // カーソル列位置設定
                let col = params.iter().next().unwrap_or(&[1])[0] as usize;
                self.cursor_col = col.saturating_sub(1).min(self.cols - 1);
            }
            'h' => {
                // Set Mode - 多くは無視
                // 基本的な端末モード設定なので無視
            }
            'l' => {
                // Reset Mode - 多くは無視
                // 基本的な端末モード設定なので無視
            }
            's' => {
                // カーソル位置保存 - 簡略化のため無視
            }
            'u' => {
                // カーソル位置復元 - 簡略化のため無視
            }
            _ => {
                // 他のCSIコマンドは無視（verboseログも削除）
            }
        }
    }

    /// ESCシーケンスディスパッチ
    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {
        // 今回は使用しない
    }
}