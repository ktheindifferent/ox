# Cross-Platform Compatibility Todo List

## 🎯 High Priority - Core Platform Issues

### 1. Terminal/PTY Support
- [ ] **Windows PTY Implementation**: Currently using Unix-only `ptyprocess`, `mio::unix`, and `nix` crates (src/pty.rs)
  - [ ] Implement Windows ConPTY support or use cross-platform alternative like `portable-pty`
  - [ ] Abstract PTY operations behind a platform-agnostic interface
  - [ ] Handle shell detection for Windows (PowerShell, cmd.exe)
  
### 2. File Path Handling
- [ ] **Path Separator Consistency**: Mixed usage of `/` and `\\` in Lua plugins
  - [ ] Audit all path operations in `plugin/bootstrap.lua` and `plugin/plugin_manager.lua`
  - [ ] Use `std::path::MAIN_SEPARATOR` consistently in Rust code
  - [ ] Implement proper path normalization functions for Lua plugins
  
### 3. Build System Improvements
- [ ] **Cross-Platform Build Script**: Current `build.sh` is Unix-only
  - [ ] Create `build.ps1` for Windows or migrate to cross-platform build tool
  - [ ] Consider using `cargo-make` or `just` for unified build commands
  - [ ] Document build prerequisites for each platform
  
## 🔧 Medium Priority - Feature Parity

### 4. Terminal Integration Features
- [ ] **Integrated Terminal**: Disabled on Windows (conditional compilation in multiple files)
  - [ ] Implement Windows terminal support using Windows Console API
  - [ ] Test terminal colors and escape sequences on all platforms
  - [ ] Ensure proper signal handling across platforms

### 5. File System Operations
- [ ] **Home Directory Resolution**: Uses Unix-style `~` expansion
  - [ ] Use `dirs` or `directories` crate for proper home/config directory detection
  - [ ] Handle Windows environment variables (%USERPROFILE%, %APPDATA%)
  - [ ] Standardize config file locations per platform conventions

### 6. Clipboard Integration
- [ ] **System Clipboard**: Currently using OSC 52 escape sequences (src/ui.rs:278)
  - [ ] Add native clipboard support using `arboard` or `copypasta` crate
  - [ ] Provide fallback for terminals without OSC 52 support
  - [ ] Test clipboard functionality in different terminal emulators

## 📦 Low Priority - Platform-Specific Enhancements

### 7. Package Distribution
- [ ] **Windows Installer**: Only provides `.exe` in build script
  - [ ] Create MSI installer using WiX or cargo-wix
  - [ ] Add to Windows Package Manager (winget)
  - [ ] Create portable ZIP distribution
  
### 8. macOS Specific
- [ ] **macOS Build**: Uses custom SDK path in build.sh
  - [ ] Document official macOS build process
  - [ ] Create Homebrew formula
  - [ ] Add code signing and notarization support
  - [ ] Test on both Intel and Apple Silicon

### 9. Linux Distribution
- [ ] **Package Formats**: Currently supports DEB and RPM
  - [ ] Add AppImage support for universal Linux distribution
  - [ ] Create Flatpak manifest
  - [ ] Add to Snap Store
  - [ ] Create AUR package for Arch Linux

## 🧪 Testing & Documentation

### 10. Cross-Platform Testing
- [ ] **CI/CD Pipeline**: Set up automated testing on all platforms
  - [ ] Configure GitHub Actions for Windows, macOS, and Linux
  - [ ] Add integration tests for platform-specific features
  - [ ] Test on different terminal emulators per platform
  
### 11. Documentation Updates
- [ ] **Platform-Specific Instructions**: 
  - [ ] Document Windows-specific limitations and workarounds
  - [ ] Add troubleshooting guide for common platform issues
  - [ ] Create platform compatibility matrix
  - [ ] Document required dependencies per platform

### 12. Plugin System
- [ ] **Lua Plugin Compatibility**: Ensure plugins work across platforms
  - [ ] Audit all plugins for hardcoded paths or platform assumptions
  - [ ] Test plugin manager on all platforms
  - [ ] Document plugin API platform differences
  - [ ] Add platform detection utilities for plugin authors

## 🐛 Bug Fixes & Improvements

### 13. Shell Detection
- [ ] **Shell Support**: Currently Unix shells only (bash, dash, zsh, fish)
  - [ ] Add Windows shell detection (PowerShell, cmd)
  - [ ] Handle WSL environments properly
  - [ ] Support alternative shells (nu, xonsh, etc.)

### 14. File Permissions
- [ ] **Unix Permissions**: Uses Unix-specific file operations
  - [ ] Abstract file permission handling
  - [ ] Handle Windows file attributes properly
  - [ ] Test file operations with different permission levels

### 15. Process Management
- [ ] **Signal Handling**: Unix-specific signal handling
  - [ ] Implement Windows process control
  - [ ] Handle Ctrl+C/Ctrl+Break consistently
  - [ ] Proper subprocess termination on all platforms

## 🚀 Performance Optimizations

### 16. Platform-Specific Optimizations
- [ ] **Rendering Performance**: Optimize for different terminal capabilities
  - [ ] Detect and use platform-specific terminal features
  - [ ] Optimize for Windows Terminal vs traditional console
  - [ ] Profile and optimize hot paths per platform

### 17. Resource Management
- [ ] **Memory and File Handles**: Ensure proper cleanup on all platforms
  - [ ] Test for resource leaks on Windows
  - [ ] Handle file locking differences between platforms
  - [ ] Optimize for platform-specific file system characteristics

## 📋 Development Environment

### 18. Developer Experience
- [ ] **Cross-Platform Development**:
  - [ ] Add devcontainer configuration for consistent development
  - [ ] Create platform-specific development guides
  - [ ] Document cross-compilation setup
  - [ ] Add pre-commit hooks that work on all platforms

### 19. Debugging Support
- [ ] **Platform-Specific Debugging**:
  - [ ] Add debug configurations for VS Code on all platforms
  - [ ] Document platform-specific debugging techniques
  - [ ] Add logging that works consistently across platforms

### 20. Dependency Management
- [ ] **Platform Dependencies**:
  - [ ] Audit and minimize platform-specific dependencies
  - [ ] Use feature flags for optional platform features
  - [ ] Document dependency installation per platform
  - [ ] Consider static linking for easier distribution

## ✅ Completed Items
- Initial analysis of codebase for platform-specific code
- Identified conditional compilation blocks for Windows vs Unix
- Located path handling inconsistencies
- Reviewed build system limitations

## 📝 Notes
- Priority should be given to items that block basic functionality on Windows
- Consider creating abstraction layers for platform-specific features
- Maintain backward compatibility while adding new platform support
- Regular testing on all target platforms is essential