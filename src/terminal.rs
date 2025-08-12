//! Cross-platform terminal detection and capabilities

use std::env;

/// Terminal types
#[derive(Debug, Clone, PartialEq)]
pub enum TerminalType {
    // Windows terminals
    WindowsTerminal,
    WindowsConsole,
    ConEmu,
    Cmder,
    
    // Cross-platform terminals
    Alacritty,
    WezTerm,
    Kitty,
    VSCode,
    
    // Unix terminals
    Gnome,
    Konsole,
    Xterm,
    Rxvt,
    Tmux,
    Screen,
    ITerm2,
    Terminal,  // macOS Terminal.app
    
    // Other
    Unknown,
}

impl TerminalType {
    /// Detect the current terminal type
    pub fn detect() -> Self {
        // Check for specific terminal environment variables
        if let Ok(term_program) = env::var("TERM_PROGRAM") {
            match term_program.to_lowercase().as_str() {
                "vscode" => return Self::VSCode,
                "iterm.app" => return Self::ITerm2,
                "apple_terminal" => return Self::Terminal,
                "wezterm" => return Self::WezTerm,
                _ => {}
            }
        }
        
        // Check for Windows Terminal
        if env::var("WT_SESSION").is_ok() || env::var("WT_PROFILE_ID").is_ok() {
            return Self::WindowsTerminal;
        }
        
        // Check for ConEmu/Cmder
        if env::var("ConEmuPID").is_ok() {
            return Self::ConEmu;
        }
        
        // Check for Alacritty
        if env::var("ALACRITTY_WINDOW_ID").is_ok() {
            return Self::Alacritty;
        }
        
        // Check for Kitty
        if env::var("KITTY_WINDOW_ID").is_ok() {
            return Self::Kitty;
        }
        
        // Check terminal emulator hints
        if let Ok(term) = env::var("TERM") {
            let term_lower = term.to_lowercase();
            
            if term_lower.contains("alacritty") {
                return Self::Alacritty;
            }
            if term_lower.contains("kitty") {
                return Self::Kitty;
            }
            if term_lower.contains("xterm") {
                return Self::Xterm;
            }
            if term_lower.contains("rxvt") {
                return Self::Rxvt;
            }
            if term_lower.contains("screen") {
                return Self::Screen;
            }
            if term_lower.contains("tmux") {
                return Self::Tmux;
            }
        }
        
        // Check for GNOME Terminal
        if env::var("GNOME_TERMINAL_SERVICE").is_ok() 
            || env::var("VTE_VERSION").is_ok() {
            return Self::Gnome;
        }
        
        // Check for Konsole
        if env::var("KONSOLE_VERSION").is_ok() {
            return Self::Konsole;
        }
        
        // On Windows, default to Windows Console if nothing else detected
        #[cfg(target_os = "windows")]
        {
            return Self::WindowsConsole;
        }
        
        Self::Unknown
    }
    
    /// Check if terminal supports true color (24-bit RGB)
    pub fn supports_true_color(&self) -> bool {
        // First check COLORTERM environment variable
        if let Ok(colorterm) = env::var("COLORTERM") {
            if colorterm.contains("truecolor") || colorterm.contains("24bit") {
                return true;
            }
        }
        
        // Check specific terminals known to support true color
        match self {
            Self::WindowsTerminal |
            Self::Alacritty |
            Self::WezTerm |
            Self::Kitty |
            Self::VSCode |
            Self::ITerm2 |
            Self::Gnome |
            Self::Konsole => true,
            
            Self::WindowsConsole => {
                // Windows 10 1703+ supports true color
                #[cfg(target_os = "windows")]
                {
                    if let Ok(version) = env::var("WINDOWS_VERSION") {
                        // Simple check - proper implementation would use Windows API
                        return version >= "10.0.15063".to_string();
                    }
                    // Assume modern Windows supports it
                    true
                }
                #[cfg(not(target_os = "windows"))]
                false
            }
            
            Self::Terminal => {
                // macOS Terminal.app supports true color since macOS 10.13
                true
            }
            
            Self::Xterm | Self::Rxvt => {
                // Check TERM variable for color support hints
                if let Ok(term) = env::var("TERM") {
                    term.contains("256color") || term.contains("truecolor")
                } else {
                    false
                }
            }
            
            Self::Tmux | Self::Screen => {
                // Depends on the underlying terminal
                if let Ok(term) = env::var("TERM") {
                    term.contains("256color") || term.contains("truecolor")
                } else {
                    false
                }
            }
            
            _ => false,
        }
    }
    
    /// Check if terminal supports Unicode
    pub fn supports_unicode(&self) -> bool {
        // Check LANG/LC_ALL for UTF-8 support
        let is_utf8 = env::var("LANG")
            .or_else(|_| env::var("LC_ALL"))
            .map(|v| v.to_lowercase().contains("utf-8") || v.to_lowercase().contains("utf8"))
            .unwrap_or(false);
        
        match self {
            // Modern terminals generally support Unicode
            Self::WindowsTerminal |
            Self::Alacritty |
            Self::WezTerm |
            Self::Kitty |
            Self::VSCode |
            Self::ITerm2 |
            Self::Terminal |
            Self::Gnome |
            Self::Konsole => true,
            
            Self::WindowsConsole => {
                // Windows Console supports Unicode with proper code page
                #[cfg(target_os = "windows")]
                {
                    // Would need to check console code page
                    // For now, assume modern Windows supports it
                    true
                }
                #[cfg(not(target_os = "windows"))]
                false
            }
            
            _ => is_utf8,
        }
    }
    
    /// Check if terminal supports mouse input
    pub fn supports_mouse(&self) -> bool {
        match self {
            Self::WindowsTerminal |
            Self::Alacritty |
            Self::WezTerm |
            Self::Kitty |
            Self::VSCode |
            Self::ITerm2 |
            Self::Terminal |
            Self::Gnome |
            Self::Konsole |
            Self::Xterm => true,
            
            Self::WindowsConsole => {
                // Windows Console has limited mouse support
                #[cfg(target_os = "windows")]
                true
                #[cfg(not(target_os = "windows"))]
                false
            }
            
            _ => false,
        }
    }
    
    /// Check if terminal supports OSC 52 (clipboard via escape sequences)
    pub fn supports_osc52(&self) -> bool {
        match self {
            Self::Alacritty |
            Self::WezTerm |
            Self::Kitty |
            Self::ITerm2 |
            Self::WindowsTerminal |
            Self::Xterm => true,
            
            Self::Tmux => {
                // Tmux supports OSC 52 with proper configuration
                env::var("TMUX").is_ok()
            }
            
            _ => false,
        }
    }
    
    /// Get terminal name as string
    pub fn name(&self) -> &str {
        match self {
            Self::WindowsTerminal => "Windows Terminal",
            Self::WindowsConsole => "Windows Console",
            Self::ConEmu => "ConEmu",
            Self::Cmder => "Cmder",
            Self::Alacritty => "Alacritty",
            Self::WezTerm => "WezTerm",
            Self::Kitty => "Kitty",
            Self::VSCode => "VS Code Terminal",
            Self::Gnome => "GNOME Terminal",
            Self::Konsole => "Konsole",
            Self::Xterm => "XTerm",
            Self::Rxvt => "RXVT",
            Self::Tmux => "tmux",
            Self::Screen => "GNU Screen",
            Self::ITerm2 => "iTerm2",
            Self::Terminal => "Terminal.app",
            Self::Unknown => "Unknown Terminal",
        }
    }
}

/// Terminal capabilities
pub struct TerminalCapabilities {
    pub terminal_type: TerminalType,
    pub true_color: bool,
    pub unicode: bool,
    pub mouse: bool,
    pub osc52: bool,
}

impl TerminalCapabilities {
    /// Detect current terminal capabilities
    pub fn detect() -> Self {
        let terminal_type = TerminalType::detect();
        Self {
            true_color: terminal_type.supports_true_color(),
            unicode: terminal_type.supports_unicode(),
            mouse: terminal_type.supports_mouse(),
            osc52: terminal_type.supports_osc52(),
            terminal_type,
        }
    }
    
    /// Check if running in a known terminal emulator
    pub fn is_known_terminal(&self) -> bool {
        self.terminal_type != TerminalType::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_terminal_detection() {
        let terminal = TerminalType::detect();
        println!("Detected terminal: {:?}", terminal);
        println!("Terminal name: {}", terminal.name());
    }
    
    #[test]
    fn test_capabilities() {
        let caps = TerminalCapabilities::detect();
        println!("Terminal: {}", caps.terminal_type.name());
        println!("True color: {}", caps.true_color);
        println!("Unicode: {}", caps.unicode);
        println!("Mouse: {}", caps.mouse);
        println!("OSC52: {}", caps.osc52);
    }
}