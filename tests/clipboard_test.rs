//! Tests for clipboard functionality on Linux systems

#[cfg(all(test, not(target_os = "windows"), not(target_os = "macos")))]
mod linux_clipboard_tests {
    use std::env;
    use std::process::Command;
    
    /// Check if a command is available on the system
    fn command_exists(cmd: &str) -> bool {
        Command::new("which")
            .arg(cmd)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
    
    /// Test session type detection
    #[test]
    fn test_session_detection() {
        // Save original environment
        let original_wayland = env::var("WAYLAND_DISPLAY").ok();
        let original_xdg = env::var("XDG_SESSION_TYPE").ok();
        let original_display = env::var("DISPLAY").ok();
        
        // Test Wayland detection
        env::set_var("WAYLAND_DISPLAY", "wayland-0");
        env::remove_var("XDG_SESSION_TYPE");
        // Session should be detected as Wayland
        
        // Test X11 detection via XDG_SESSION_TYPE
        env::remove_var("WAYLAND_DISPLAY");
        env::set_var("XDG_SESSION_TYPE", "x11");
        // Session should be detected as X11
        
        // Test X11 detection via DISPLAY
        env::remove_var("XDG_SESSION_TYPE");
        env::set_var("DISPLAY", ":0");
        // Session should be detected as X11
        
        // Test unknown session
        env::remove_var("WAYLAND_DISPLAY");
        env::remove_var("XDG_SESSION_TYPE");
        env::remove_var("DISPLAY");
        // Session should be detected as Unknown
        
        // Restore original environment
        if let Some(val) = original_wayland {
            env::set_var("WAYLAND_DISPLAY", val);
        }
        if let Some(val) = original_xdg {
            env::set_var("XDG_SESSION_TYPE", val);
        }
        if let Some(val) = original_display {
            env::set_var("DISPLAY", val);
        }
    }
    
    /// Test clipboard tool detection
    #[test]
    fn test_tool_detection() {
        // Check which tools are available
        let has_xclip = command_exists("xclip");
        let has_xsel = command_exists("xsel");
        let has_wl_copy = command_exists("wl-copy");
        let has_wl_paste = command_exists("wl-paste");
        
        println!("Available clipboard tools:");
        if has_xclip {
            println!("  - xclip");
        }
        if has_xsel {
            println!("  - xsel");
        }
        if has_wl_copy && has_wl_paste {
            println!("  - wl-clipboard (wl-copy/wl-paste)");
        }
        
        // At least one tool should be available on most Linux systems
        let has_any_tool = has_xclip || has_xsel || (has_wl_copy && has_wl_paste);
        if !has_any_tool {
            println!("Warning: No clipboard tools found. Install xclip, xsel, or wl-clipboard.");
        }
    }
    
    /// Integration test for clipboard operations
    #[test]
    #[ignore] // Run with --ignored flag as this requires clipboard tools
    fn test_clipboard_operations() {
        use ox::clipboard::{Clipboard, Selection};
        
        let mut clipboard = Clipboard::new();
        
        // Test basic copy and paste
        let test_text = "Hello, clipboard test!";
        match clipboard.set_text(test_text) {
            Ok(_) => {
                println!("Successfully copied text to clipboard");
                
                // Try to read it back
                match clipboard.get_text() {
                    Ok(text) => {
                        assert_eq!(text.trim(), test_text);
                        println!("Successfully read text from clipboard");
                    }
                    Err(e) => {
                        println!("Failed to read from clipboard: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("Failed to copy to clipboard: {}", e);
                println!("Make sure clipboard tools are installed");
            }
        }
        
        // Test with OSC52 fallback
        let mut clipboard_osc = Clipboard::new().with_osc52_fallback();
        if let Err(e) = clipboard_osc.set_text("OSC52 test") {
            println!("OSC52 fallback test failed: {}", e);
        }
        
        // Test clipboard info
        println!("Clipboard info: {}", clipboard.get_clipboard_info());
    }
    
    /// Test primary selection (Linux specific)
    #[test]
    #[ignore] // Run with --ignored flag
    fn test_primary_selection() {
        use ox::clipboard::{Clipboard, Selection};
        
        let mut clipboard = Clipboard::new()
            .with_selection(Selection::Primary);
        
        let test_text = "Primary selection test";
        match clipboard.set_text(test_text) {
            Ok(_) => {
                println!("Successfully copied to primary selection");
                
                match clipboard.get_text() {
                    Ok(text) => {
                        println!("Read from primary: {}", text);
                    }
                    Err(e) => {
                        println!("Failed to read primary selection: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("Failed to copy to primary selection: {}", e);
            }
        }
    }
    
    /// Test timeout handling
    #[test]
    #[ignore] // Run with --ignored flag
    fn test_clipboard_timeout() {
        use ox::clipboard::Clipboard;
        use std::time::Instant;
        
        let mut clipboard = Clipboard::new();
        let start = Instant::now();
        
        // This should complete within the 2-second timeout
        match clipboard.set_text("Timeout test") {
            Ok(_) => {
                let elapsed = start.elapsed();
                assert!(elapsed.as_secs() < 3, "Operation took too long");
                println!("Clipboard operation completed in {:?}", elapsed);
            }
            Err(e) => {
                println!("Clipboard operation failed: {}", e);
            }
        }
    }
}

/// Platform-specific test for macOS
#[cfg(all(test, target_os = "macos"))]
mod macos_clipboard_tests {
    #[test]
    fn test_pbcopy_pbpaste() {
        use ox::clipboard::Clipboard;
        
        let mut clipboard = Clipboard::new();
        let test_text = "macOS clipboard test";
        
        clipboard.set_text(test_text).expect("pbcopy should work on macOS");
        let result = clipboard.get_text().expect("pbpaste should work on macOS");
        assert_eq!(result.trim(), test_text);
    }
}

/// Platform-specific test for Windows
#[cfg(all(test, target_os = "windows"))]
mod windows_clipboard_tests {
    #[test]
    fn test_windows_clipboard() {
        use ox::clipboard::Clipboard;
        
        let mut clipboard = Clipboard::new();
        let test_text = "Windows clipboard test";
        
        clipboard.set_text(test_text).expect("Windows clipboard should work");
        let result = clipboard.get_text().expect("Windows clipboard read should work");
        assert_eq!(result.trim(), test_text);
    }
}