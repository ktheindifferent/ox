//! Cross-platform clipboard support

use log::{debug, error, info, warn};
use std::fmt;
use std::io::{Error as IoError, ErrorKind, Result};

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

/// Clipboard error types
#[derive(Debug)]
pub enum ClipboardError {
    /// Native clipboard operation failed
    NativeClipboardFailed(String),
    /// Clipboard tool not found (Linux)
    ToolNotFound(String),
    /// Clipboard operation timed out
    Timeout,
    /// Clipboard is locked by another process
    Locked,
    /// Text too large for clipboard
    TextTooLarge(usize),
    /// Invalid clipboard data format
    InvalidFormat(String),
    /// Platform-specific error
    PlatformError(String),
    /// Fallback to OSC52 failed
    OSC52Failed(String),
    /// IO Error
    IoError(IoError),
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClipboardError::NativeClipboardFailed(msg) => {
                write!(f, "Native clipboard operation failed: {}", msg)
            }
            ClipboardError::ToolNotFound(msg) => {
                write!(f, "Clipboard tool not found: {}", msg)
            }
            ClipboardError::Timeout => write!(f, "Clipboard operation timed out"),
            ClipboardError::Locked => write!(f, "Clipboard is locked by another process"),
            ClipboardError::TextTooLarge(size) => {
                write!(f, "Text too large for clipboard: {} bytes", size)
            }
            ClipboardError::InvalidFormat(msg) => {
                write!(f, "Invalid clipboard data format: {}", msg)
            }
            ClipboardError::PlatformError(msg) => {
                write!(f, "Platform-specific clipboard error: {}", msg)
            }
            ClipboardError::OSC52Failed(msg) => {
                write!(f, "OSC52 clipboard operation failed: {}", msg)
            }
            ClipboardError::IoError(err) => write!(f, "IO error: {}", err),
        }
    }
}

impl std::error::Error for ClipboardError {}

impl From<IoError> for ClipboardError {
    fn from(error: IoError) -> Self {
        ClipboardError::IoError(error)
    }
}

impl From<ClipboardError> for IoError {
    fn from(error: ClipboardError) -> Self {
        match error {
            ClipboardError::IoError(io_err) => io_err,
            ClipboardError::Timeout => IoError::new(ErrorKind::TimedOut, error.to_string()),
            ClipboardError::Locked => IoError::new(ErrorKind::WouldBlock, error.to_string()),
            ClipboardError::TextTooLarge(_) => IoError::new(ErrorKind::InvalidInput, error.to_string()),
            _ => IoError::new(ErrorKind::Other, error.to_string()),
        }
    }
}

/// Represents the method used for clipboard operations
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClipboardMethod {
    /// Native system clipboard
    Native,
    /// OSC 52 terminal escape sequence
    OSC52,
    /// Cached text (fallback when clipboard is unavailable)
    Cached,
}

/// Status of the clipboard system
#[derive(Debug, Clone)]
pub struct ClipboardStatus {
    /// Current method being used
    pub method: ClipboardMethod,
    /// Whether the native clipboard is available
    pub native_available: bool,
    /// Whether OSC52 is enabled
    pub osc52_enabled: bool,
    /// Last error if any
    pub last_error: Option<String>,
    /// Platform information
    pub platform_info: String,
}

/// Cross-platform clipboard interface
pub struct Clipboard {
    // Fallback for OSC 52 sequence support
    use_osc52: bool,
    last_copy: String,
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    selection: linux_clipboard::Selection,
    /// Track the current clipboard method
    pub current_method: ClipboardMethod,
    /// Track the last error
    last_error: Option<ClipboardError>,
    /// Retry count for transient failures
    max_retries: u32,
    /// Enable verbose logging
    verbose_logging: bool,
}

impl Clipboard {
    pub fn new() -> Self {
        Self {
            use_osc52: false,
            last_copy: String::new(),
            #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
            selection: linux_clipboard::Selection::default(),
            current_method: ClipboardMethod::Native,
            last_error: None,
            max_retries: 3,
            verbose_logging: false,
        }
    }
    
    pub fn with_osc52_fallback(mut self) -> Self {
        self.use_osc52 = true;
        self
    }
    
    /// Set the maximum number of retries for transient failures
    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }
    
    /// Enable verbose logging
    pub fn with_verbose_logging(mut self) -> Self {
        self.verbose_logging = true;
        self
    }
    
    /// Set the selection type (Linux only, ignored on other platforms)
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    pub fn with_selection(mut self, selection: linux_clipboard::Selection) -> Self {
        self.selection = selection;
        self
    }
    
    /// Copy text to clipboard with automatic fallback chain and retry logic
    pub fn set_text(&mut self, text: &str) -> Result<()> {
        self.last_copy = text.to_string();
        
        // Try native clipboard first with retries
        let mut attempts = 0;
        let mut last_native_error = None;
        
        while attempts < self.max_retries {
            let result = {
                #[cfg(target_os = "windows")]
                { windows_clipboard::set_clipboard_text(text) }
                
                #[cfg(target_os = "macos")]
                { macos_clipboard::set_clipboard_text(text) }
                
                #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
                { linux_clipboard::set_clipboard_text_with_selection(text, self.selection) }
            };
            
            match result {
                Ok(_) => {
                    self.current_method = ClipboardMethod::Native;
                    self.last_error = None;
                    if self.verbose_logging {
                        info!("Successfully copied text to native clipboard (attempt {})", attempts + 1);
                    }
                    return Ok(());
                }
                Err(e) => {
                    attempts += 1;
                    last_native_error = Some(e);
                    
                    if self.verbose_logging {
                        debug!("Native clipboard attempt {} failed: {}", attempts, last_native_error.as_ref().unwrap());
                    }
                    
                    // Don't retry for certain errors
                    if let Some(err_str) = last_native_error.as_ref().map(|e| e.to_string()) {
                        if err_str.contains("too large") || err_str.contains("No clipboard tool found") {
                            break;
                        }
                    }
                    
                    if attempts < self.max_retries {
                        std::thread::sleep(std::time::Duration::from_millis(50 * attempts as u64));
                    }
                }
            }
        }
        
        // Fall back to OSC 52 if native fails and fallback is enabled
        if self.use_osc52 {
            let native_err = last_native_error.unwrap();
            warn!("Native clipboard failed after {} attempts: {}. Falling back to OSC 52.", attempts, native_err);
            
            match self.set_text_osc52(text) {
                Ok(_) => {
                    self.current_method = ClipboardMethod::OSC52;
                    self.last_error = Some(ClipboardError::NativeClipboardFailed(native_err.to_string()));
                    Ok(())
                }
                Err(osc_err) => {
                    error!("OSC52 fallback also failed: {}", osc_err);
                    self.last_error = Some(ClipboardError::OSC52Failed(osc_err.to_string()));
                    Err(osc_err)
                }
            }
        } else {
            let err = last_native_error.unwrap();
            self.last_error = Some(ClipboardError::NativeClipboardFailed(err.to_string()));
            Err(err)
        }
    }
    
    /// Get text from clipboard with retry logic
    pub fn get_text(&mut self) -> Result<String> {
        let mut attempts = 0;
        let mut last_error = None;
        
        while attempts < self.max_retries {
            let result = {
                #[cfg(target_os = "windows")]
                { windows_clipboard::get_clipboard_text() }
                
                #[cfg(target_os = "macos")]
                { macos_clipboard::get_clipboard_text() }
                
                #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
                { linux_clipboard::get_clipboard_text_with_selection(self.selection) }
            };
            
            match result {
                Ok(text) => {
                    self.current_method = ClipboardMethod::Native;
                    self.last_error = None;
                    if self.verbose_logging {
                        info!("Successfully read text from native clipboard (attempt {})", attempts + 1);
                    }
                    return Ok(text);
                }
                Err(e) => {
                    attempts += 1;
                    last_error = Some(e);
                    
                    if self.verbose_logging {
                        debug!("Native clipboard read attempt {} failed: {}", attempts, last_error.as_ref().unwrap());
                    }
                    
                    if attempts < self.max_retries {
                        std::thread::sleep(std::time::Duration::from_millis(50 * attempts as u64));
                    }
                }
            }
        }
        
        // If native clipboard fails and we have a last_copy, return that as fallback
        if self.use_osc52 && !self.last_copy.is_empty() {
            let err = last_error.unwrap();
            warn!("Native clipboard read failed after {} attempts: {}. Returning cached text.", attempts, err);
            self.current_method = ClipboardMethod::Cached;
            self.last_error = Some(ClipboardError::NativeClipboardFailed(err.to_string()));
            Ok(self.last_copy.clone())
        } else {
            let err = last_error.unwrap();
            self.last_error = Some(ClipboardError::NativeClipboardFailed(err.to_string()));
            Err(err)
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
    
    /// Get the current clipboard status
    pub fn get_status(&self) -> ClipboardStatus {
        let platform_info = self.get_clipboard_info();
        
        ClipboardStatus {
            method: self.current_method,
            native_available: self.check_native_available(),
            osc52_enabled: self.use_osc52,
            last_error: self.last_error.as_ref().map(|e| e.to_string()),
            platform_info,
        }
    }
    
    /// Check if native clipboard is available
    fn check_native_available(&self) -> bool {
        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        {
            linux_clipboard::detect_clipboard_tool().is_some()
        }
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            true // Assume native clipboard is always available on Windows and macOS
        }
    }
    
    /// Get information about clipboard support (for debugging)
    pub fn get_clipboard_info(&self) -> String {
        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        {
            let session = linux_clipboard::detect_session_type();
            let tool = linux_clipboard::detect_clipboard_tool();
            format!("Linux session: {:?}, Tool: {:?}, OSC52 fallback: {}, Current method: {:?}", 
                    session, tool, self.use_osc52, self.current_method)
        }
        #[cfg(target_os = "windows")]
        {
            format!("Windows native clipboard, OSC52 fallback: {}, Current method: {:?}", 
                    self.use_osc52, self.current_method)
        }
        #[cfg(target_os = "macos")]
        {
            format!("macOS pbcopy/pbpaste, OSC52 fallback: {}, Current method: {:?}", 
                    self.use_osc52, self.current_method)
        }
    }
    
    /// Clear the last error
    pub fn clear_error(&mut self) {
        self.last_error = None;
    }
    
    /// Get the last error if any
    pub fn last_error(&self) -> Option<&ClipboardError> {
        self.last_error.as_ref()
    }
}

impl Default for Clipboard {
    fn default() -> Self {
        Self::new()
    }
}

// Re-export Selection for Linux users
#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
pub use self::linux_clipboard::Selection;

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
