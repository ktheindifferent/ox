# Clipboard Support in Ox Editor

## Overview

Ox provides comprehensive clipboard support across Linux, macOS, and Windows platforms. The clipboard implementation includes intelligent tool detection, session-aware behavior, and automatic fallback mechanisms.

## Platform Support

### Linux

#### Session Detection
Ox automatically detects your desktop session type to use the most appropriate clipboard tool:
- **Wayland**: Prioritizes `wl-clipboard` tools
- **X11**: Prioritizes `xclip` or `xsel`
- **Unknown/SSH**: Falls back to available tools or OSC 52

#### Required Tools

Install at least one of the following clipboard tools:

**For Wayland:**
```bash
# Debian/Ubuntu
sudo apt-get install wl-clipboard

# Fedora
sudo dnf install wl-clipboard

# Arch Linux
sudo pacman -S wl-clipboard
```

**For X11:**
```bash
# Debian/Ubuntu
sudo apt-get install xclip
# or
sudo apt-get install xsel

# Fedora
sudo dnf install xclip
# or
sudo dnf install xsel

# Arch Linux
sudo pacman -S xclip
# or
sudo pacman -S xsel
```

#### Features
- **Dual Selection Support**: Both clipboard and primary selections
- **Automatic Tool Detection**: Caches the best available tool on first use
- **Session-Aware**: Chooses appropriate tools based on Wayland vs X11
- **Timeout Protection**: 2-second timeout prevents hanging operations
- **Fallback Chain**: Native tool → OSC 52 → cached text

### macOS

Uses native `pbcopy` and `pbpaste` commands (pre-installed on all macOS systems).

### Windows

Uses native Windows clipboard API through system calls (no external dependencies required).

## OSC 52 Fallback

When native clipboard tools are unavailable (e.g., SSH sessions, containers), Ox can fall back to OSC 52 escape sequences to interact with the terminal's clipboard buffer.

To enable OSC 52 fallback:
```rust
let clipboard = Clipboard::new().with_osc52_fallback();
```

**Note**: OSC 52 support depends on your terminal emulator. Most modern terminals support it, including:
- iTerm2
- Windows Terminal
- Alacritty
- Kitty
- WezTerm
- Many others

## API Usage

### Basic Operations
```rust
use ox::clipboard::Clipboard;

// Create clipboard instance
let mut clipboard = Clipboard::new();

// Copy text
clipboard.set_text("Hello, world!")?;

// Paste text
let text = clipboard.get_text()?;
```

### Linux Primary Selection
```rust
use ox::clipboard::{Clipboard, Selection};

// Use primary selection (middle-click paste)
let mut clipboard = Clipboard::new()
    .with_selection(Selection::Primary);

clipboard.set_text("Primary selection text")?;
```

### Debugging
```rust
// Get clipboard system information
let info = clipboard.get_clipboard_info();
println!("{}", info);
// Output: "Linux session: Wayland, Tool: Some(WlClipboard), OSC52 fallback: false"
```

## Troubleshooting

### Linux

#### No clipboard tool found
**Error**: "No clipboard tool found. Install xclip, xsel, or wl-clipboard"

**Solution**: Install one of the recommended clipboard tools for your session type (see Required Tools above).

#### Timeout errors
**Error**: "Clipboard operation timed out"

**Possible causes**:
- Clipboard tool is hanging
- System clipboard service is unresponsive
- Running in a container without clipboard access

**Solutions**:
- Try a different clipboard tool
- Enable OSC 52 fallback for terminal-based clipboard
- Check if clipboard service is running (`systemctl --user status clipmenud` on some systems)

#### Wayland clipboard not working
**Symptoms**: Clipboard operations fail on Wayland despite having wl-clipboard installed

**Solutions**:
- Ensure `WAYLAND_DISPLAY` environment variable is set
- Check if XWayland is available as fallback
- Install xclip for XWayland compatibility

### SSH Sessions

For clipboard support over SSH:

1. **Enable OSC 52** in your Ox configuration
2. **Configure your terminal** to allow OSC 52 sequences:
   - iTerm2: Preferences → General → Selection → "Applications in terminal may access clipboard"
   - Windows Terminal: Enabled by default
   - tmux: Add to `.tmux.conf`: `set -g set-clipboard on`

3. **Use SSH with X11 forwarding** (alternative):
   ```bash
   ssh -X user@host
   ```

## Testing

Run clipboard tests:
```bash
# Basic tests
cargo test --test clipboard_test

# Including integration tests (requires clipboard tools)
cargo test --test clipboard_test -- --ignored --nocapture
```

## Implementation Details

### Tool Detection Order

**Wayland Session:**
1. wl-clipboard (wl-copy/wl-paste)
2. xclip (XWayland fallback)
3. xsel (XWayland fallback)

**X11 Session:**
1. xclip
2. xsel
3. wl-clipboard (as last resort)

### Caching

- Session type is detected once and cached
- Clipboard tool is detected once and cached
- Reduces overhead of repeated `which` commands

### Error Handling

The clipboard system implements a multi-level fallback strategy:

1. **Native tool attempt**: Uses detected clipboard tool
2. **OSC 52 fallback**: If enabled and native fails
3. **Cached text fallback**: Returns last successfully copied text (read operations only)
4. **Error propagation**: Clear error messages guide users to solutions

## Performance Considerations

- **Tool detection**: Happens once per program execution (cached)
- **Timeout**: 2-second timeout prevents indefinite hanging
- **Process spawning**: Minimal overhead for clipboard operations
- **Memory**: Stores last copied text for fallback (cleared on program exit)

## Security Notes

- Clipboard contents are not logged or persisted to disk
- OSC 52 sequences are base64-encoded
- No clipboard history is maintained
- Primary selection is separate from clipboard selection on Linux

## Contributing

When contributing clipboard-related changes:

1. Test on multiple platforms (Linux/X11, Linux/Wayland, macOS, Windows)
2. Ensure backward compatibility
3. Add tests for new functionality
4. Update this documentation
5. Consider security implications of clipboard access