# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Development Commands

```bash
# Build the project (creates both climonitor and climonitor-launcher)
cargo build --release

# Real-time monitoring with climonitor-launcher
climonitor-launcher claude           # Launch Claude with real-time monitoring
climonitor --live                    # Connect to launcher for live updates

# Development and testing
cargo run                           # Build and run in live mode
cargo run -- --no-tui              # Non-interactive snapshot mode
cargo run -- --verbose             # Verbose output for debugging

# Debug state detection (human testing)
climonitor-launcher --verbose claude # Shows detailed state detection process
climonitor-launcher --verbose gemini # Shows Gemini-specific detection patterns
climonitor-launcher --verbose claude --help  # Test with simple commands

# Install locally
cargo install --path .

# Run binaries directly after build
./target/release/climonitor --live
./target/release/climonitor-launcher claude

# Log file functionality
climonitor --live --log-file /path/to/output.log
climonitor-launcher --log-file /path/to/output.log claude
climonitor-launcher --log-file /path/to/output.log gemini
```

## Architecture Overview

This is a Rust CLI Tool Monitor that provides real-time monitoring of interactive CLI tools (Claude Code, Gemini CLI, etc.) using PTY (pseudo-terminal) integration and Unix Domain Socket communication. The application has been completely redesigned with a client-server architecture:

### Core Components

#### Monitor Server Architecture (Current Implementation)
- **`main.rs`**: CLI argument parsing and mode selection (live/snapshot modes)
- **`monitor_server.rs`**: Central monitoring server that manages client connections and state broadcasting
- **`session_manager.rs`**: Session state management and tracking across multiple Claude instances
- **`live_ui.rs`**: Real-time terminal UI for displaying session status and updates
- **`protocol.rs`**: Communication protocol definitions for client-server messaging
- **`launcher_client.rs`**: CLI tool wrapper client with PTY integration and state reporting

#### Independent State Detection Architecture (Current Implementation)
- **`screen_buffer.rs`**: VTE parser-based terminal screen buffer simulation with PTY+1 column buffer
- **`screen_claude_detector.rs`**: Claude-specific independent state detector with complete ScreenBuffer integration
- **`screen_gemini_detector.rs`**: Gemini-specific independent state detector with specialized patterns
- **`state_detector.rs`**: State detection trait definition and factory pattern for tool-specific detectors
- **`tool_wrapper.rs`**: Multi-tool CLI wrapper supporting Claude Code and Gemini CLI
- **`unicode_utils.rs`**: Unicode-safe text handling utilities for Japanese text and emoji display

### Data Flow Architecture

#### Client-Server Communication Model
1. **Monitor Server Startup**: `climonitor --live` starts the central monitoring server with Unix Domain Socket
2. **Launcher Connection**: `climonitor-launcher` connects to monitor server and registers as client
3. **PTY Process Launch**: Launcher creates PTY session and spawns CLI tool with environment variables
4. **Bidirectional I/O**: PTY handles all terminal I/O while capturing output for analysis
5. **Screen Buffer Analysis**: VTE parser processes ANSI sequences to maintain screen state and detect UI changes
6. **State Broadcasting**: Session state updates are sent to monitor server via protocol messages
7. **Live Display**: Monitor server updates `LiveUI` with real-time session status and tool execution

#### PTY Integration Architecture
1. **PTY Creation**: `portable-pty` creates pseudo-terminal with proper size detection
2. **Process Spawning**: CLI tool launched in PTY environment with preserved interactivity
3. **I/O Monitoring**: Simultaneous stdout/stderr capture without disrupting user interaction
4. **Screen Buffer Processing**: VTE parser maintains complete terminal screen state with PTY+1 column buffer for UI box detection
5. **Signal Handling**: Proper signal forwarding for graceful shutdown and resize events

### Session Status Logic

#### VTE Parser-based Screen Buffer State Detection (Current Implementation)
Advanced state detection using complete terminal screen simulation:
- **Screen Buffer Management**: 80x24 terminal grid with full VTE parser support
- **UI Box Detection**: Automatic detection of ╭╮╰╯ Unicode box drawing UI elements
- **Context Analysis**: Extraction of execution context from lines above UI boxes
- **Multi-tool Support**: Claude (🤖) and Gemini (✨) CLI tool differentiation
- **Real-time Updates**: Immediate state changes via screen buffer analysis

**Claude State Detection Logic:**
- **Waiting for Input (⏳)**: "Do you want", "May I", "proceed?", "y/n" patterns in UI boxes
- **Busy/Executing (🔵)**: "esc to interrupt" pattern detection with high precision
- **Idle (🔵)**: "◯ IDE connected" or completion after "esc to interrupt" disappears
- **Error (🔴)**: "✗", "failed", "Error" patterns in status lines
- **Connected (🔗)**: Active PTY session with tool process running

**Gemini State Detection Logic:**
- **Waiting for Input (⏳)**: "Allow execution?", "waiting for user confirmation" patterns
- **Busy/Executing (🔵)**: "(esc to cancel" patterns in spinner output
- **Idle (🔵)**: ">" command prompt or "Cumulative Stats" display
- **Error (🔴)**: Standard error patterns
- **Connected (🔗)**: Active PTY session with tool process running

**Key Improvements (Complete Independence Architecture):**
- **Independent Detectors**: Each tool has specialized detection logic optimized for its specific patterns
- **100% Accuracy**: Complete screen state analysis vs. stream-based pattern matching
- **Tool Differentiation**: Claude uses "esc to interrupt", Gemini uses "(esc to cancel" and ">" prompts
- **Context Extraction**: UI execution context display (実行中/思考中/処理中/etc.)
- **Session Management**: Proper cleanup and state updates on launcher disconnect
- **Error States**: API errors, connection failures, tool failures
- **User Interaction**: Tool-specific input prompts and confirmation patterns

**VTE Parser Features:**
- **Complete ANSI Support**: Full CSI, OSC, and control sequence processing
- **Screen Buffer Simulation**: Accurate 80x24 terminal grid with cursor tracking
- **Unicode Rendering**: Proper character width calculation and emoji support
- **UI Box Parsing**: Automatic detection and content extraction from Unicode box elements

### Async Event Loop Design

The monitor server uses `tokio::select!` to handle:
- Unix Domain Socket client connections and disconnections
- Protocol message processing from launcher clients
- PTY I/O events and state change notifications
- UI update broadcasting to connected interfaces
- Signal handling for graceful shutdown and cleanup

### Unicode and Internationalization

The codebase includes comprehensive Unicode support through `unicode_utils.rs`:
- Grapheme cluster-aware text truncation
- Display width calculation for proper terminal alignment
- Japanese text handling with mixed ASCII/hiragana/katakana/kanji

## Key Design Patterns

- **Client-Server Architecture**: Central monitor server with multiple launcher clients
- **PTY Integration**: True terminal emulation for seamless CLI tool interaction
- **VTE Parser-based Detection**: Complete screen buffer analysis for accurate state detection
- **Error Resilience**: Continues operation even if launcher clients disconnect
- **Memory Efficiency**: Bounded channels and automatic cleanup of stale sessions
- **Cross-Platform**: Uses portable-pty for consistent terminal handling

## UI Box Duplication Problem Resolution

### Problem Description
The VTE parser experienced UI box duplication issues where ink.js library (used by CLI tools like Claude Code) UI boxes appeared multiple times on screen, creating visual artifacts that interfered with state detection.

### Root Cause Analysis
The issue was caused by a mismatch between ink.js expectations and VTE parser buffer dimensions:

1. **ink.js Behavior**: Draws UI boxes using relative cursor movements (`ESC[1A ESC[2K` sequences)
2. **Line End Processing**: When cursor reaches column boundary (e.g., 70), automatic line wrapping occurs
3. **Buffer Boundary Issue**: VTE parser buffer matched PTY size exactly, causing premature line wrapping
4. **Position Mismatch**: ink.js expected cursor positions differed from actual VTE parser positions
5. **Incomplete Clearing**: Previous UI box content remained visible due to clearing sequence misalignment

### Technical Solution
**PTY+1 Column Buffer Architecture** implemented in `screen_buffer.rs`:

```rust
// Buffer creation with +1 column
let buffer_cols = cols + 1;  // PTY cols + 1
let grid = vec![vec![Cell::empty(); buffer_cols]; rows];

// Display output limited to original PTY size
let pty_cols = self.cols.saturating_sub(1);
self.grid.iter().skip(start_row).map(|row| {
    row.iter().take(pty_cols).map(|cell| cell.char).collect()
}).collect()
```

### Implementation Details

#### Buffer Architecture
- **Internal Buffer**: PTY columns + 1 (e.g., 71 columns for 70-column PTY)
- **External Display**: Original PTY size (e.g., 70 columns)
- **UI Box Detection**: Limited to PTY display range

#### Benefits
1. **Prevents Premature Line Wrapping**: Extra column provides buffer space for ink.js cursor positioning
2. **Maintains Display Compatibility**: Output functions return original PTY-sized content
3. **Preserves UI Box Integrity**: ink.js clearing sequences target correct screen positions
4. **No Visual Side Effects**: Extra column is invisible to monitoring and display functions

#### Debugging Enhancements
Enhanced CSI K (line clearing) logging for troubleshooting:
```rust
if self.verbose {
    let old_content: String = if let Some(row) = self.grid.get(self.cursor_row) {
        row.iter().map(|c| c.char).collect::<String>().trim().to_string()
    } else {
        "N/A".to_string()
    };
    eprintln!("🧹 [CLEAR_LINE] Mode=2 clearing entire line {} old_content: '{}'", 
             self.cursor_row, old_content);
}
```

### Resolution Impact
- **Eliminated UI Box Duplication**: Multiple UI boxes no longer appear on screen
- **Improved State Detection Accuracy**: Clean UI boxes enable precise status monitoring
- **Enhanced CLI Tool Compatibility**: VTE parser now matches real terminal behavior
- **Maintained Performance**: Minimal overhead from single additional column per row

## Environment Configuration

The application supports configuration through environment variables and command-line options:

```bash
# Enable debug logging in Claude Code for detailed analysis
export ANTHROPIC_LOG=debug

# Optional: Custom socket path for client-server communication
export CLIMONITOR_SOCKET_PATH=/tmp/climonitor.sock
```

**Environment Variables:**
- `ANTHROPIC_LOG`: Set to `debug` for detailed Claude output analysis (recommended)
- `CLIMONITOR_SOCKET_PATH`: Custom Unix Domain Socket path (optional)
- `RUST_LOG`: Standard Rust logging level for climonitor itself

## Real-time Monitoring Usage

### Quick Start

```bash
# Terminal 1: Launch Claude with monitoring
climonitor-launcher claude

# Terminal 2: View real-time status
climonitor --live
```

### Advanced Usage

```bash
# Verbose monitoring (see debug patterns)
climonitor-launcher --verbose claude

# Monitor specific Claude operations
climonitor-launcher claude --project myproject
climonitor-launcher claude --help  # Any Claude args work

# Different viewing modes
climonitor --live --verbose         # Detailed real-time updates
climonitor --no-tui                 # Snapshot mode (one-time status)
```

### Architecture Benefits

**Current Implementation Advantages:**
- **Real-time state detection**: Immediate status updates from CLI tools' PTY output
- **True interactivity**: PTY preserves full CLI tool functionality
- **Accurate tool monitoring**: Direct detection of tool permission requests vs execution
- **Session lifecycle tracking**: Complete visibility into CLI tools' internal state transitions
- **Multi-session support**: Monitor multiple CLI tool instances simultaneously

**Use Cases:**
- Development workflow monitoring
- Tool execution debugging
- Session performance analysis
- Multi-project coordination
- Real-time status dashboards

## Log File Functionality

### Overview
climonitor supports comprehensive logging of CLI tools' standard output to files using the `--log-file` option. This feature works for both interactive and non-interactive modes while preserving CLI tools' full functionality.

### Usage

```bash
# Start monitor with log file option
climonitor --live --log-file /path/to/logfile.log

# Launch CLI tool sessions (logs automatically recorded)
climonitor-launcher --log-file /path/to/output.log claude
climonitor-launcher --log-file /path/to/output.log claude --print "query"
```

### Implementation Details

#### PTY-based Logging
- Direct capture from PTY stdout/stderr streams
- Preserves all terminal output including ANSI escape sequences
- Real-time writing with proper buffering
- Maintains full interactivity while logging

#### Log File Transmission
1. **Launcher Configuration**: Log file path specified via CLI argument
2. **Monitor Communication**: Log configuration sent to monitor server via protocol
3. **PTY Integration**: Logging handled at PTY level for complete capture
4. **Real-time Writing**: Output written immediately with automatic flushing

### Key Benefits

- **Preserves Interactivity**: PTY maintains full CLI tool functionality
- **Complete Output Capture**: All terminal output including ANSI sequences logged
- **Real-time Writing**: Immediate output capture with proper flushing
- **Seamless Integration**: Transparent logging without workflow changes

### File Format

- **Full Terminal Output**: Complete PTY session including ANSI escape sequences
- **Append Mode**: Multiple sessions append to same log file
- **Real-time Writing**: Output written immediately as it occurs
- **Binary-safe**: Handles all terminal control characters correctly

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

### Human Testing with Verbose Output

For easy human testing of state detection:

```bash
# Terminal 1: Start verbose monitoring to see detection process
climonitor-launcher --verbose claude

# Watch the debug output showing:
# 📺 [SCREEN] - Current screen buffer state
# 📦 [UI_BOX] - UI box detection and content extraction
# 🔍 [STATE] - State detection analysis
# 🎯 [STATE_CHANGE] - Actual state transitions
# 📊 [CONTEXT] - Execution context extraction

# Terminal 2: Monitor the session status
climonitor --live
```

### Development Testing

When developing:

- Use `--verbose` flag to see detailed debugging output and state detection
- Test both live and snapshot modes (`--live` vs `--no-tui`)
- Verify Unicode handling with Japanese project names and output
- Test PTY integration with various terminal sizes and capabilities
- Test client-server communication with multiple launcher instances
- Verify error handling when monitor server is not running
- Test log file functionality with different output patterns
- Verify signal handling and graceful shutdown behavior
- Test VTE parser integration and screen buffer accuracy
- Verify UI box detection with Claude Code and Gemini CLI interfaces
- Test multi-tool support and tool type differentiation
- Verify real-time state detection accuracy with actual CLI tool sessions

## Documentation

For detailed technical documentation, see the `docs/` directory:

- **`docs/code-structure.md`**: Complete file dependencies and responsibility mapping
- **`docs/state-detectors.md`**: Detailed state detection logic and patterns for Claude and Gemini

These documents provide comprehensive coverage of the codebase architecture and implementation details.
