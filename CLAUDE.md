# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Development Commands

```bash
# Build the project (creates both ccmonitor and ccmonitor-launcher)
cargo build --release

# Phase 3: Real-time monitoring with ccmonitor-launcher
ccmonitor-launcher claude           # Launch Claude with real-time monitoring
ccmonitor --live                    # Connect to launcher for live updates

# Traditional JSONL-based monitoring
cargo run                           # TUI mode
cargo run -- --no-tui              # Non-interactive mode
cargo run -- --watch               # Continuous monitoring
cargo run -- --demo                # Test 1-second timer

# Build options
cargo run -- --no-tui --verbose
cargo run -- --project ccmonitor

# Install locally
cargo install --path .

# Run binaries directly after build
./target/release/ccmonitor --live
./target/release/ccmonitor-launcher claude
```

## Architecture Overview

This is a Rust CLI tool that monitors Claude session files in real-time. The application has a modular architecture with clear separation of concerns:

### Core Components

#### Traditional JSONL Monitoring (Phase 1/2)
- **`main.rs`**: CLI argument parsing with clap, TTY detection, and mode selection (TUI vs non-interactive)
- **`app.rs`**: Main application orchestrator that coordinates all components and handles both TUI and non-TUI modes
- **`config.rs`**: Configuration management including environment variable loading and log directory resolution
- **`session.rs`**: Core domain logic for Claude session state management and JSONL message parsing
- **`watcher.rs`**: File system monitoring using notify crate to watch configurable log directory
- **`ui.rs`**: Terminal UI rendering using ratatui for the interactive dashboard
- **`unicode_utils.rs`**: Unicode-safe text handling utilities for Japanese text and emoji display

#### Real-time Output Monitoring (Phase 3)
- **`launcher.rs`**: Claude Code process wrapper with stdout/stderr monitoring
- **`output_analyzer.rs`**: Real-time log analysis engine with regex pattern matching for state detection
- **`state_broadcaster.rs`**: Unix Domain Socket-based state broadcasting system for real-time updates
- **`ccmonitor-launcher`**: Standalone binary that launches Claude Code with real-time monitoring

### Data Flow Architecture

#### Phase 1/2: JSONL File Monitoring
1. **Configuration Loading**: `Config` loads environment variables from `.env.local` to determine log directory
2. **File Monitoring**: `FileWatcher` monitors configurable log directory (default: `~/.claude/projects/*.jsonl`) using async file system events
3. **Message Parsing**: Raw JSONL lines are deserialized into `SessionMessage` structs with serde
4. **State Management**: `SessionStore` maintains a HashMap of active sessions, updating status based on message analysis
5. **Status Classification**: Sessions are categorized as Active/Waiting/Error/Idle based on message content and timing
6. **Display**: Either TUI mode (real-time dashboard) or non-TUI mode (snapshot output)

#### Phase 3: Real-time Output Stream Monitoring
1. **Process Launch**: `ccmonitor-launcher` starts Claude Code as child process with `ANTHROPIC_LOG=debug`
2. **Output Capture**: stdout/stderr streams are captured and monitored in real-time
3. **Pattern Analysis**: `OutputAnalyzer` uses regex patterns to detect state changes from debug logs
4. **State Broadcasting**: Unix Domain Socket broadcasts state updates to connected clients
5. **Live Updates**: `ccmonitor --live` receives real-time state updates and displays current status
6. **Hybrid Display**: Combines real-time updates with traditional JSONL monitoring for comprehensive view

### Session Status Logic

#### Traditional Status Detection (Phase 1/2)
The session status determination is core business logic in `session.rs`:
- **Active (🟢)**: Claude executing tools or waiting for tool results (`tool_use` stop reason)
- **Approve (🟡)**: Claude completed response, awaiting user input (`end_turn` stop reason)  
- **Finish (🔵)**: Text response completed
- **Error (🔴)**: Tool execution errors detected in `tool_use_result` field
- **Idle (⚪)**: No activity for >5 minutes

#### Real-time Status Detection (Phase 3)
Enhanced status detection using Claude Code debug output patterns:
- **API Requests**: Detects "Making API request" patterns → Active
- **Tool Execution**: Detects "Tool execution started" / "using tool:" → Active with tool name
- **User Approval**: Detects "Waiting for user approval" / "Press Enter to continue" → Approve
- **Tool Completion**: Detects "Tool execution completed" / "tool finished" → Finish
- **Error Detection**: Detects "Error:" / "Exception:" / "Failed:" patterns → Error
- **Session Identification**: Extracts session IDs from debug logs for accurate tracking

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

## Phase 3: Real-time Monitoring Usage

### Quick Start
```bash
# Terminal 1: Launch Claude with monitoring
ccmonitor-launcher claude

# Terminal 2: View real-time status
ccmonitor --live
```

### Advanced Usage
```bash
# Verbose monitoring (see debug patterns)
ccmonitor-launcher --verbose claude

# Monitor specific Claude operations
ccmonitor-launcher claude --project myproject
ccmonitor-launcher claude --help  # Any Claude args work

# Different viewing modes
ccmonitor --live --verbose         # Detailed real-time updates
ccmonitor --live --project myproj  # Filter by project
```

### Architecture Benefits

**Phase 3 Advantages:**
- **Real-time state detection**: Immediate status updates from Claude Code's debug output
- **Accurate tool monitoring**: Direct detection of tool permission requests vs execution
- **Session lifecycle tracking**: Complete visibility into Claude Code's internal state transitions
- **Hybrid monitoring**: Combines real-time updates with traditional JSONL fallback

**When to use Phase 3:**
- Need immediate status updates
- Want to monitor tool permission flow
- Debugging Claude Code behavior
- Real-time development workflow

**When to use traditional mode:**
- Analyzing historical sessions
- Low-overhead monitoring
- Environments where process wrapping isn't feasible
- Retrospective session analysis

## Log File Functionality

### Overview
ccmonitor supports comprehensive logging of Claude's standard output to files using the `--log-file` option. This feature works for both interactive and non-interactive modes while preserving Claude's full functionality.

### Usage
```bash
# Start monitor with log file option
ccmonitor --log-file /path/to/logfile.log

# Launch Claude sessions (logs automatically recorded)
ccmonitor-launcher claude                    # Interactive mode with logging
ccmonitor-launcher claude --print "query"   # Non-interactive mode with logging
```

### Implementation Details

#### Non-interactive Mode (--print)
- Direct stdout/stderr capture and file writing
- Maintains exact Claude output format
- Low overhead with buffered I/O

#### Interactive Mode (Default)
- Uses `script` command to preserve true interactivity
- Captures full terminal session including ANSI escape sequences
- Maintains Claude's TTY-based UI features
- Implementation: `script -q -a logfile claude [args]`

#### Technical Architecture
1. **Monitor Server**: Receives log file path via CLI argument
2. **Configuration Transmission**: Sends log file config to launcher via Unix Domain Socket
3. **Launcher Client**: Receives log configuration and applies appropriate logging method
4. **Mode Detection**: Automatically detects interactive vs non-interactive based on `--print` argument

### Key Benefits
- **Preserves Interactivity**: Interactive mode remains fully functional
- **Complete Output Capture**: All output including control characters logged
- **Seamless Integration**: No user workflow changes required
- **Mode-Agnostic**: Works consistently across all Claude execution modes

### File Format
- **Non-interactive**: Clean text output matching Claude's standard output
- **Interactive**: Full terminal session recording with ANSI escape sequences
- **Append Mode**: Multiple sessions append to same log file
- **Real-time Writing**: Output written immediately with proper flushing

## Future Extension Considerations

### Integrated Session Control Design Principles
The current architecture is designed with future extensibility in mind for potential integrated session control features (monitor-side session interaction).

#### Design Philosophy
- **Preserve Simplicity**: Current launcher independence maintained
- **Avoid Premature Complexity**: True simultaneous input from multiple sources deemed impractical
- **Extensibility Over Features**: Design for future enhancement without current complexity

#### Architectural Patterns for Future Extension
```rust
// Future-ready abstractions
trait InputSource {
    fn read_input(&mut self) -> Result<String>;
}

trait OutputSink {
    fn write_output(&mut self, data: &str) -> Result<()>;
}

// Current: Simple implementation, Future: Extensible
struct LauncherClient {
    input: Box<dyn InputSource>,    // Currently: Stdin, Future: Socket/Multiple
    output: Box<dyn OutputSink>,    // Currently: Stdout, Future: Fanout/Multiple
}
```

#### Identified Extension Paths
1. **Exclusive Control**: One active controller (launcher OR monitor) at a time
2. **Delegated Input**: Transfer input control between launcher/monitor
3. **Read-only Monitor**: Monitor displays output, launcher retains full control
4. **Session Multiplexing**: Monitor as session selector/switcher

#### Development Guidelines for Extensibility
- **Input/Output Abstraction**: Isolate I/O handling into swappable components
- **Protocol Extensibility**: Design socket messages for future bidirectional communication
- **State Management Separation**: Clear separation between session state and I/O handling
- **Modular Architecture**: Components that can be enhanced without major refactoring

#### Why Not Implemented Now
- **Complexity vs Value**: Current simple design meets immediate needs effectively
- **Use Case Uncertainty**: Real user demand for integrated control unclear
- **Technical Challenges**: True simultaneous input creates more problems than solutions
- **Maintenance Burden**: Additional complexity would complicate debugging and maintenance

#### Future Implementation Strategy
1. **Phase 1**: Enhanced protocol with bidirectional communication capability
2. **Phase 2**: Input delegation mechanisms (transfer control, not share)
3. **Phase 3**: UI improvements for session selection and control handoff
4. **Phase 4**: Advanced features based on real usage patterns

This approach ensures the current system remains simple and maintainable while preserving the option for sophisticated features if genuine need emerges.

## Testing Strategy

When developing:
- Use `--verbose` flag to see detailed debugging output
- Test both TUI and `--no-tui` modes to ensure compatibility
- Verify Unicode handling with Japanese project names and messages
- Test with missing default directory and custom `CLAUDE_LOG_DIR` for proper error handling
- Create `.env.local` with custom settings to test configuration loading
- Test Phase 3 real-time monitoring with `ccmonitor-launcher --verbose`
- Verify error handling when ccmonitor-launcher is not running
- Test log file functionality with both interactive and non-interactive modes
- Verify log file append behavior and real-time writing