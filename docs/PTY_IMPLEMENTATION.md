# PTY (Pseudo-Terminal) Implementation

## Overview

Ox Editor includes comprehensive pseudo-terminal (PTY) support for integrated terminal functionality. The implementation provides a cross-platform abstraction layer that works seamlessly on Unix-like systems (Linux, macOS) and Windows.

## Architecture

### Cross-Platform Abstraction (`src/pty_cross.rs`)

The main PTY abstraction provides a unified interface across all platforms:

- **Unix/Linux/macOS**: Uses traditional PTY via `ptyprocess` crate
- **Windows**: Uses ConPTY API (Windows 10 1809+) with fallback to `portable-pty`

### Platform-Specific Implementations

#### Unix Implementation
- Located in `src/pty.rs` (conditionally compiled)
- Uses `ptyprocess` for PTY creation and management
- Leverages `mio` for non-blocking I/O operations
- Supports shell detection via `$SHELL` environment variable

#### Windows Implementation (`src/conpty_windows.rs`)
- Native ConPTY API integration for Windows 10 1809+
- Automatic fallback to `portable-pty` for older Windows versions
- Supports PowerShell, PowerShell Core, and cmd.exe
- Includes Windows-specific signal handling (Ctrl+C, Ctrl+Break)

### Error Handling (`src/pty_error.rs`)

Custom error types provide detailed error information:
- `PtyError`: Comprehensive error enum for all PTY operations
- `PtyResult<T>`: Convenient Result type alias
- Lock poisoning recovery mechanisms
- Context-aware error messages

## Features

### Shell Detection
The system automatically detects and configures the appropriate shell:

**Unix/Linux/macOS:**
- Bash
- Zsh
- Fish
- Dash

**Windows:**
- PowerShell Core (pwsh.exe)
- Windows PowerShell (powershell.exe)
- Command Prompt (cmd.exe)
- WSL integration

### Key Capabilities

1. **Asynchronous I/O**: Non-blocking read/write operations
2. **Thread Safety**: Arc<Mutex<>> wrapper for concurrent access
3. **Automatic Cleanup**: Proper resource cleanup on drop
4. **Signal Handling**: Platform-appropriate signal support
5. **Resize Support**: Dynamic terminal size adjustment
6. **Shell-Specific Behaviors**: Handles echo modes and newline quirks

## Usage

### Creating a PTY Instance

```rust
use ox::pty_cross::{Pty, Shell};

// Detect and use system default shell
let shell = Shell::detect();
let pty = Pty::new(shell)?;

// Or specify a shell explicitly
let pty = Pty::new(Shell::Bash)?;
```

### Running Commands

```rust
{
    let mut pty_lock = pty.lock().unwrap();
    
    // Run a command
    pty_lock.run_command("ls -la\n")?;
    
    // Access output
    println!("Output: {}", pty_lock.output);
    
    // Silent command (clears output first)
    pty_lock.silent_run_command("pwd\n")?;
}
```

### Character Input

```rust
{
    let mut pty_lock = pty.lock().unwrap();
    
    // Send individual characters
    pty_lock.char_input('h')?;
    pty_lock.char_input('e')?;
    pty_lock.char_input('l')?;
    pty_lock.char_input('l')?;
    pty_lock.char_input('o')?;
    
    // Newline executes the accumulated input
    pty_lock.char_input('\n')?;
    
    // Backspace support
    pty_lock.char_pop();
}
```

### Windows-Specific Features

```rust
#[cfg(target_os = "windows")]
{
    use ox::conpty_windows::{ConPty, ConPtySignal};
    
    // Create ConPTY directly
    let mut conpty = ConPty::new("pwsh.exe -NoLogo", 24, 80)?;
    
    // Send signals
    conpty.send_signal(ConPtySignal::CtrlC)?;
    
    // Resize terminal
    conpty.resize(30, 100)?;
    
    // Check if ConPTY is available
    if ConPty::is_conpty_available() {
        println!("ConPTY is supported!");
    }
}
```

## Configuration

The PTY system can be configured through the editor's configuration:

```lua
-- In .oxrc or configuration file
terminal = {
    mouse_enabled = true,
    scroll_amount = 1,
    shell = "zsh"  -- Unix only
}
```

## Testing

Comprehensive test suites are provided:

- `tests/cross_platform_pty_tests.rs`: Cross-platform integration tests
- `tests/windows_pty_tests.rs`: Windows-specific ConPTY tests

Run tests with:
```bash
cargo test --test cross_platform_pty_tests
cargo test --test windows_pty_tests  # Windows only
```

## Platform Requirements

### Unix/Linux/macOS
- No special requirements
- Works with standard system shells

### Windows
- **Recommended**: Windows 10 version 1809 (build 17763) or later for ConPTY
- **Fallback**: Uses portable-pty on older Windows versions
- PowerShell Core provides best experience

## Known Limitations

1. **Windows**: Some ANSI escape sequences may not work on older Windows versions
2. **CI/CD**: PTY creation may fail in environments without TTY allocation
3. **WSL**: Full WSL integration requires WSL to be installed and configured

## Troubleshooting

### "ConPTY not available" on Windows
- Ensure Windows 10 1809 or later is installed
- Check Windows updates
- Fallback to portable-pty will be used automatically

### Shell not detected
- Check `$SHELL` environment variable (Unix)
- Ensure shell executable is in PATH
- Default shells will be used as fallback

### PTY creation fails
- Verify terminal emulator supports PTY
- Check file descriptor limits (Unix)
- Ensure proper permissions for PTY device files

## Contributing

When contributing PTY-related changes:

1. Ensure cross-platform compatibility
2. Add tests for new functionality
3. Update this documentation
4. Test on multiple platforms when possible
5. Handle errors gracefully with appropriate fallbacks

## Related Files

- `src/pty_cross.rs`: Main cross-platform abstraction
- `src/pty.rs`: Unix implementation
- `src/conpty_windows.rs`: Windows ConPTY implementation
- `src/pty_error.rs`: Error handling
- `src/config/interface.rs`: Terminal configuration
- `src/editor/documents.rs`: Terminal document integration