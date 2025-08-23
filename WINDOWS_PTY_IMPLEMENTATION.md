# Windows ConPTY Implementation for Ox Editor

## Overview

This document describes the Windows ConPTY (Console Pseudo Terminal) implementation that enables terminal features in the Ox editor on Windows platforms.

## Implementation Details

### Technology Stack
- **portable-pty v0.9.0**: Cross-platform PTY library that provides ConPTY support on Windows 10+
- **ConPTY API**: Windows native pseudo-terminal interface (available on Windows 10 1809+)
- **Shell Support**: PowerShell Core (pwsh), PowerShell, and cmd.exe

### Key Features

1. **Automatic Shell Detection**
   - Prioritizes PowerShell Core (pwsh.exe) if available
   - Falls back to Windows PowerShell (powershell.exe)
   - Uses cmd.exe as last resort
   - Detects running shell environment via environment variables

2. **Terminal Emulator Compatibility**
   - Windows Terminal: Full ConPTY support
   - VS Code Terminal: Full ConPTY support
   - PowerShell/PowerShell Core: Native integration
   - ConEmu/Cmder: ConPTY support on Windows 10+
   - Command Prompt: Basic PTY functionality

3. **Resource Management**
   - Graceful process termination on PTY drop
   - Proper cleanup of file handles
   - Thread-safe reader/writer access
   - Zombie process prevention

4. **Error Handling**
   - Validates child process state before I/O operations
   - Handles broken pipe errors gracefully
   - UTF-8 validation with lossy conversion for invalid sequences
   - Timeout handling for non-blocking reads

## Testing

### Running Tests

```bash
# Run all tests including Windows-specific tests
cargo test

# Run only PTY-related tests
cargo test pty_cross
```

### Manual Testing on Windows

1. **Basic PTY Creation**
   ```rust
   let pty = Pty::new(Shell::Cmd)?;
   ```

2. **Command Execution**
   ```rust
   let mut pty_lock = pty.lock().unwrap();
   pty_lock.run_command("echo Hello, Windows!\n")?;
   ```

3. **Shell Detection**
   ```rust
   let shell = Shell::detect();
   println!("Detected shell: {:?}", shell);
   ```

### Test Coverage

The implementation includes comprehensive tests for:
- Shell detection on different platforms
- PTY creation and initialization
- Basic I/O operations
- Resource cleanup
- Error handling
- Lua integration
- Cross-platform compatibility

## Known Limitations

1. **Windows Version Requirements**
   - ConPTY requires Windows 10 version 1809 or later
   - Older Windows versions will receive an error message

2. **Terminal Features**
   - Some ANSI escape sequences may behave differently
   - Terminal resizing may have slight delays
   - Mouse support depends on terminal emulator

3. **Performance Considerations**
   - Initial PTY creation may take longer than on Unix systems
   - Buffer sizes are optimized for typical usage (10KB)

## Future Improvements

1. Add support for WinPTY as fallback for older Windows versions
2. Implement configurable buffer sizes
3. Add telemetry for debugging PTY issues
4. Optimize shell detection caching
5. Add support for WSL shells

## Debugging

### Environment Variables
- `COMSPEC`: Default command processor (usually cmd.exe)
- `PSModulePath`: Indicates PowerShell environment

### Common Issues

1. **"PTY not supported" error**
   - Ensure Windows 10 1809+ is installed
   - Check if ConPTY is available in system

2. **Shell not detected correctly**
   - Verify PowerShell/cmd.exe is in PATH
   - Check environment variables

3. **I/O errors**
   - Ensure child process is alive
   - Check for proper UTF-8 encoding

## References

- [ConPTY Documentation](https://docs.microsoft.com/en-us/windows/console/creating-a-pseudoconsole-session)
- [portable-pty Documentation](https://docs.rs/portable-pty/)
- [Windows Terminal Documentation](https://docs.microsoft.com/en-us/windows/terminal/)