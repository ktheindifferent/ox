# Ox Editor - Project Overview for AI Assistant

## Project Description
Ox is a lightweight, flexible terminal-based text editor written in Rust. It's designed to be simple but powerful, featuring a plugin system, syntax highlighting, and extensive configurability through Lua scripting. The editor runs as a text-user-interface (TUI) in the terminal, similar to vim, nano, and micro, but built from the ground up with its own architecture.

## Core Technology Stack
- **Language**: Rust (Edition 2021)
- **Version**: 0.7.7
- **License**: GPL-2.0
- **Target Platforms**: Linux (primary), macOS, Windows
- **Scripting**: Lua 5.4 (for configuration and plugins)

## Project Structure

### Root Structure
```
/root/repo/
├── src/                 # Main application source code
├── kaolinite/          # Text buffer management library (workspace member)
├── plugins/            # Lua plugins and themes
├── config/             # Default configuration files
├── assets/             # Images and media files
├── build scripts       # Platform-specific build scripts
└── documentation       # README, LICENSE, todo.md
```

### Core Components

#### `/src/` - Main Application
- **main.rs**: Entry point and application initialization
- **cli.rs**: Command-line argument parsing
- **editor/**: Core editor functionality
  - Document management
  - Cursor control
  - File tree navigation
  - Interface rendering
  - Mouse support
  - Syntax scanning
- **config/**: Configuration system
  - Editor settings
  - Syntax highlighting
  - Key bindings
  - Color themes
  - Task runner
  - AI assistant integration
- **terminal.rs**: Terminal abstraction layer
- **pty.rs, pty_cross.rs**: Terminal emulation (PTY) support
- **clipboard.rs**: System clipboard integration
- **ui.rs**: User interface rendering
- **events.rs**: Event handling system
- **error.rs**: Error management
- **dirs.rs**: Directory and path utilities

#### `/kaolinite/` - Text Buffer Library
A separate workspace member providing:
- Document structure and manipulation
- Cursor management
- Text searching capabilities
- Word and line operations
- File I/O operations
- Comprehensive test suite

#### `/plugins/` - Lua Extensions
Built-in plugins including:
- **ai.lua**: AI code assistance
- **autoindent.lua**: Smart indentation
- **discord_rpc.lua**: Discord integration
- **emmet.lua**: HTML/CSS expansion
- **git.lua**: Git integration
- **live_html.lua**: HTML preview
- **pairs.lua**: Auto-pairing brackets
- **pomodoro.lua**: Timer functionality
- **quickcomment.lua**: Comment toggling
- **todo.lua**: Todo list management
- **typing_speed.lua**: Typing metrics
- **update_notification.lua**: Version checking
- **themes/**: Color schemes (default16, galaxy, omni, tropical, transparent)

## Key Dependencies

### Core Dependencies
- **crossterm** (0.28.1): Cross-platform terminal manipulation
- **mlua** (0.10): Lua integration with vendored Lua 5.4
- **synoptic** (2.2.9): Syntax highlighting
- **kaolinite**: Internal text buffer management
- **regex** (1.11.1): Regular expression support
- **jargon-args** (0.2.7): CLI argument parsing
- **alinio** (0.2.1): Text alignment utilities
- **shellexpand** (3.1.0): Shell path expansion
- **base64** (0.22.1): Base64 encoding/decoding
- **error_set** (0.7): Error handling utilities

### Platform-Specific Dependencies
Unix/Linux/macOS only:
- **ptyprocess** (0.4.1): PTY process management
- **mio** (1.0.3): Async I/O
- **nix** (0.29.0): Unix system calls

## Key Features

### Core Editing
- Syntax highlighting for multiple languages
- Multiple cursors and macros
- Undo/redo system
- Search and replace with regex
- Mouse support for navigation and selection
- Split view for multiple documents
- File tree browser
- Integrated terminal support (Unix-like systems)

### Configuration System
- Lua-based configuration
- Customizable key bindings
- Themeable interface
- Plugin system for extensibility
- Setup wizard for initial configuration

### Platform Support
- Primary support for Linux
- macOS compatibility with Homebrew/MacPorts
- Windows support (with some limitations)
- Cross-platform build system using cargo-make

## Current Development Status

### Active Development Areas (from todo.md)
1. **Cross-platform compatibility improvements**
   - Windows PTY implementation
   - Path handling consistency
   - Build system enhancements

2. **Recently Completed**
   - Cross-platform PTY abstraction layer
   - Windows clipboard support
   - Cross-platform build configuration
   - Home directory resolution fixes

3. **Known Limitations**
   - Terminal integration limited on Windows
   - Some plugins may have platform-specific issues
   - PTY features not fully implemented on Windows

## Build System
- **Cargo**: Primary build tool
- **cargo-make** (Makefile.toml): Cross-platform task runner
- **Platform scripts**: build.sh (Unix), build.ps1 (Windows)
- **Testing**: test.sh for automated testing
- **Update system**: update.sh for version management

## Testing
- Comprehensive test suite in kaolinite/tests/
- Test data files for various scenarios
- Integration with cargo test framework

## Distribution
- Binary releases for all platforms
- Package formats: DEB (Debian/Ubuntu), RPM (Fedora)
- AUR packages for Arch Linux
- Homebrew formula for macOS
- Direct executable for Windows

## Project Maintenance
- Active development on GitHub
- Regular updates and bug fixes
- Community contributions welcome
- Extensive wiki documentation available

## Important Notes for Development
1. The project uses a workspace structure with kaolinite as a member
2. Lua plugins should use platform-agnostic path handling
3. Terminal features may require conditional compilation for Windows
4. The editor prioritizes simplicity and efficiency
5. All contributions should maintain cross-platform compatibility where possible

## Commands and Scripts
- Run tests: `cargo test`
- Build release: `cargo build --release`
- Run editor: `cargo run` or `ox` (if installed)
- Platform builds: Use Makefile.toml with cargo-make or platform-specific scripts

## Contact and Resources
- GitHub: https://github.com/curlpipe/ox
- Wiki: https://github.com/curlpipe/ox/wiki/
- Discord: Contact handle 'curlpipe'
- License: GNU GPL v2.0