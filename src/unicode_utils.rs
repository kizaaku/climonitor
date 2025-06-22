use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Unicode文字列を安全に指定された文字数で切り取る
pub fn truncate_str(s: &str, max_chars: usize) -> String {
    let graphemes: Vec<&str> = s.graphemes(true).collect();
    if graphemes.len() <= max_chars {
        return s.to_string();
    }
    
    graphemes[..max_chars].concat()
}

/// Unicode文字列を指定された表示幅で切り取る
pub fn truncate_by_width(s: &str, max_width: usize) -> String {
    let mut result = String::new();
    let mut current_width = 0;
    
    for grapheme in s.graphemes(true) {
        let grapheme_width = UnicodeWidthStr::width(grapheme);
        if current_width + grapheme_width > max_width {
            break;
        }
        result.push_str(grapheme);
        current_width += grapheme_width;
    }
    
    result
}

/// 文字列の表示幅を計算（制御文字を無視）
pub fn display_width(s: &str) -> usize {
    s.graphemes(true)
        .map(|g| UnicodeWidthStr::width(g))
        .sum()
}

/// 安全なバイト境界でのID切り取り（8文字まで）
pub fn truncate_id(id: &str) -> String {
    truncate_str(id, 8)
}

/// メッセージプレビュー用の安全な切り取り
pub fn truncate_message(msg: &str, max_width: usize) -> String {
    let truncated = truncate_by_width(msg, max_width);
    if display_width(&truncated) < display_width(msg) {
        format!("{}...", truncated)
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_str() {
        assert_eq!(truncate_str("hello", 3), "hel");
        assert_eq!(truncate_str("こんにちは", 3), "こんに");
        assert_eq!(truncate_str("👨‍💻🎉", 1), "👨‍💻");
    }

    #[test]
    fn test_truncate_by_width() {
        assert_eq!(truncate_by_width("hello", 3), "hel");
        assert_eq!(truncate_by_width("こんにちは", 6), "こんに"); // 全角文字は幅2
        assert_eq!(truncate_by_width("あaい", 3), "あa"); // 全角2 + 半角1
    }

    #[test]
    fn test_display_width() {
        assert_eq!(display_width("hello"), 5);
        assert_eq!(display_width("こんにちは"), 10); // 全角文字は幅2
        assert_eq!(display_width("あaい"), 5); // 2 + 1 + 2
    }
}