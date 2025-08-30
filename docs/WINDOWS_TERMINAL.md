# Windows Terminal Support in Ox Editor

## Overview

Ox Editor now includes full Windows terminal support using the ConPTY (Console Pseudo Terminal) API, providing feature parity with Unix-like systems. This implementation enables integrated terminal functionality on Windows 10 version 1809 (build 17763) and later.

## System Requirements

### Minimum Requirements
- **Windows 10 version 1809** (October 2018 Update, build 17763) or later
- **Windows 11** (all versions supported)
- **Windows Terminal** (recommended) or any ConPTY-compatible terminal emulator

### Supported Terminal Emulators
- **Windows Terminal** - Full ConPTY support with all features
- **PowerShell/PowerShell Core** - Native ConPTY integration  
- **Command Prompt (cmd.exe)** - Basic PTY functionality
- **VS Code Terminal** - Full ConPTY support
- **ConEmu/Cmder** - ConPTY support on Windows 10+
- **MinTTY (Git Bash/MSYS2)** - May require special handling

## Features

### Shell Support
The implementation automatically detects and supports multiple Windows shells:

1. **PowerShell Core (pwsh.exe)** - Preferred shell with modern features
2. **Windows PowerShell (powershell.exe)** - Legacy PowerShell support
3. **Command Prompt (cmd.exe)** - Traditional Windows shell
4. **Windows Subsystem for Linux (wsl.exe)** - Linux environment support
5. **Git Bash** - MinGW/MSYS2 bash support

### Terminal Operations
- **Process spawning** with CreatePseudoConsole API
- **Bidirectional I/O** with proper buffering
- **Window resizing** with dynamic PTY size adjustment
- **Signal handling** (Ctrl+C, Ctrl+Break, Ctrl+Z)
- **ANSI escape sequences** for colors and formatting
- **Graceful process termination** with cleanup

## Technical Implementation

### Architecture
The Windows terminal support uses a dual-backend approach:

1. **Native ConPTY** (Primary)
   - Direct integration with Windows ConPTY API
   - Optimal performance and compatibility
   - Available on Windows 10 1809+

2. **Portable PTY** (Fallback)
   - Cross-platform compatibility layer
   - Used when ConPTY is unavailable
   - Ensures basic functionality on older systems

### Key Components

#### `conpty_windows.rs`
Native Windows ConPTY implementation providing:
- Direct ConPTY API bindings
- Process management with Windows handles
- Asynchronous I/O with background reader thread
- Signal injection for process control

#### `pty_cross.rs`
Cross-platform abstraction layer offering:
- Unified PTY interface for all platforms
- Automatic backend selection (ConPTY vs portable-pty)
- Shell detection and configuration
- Platform-specific optimizations

## Usage

### Opening a Terminal
Terminals can be opened in any direction using the editor API:

```lua
-- Open terminal to the right
editor:open_terminal_right()

-- Open terminal below with a command
editor:open_terminal_down("npm start")

-- Open terminal to the left
editor:open_terminal_left()

-- Open terminal above
editor:open_terminal_up()
```

### Shell Configuration
The terminal automatically detects the best available shell, but you can configure it:

```lua
-- In your .oxrc configuration file
terminal.shell = "pwsh"       -- PowerShell Core
terminal.shell = "powershell" -- Windows PowerShell  
terminal.shell = "cmd"        -- Command Prompt
terminal.shell = "wsl"        -- WSL/Linux
```

### Running Files
The integrated terminal supports running files based on their type:

```lua
-- Configure runners for different file types
runner = {
    python = {
        run = "python {file_path}"
    },
    javascript = {
        run = "node {file_path}"
    },
    rust = {
        compile = "rustc {file_path}",
        run = "{file_path}.exe"
    }
}
```

## Keyboard Shortcuts

### Terminal Control
- **Ctrl+C** - Send interrupt signal (SIGINT equivalent)
- **Ctrl+Break** - Force break (Windows-specific)
- **Ctrl+Z** - Suspend process (limited support)
- **Ctrl+D** - Send EOF (exit shell)

### Navigation
Standard Ox navigation keys work within terminal windows:
- Arrow keys for cursor movement
- Page Up/Down for scrolling
- Home/End for line navigation

## Troubleshooting

### ConPTY Not Available
If you see "ConPTY requires Windows 10 version 1809 or later":
1. Check Windows version: `winver` in Run dialog
2. Update Windows if on an older version
3. The editor will automatically fall back to portable-pty

### Terminal Not Responding
1. Check if the shell process is running
2. Try a different shell (e.g., cmd instead of PowerShell)
3. Verify no antivirus is blocking PTY operations

### Character Encoding Issues
1. Ensure Windows Terminal or compatible emulator is used
2. Check terminal encoding settings (UTF-8 recommended)
3. Verify ANSI escape sequence support is enabled

### Performance Issues
1. Disable unnecessary PowerShell modules
2. Use PowerShell Core (pwsh) instead of Windows PowerShell
3. Consider using cmd.exe for simple operations

## Known Limitations

### Windows-Specific
- Some Unix-specific escape sequences may not work
- Signal handling differs from Unix (no SIGTSTP, SIGCONT)
- PTY size limited by Windows console buffer constraints

### Shell-Specific
- PowerShell may add extra formatting to output
- Interactive prompts may behave differently than on Unix
- Some shells may require specific flags for PTY mode

## Development

### Building on Windows
```bash
# Requires Rust toolchain
cargo build --release

# Run tests including Windows-specific tests
cargo test

# Run with debug output
RUST_LOG=debug cargo run
```

### Testing ConPTY
```rust
// Check if ConPTY is available
if ConPty::is_conpty_available() {
    println!("ConPTY is supported!");
}

// Detect available shells
let shells = WindowsShellDetector::detect_available_shells();
for shell in shells {
    println!("Found: {} at {}", shell.name, shell.executable);
}
```

## Future Enhancements

Planned improvements for Windows terminal support:

1. **Enhanced WSL Integration**
   - Direct WSL distribution selection
   - Linux path translation
   - Seamless file operations

2. **Advanced ConPTY Features**
   - Mouse input support in terminal
   - Extended color palette (24-bit color)
   - Custom process environments

3. **Performance Optimizations**
   - Reduced latency for I/O operations
   - Improved scrollback buffer management
   - Faster process spawning

4. **Developer Tools**
   - Integrated debugger support
   - Build system integration
   - Task runner improvements

## Contributing

Contributions to improve Windows terminal support are welcome! Areas of interest:

- Testing on different Windows versions
- Support for additional shells and terminal emulators
- Performance optimizations
- Bug fixes and compatibility improvements

Please see [CONTRIBUTING.md](../CONTRIBUTING.md) for guidelines.

## Resources

### Documentation
- [ConPTY Documentation](https://docs.microsoft.com/en-us/windows/console/creating-a-pseudoconsole-session)
- [Windows Terminal Sequences](https://docs.microsoft.com/en-us/windows/console/console-virtual-terminal-sequences)
- [Windows Console API](https://docs.microsoft.com/en-us/windows/console/)

### Related Projects
- [Windows Terminal](https://github.com/microsoft/terminal)
- [portable-pty](https://github.com/wez/portable-pty)
- [winpty](https://github.com/rprichard/winpty)

## License

The Windows terminal support is part of Ox Editor and licensed under GPL-2.0. See [LICENSE](../LICENSE) for details.