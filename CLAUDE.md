# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Development Commands

```bash
# Build the project (creates both ccmonitor and ccmonitor-launcher)
cargo build --release

# Real-time monitoring with ccmonitor-launcher
ccmonitor-launcher claude           # Launch Claude with real-time monitoring
ccmonitor --live                    # Connect to launcher for live updates

# Development and testing
cargo run                           # Build and run in live mode
cargo run -- --no-tui              # Non-interactive snapshot mode
cargo run -- --verbose             # Verbose output for debugging

# Install locally
cargo install --path .

# Run binaries directly after build
./target/release/ccmonitor --live
./target/release/ccmonitor-launcher claude

# Log file functionality
ccmonitor --live --log-file /path/to/output.log
ccmonitor-launcher --log-file /path/to/output.log claude
```

## Architecture Overview

This is a Rust CLI tool that provides real-time monitoring of Claude Code sessions using PTY (pseudo-terminal) integration and Unix Domain Socket communication. The application has been completely redesigned with a client-server architecture:

### Core Components

#### Monitor Server Architecture (Current Implementation)
- **`main.rs`**: CLI argument parsing and mode selection (live/snapshot modes)
- **`monitor_server.rs`**: Central monitoring server that manages client connections and state broadcasting
- **`session_manager.rs`**: Session state management and tracking across multiple Claude instances
- **`live_ui.rs`**: Real-time terminal UI for displaying session status and updates
- **`protocol.rs`**: Communication protocol definitions for client-server messaging
- **`launcher_client.rs`**: Claude Code wrapper client with PTY integration and state reporting

#### PTY-based Process Monitoring
- **`claude_wrapper.rs`**: PTY-based Claude Code process execution with bidirectional I/O
- **`process_monitor.rs`**: Real-time process state monitoring and event detection
- **`standard_analyzer.rs`**: ANSI-aware output analysis for state detection from Claude's debug output
- **`ansi_utils.rs`**: ANSI escape sequence handling and terminal output processing
- **`unicode_utils.rs`**: Unicode-safe text handling utilities for Japanese text and emoji display

### Data Flow Architecture

#### Client-Server Communication Model
1. **Monitor Server Startup**: `ccmonitor --live` starts the central monitoring server with Unix Domain Socket
2. **Launcher Connection**: `ccmonitor-launcher` connects to monitor server and registers as client
3. **PTY Process Launch**: Launcher creates PTY session and spawns Claude Code with environment variables
4. **Bidirectional I/O**: PTY handles all terminal I/O while capturing output for analysis
5. **State Analysis**: `StandardAnalyzer` processes ANSI-cleaned output to detect Claude state changes
6. **State Broadcasting**: Session state updates are sent to monitor server via protocol messages
7. **Live Display**: Monitor server updates `LiveUI` with real-time session status and tool execution

#### PTY Integration Architecture
1. **PTY Creation**: `portable-pty` creates pseudo-terminal with proper size detection
2. **Process Spawning**: Claude Code launched in PTY environment with preserved interactivity
3. **I/O Monitoring**: Simultaneous stdout/stderr capture without disrupting user interaction
4. **ANSI Processing**: `ansi_utils` strips escape sequences for clean state analysis
5. **Signal Handling**: Proper signal forwarding for graceful shutdown and resize events

### Session Status Logic

#### PTY-based Real-time Status Detection
Enhanced status detection using direct Claude Code output analysis via PTY integration:
- **Active (🟢)**: Detects tool execution patterns and API request indicators in real-time
- **Thinking (🤔)**: Claude processing user input or generating responses
- **Tool Use (🔧)**: Specific tool execution detected with tool name identification
- **Waiting (⏳)**: User approval required for tool execution or input needed
- **Error (🔴)**: Exception patterns, tool failures, or process errors detected
- **Idle (⚪)**: No activity detected for configured timeout period
- **Connected (🔗)**: Active PTY session with Claude Code process running

#### Pattern Recognition System
`StandardAnalyzer` uses regex patterns to identify:
- **Tool Execution**: "Tool:" patterns, permission requests, execution confirmations
- **API Communication**: Request/response cycles, token usage, rate limiting
- **User Interaction**: Input prompts, approval requests, confirmation dialogs
- **Error States**: Exception traces, tool failures, connection issues
- **Process States**: Startup, shutdown, signal handling, resource usage

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
- **PTY Integration**: True terminal emulation for seamless Claude Code interaction
- **Real-time State Detection**: Immediate status updates via output stream analysis
- **Error Resilience**: Continues operation even if launcher clients disconnect
- **Memory Efficiency**: Bounded channels and automatic cleanup of stale sessions
- **Cross-Platform**: Uses portable-pty for consistent terminal handling

## Environment Configuration

The application supports configuration through environment variables and command-line options:

```bash
# Enable debug logging in Claude Code for detailed analysis
export ANTHROPIC_LOG=debug

# Optional: Custom socket path for client-server communication
export CCMONITOR_SOCKET_PATH=/tmp/ccmonitor.sock
```

**Environment Variables:**
- `ANTHROPIC_LOG`: Set to `debug` for detailed Claude output analysis (recommended)
- `CCMONITOR_SOCKET_PATH`: Custom Unix Domain Socket path (optional)
- `RUST_LOG`: Standard Rust logging level for ccmonitor itself

## Real-time Monitoring Usage

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
ccmonitor --no-tui                 # Snapshot mode (one-time status)
```

### Architecture Benefits

**Current Implementation Advantages:**
- **Real-time state detection**: Immediate status updates from Claude Code's PTY output
- **True interactivity**: PTY preserves full Claude Code functionality
- **Accurate tool monitoring**: Direct detection of tool permission requests vs execution
- **Session lifecycle tracking**: Complete visibility into Claude Code's internal state transitions
- **Multi-session support**: Monitor multiple Claude instances simultaneously

**Use Cases:**
- Development workflow monitoring
- Tool execution debugging
- Session performance analysis
- Multi-project coordination
- Real-time status dashboards

## Log File Functionality

### Overview
ccmonitor supports comprehensive logging of Claude's standard output to files using the `--log-file` option. This feature works for both interactive and non-interactive modes while preserving Claude's full functionality.

### Usage

```bash
# Start monitor with log file option
ccmonitor --live --log-file /path/to/logfile.log

# Launch Claude sessions (logs automatically recorded)
ccmonitor-launcher --log-file /path/to/output.log claude
ccmonitor-launcher --log-file /path/to/output.log claude --print "query"
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

- **Preserves Interactivity**: PTY maintains full Claude Code functionality
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

When developing:

- Use `--verbose` flag to see detailed debugging output and state detection
- Test both live and snapshot modes (`--live` vs `--no-tui`)
- Verify Unicode handling with Japanese project names and output
- Test PTY integration with various terminal sizes and capabilities
- Test client-server communication with multiple launcher instances
- Verify error handling when monitor server is not running
- Test log file functionality with different output patterns
- Verify signal handling and graceful shutdown behavior
- Test ANSI escape sequence processing and cleaning
- Verify real-time state detection accuracy with actual Claude sessions
