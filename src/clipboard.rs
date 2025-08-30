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
    use std::process::{Command, Stdio};
    use std::time::Duration;
    use std::sync::OnceLock;
    use std::env;
    
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub(super) enum SessionType {
        Wayland,
        X11,
        Unknown,
    }
    
    #[derive(Debug, Clone, Copy)]
    pub(super) enum ClipboardTool {
        WlClipboard,  // wl-copy/wl-paste for Wayland
        Xclip,        // xclip for X11
        Xsel,         // xsel for X11
    }
    
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum Selection {
        Clipboard,
        Primary,
    }
    
    impl Default for Selection {
        fn default() -> Self {
            Selection::Clipboard
        }
    }
    
    // Cache for detected clipboard tool
    static CLIPBOARD_TOOL: OnceLock<Option<ClipboardTool>> = OnceLock::new();
    static SESSION_TYPE: OnceLock<SessionType> = OnceLock::new();
    
    pub(super) fn detect_session_type() -> SessionType {
        *SESSION_TYPE.get_or_init(|| {
            // Check for Wayland session
            if env::var("WAYLAND_DISPLAY").is_ok() {
                return SessionType::Wayland;
            }
            
            // Check XDG_SESSION_TYPE
            if let Ok(session_type) = env::var("XDG_SESSION_TYPE") {
                match session_type.to_lowercase().as_str() {
                    "wayland" => return SessionType::Wayland,
                    "x11" => return SessionType::X11,
                    _ => {}
                }
            }
            
            // Check for X11 display
            if env::var("DISPLAY").is_ok() {
                return SessionType::X11;
            }
            
            SessionType::Unknown
        })
    }
    
    fn check_tool_availability(tool: &str) -> bool {
        Command::new("which")
            .arg(tool)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
    
    pub(super) fn detect_clipboard_tool() -> Option<ClipboardTool> {
        *CLIPBOARD_TOOL.get_or_init(|| {
            let session = detect_session_type();
            
            // Prefer tools based on session type
            match session {
                SessionType::Wayland => {
                    // For Wayland, prefer wl-clipboard
                    if check_tool_availability("wl-copy") && check_tool_availability("wl-paste") {
                        return Some(ClipboardTool::WlClipboard);
                    }
                    // Fall back to X11 tools (might work through XWayland)
                    if check_tool_availability("xclip") {
                        return Some(ClipboardTool::Xclip);
                    }
                    if check_tool_availability("xsel") {
                        return Some(ClipboardTool::Xsel);
                    }
                }
                SessionType::X11 | SessionType::Unknown => {
                    // For X11 or unknown, prefer X11 tools
                    if check_tool_availability("xclip") {
                        return Some(ClipboardTool::Xclip);
                    }
                    if check_tool_availability("xsel") {
                        return Some(ClipboardTool::Xsel);
                    }
                    // Try Wayland tools as last resort
                    if check_tool_availability("wl-copy") && check_tool_availability("wl-paste") {
                        return Some(ClipboardTool::WlClipboard);
                    }
                }
            }
            
            None
        })
    }
    
    fn execute_with_timeout(mut cmd: Command, input: Option<&[u8]>, timeout: Duration) -> Result<Vec<u8>> {
        use std::io::Write;
        use std::thread;
        use std::sync::mpsc;
        
        if let Some(_data) = input {
            cmd.stdin(Stdio::piped());
        }
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped());
        
        let mut child = cmd.spawn()
            .map_err(|e| Error::new(ErrorKind::Other, format!("Failed to spawn process: {}", e)))?;
        
        // Write input if provided
        if let Some(data) = input {
            if let Some(stdin) = child.stdin.as_mut() {
                stdin.write_all(data)
                    .map_err(|e| Error::new(ErrorKind::Other, format!("Failed to write to stdin: {}", e)))?;
            }
        }
        
        // Set up timeout
        let (tx, rx) = mpsc::channel();
        let _child_id = child.id();
        
        thread::spawn(move || {
            thread::sleep(timeout);
            tx.send(()).ok();
        });
        
        // Wait for process or timeout
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    if status.success() {
                        let output = child.wait_with_output()
                            .map_err(|e| Error::new(ErrorKind::Other, format!("Failed to get output: {}", e)))?;
                        return Ok(output.stdout);
                    } else {
                        return Err(Error::new(ErrorKind::Other, "Command failed"));
                    }
                }
                Ok(None) => {
                    // Still running, check for timeout
                    if rx.try_recv().is_ok() {
                        // Timeout occurred, kill the process
                        child.kill().ok();
                        return Err(Error::new(ErrorKind::TimedOut, "Clipboard operation timed out"));
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Err(e) => {
                    return Err(Error::new(ErrorKind::Other, format!("Failed to wait for process: {}", e)));
                }
            }
        }
    }
    
    pub fn set_clipboard_text_with_selection(text: &str, selection: Selection) -> Result<()> {
        let tool = detect_clipboard_tool()
            .ok_or_else(|| Error::new(ErrorKind::NotFound, 
                "No clipboard tool found. Install xclip, xsel, or wl-clipboard"))?;
        
        let timeout = Duration::from_secs(2);
        
        match tool {
            ClipboardTool::Xclip => {
                let mut cmd = Command::new("xclip");
                cmd.arg("-selection");
                match selection {
                    Selection::Clipboard => cmd.arg("clipboard"),
                    Selection::Primary => cmd.arg("primary"),
                };
                execute_with_timeout(cmd, Some(text.as_bytes()), timeout)?;
            }
            ClipboardTool::Xsel => {
                let mut cmd = Command::new("xsel");
                match selection {
                    Selection::Clipboard => cmd.arg("--clipboard"),
                    Selection::Primary => cmd.arg("--primary"),
                };
                cmd.arg("--input");
                execute_with_timeout(cmd, Some(text.as_bytes()), timeout)?;
            }
            ClipboardTool::WlClipboard => {
                let mut cmd = Command::new("wl-copy");
                if selection == Selection::Primary {
                    cmd.arg("--primary");
                }
                execute_with_timeout(cmd, Some(text.as_bytes()), timeout)?;
            }
        }
        
        Ok(())
    }
    
    pub fn get_clipboard_text_with_selection(selection: Selection) -> Result<String> {
        let tool = detect_clipboard_tool()
            .ok_or_else(|| Error::new(ErrorKind::NotFound, 
                "No clipboard tool found. Install xclip, xsel, or wl-clipboard"))?;
        
        let timeout = Duration::from_secs(2);
        
        let output = match tool {
            ClipboardTool::Xclip => {
                let mut cmd = Command::new("xclip");
                cmd.arg("-selection");
                match selection {
                    Selection::Clipboard => cmd.arg("clipboard"),
                    Selection::Primary => cmd.arg("primary"),
                };
                cmd.arg("-out");
                execute_with_timeout(cmd, None, timeout)?
            }
            ClipboardTool::Xsel => {
                let mut cmd = Command::new("xsel");
                match selection {
                    Selection::Clipboard => cmd.arg("--clipboard"),
                    Selection::Primary => cmd.arg("--primary"),
                };
                cmd.arg("--output");
                execute_with_timeout(cmd, None, timeout)?
            }
            ClipboardTool::WlClipboard => {
                let mut cmd = Command::new("wl-paste");
                if selection == Selection::Primary {
                    cmd.arg("--primary");
                }
                cmd.arg("--no-newline");
                execute_with_timeout(cmd, None, timeout)?
            }
        };
        
        Ok(String::from_utf8_lossy(&output).to_string())
    }
    
    // Public API maintaining backward compatibility
    pub fn set_clipboard_text(text: &str) -> Result<()> {
        set_clipboard_text_with_selection(text, Selection::default())
    }
    
    pub fn get_clipboard_text() -> Result<String> {
        get_clipboard_text_with_selection(Selection::default())
    }
}

/// Cross-platform clipboard interface
pub struct Clipboard {
    // Fallback for OSC 52 sequence support
    use_osc52: bool,
    last_copy: String,
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    selection: linux_clipboard::Selection,
}

impl Clipboard {
    pub fn new() -> Self {
        Self {
            use_osc52: false,
            last_copy: String::new(),
            #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
            selection: linux_clipboard::Selection::default(),
        }
    }
    
    pub fn with_osc52_fallback(mut self) -> Self {
        self.use_osc52 = true;
        self
    }
    
    /// Set the selection type (Linux only, ignored on other platforms)
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    pub fn with_selection(mut self, selection: linux_clipboard::Selection) -> Self {
        self.selection = selection;
        self
    }
    
    /// Copy text to clipboard with automatic fallback chain
    pub fn set_text(&mut self, text: &str) -> Result<()> {
        self.last_copy = text.to_string();
        
        // Try native clipboard first
        let result = {
            #[cfg(target_os = "windows")]
            { windows_clipboard::set_clipboard_text(text) }
            
            #[cfg(target_os = "macos")]
            { macos_clipboard::set_clipboard_text(text) }
            
            #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
            { linux_clipboard::set_clipboard_text_with_selection(text, self.selection) }
        };
        
        // Fall back to OSC 52 if native fails and fallback is enabled
        match result {
            Ok(_) => Ok(()),
            Err(e) if self.use_osc52 => {
                // Log the native error for debugging
                eprintln!("Native clipboard failed: {}. Falling back to OSC 52.", e);
                self.set_text_osc52(text)
            }
            Err(e) => Err(e)
        }
    }
    
    /// Get text from clipboard
    pub fn get_text(&self) -> Result<String> {
        let result = {
            #[cfg(target_os = "windows")]
            { windows_clipboard::get_clipboard_text() }
            
            #[cfg(target_os = "macos")]
            { macos_clipboard::get_clipboard_text() }
            
            #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
            { linux_clipboard::get_clipboard_text_with_selection(self.selection) }
        };
        
        // If native clipboard fails and we have a last_copy, return that as fallback
        match result {
            Ok(text) => Ok(text),
            Err(e) if self.use_osc52 && !self.last_copy.is_empty() => {
                eprintln!("Native clipboard read failed: {}. Returning last copied text.", e);
                Ok(self.last_copy.clone())
            }
            Err(e) => Err(e)
        }
    }
    
    /// Copy using OSC 52 escape sequence (terminal clipboard)
    pub fn set_text_osc52(&self, text: &str) -> Result<()> {
        use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
        // Use both OSC 52 formats for better compatibility
        print!("\x1b]52;c;{}\x1b\\", BASE64_STANDARD.encode(text));
        // Also send the version with BEL terminator for older terminals
        print!("\x1b]52;c;{}\x07", BASE64_STANDARD.encode(text));
        Ok(())
    }
    
    /// Get the last copied text (from this instance)
    pub fn last_copied(&self) -> &str {
        &self.last_copy
    }
    
    /// Get information about clipboard support (for debugging)
    pub fn get_clipboard_info(&self) -> String {
        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        {
            let session = linux_clipboard::detect_session_type();
            let tool = linux_clipboard::detect_clipboard_tool();
            format!("Linux session: {:?}, Tool: {:?}, OSC52 fallback: {}", 
                    session, tool, self.use_osc52)
        }
        #[cfg(target_os = "windows")]
        {
            format!("Windows native clipboard, OSC52 fallback: {}", self.use_osc52)
        }
        #[cfg(target_os = "macos")]
        {
            format!("macOS pbcopy/pbpaste, OSC52 fallback: {}", self.use_osc52)
        }
    }
}

impl Default for Clipboard {
    fn default() -> Self {
        Self::new()
    }
}

// Re-export Selection for Linux users
#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
pub use linux_clipboard::Selection;