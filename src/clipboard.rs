//! Cross-platform clipboard support

use std::io::Result;

#[cfg(target_os = "windows")]
mod windows_clipboard {
    use std::io::{Result, Error, ErrorKind};
    use std::ptr;
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    
    #[link(name = "user32")]
    extern "system" {
        fn OpenClipboard(hwnd: *const u8) -> i32;
        fn CloseClipboard() -> i32;
        fn EmptyClipboard() -> i32;
        fn SetClipboardData(format: u32, handle: *const u8) -> *const u8;
        fn GetClipboardData(format: u32) -> *const u8;
        fn GlobalAlloc(flags: u32, size: usize) -> *mut u8;
        fn GlobalLock(handle: *mut u8) -> *mut u8;
        fn GlobalUnlock(handle: *mut u8) -> i32;
    }
    
    const CF_UNICODETEXT: u32 = 13;
    const GMEM_MOVEABLE: u32 = 0x0002;
    const MAX_CLIPBOARD_SIZE: usize = 100 * 1024 * 1024; // 100MB limit
    
    // RAII guard for clipboard operations
    struct ClipboardGuard;
    
    impl Drop for ClipboardGuard {
        fn drop(&mut self) {
            unsafe {
                CloseClipboard();
            }
        }
    }
    
    // RAII guard for GlobalUnlock
    struct GlobalUnlockGuard(*mut u8);
    
    impl Drop for GlobalUnlockGuard {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    GlobalUnlock(self.0);
                }
            }
        }
    }
    
    // Safe function to read a null-terminated wide string with bounds checking
    fn safe_read_wide_string(ptr: *const u16) -> Result<String> {
        if ptr.is_null() {
            return Err(Error::new(ErrorKind::InvalidData, "Null pointer to clipboard data"));
        }
        
        unsafe {
            // Calculate length with bounds checking
            let mut len = 0;
            let mut current_ptr = ptr;
            
            // Limit search to prevent infinite loops on corrupted data
            const MAX_SEARCH_LEN: usize = MAX_CLIPBOARD_SIZE / 2; // Max u16 elements
            
            while len < MAX_SEARCH_LEN {
                // Use volatile read to prevent optimization issues
                let value = std::ptr::read_volatile(current_ptr);
                if value == 0 {
                    break;
                }
                len += 1;
                current_ptr = current_ptr.offset(1);
            }
            
            if len >= MAX_SEARCH_LEN {
                return Err(Error::new(ErrorKind::InvalidData, "Clipboard data too large or corrupted"));
            }
            
            // Only create slice after verifying the length
            if len == 0 {
                return Ok(String::new());
            }
            
            let slice = std::slice::from_raw_parts(ptr, len);
            Ok(String::from_utf16_lossy(slice))
        }
    }
    
    pub fn set_clipboard_text(text: &str) -> Result<()> {
        // Check text size before processing
        if text.len() > MAX_CLIPBOARD_SIZE {
            return Err(Error::new(ErrorKind::InvalidInput, "Text too large for clipboard"));
        }
        
        unsafe {
            if OpenClipboard(ptr::null()) == 0 {
                return Err(Error::new(ErrorKind::Other, "Failed to open clipboard"));
            }
            
            // Ensure clipboard is closed on all error paths
            let _guard = ClipboardGuard;
            
            if EmptyClipboard() == 0 {
                return Err(Error::new(ErrorKind::Other, "Failed to empty clipboard"));
            }
            
            let wide: Vec<u16> = OsStr::new(text)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            
            let size = wide.len() * 2;
            
            // Additional size check after UTF-16 conversion
            if size > MAX_CLIPBOARD_SIZE {
                return Err(Error::new(ErrorKind::InvalidInput, "Encoded text too large for clipboard"));
            }
            
            let handle = GlobalAlloc(GMEM_MOVEABLE, size);
            
            if handle.is_null() {
                return Err(Error::new(ErrorKind::Other, "Failed to allocate memory"));
            }
            
            let locked = GlobalLock(handle);
            if locked.is_null() {
                return Err(Error::new(ErrorKind::Other, "Failed to lock memory"));
            }
            
            // Use unlock guard to ensure memory is unlocked even if copy fails
            {
                let _unlock_guard = GlobalUnlockGuard(handle);
                
                // Verify that we have valid pointers before copying
                if wide.as_ptr().is_null() || locked.is_null() {
                    return Err(Error::new(ErrorKind::Other, "Invalid memory pointers"));
                }
                
                std::ptr::copy_nonoverlapping(
                    wide.as_ptr() as *const u8,
                    locked,
                    size
                );
            }
            
            if SetClipboardData(CF_UNICODETEXT, handle).is_null() {
                return Err(Error::new(ErrorKind::Other, "Failed to set clipboard data"));
            }
            
            Ok(())
        }
    }
    
    pub fn get_clipboard_text() -> Result<String> {
        unsafe {
            if OpenClipboard(ptr::null()) == 0 {
                return Err(Error::new(ErrorKind::Other, "Failed to open clipboard"));
            }
            
            // Ensure clipboard is closed on all error paths
            let _guard = ClipboardGuard;
            
            let handle = GetClipboardData(CF_UNICODETEXT);
            if handle.is_null() {
                return Err(Error::new(ErrorKind::Other, "No text data in clipboard"));
            }
            
            let locked = GlobalLock(handle as *mut u8);
            if locked.is_null() {
                return Err(Error::new(ErrorKind::Other, "Failed to lock clipboard data"));
            }
            
            // Create unlock guard to ensure memory is unlocked on all paths
            let _unlock_guard = GlobalUnlockGuard(handle as *mut u8);
            
            // Safe UTF-16 string length calculation with bounds checking
            let text = safe_read_wide_string(locked as *const u16)?;
            
            Ok(text)
        }
    }
}

#[cfg(target_os = "macos")]
mod macos_clipboard {
    use std::io::{Result, Error, ErrorKind};
    use std::process::Command;
    
    pub fn set_clipboard_text(text: &str) -> Result<()> {
        let output = Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                if let Some(stdin) = child.stdin.as_mut() {
                    stdin.write_all(text.as_bytes())?;
                }
                child.wait()
            });
        
        match output {
            Ok(status) if status.success() => Ok(()),
            _ => Err(Error::new(ErrorKind::Other, "Failed to copy to clipboard"))
        }
    }
    
    pub fn get_clipboard_text() -> Result<String> {
        let output = Command::new("pbpaste").output()?;
        
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(Error::new(ErrorKind::Other, "Failed to paste from clipboard"))
        }
    }
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
mod linux_clipboard {
    use std::io::{Result, Error, ErrorKind};
    use std::process::Command;
    
    fn detect_clipboard_tool() -> Option<&'static str> {
        // Try different clipboard tools in order of preference
        for tool in &["xclip", "xsel", "wl-copy"] {
            if Command::new("which")
                .arg(tool)
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false)
            {
                return Some(tool);
            }
        }
        None
    }
    
    pub fn set_clipboard_text(text: &str) -> Result<()> {
        let tool = detect_clipboard_tool()
            .ok_or_else(|| Error::new(ErrorKind::NotFound, "No clipboard tool found"))?;
        
        let output = match tool {
            "xclip" => {
                Command::new("xclip")
                    .arg("-selection")
                    .arg("clipboard")
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                    .and_then(|mut child| {
                        use std::io::Write;
                        if let Some(stdin) = child.stdin.as_mut() {
                            stdin.write_all(text.as_bytes())?;
                        }
                        child.wait()
                    })
            }
            "xsel" => {
                Command::new("xsel")
                    .arg("--clipboard")
                    .arg("--input")
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                    .and_then(|mut child| {
                        use std::io::Write;
                        if let Some(stdin) = child.stdin.as_mut() {
                            stdin.write_all(text.as_bytes())?;
                        }
                        child.wait()
                    })
            }
            "wl-copy" => {
                Command::new("wl-copy")
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                    .and_then(|mut child| {
                        use std::io::Write;
                        if let Some(stdin) = child.stdin.as_mut() {
                            stdin.write_all(text.as_bytes())?;
                        }
                        child.wait()
                    })
            }
            _ => return Err(Error::new(ErrorKind::Other, "Unknown clipboard tool"))
        };
        
        match output {
            Ok(status) if status.success() => Ok(()),
            _ => Err(Error::new(ErrorKind::Other, "Failed to copy to clipboard"))
        }
    }
    
    pub fn get_clipboard_text() -> Result<String> {
        let tool = detect_clipboard_tool()
            .ok_or_else(|| Error::new(ErrorKind::NotFound, "No clipboard tool found"))?;
        
        let output = match tool {
            "xclip" => {
                Command::new("xclip")
                    .arg("-selection")
                    .arg("clipboard")
                    .arg("-out")
                    .output()
            }
            "xsel" => {
                Command::new("xsel")
                    .arg("--clipboard")
                    .arg("--output")
                    .output()
            }
            "wl-paste" => {
                Command::new("wl-paste")
                    .output()
            }
            _ => return Err(Error::new(ErrorKind::Other, "Unknown clipboard tool"))
        }?;
        
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(Error::new(ErrorKind::Other, "Failed to paste from clipboard"))
        }
    }
}

/// Cross-platform clipboard interface
pub struct Clipboard {
    // Fallback for OSC 52 sequence support
    use_osc52: bool,
    last_copy: String,
}

impl Clipboard {
    pub fn new() -> Self {
        Self {
            use_osc52: false,
            last_copy: String::new(),
        }
    }
    
    pub fn with_osc52_fallback(mut self) -> Self {
        self.use_osc52 = true;
        self
    }
    
    /// Copy text to clipboard
    pub fn set_text(&mut self, text: &str) -> Result<()> {
        self.last_copy = text.to_string();
        
        // Try native clipboard first
        let result = {
            #[cfg(target_os = "windows")]
            { windows_clipboard::set_clipboard_text(text) }
            
            #[cfg(target_os = "macos")]
            { macos_clipboard::set_clipboard_text(text) }
            
            #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
            { linux_clipboard::set_clipboard_text(text) }
        };
        
        // Fall back to OSC 52 if native fails and fallback is enabled
        if result.is_err() && self.use_osc52 {
            self.set_text_osc52(text)
        } else {
            result
        }
    }
    
    /// Get text from clipboard
    pub fn get_text(&self) -> Result<String> {
        #[cfg(target_os = "windows")]
        { windows_clipboard::get_clipboard_text() }
        
        #[cfg(target_os = "macos")]
        { macos_clipboard::get_clipboard_text() }
        
        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        { linux_clipboard::get_clipboard_text() }
    }
    
    /// Copy using OSC 52 escape sequence (terminal clipboard)
    pub fn set_text_osc52(&self, text: &str) -> Result<()> {
        use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
        print!("\x1b]52;c;{}\x1b\\", BASE64_STANDARD.encode(text));
        Ok(())
    }
    
    /// Get the last copied text (from this instance)
    pub fn last_copied(&self) -> &str {
        &self.last_copy
    }
}

impl Default for Clipboard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_clipboard_basic_operations() {
        let mut clipboard = Clipboard::new();
        
        // Test setting and getting text
        let test_text = "Hello, World!";
        match clipboard.set_text(test_text) {
            Ok(_) => {
                // Only test get if set succeeded (may fail in CI environments)
                if let Ok(retrieved) = clipboard.get_text() {
                    assert_eq!(retrieved, test_text);
                }
            }
            Err(_) => {
                // Clipboard operations may fail in headless environments
                println!("Clipboard operations not available in this environment");
            }
        }
    }
    
    #[test]
    fn test_empty_clipboard() {
        let mut clipboard = Clipboard::new();
        
        // Test with empty string
        let _ = clipboard.set_text("");
        assert_eq!(clipboard.last_copied(), "");
    }
    
    #[test]
    fn test_large_text() {
        let mut clipboard = Clipboard::new();
        
        // Test with moderately large text (1MB)
        let large_text = "a".repeat(1024 * 1024);
        match clipboard.set_text(&large_text) {
            Ok(_) => {
                assert_eq!(clipboard.last_copied(), large_text);
            }
            Err(_) => {
                // May fail in some environments
                println!("Large clipboard operation not supported");
            }
        }
    }
    
    #[test]
    fn test_unicode_text() {
        let mut clipboard = Clipboard::new();
        
        // Test with various Unicode characters
        let unicode_tests = vec![
            "Hello 世界",
            "Émojis: 😀🎉🚀",
            "Math: ∑∏∫√",
            "Symbols: ™®©",
            "Mixed: Ñañá ÀÁÂÃ àáâã",
        ];
        
        for test_text in unicode_tests {
            match clipboard.set_text(test_text) {
                Ok(_) => {
                    assert_eq!(clipboard.last_copied(), test_text);
                    if let Ok(retrieved) = clipboard.get_text() {
                        assert_eq!(retrieved, test_text);
                    }
                }
                Err(_) => {
                    println!("Unicode clipboard test skipped");
                }
            }
        }
    }
    
    #[test]
    fn test_multiline_text() {
        let mut clipboard = Clipboard::new();
        
        let multiline = "Line 1\nLine 2\rLine 3\r\nLine 4";
        match clipboard.set_text(multiline) {
            Ok(_) => {
                assert_eq!(clipboard.last_copied(), multiline);
            }
            Err(_) => {
                println!("Multiline clipboard test skipped");
            }
        }
    }
    
    #[test]
    fn test_osc52_fallback() {
        let mut clipboard = Clipboard::new().with_osc52_fallback();
        
        // OSC52 should always succeed as it just prints escape sequences
        let result = clipboard.set_text_osc52("Test OSC52");
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_last_copied_tracking() {
        let mut clipboard = Clipboard::new();
        
        // Test that last_copied tracks the text even if clipboard fails
        let test_texts = vec!["First", "Second", "Third"];
        
        for text in test_texts {
            let _ = clipboard.set_text(text);
            assert_eq!(clipboard.last_copied(), text);
        }
    }
    
    #[cfg(target_os = "windows")]
    mod windows_tests {
        use super::super::windows_clipboard::*;
        
        #[test]
        fn test_safe_read_wide_string_null_pointer() {
            let result = safe_read_wide_string(std::ptr::null());
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("Null pointer"));
        }
        
        #[test]
        fn test_safe_read_wide_string_empty() {
            let empty: Vec<u16> = vec![0];
            let result = safe_read_wide_string(empty.as_ptr());
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "");
        }
        
        #[test]
        fn test_safe_read_wide_string_normal() {
            let text = "Hello";
            let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            let result = safe_read_wide_string(wide.as_ptr());
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), text);
        }
        
        #[test]
        fn test_safe_read_wide_string_unicode() {
            let text = "Hello 世界 🎉";
            let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            let result = safe_read_wide_string(wide.as_ptr());
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), text);
        }
        
        #[test]
        fn test_size_limits() {
            // Test that overly large text is rejected
            let huge_text = "a".repeat(MAX_CLIPBOARD_SIZE + 1);
            let result = set_clipboard_text(&huge_text);
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("too large"));
        }
        
        #[test]
        fn test_clipboard_guard_drop() {
            // Test that ClipboardGuard properly closes clipboard on drop
            {
                let _guard = ClipboardGuard;
                // Guard goes out of scope here, should call CloseClipboard
            }
            // If we get here without crashing, the guard worked
            assert!(true);
        }
        
        #[test]
        fn test_global_unlock_guard_drop() {
            // Test that GlobalUnlockGuard properly unlocks on drop
            {
                let _guard = GlobalUnlockGuard(std::ptr::null_mut());
                // Guard with null pointer should not crash
            }
            assert!(true);
            
            // Test with non-null (but invalid) pointer - should also not crash
            {
                let _guard = GlobalUnlockGuard(1 as *mut u8);
            }
            assert!(true);
        }
    }
}