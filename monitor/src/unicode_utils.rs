// Unicode utilities for text handling
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// 文字列を指定された表示幅に切り詰め（Unicode文字境界と表示幅を考慮）
pub fn truncate_str(s: &str, max_width: usize) -> String {
    if max_width <= 3 {
        return "...".to_string();
    }

    let current_width = s.width();

    // 既に表示幅が収まっている場合はそのまま返す
    if current_width <= max_width {
        return s.to_string();
    }

    let ellipsis_width = 3; // "..."の表示幅
    let target_width = max_width.saturating_sub(ellipsis_width);

    let mut accumulated_width = 0;
    let mut result = String::new();

    for grapheme in s.graphemes(true) {
        let grapheme_width = grapheme.width();

        // 次の文字を追加すると幅を超える場合は終了
        if accumulated_width + grapheme_width > target_width {
            break;
        }

        result.push_str(grapheme);
        accumulated_width += grapheme_width;
    }

    format!("{result}...")
}
