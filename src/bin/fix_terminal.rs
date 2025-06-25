use std::io::{self, Write};

fn main() -> io::Result<()> {
    println!("🔧 Emergency terminal reset...");
    
    // 複数の方法でターミナルリセットを試行
    
    // 1. エスケープシーケンスによるリセット
    print!("\x1b[!p");           // Soft terminal reset
    print!("\x1b[?3;4l\x1b[4l\x1b>"); // Reset various modes
    print!("\x1bc");             // Full reset
    io::stdout().flush()?;
    
    // 2. 標準的なターミナル設定を強制復元
    #[cfg(unix)]
    unsafe {
        let mut termios: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(libc::STDIN_FILENO, &mut termios) == 0 {
            // 標準的な設定を強制適用
            termios.c_lflag |= libc::ICANON | libc::ECHO | libc::ECHONL | libc::ISIG;
            termios.c_iflag |= libc::ICRNL;
            termios.c_oflag |= libc::OPOST;
            termios.c_cc[libc::VMIN] = 1;
            termios.c_cc[libc::VTIME] = 0;
            
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, &termios);
        }
    }
    
    println!("✅ Terminal emergency reset completed!");
    println!("If terminal is still broken, try: stty sane");
    
    Ok(())
}