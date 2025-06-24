//! ANSI escape sequence processing utilities
//! 
//! Provides functions to clean and process terminal output containing ANSI escape sequences
//! for both UI analysis and log output filtering.

use regex::Regex;
use std::sync::OnceLock;

/// ANSI escape sequence patterns
static ANSI_ESCAPE_PATTERN: OnceLock<Regex> = OnceLock::new();
static CURSOR_CONTROL_PATTERN: OnceLock<Regex> = OnceLock::new();
static SCREEN_CONTROL_PATTERN: OnceLock<Regex> = OnceLock::new();

/// Initialize regex patterns
fn init_patterns() {
    ANSI_ESCAPE_PATTERN.get_or_init(|| {
        // ANSI escape sequences including DEC private modes like [?25h
        Regex::new(r"\x1b\[[?]?[0-9;]*[a-zA-Z]").unwrap()
    });
    
    CURSOR_CONTROL_PATTERN.get_or_init(|| {
        // Cursor movement and control sequences
        Regex::new(r"\x1b\[[0-9]*[ABCDEFGHJK]|\x1b\[[0-9;]*[Hf]").unwrap()
    });
    
    SCREEN_CONTROL_PATTERN.get_or_init(|| {
        // Screen clearing and scrolling
        Regex::new(r"\x1b\[[0-9]*[JS]|\x1b\[2K|\x1b\[K").unwrap()
    });
}

/// Strip all ANSI escape sequences from text
pub fn strip_ansi_codes(text: &str) -> String {
    init_patterns();
    
    let ansi_pattern = ANSI_ESCAPE_PATTERN.get().unwrap();
    let cursor_pattern = CURSOR_CONTROL_PATTERN.get().unwrap();
    let screen_pattern = SCREEN_CONTROL_PATTERN.get().unwrap();
    
    // Apply all patterns sequentially
    let step1 = ansi_pattern.replace_all(text, "");
    let step2 = cursor_pattern.replace_all(&step1, "");
    let result = screen_pattern.replace_all(&step2, "");
    
    result.to_string()
}

/// Clean text for UI analysis (removes ANSI codes and normalizes whitespace)
pub fn clean_for_analysis(text: &str) -> String {
    let clean = strip_ansi_codes(text);
    
    // Normalize whitespace but preserve line structure
    let lines: Vec<String> = clean
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();
    
    lines.join("\n")
}

/// Clean text for log output (removes ANSI codes but preserves formatting structure)
pub fn clean_for_logging(text: &str) -> String {
    let clean = strip_ansi_codes(text);
    
    // Remove excessive empty lines but preserve structure
    let lines: Vec<&str> = clean.lines().collect();
    let mut result = Vec::new();
    let mut consecutive_empty = 0;
    
    for line in lines {
        if line.trim().is_empty() {
            consecutive_empty += 1;
            // Allow max 2 consecutive empty lines
            if consecutive_empty <= 2 {
                result.push(line);
            }
        } else {
            consecutive_empty = 0;
            result.push(line);
        }
    }
    
    result.join("\n")
}

/// Extract text content from Claude UI boxes (removes borders and ANSI codes)
pub fn extract_ui_box_content(text: &str) -> Option<String> {
    let clean = strip_ansi_codes(text);
    
    // Look for box patterns (╭╮╯╰ or ┌┐└┘)
    let lines: Vec<&str> = clean.lines().collect();
    if lines.len() < 3 {
        return None;
    }
    
    // Check if first and last lines contain box characters
    let first_line = lines.first()?;
    let last_line = lines.last()?;
    
    if (first_line.contains('╭') || first_line.contains('┌')) &&
       (last_line.contains('╯') || last_line.contains('└')) {
        
        // Extract content lines (skip first and last border lines)
        let content_lines: Vec<String> = lines[1..lines.len()-1]
            .iter()
            .map(|line| {
                // Remove box border characters from sides
                let trimmed = line.trim_start_matches('│')
                                 .trim_start_matches('┃')
                                 .trim_end_matches('│')
                                 .trim_end_matches('┃')
                                 .trim();
                trimmed.to_string()
            })
            .filter(|line| !line.is_empty())
            .collect();
        
        if content_lines.is_empty() {
            None
        } else {
            Some(content_lines.join("\n"))
        }
    } else {
        None
    }
}

/// Check if text contains Claude UI elements
pub fn contains_claude_ui_elements(text: &str) -> bool {
    let clean = clean_for_analysis(text);
    
    // Look for Claude-specific UI patterns
    clean.contains("Welcome to Claude Code") ||
    clean.contains("for help") ||
    clean.contains("for shortcuts") ||
    clean.contains("IDE connected") ||
    clean.contains("IDE disconnected") ||
    clean.contains("usage limit") ||
    clean.contains("approaching usage limit") ||
    clean.contains("resets at")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi_codes() {
        let input = "\x1b[38;2;215;119;87m╭─────╮\x1b[39m";
        let expected = "╭─────╮";
        assert_eq!(strip_ansi_codes(input), expected);
    }

    #[test]
    fn test_clean_for_analysis() {
        let input = "\x1b[2K\x1b[1A  \n\n  test  \n\n";
        let expected = "test";
        assert_eq!(clean_for_analysis(input), expected);
    }

    #[test]
    fn test_extract_ui_box_content() {
        let input = "╭─────────╮\n│ Welcome │\n│ to test │\n╰─────────╯";
        let result = extract_ui_box_content(input);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "Welcome\nto test");
    }

    #[test]
    fn test_contains_claude_ui_elements() {
        let input = "\x1b[38m│ Welcome to Claude Code! │\x1b[39m";
        assert!(contains_claude_ui_elements(input));
        
        let normal_text = "This is just normal output";
        assert!(!contains_claude_ui_elements(normal_text));
    }

    #[test]
    fn test_clean_for_logging() {
        let input = "Line 1\n\n\n\n\nLine 2\n\nLine 3";
        let result = clean_for_logging(input);
        // Should reduce excessive empty lines
        assert!(result.matches('\n').count() < input.matches('\n').count());
        assert!(result.contains("Line 1"));
        assert!(result.contains("Line 2"));
        assert!(result.contains("Line 3"));
    }
}