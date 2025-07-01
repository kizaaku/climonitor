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
            char: ' ', // 明示的に空白文字を設定（Unicode box文字対応）
            fg_color: None,
            bg_color: None,
            bold: false,
            italic: false,
            underline: false,
        }
    }
}

impl Cell {
    /// 完全にクリアされたセルを作成（Unicode box文字の残骸を確実に除去）
    pub fn empty() -> Self {
        Self {
            char: ' ', // 空白文字を明示的に設定
            fg_color: None,
            bg_color: None,
            bold: false,
            italic: false,
            underline: false,
        }
    }
}

/// スクリーンバッファ - 通常の端末画面表現（PTYサイズに動的対応）
pub struct ScreenBuffer {
    /// グリッド（行×列）- PTYサイズに合わせて動的に設定
    grid: Vec<Vec<Cell>>,
    /// 現在のカーソル位置
    cursor_row: usize,
    cursor_col: usize,
    /// 画面サイズ（PTYサイズと同期）
    rows: usize,
    cols: usize,
    /// 現在の文字属性
    current_fg: Option<u8>,
    current_bg: Option<u8>,
    current_bold: bool,
    current_italic: bool,
    current_underline: bool,
    /// スクロール範囲（DECSTBM）
    scroll_top: usize,
    scroll_bottom: usize,
    /// VTE Parser
    parser: Parser,
    /// デバッグモード
    verbose: bool,
}

impl ScreenBuffer {
    /// 新しいスクリーンバッファを作成（PTYサイズに動的対応）
    pub fn new(rows: usize, cols: usize, verbose: bool) -> Self {
        // Unicode box文字残骸を防ぐためCell::empty()を使用
        // バッファ列数をPTY+1にして行末問題を回避
        let buffer_cols = cols + 1;
        let grid = vec![vec![Cell::empty(); buffer_cols]; rows];

        if verbose {
            eprintln!(
                "🖥️  [BUFFER_INIT] Screen buffer: {rows}x{buffer_cols} (PTY {rows}x{cols} + 1 col)"
            );
        }

        Self {
            grid,
            cursor_row: 0,
            cursor_col: 0,
            rows,
            cols: buffer_cols,
            current_fg: None,
            current_bg: None,
            current_bold: false,
            current_italic: false,
            current_underline: false,
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
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

    /// 現在の画面内容を文字列の配列として取得（実際の端末表示に準拠）
    pub fn get_screen_lines(&self) -> Vec<String> {
        // 実際の端末は現在表示されている範囲のみを返す
        // グリッドサイズが設定行数を超えている場合は下部のみを取得
        let start_row = if self.grid.len() > self.rows {
            self.grid.len() - self.rows
        } else {
            0
        };

        if self.verbose {
            eprintln!(
                "📺 [VISIBLE_CHECK] grid.len()={}, self.rows={}, start_row={}",
                self.grid.len(),
                self.rows,
                start_row
            );
            if start_row > 0 {
                eprintln!(
                    "📺 [VISIBLE_AREA] Showing visible area: rows {}-{} of total {}",
                    start_row,
                    self.grid.len() - 1,
                    self.grid.len()
                );
            }
        }

        // PTY表示範囲のみを返す（バッファは+1列だが表示は元のPTYサイズ）
        let pty_cols = self.cols.saturating_sub(1); // 元のPTY列数
        self.grid
            .iter()
            .skip(start_row)
            .map(|row| row.iter().take(pty_cols).map(|cell| cell.char).collect())
            .collect()
    }

    /// UI boxを検出（改善版）
    pub fn find_ui_boxes(&self) -> Vec<UIBox> {
        let mut boxes = Vec::new();
        let lines = self.get_screen_lines();
        let mut processed_rows = std::collections::HashSet::new();

        let start_row = if self.grid.len() > self.rows {
            self.grid.len() - self.rows
        } else {
            0
        };

        if self.verbose {
            eprintln!(
                "🔍 [UI_BOX_DEBUG] Analyzing {} lines for UI boxes (grid offset: {}):",
                lines.len(),
                start_row
            );
            for (i, line) in lines.iter().enumerate() {
                if !line.trim().is_empty() {
                    let row_num = start_row + i;
                    eprintln!("  {row_num}: '{line}'");
                }
            }
        }

        // 1. 完全なUI box（╭から╰まで）を検索
        for (row_idx, line) in lines.iter().enumerate().rev() {
            if processed_rows.contains(&row_idx) {
                continue;
            }

            if line.trim_start().starts_with('╭') && !line.contains('�') {
                if let Some(mut ui_box) = self.parse_ui_box_at(&lines, row_idx) {
                    // 行番号をグリッド座標に変換
                    ui_box.start_row += start_row;
                    ui_box.end_row += start_row;

                    for r in ui_box.start_row..=ui_box.end_row {
                        processed_rows.insert(r - start_row);
                    }

                    if self.verbose {
                        let start_row = ui_box.start_row;
                        let end_row = ui_box.end_row;
                        let content_count = ui_box.content_lines.len();
                        eprintln!("📦 [COMPLETE_BOX] Found complete UI box at rows {start_row}-{end_row} with {content_count} content lines");
                    }

                    boxes.push(ui_box);
                }
            }
        }

        // 2. 部分的なUI box（│の連続領域）を検索
        if boxes.is_empty() {
            if let Some(mut partial_box) = self.find_partial_ui_box(&lines) {
                // 行番号をグリッド座標に変換
                partial_box.start_row += start_row;
                partial_box.end_row += start_row;

                if self.verbose {
                    eprintln!(
                        "📦 [PARTIAL_BOX] Found partial UI box at rows {}-{} with {} content lines",
                        partial_box.start_row,
                        partial_box.end_row,
                        partial_box.content_lines.len()
                    );
                }
                boxes.push(partial_box);
            }
        }

        boxes.sort_by(|a, b| b.start_row.cmp(&a.start_row));
        boxes
    }

    /// 部分的なUI box（│の連続領域）を検出
    fn find_partial_ui_box(&self, lines: &[String]) -> Option<UIBox> {
        let mut content_lines = Vec::new();
        let mut start_row = None;
        let mut end_row = None;

        for (row_idx, line) in lines.iter().enumerate() {
            if line.trim_start().starts_with('│') {
                if start_row.is_none() {
                    start_row = Some(row_idx);
                }
                end_row = Some(row_idx);

                let content = line.trim_start_matches('│').trim_end_matches('│').trim();
                if !content.is_empty() {
                    content_lines.push(content.to_string());
                }
            }
        }

        if let (Some(start), Some(end)) = (start_row, end_row) {
            // 最低3行の│が必要（確認待ちボックスの判定）
            if end - start >= 2 && !content_lines.is_empty() {
                Some(UIBox {
                    start_row: start,
                    end_row: end,
                    content_lines,
                    above_lines: Vec::new(),
                    below_lines: Vec::new(),
                })
            } else {
                None
            }
        } else {
            None
        }
    }

    /// 指定位置からUI boxを解析
    fn parse_ui_box_at(&self, lines: &[String], start_row: usize) -> Option<UIBox> {
        if start_row >= lines.len() {
            return None;
        }

        let mut content_lines = Vec::new();
        let mut end_row = None;

        // ╰で終わる行を探す（中間に別の╭がないことを確認）
        for (idx, line) in lines.iter().enumerate().skip(start_row + 1) {
            // 中間に別の╭があった場合は無効なboxとして扱う
            if line.trim_start().starts_with('╭') {
                if self.verbose {
                    eprintln!(
                        "📦 [INVALID_BOX] Found nested ╭ at row {idx} while parsing box from row {start_row}"
                    );
                }
                return None;
            }

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
                for line in lines.iter().take(start_row) {
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
        if self.cursor_row < self.grid.len() && self.cursor_col < self.cols {
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
                // 範囲外に移動した場合はスクロール
                if self.cursor_row >= self.grid.len() {
                    if self.verbose
                        && (ch == '╭'
                            || ch == '╰'
                            || ch == '│'
                            || ch == '─'
                            || ch == '╮'
                            || ch == '╯')
                    {
                        eprintln!(
                            "🔄 [INSERT_SCROLL] '{}' triggered scroll at ({}, {}) grid_len={}",
                            ch,
                            self.cursor_row,
                            self.cursor_col,
                            self.grid.len()
                        );
                    }
                    self.cursor_row = self.grid.len().saturating_sub(1);
                    self.scroll_up();
                }
            }
        }
    }

    /// 画面をクリア
    fn clear_screen(&mut self) {
        if self.verbose {
            eprintln!(
                "🧹 [CLEAR_SCREEN] Clearing entire screen buffer ({}x{})",
                self.rows, self.cols
            );
        }

        // バッファを完全にリセット（Unicode box文字残骸を確実に除去）
        self.grid = vec![vec![Cell::empty(); self.cols]; self.rows];

        self.cursor_row = 0;
        self.cursor_col = 0;
    }

    /// 1行上にスクロール（スクロール範囲考慮）
    fn scroll_up(&mut self) {
        self.scroll_up_n(1);
    }

    /// n行上にスクロール（スクロール範囲考慮）
    fn scroll_up_n(&mut self, n: usize) {
        if self.scroll_top >= self.scroll_bottom {
            return;
        }

        let scroll_region_size = self.scroll_bottom - self.scroll_top + 1;
        let actual_scroll = n.min(scroll_region_size);

        if self.verbose && actual_scroll > 0 {
            eprintln!(
                "🔄 [SCROLL_UP] Scrolling {} lines in region {}-{}, grid_size: {}",
                actual_scroll,
                self.scroll_top,
                self.scroll_bottom,
                self.grid.len()
            );
        }

        // 実際の端末に準拠したスクロール処理：全体的な上シフト
        for _ in 0..actual_scroll {
            // スクロール範囲内の内容を1行ずつ上にシフト
            for row in self.scroll_top..self.scroll_bottom {
                if row + 1 < self.grid.len() {
                    // 下の行の内容を上の行にコピー
                    self.grid[row] = self.grid[row + 1].clone();
                }
            }

            // 最下行（scroll_bottom）をクリア
            if self.scroll_bottom < self.grid.len() {
                let old_content: String = self.grid[self.scroll_bottom]
                    .iter()
                    .map(|c| c.char)
                    .collect();
                self.grid[self.scroll_bottom] = vec![Cell::empty(); self.cols];

                if self.verbose && !old_content.trim().is_empty() {
                    eprintln!(
                        "🗑️  [SCROLL_CLEAR] Bottom line cleared: '{}'",
                        old_content.trim()
                    );
                }
            }
        }
    }

    /// n行下にスクロール（スクロール範囲考慮）
    fn scroll_down_n(&mut self, n: usize) {
        if self.scroll_top >= self.scroll_bottom {
            return;
        }

        let scroll_region_size = self.scroll_bottom - self.scroll_top + 1;
        let actual_scroll = n.min(scroll_region_size);

        if self.verbose && actual_scroll > 0 {
            eprintln!(
                "🔄 [SCROLL_DOWN] Scrolling {} lines in region {}-{}",
                actual_scroll, self.scroll_top, self.scroll_bottom
            );
        }

        // スクロール範囲内の行を下にシフト（グリッドサイズ固定）
        for _ in 0..actual_scroll {
            // self.scroll_bottom から self.scroll_top + 1 までを逆順に処理
            for row in (self.scroll_top..self.scroll_bottom).rev() {
                if row + 1 < self.grid.len() {
                    self.grid[row + 1] = self.grid[row].clone();
                }
            }
            // 先頭行（scroll_top）をクリア
            if self.scroll_top < self.grid.len() {
                self.grid[self.scroll_top] = vec![Cell::empty(); self.cols];
            }
        }
    }

    /// カーソルから画面末尾まで消去
    fn clear_from_cursor_to_end(&mut self) {
        // 現在の行のカーソル位置から行末まで消去
        if let Some(row) = self.grid.get_mut(self.cursor_row) {
            for cell in row.iter_mut().skip(self.cursor_col) {
                *cell = Cell::default();
            }
        }

        // 次の行から最後の行まで全て消去
        for row in self.grid.iter_mut().skip(self.cursor_row + 1) {
            for cell in row.iter_mut() {
                *cell = Cell::default();
            }
        }
    }

    /// 画面先頭からカーソルまで消去
    fn clear_from_start_to_cursor(&mut self) {
        // 最初の行から現在の行の前まで全て消去
        for row in self.grid.iter_mut().take(self.cursor_row) {
            for cell in row.iter_mut() {
                *cell = Cell::default();
            }
        }

        // 現在の行の行頭からカーソル位置まで消去
        if let Some(row) = self.grid.get_mut(self.cursor_row) {
            for cell in row.iter_mut().take(self.cursor_col + 1) {
                *cell = Cell::default();
            }
        }
    }

    /// n行を挿入（IL - Insert Line）
    fn insert_lines(&mut self, n: usize) {
        let insert_row = self.cursor_row;
        if insert_row > self.scroll_bottom {
            return; // スクロール範囲外
        }

        let actual_insert = n.min(self.scroll_bottom - insert_row + 1);

        if self.verbose && actual_insert > 0 {
            eprintln!("📝 [INSERT_LINES] Inserting {actual_insert} lines at row {insert_row}");
        }

        // 挿入位置から下の行を下にシフト（グリッドサイズ固定）
        for _ in 0..actual_insert {
            // self.scroll_bottom から insert_row + 1 までを逆順に処理
            for row in (insert_row..self.scroll_bottom).rev() {
                if row + 1 < self.grid.len() {
                    self.grid[row + 1] = self.grid[row].clone();
                }
            }
            // 挿入行をクリア
            if insert_row < self.grid.len() {
                self.grid[insert_row] = vec![Cell::empty(); self.cols];
            }
        }
    }

    /// n行を削除（DL - Delete Line）
    fn delete_lines(&mut self, n: usize) {
        let delete_row = self.cursor_row;
        if delete_row > self.scroll_bottom {
            return; // スクロール範囲外
        }

        let actual_delete = n.min(self.scroll_bottom - delete_row + 1);

        if self.verbose && actual_delete > 0 {
            eprintln!("🗑️  [DELETE_LINES] Deleting {actual_delete} lines at row {delete_row}");
        }

        // 削除位置から下の行を上にシフト（グリッドサイズ固定）
        for _ in 0..actual_delete {
            for row in delete_row..self.scroll_bottom {
                if row + 1 < self.grid.len() {
                    self.grid[row] = self.grid[row + 1].clone();
                }
            }
            // スクロール範囲の最下部をクリア
            if self.scroll_bottom < self.grid.len() {
                self.grid[self.scroll_bottom] = vec![Cell::empty(); self.cols];
            }
        }
    }

    /// n文字を挿入（ICH - Insert Character）
    fn insert_characters(&mut self, n: usize) {
        if self.cursor_row >= self.grid.len() {
            return;
        }

        let row = &mut self.grid[self.cursor_row];
        let insert_col = self.cursor_col;
        let actual_insert = n.min(self.cols - insert_col);

        if self.verbose && actual_insert > 0 {
            eprintln!(
                "📝 [INSERT_CHARS] Inserting {} chars at row {} col {}",
                actual_insert, self.cursor_row, insert_col
            );
        }

        // 行末から文字を削除し、挿入位置に空白を挿入
        for _ in 0..actual_insert {
            if row.len() > insert_col && !row.is_empty() {
                row.pop(); // 行末の文字を削除
                if insert_col <= row.len() {
                    row.insert(insert_col, Cell::default()); // 挿入位置に空白を挿入
                }
            }
        }
    }

    /// n文字を削除（DCH - Delete Character）
    fn delete_characters(&mut self, n: usize) {
        if self.cursor_row >= self.grid.len() {
            return;
        }

        let row = &mut self.grid[self.cursor_row];
        let delete_col = self.cursor_col;
        let actual_delete = n.min(row.len() - delete_col);

        if self.verbose && actual_delete > 0 {
            eprintln!(
                "🗑️  [DELETE_CHARS] Deleting {} chars at row {} col {}",
                actual_delete, self.cursor_row, delete_col
            );
        }

        // 削除位置から文字を削除し、行末に空白を追加
        for _ in 0..actual_delete {
            if delete_col < row.len() && !row.is_empty() {
                row.remove(delete_col);
                if row.len() < self.cols {
                    row.push(Cell::default()); // 行末に空白を追加
                }
            }
        }
    }
}

/// UI boxの情報
#[derive(Debug, Clone)]
pub struct UIBox {
    pub start_row: usize,
    pub end_row: usize,
    pub content_lines: Vec<String>,
    pub above_lines: Vec<String>, // ボックス上の行（実行コンテキスト）
    pub below_lines: Vec<String>, // ボックス下の行（ステータス）
}

/// VTE Performトレイトの実装
impl Perform for ScreenBuffer {
    /// 通常の文字の印刷
    fn print(&mut self, c: char) {
        if self.verbose {
            if c == '╭' || c == '╰' || c == '│' || c == '─' || c == '╮' || c == '╯' {
                eprintln!(
                    "🖨️  [PRINT_BOX] '{}' at ({}, {}) [U+{:04X}] grid_size={}x{}",
                    c,
                    self.cursor_row,
                    self.cursor_col,
                    c as u32,
                    self.grid.len(),
                    self.cols
                );
            } else if !c.is_whitespace() && c != '\u{0}' {
                eprintln!(
                    "🖨️  [PRINT_CHAR] '{}' at ({}, {}) grid_size={}x{}",
                    c,
                    self.cursor_row,
                    self.cursor_col,
                    self.grid.len(),
                    self.cols
                );
            }
        }
        self.insert_char(c);
    }

    /// 実行文字（制御文字）の処理
    fn execute(&mut self, byte: u8) {
        if self.verbose && byte != b'\0' {
            eprintln!(
                "⚡ [EXECUTE] Control char: 0x{:02X} ({}) at ({}, {})",
                byte, byte as char, self.cursor_row, self.cursor_col
            );
        }
        match byte {
            b'\n' => {
                // 改行：カーソルを次の行の先頭に移動
                self.cursor_col = 0; // 列を0にリセット
                self.cursor_row += 1;
                if self.cursor_row >= self.grid.len() {
                    self.cursor_row = self.grid.len().saturating_sub(1);
                    // スクロール処理：1行上にスクロール
                    self.scroll_up();
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
                    eprintln!("Unhandled execute: 0x{byte:02x}");
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
        if self.verbose {
            let param_str: Vec<String> = params.iter().map(|p| format!("{p:?}")).collect();
            eprintln!(
                "🎛️  [CSI] Dispatching '{}' with params: [{}]",
                c,
                param_str.join(", ")
            );
        }

        match c {
            'H' | 'f' => {
                // カーソル位置設定（VTE標準準拠）
                let row = params.iter().next().unwrap_or(&[1])[0] as usize;
                let col = params.iter().nth(1).unwrap_or(&[1])[0] as usize;
                let new_row = row.saturating_sub(1);
                let new_col = col.saturating_sub(1);

                if self.verbose {
                    eprintln!(
                        "📍 [CURSOR_POS] Moving cursor to ({new_row}, {new_col}) [params: row={row}, col={col}]"
                    );
                }

                self.set_cursor(new_row, new_col);
            }
            'A' => {
                // カーソル上移動（列位置は保持）
                let count = params.iter().next().unwrap_or(&[1])[0] as usize;
                let old_row = self.cursor_row;
                self.cursor_row = self.cursor_row.saturating_sub(count);
                // 列位置は変更しない（VTE仕様準拠）

                if self.verbose {
                    eprintln!(
                        "⬆️  [CURSOR_UP] Moving cursor up {} lines: {} -> {}, col remains {}",
                        count, old_row, self.cursor_row, self.cursor_col
                    );
                }
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
                match mode {
                    0 => {
                        // カーソルから画面末尾まで消去
                        if self.verbose {
                            eprintln!(
                                "🧹 [CLEAR_TO_END] Clearing from cursor ({}, {}) to end of screen",
                                self.cursor_row, self.cursor_col
                            );
                        }
                        self.clear_from_cursor_to_end();
                    }
                    1 => {
                        // 画面先頭からカーソルまで消去
                        if self.verbose {
                            let cursor_row = self.cursor_row;
                            let cursor_col = self.cursor_col;
                            eprintln!("🧹 [CLEAR_TO_START] Clearing from start of screen to cursor ({cursor_row}, {cursor_col})");
                        }
                        self.clear_from_start_to_cursor();
                    }
                    2 => {
                        // 画面全体消去
                        if self.verbose {
                            eprintln!("🧹 [CLEAR_SCREEN] Clearing entire screen");
                        }
                        self.clear_screen();
                    }
                    _ => {}
                }
            }
            'K' => {
                // 行消去
                let mode = params.iter().next().unwrap_or(&[0])[0];
                if self.cursor_row < self.grid.len() {
                    match mode {
                        0 => {
                            // カーソル位置から行末まで消去
                            if let Some(row) = self.grid.get_mut(self.cursor_row) {
                                for cell in row.iter_mut().skip(self.cursor_col) {
                                    *cell = Cell::empty();
                                }
                            }
                        }
                        1 => {
                            // 行頭からカーソル位置まで消去
                            if let Some(row) = self.grid.get_mut(self.cursor_row) {
                                for cell in row.iter_mut().take(self.cursor_col + 1) {
                                    *cell = Cell::empty();
                                }
                            }
                        }
                        2 => {
                            // 行全体を消去
                            if self.verbose {
                                let old_content: String =
                                    if let Some(row) = self.grid.get(self.cursor_row) {
                                        row.iter()
                                            .map(|c| c.char)
                                            .collect::<String>()
                                            .trim()
                                            .to_string()
                                    } else {
                                        "N/A".to_string()
                                    };
                                let cursor_row = self.cursor_row;
                                let grid_height = self.grid.len();
                                let cols = self.cols;
                                eprintln!("🧹 [CLEAR_LINE] Mode=2 clearing entire line {cursor_row} (grid size: {grid_height}x{cols}) old_content: '{old_content}'");
                            }
                            if let Some(row) = self.grid.get_mut(self.cursor_row) {
                                for cell in row.iter_mut() {
                                    *cell = Cell::empty();
                                }
                                if self.verbose {
                                    eprintln!(
                                        "✅ [CLEAR_LINE] Line {} successfully cleared",
                                        self.cursor_row
                                    );
                                }
                            } else {
                                let cursor_row = self.cursor_row;
                                let grid_height = self.grid.len();
                                eprintln!("❌ [CLEAR_LINE_ERROR] Cursor row {cursor_row} is out of bounds (grid height: {grid_height})");
                            }
                        }
                        _ => {}
                    }
                }
            }
            'L' => {
                // IL - Insert Line
                let count = params.iter().next().unwrap_or(&[1])[0] as usize;
                self.insert_lines(count);
            }
            'M' => {
                // DL - Delete Line
                let count = params.iter().next().unwrap_or(&[1])[0] as usize;
                self.delete_lines(count);
            }
            '@' => {
                // ICH - Insert Character
                let count = params.iter().next().unwrap_or(&[1])[0] as usize;
                self.insert_characters(count);
            }
            'P' => {
                // DCH - Delete Character
                let count = params.iter().next().unwrap_or(&[1])[0] as usize;
                self.delete_characters(count);
            }
            'S' => {
                // SU - Scroll Up
                let count = params.iter().next().unwrap_or(&[1])[0] as usize;
                self.scroll_up_n(count);
            }
            'T' => {
                // SD - Scroll Down
                let count = params.iter().next().unwrap_or(&[1])[0] as usize;
                self.scroll_down_n(count);
            }
            'r' => {
                // DECSTBM - Set Top and Bottom Margins (スクロール範囲設定)
                let top = params.iter().next().unwrap_or(&[1])[0] as usize;
                let bottom = params.iter().nth(1).unwrap_or(&[self.rows as u16])[0] as usize;

                self.scroll_top = top.saturating_sub(1);
                self.scroll_bottom = bottom.saturating_sub(1).min(self.rows.saturating_sub(1));

                if self.verbose {
                    eprintln!(
                        "🔧 [DECSTBM] Set scroll region to {}-{}",
                        self.scroll_top, self.scroll_bottom
                    );
                }

                // カーソルをホームポジション（スクロール範囲の左上）に移動
                self.set_cursor(self.scroll_top, 0);
            }
            'm' => {
                // SGR（Select Graphic Rendition）- 文字属性設定
                for param in params.iter() {
                    if let Some(&value) = param.first() {
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
                            2 => {} // Dim/faint - 無視
                            30..=37 => self.current_fg = Some(value as u8 - 30),
                            38 => {
                                // 拡張前景色 - 複雑なので簡略化
                                // 次のパラメータは無視
                            }
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
                // Set Mode - 重要なモードを実装
                for param in params.iter() {
                    if let Some(&value) = param.first() {
                        match value {
                            25 => {
                                // Show cursor
                                if self.verbose {
                                    eprintln!("👁️  [CURSOR_SHOW] Cursor visibility: ON");
                                }
                            }
                            1049 => {
                                // Save cursor and switch to alternate screen buffer
                                if self.verbose {
                                    eprintln!("🔄 [ALT_SCREEN] Switch to alternate screen buffer");
                                }
                                // 現在の画面をクリア（alternate screenの効果をエミュレート）
                                self.clear_screen();
                            }
                            1047 => {
                                // Switch to alternate screen buffer
                                if self.verbose {
                                    eprintln!(
                                        "🔄 [ALT_SCREEN] Switch to alternate screen buffer (1047)"
                                    );
                                }
                                self.clear_screen();
                            }
                            47 => {
                                // Switch to alternate screen buffer (older variant)
                                if self.verbose {
                                    eprintln!(
                                        "🔄 [ALT_SCREEN] Switch to alternate screen buffer (47)"
                                    );
                                }
                                self.clear_screen();
                            }
                            2004 => {
                                // Bracketed Paste Mode - Enable
                                if self.verbose {
                                    eprintln!("📋 [BRACKETED_PASTE] Enable bracketed paste mode");
                                }
                                // TODO: Set internal flag for bracketed paste mode
                            }
                            1004 => {
                                // Focus Tracking Mode - Enable
                                if self.verbose {
                                    eprintln!("👀 [FOCUS_TRACKING] Enable focus tracking mode");
                                }
                                // TODO: Set internal flag and implement focus event notification
                            }
                            _ => {
                                if self.verbose {
                                    eprintln!("❓ [MODE_SET] Unhandled mode: {value}");
                                }
                            }
                        }
                    }
                }
            }
            'l' => {
                // Reset Mode - 重要なモードを実装
                for param in params.iter() {
                    if let Some(&value) = param.first() {
                        match value {
                            25 => {
                                // Hide cursor
                                if self.verbose {
                                    eprintln!("🙈 [CURSOR_HIDE] Cursor visibility: OFF");
                                }
                            }
                            1049 => {
                                // Restore cursor and switch to main screen buffer
                                if self.verbose {
                                    eprintln!("🔄 [MAIN_SCREEN] Switch to main screen buffer");
                                }
                                // メイン画面に戻る（現在の実装では何もしない）
                            }
                            1047 => {
                                // Switch to main screen buffer
                                if self.verbose {
                                    eprintln!(
                                        "🔄 [MAIN_SCREEN] Switch to main screen buffer (1047)"
                                    );
                                }
                            }
                            47 => {
                                // Switch to main screen buffer (older variant)
                                if self.verbose {
                                    eprintln!("🔄 [MAIN_SCREEN] Switch to main screen buffer (47)");
                                }
                            }
                            2004 => {
                                // Bracketed Paste Mode - Disable
                                if self.verbose {
                                    eprintln!("📋 [BRACKETED_PASTE] Disable bracketed paste mode");
                                }
                                // TODO: Clear internal flag for bracketed paste mode
                            }
                            1004 => {
                                // Focus Tracking Mode - Disable
                                if self.verbose {
                                    eprintln!("👀 [FOCUS_TRACKING] Disable focus tracking mode");
                                }
                                // TODO: Clear internal flag and stop focus event notification
                            }
                            _ => {
                                if self.verbose {
                                    eprintln!("❓ [MODE_RESET] Unhandled mode: {value}");
                                }
                            }
                        }
                    }
                }
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
