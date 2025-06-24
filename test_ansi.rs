use regex::Regex;

fn strip_ansi_codes(text: &str) -> String {
    // ANSI escape sequences including DEC private modes like [?25h
    let ansi_pattern = Regex::new(r"\x1b\[[?]?[0-9;]*[a-zA-Z]").unwrap();
    ansi_pattern.replace_all(text, "").to_string()
}

fn main() {
    let test_input = "Hello! I'm Claude Code, ready to help you with software engineering tasks in your ccmonitor project. \n\nWhat would you like to work on today?[?25h[?25h";
    
    println!("Original:");
    println!("{:?}", test_input);
    
    println!("\nCleaned:");
    let cleaned = strip_ansi_codes(test_input);
    println!("{:?}", cleaned);
    
    println!("\nResult:");
    println!("{}", cleaned);
}