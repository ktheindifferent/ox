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
    
    pub fn set_clipboard_text(text: &str) -> Result<()> {
        unsafe {
            if OpenClipboard(ptr::null()) == 0 {
                return Err(Error::new(ErrorKind::Other, "Failed to open clipboard"));
            }
            
            if EmptyClipboard() == 0 {
                CloseClipboard();
                return Err(Error::new(ErrorKind::Other, "Failed to empty clipboard"));
            }
            
            let wide: Vec<u16> = OsStr::new(text)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            
            let size = wide.len() * 2;
            let handle = GlobalAlloc(GMEM_MOVEABLE, size);
            
            if handle.is_null() {
                CloseClipboard();
                return Err(Error::new(ErrorKind::Other, "Failed to allocate memory"));
            }
            
            let locked = GlobalLock(handle);
            if locked.is_null() {
                CloseClipboard();
                return Err(Error::new(ErrorKind::Other, "Failed to lock memory"));
            }
            
            std::ptr::copy_nonoverlapping(
                wide.as_ptr() as *const u8,
                locked,
                size
            );
            
            GlobalUnlock(handle);
            
            if SetClipboardData(CF_UNICODETEXT, handle).is_null() {
                CloseClipboard();
                return Err(Error::new(ErrorKind::Other, "Failed to set clipboard data"));
            }
            
            CloseClipboard();
            Ok(())
        }
    }
    
    pub fn get_clipboard_text() -> Result<String> {
        unsafe {
            if OpenClipboard(ptr::null()) == 0 {
                return Err(Error::new(ErrorKind::Other, "Failed to open clipboard"));
            }
            
            let handle = GetClipboardData(CF_UNICODETEXT);
            if handle.is_null() {
                CloseClipboard();
                return Err(Error::new(ErrorKind::Other, "No text data in clipboard"));
            }
            
            let locked = GlobalLock(handle as *mut u8);
            if locked.is_null() {
                CloseClipboard();
                return Err(Error::new(ErrorKind::Other, "Failed to lock clipboard data"));
            }
            
            let mut len = 0;
            let mut ptr = locked as *const u16;
            while *ptr != 0 {
                len += 1;
                ptr = ptr.offset(1);
            }
            
            let slice = std::slice::from_raw_parts(locked as *const u16, len);
            let text = String::from_utf16_lossy(slice);
            
            GlobalUnlock(handle as *mut u8);
            CloseClipboard();
            
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