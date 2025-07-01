// Unicode utilities for text handling
use unicode_segmentation::UnicodeSegmentation;

/// 文字列を指定された長さに切り詰め（Unicode文字境界を考慮）
pub fn truncate_str(s: &str, max_len: usize) -> String {
    if max_len <= 3 {
        return "...".to_string();
    }

    let graphemes: Vec<&str> = s.graphemes(true).collect();

    if graphemes.len() <= max_len {
        s.to_string()
    } else {
        let truncated_len = max_len.saturating_sub(3);
        let truncated: String = graphemes.into_iter().take(truncated_len).collect();
        format!("{truncated}...")
    }
}
