# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Development Commands

```bash
# Build the project
cargo build --release

# Run in development mode
cargo run

# Run with specific options
cargo run -- --no-tui --verbose
cargo run -- --project ccmonitor

# Install locally
cargo install --path .

# Run the binary directly after build
./target/release/ccmonitor
./target/release/ccmonitor --no-tui
```

## Architecture Overview

This is a Rust CLI tool that monitors Claude session files in real-time. The application has a modular architecture with clear separation of concerns:

### Core Components

- **`main.rs`**: CLI argument parsing with clap, TTY detection, and mode selection (TUI vs non-interactive)
- **`app.rs`**: Main application orchestrator that coordinates all components and handles both TUI and non-TUI modes
- **`config.rs`**: Configuration management including environment variable loading and log directory resolution
- **`session.rs`**: Core domain logic for Claude session state management and JSONL message parsing
- **`watcher.rs`**: File system monitoring using notify crate to watch configurable log directory
- **`ui.rs`**: Terminal UI rendering using ratatui for the interactive dashboard
- **`unicode_utils.rs`**: Unicode-safe text handling utilities for Japanese text and emoji display

### Data Flow Architecture

1. **Configuration Loading**: `Config` loads environment variables from `.env.local` to determine log directory
2. **File Monitoring**: `FileWatcher` monitors configurable log directory (default: `~/.claude/projects/*.jsonl`) using async file system events
3. **Message Parsing**: Raw JSONL lines are deserialized into `SessionMessage` structs with serde
4. **State Management**: `SessionStore` maintains a HashMap of active sessions, updating status based on message analysis
5. **Status Classification**: Sessions are categorized as Active/Waiting/Error/Idle based on message content and timing
6. **Display**: Either TUI mode (real-time dashboard) or non-TUI mode (snapshot output)

### Session Status Logic

The session status determination is core business logic in `session.rs`:
- **Active (🟢)**: Claude executing tools or waiting for tool results (`tool_use` stop reason)
- **Waiting (🟡)**: Claude completed response, awaiting user input (`end_turn` stop reason)  
- **Error (🔴)**: Tool execution errors detected in `tool_use_result` field
- **Idle (⚪)**: No activity for >5 minutes

### Async Event Loop Design

The TUI mode uses `tokio::select!` to handle:
- File watcher events (new session messages)
- Keyboard input (q/Esc to quit, r to refresh)
- Periodic UI refresh timer (1-second intervals)

### Unicode and Internationalization

The codebase includes comprehensive Unicode support through `unicode_utils.rs`:
- Grapheme cluster-aware text truncation
- Display width calculation for proper terminal alignment
- Japanese text handling with mixed ASCII/hiragana/katakana/kanji

## Key Design Patterns

- **Graceful Degradation**: Automatically falls back to non-TUI mode when TTY is unavailable
- **Error Resilience**: Continues operation even if individual session files cannot be parsed
- **Memory Efficiency**: Maintains bounded message channels and periodic cleanup
- **Cross-Platform**: Uses crossterm for terminal handling across different operating systems

## Environment Configuration

The application supports custom log directory configuration through environment variables:

```bash
# Create .env.local file for custom log directory
echo "CLAUDE_LOG_DIR=/custom/path/to/claude/logs" > .env.local

# Enable debug mode for detailed logging
echo "CCMONITOR_DEBUG=1" >> .env.local
```

**Environment Variables:**
- `CLAUDE_LOG_DIR`: Custom path to Claude session log directory (defaults to `~/.claude/projects/`)
- `CCMONITOR_DEBUG`: Enable verbose debug output for JSON parsing and file operations

## Testing Strategy

When developing:
- Use `--verbose` flag to see detailed debugging output
- Test both TUI and `--no-tui` modes to ensure compatibility
- Verify Unicode handling with Japanese project names and messages
- Test with missing default directory and custom `CLAUDE_LOG_DIR` for proper error handling
- Create `.env.local` with custom settings to test configuration loading