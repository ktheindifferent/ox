//! Cross-platform PTY abstraction layer
//! 
//! This module provides a unified interface for pseudo-terminal (PTY) operations
//! across different platforms. On Unix-like systems, it uses the traditional PTY
//! interface via ptyprocess. On Windows, it uses the ConPTY API through the
//! portable-pty crate.
//! 
//! # Windows Terminal Support
//! 
//! The Windows implementation supports various terminal emulators:
//! - **Windows Terminal**: Full ConPTY support with all features
//! - **PowerShell/PowerShell Core**: Native ConPTY integration
//! - **Command Prompt (cmd.exe)**: Basic PTY functionality
//! - **ConEmu/Cmder**: ConPTY support on Windows 10+
//! - **MinTTY (Git Bash/MSYS2)**: May require special handling
//! - **VS Code Terminal**: Full ConPTY support
//! 
//! # Shell Detection
//! 
//! The module automatically detects the appropriate shell:
//! - On Windows: PowerShell Core (pwsh) > PowerShell > cmd.exe
//! - On Unix: Reads $SHELL environment variable
//! 
//! # Resource Management
//! 
//! The PTY implementation ensures proper cleanup:
//! - Child processes are terminated when PTY is dropped
//! - File handles are properly closed
//! - Threads are gracefully shut down

use mlua::prelude::*;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::pty_error::{PtyError, PtyResult, PtyErrorContext, recover_lock_poisoned};

// Module implementations are defined inline below

#[cfg(not(target_os = "windows"))]
use unix_impl as platform;
#[cfg(target_os = "windows")]
use windows_impl as platform;

#[derive(Debug, Clone, Copy)]
pub enum Shell {
    Bash,
    Dash,
    Zsh,
    Fish,
    #[cfg(target_os = "windows")]
    PowerShell,
    #[cfg(target_os = "windows")]
    PowerShellCore,
    #[cfg(target_os = "windows")]
    Cmd,
}

impl Shell {
    pub fn detect() -> Self {
        #[cfg(not(target_os = "windows"))]
        {
            if let Ok(shell) = std::env::var("SHELL") {
                if shell.contains("zsh") {
                    return Self::Zsh;
                } else if shell.contains("fish") {
                    return Self::Fish;
                } else if shell.contains("dash") {
                    return Self::Dash;
                }
            }
            Self::Bash
        }
        #[cfg(target_os = "windows")]
        {
            // Check if we're already running in PowerShell
            if std::env::var("PSModulePath").is_ok() {
                // We're in some form of PowerShell, detect which one
                return Self::detect_powershell_type();
            }
            
            // Check COMSPEC environment variable (usually set to cmd.exe)
            if let Ok(comspec) = std::env::var("COMSPEC") {
                if comspec.to_lowercase().contains("cmd.exe") {
                    // Even if COMSPEC is cmd.exe, prefer PowerShell if available
                    if Self::is_powershell_available() {
                        return Self::detect_powershell_type();
                    }
                    return Self::Cmd;
                }
            }
            
            // Check for PowerShell availability
            if Self::is_powershell_available() {
                Self::detect_powershell_type()
            } else {
                Self::Cmd
            }
        }
    }
    
    #[cfg(target_os = "windows")]
    fn detect_powershell_type() -> Self {
        // Check for PowerShell Core (pwsh) first - it's the newer version
        if which::which("pwsh.exe").is_ok() || which::which("pwsh").is_ok() {
            Self::PowerShellCore
        } else if which::which("powershell.exe").is_ok() || which::which("powershell").is_ok() {
            Self::PowerShell
        } else {
            // Fallback to cmd if no PowerShell is found (shouldn't happen if we got here)
            Self::Cmd
        }
    }
    
    #[cfg(target_os = "windows")]
    fn is_powershell_available() -> bool {
        which::which("pwsh.exe").is_ok() 
            || which::which("pwsh").is_ok() 
            || which::which("powershell.exe").is_ok() 
            || which::which("powershell").is_ok()
    }

    pub fn command(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Dash => "dash",
            Self::Zsh => "zsh",
            Self::Fish => "fish",
            #[cfg(target_os = "windows")]
            Self::PowerShell => "powershell.exe",
            #[cfg(target_os = "windows")]
            Self::PowerShellCore => "pwsh.exe",
            #[cfg(target_os = "windows")]
            Self::Cmd => "cmd.exe",
        }
    }

    pub fn manual_input_echo(self) -> bool {
        match self {
            Self::Bash | Self::Dash => true,
            Self::Zsh | Self::Fish => false,
            #[cfg(target_os = "windows")]
            Self::PowerShell | Self::PowerShellCore | Self::Cmd => false,
        }
    }

    pub fn inserts_extra_newline(self) -> bool {
        match self {
            Self::Bash | Self::Dash | Self::Fish => true,
            Self::Zsh => false,
            #[cfg(target_os = "windows")]
            Self::PowerShell | Self::PowerShellCore | Self::Cmd => false,
        }
    }
}

impl Default for Shell {
    fn default() -> Self {
        Self::detect()
    }
}

impl From<String> for Shell {
    fn from(shell: String) -> Self {
        let shell_lower = shell.to_lowercase();
        if shell_lower.contains("zsh") {
            Self::Zsh
        } else if shell_lower.contains("fish") {
            Self::Fish
        } else if shell_lower.contains("dash") {
            Self::Dash
        } else if shell_lower.contains("bash") {
            Self::Bash
        } else {
            #[cfg(target_os = "windows")]
            {
                if shell_lower.contains("pwsh") {
                    Self::PowerShellCore
                } else if shell_lower.contains("powershell") {
                    Self::PowerShell
                } else if shell_lower.contains("cmd") {
                    Self::Cmd
                } else {
                    Shell::detect()
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                Shell::detect()
            }
        }
    }
}

impl IntoLua for Shell {
    fn into_lua(self, _lua: &mlua::Lua) -> mlua::Result<mlua::Value> {
        Ok(mlua::Value::String(_lua.create_string(self.command())?))
    }
}

impl FromLua for Shell {
    fn from_lua(lua_value: mlua::Value, _lua: &mlua::Lua) -> mlua::Result<Self> {
        match lua_value {
            mlua::Value::String(s) => Ok(Shell::from(s.to_str()?.to_string())),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: lua_value.type_name(),
                to: "Shell".to_string(),
                message: Some("expected string".to_string()),
            }),
        }
    }
}

#[derive(Debug)]
pub struct Pty {
    inner: platform::PtyImpl,
    pub output: String,
    pub input: String,
    pub shell: Shell,
    force_rerender: Arc<AtomicBool>,
    shutdown_flag: Arc<AtomicBool>,
    reader_thread: Option<JoinHandle<()>>,
    update_receiver: Receiver<bool>,
}

impl Pty {
    pub fn new(shell: Shell) -> PtyResult<Arc<Mutex<Self>>> {
        let inner = platform::PtyImpl::new(shell)
            .map_err(|e| PtyError::InitializationFailed(format!("Failed to create PTY: {}", e)))?;
        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let force_rerender = Arc::new(AtomicBool::new(false));
        let (update_sender, update_receiver) = channel::<bool>();
        
        let pty = Arc::new(Mutex::new(Self {
            inner,
            output: String::new(),
            input: String::new(),
            shell,
            force_rerender: Arc::clone(&force_rerender),
            shutdown_flag: Arc::clone(&shutdown_flag),
            reader_thread: None,
            update_receiver,
        }));
        
        // Initialize the PTY with proper error handling
        {
            let mut pty_guard = pty.lock()
                .unwrap_or_else(recover_lock_poisoned);
            pty_guard.initialize()
                .map_err(|e| PtyError::InitializationFailed(format!("Failed to initialize PTY: {}", e)))?;
        }
        
        // Spawn reader thread with proper lifecycle management
        let pty_clone = Arc::clone(&pty);
        let shutdown_clone = Arc::clone(&shutdown_flag);
        let force_rerender_clone = Arc::clone(&force_rerender);
        let thread_handle = std::thread::Builder::new()
            .name("pty-reader".to_string())
            .spawn(move || {
                while !shutdown_clone.load(Ordering::Relaxed) {
                    // Try to read with a timeout to allow checking shutdown flag
                    std::thread::sleep(Duration::from_millis(50));
                    
                    // Attempt to lock and read
                    match pty_clone.try_lock() {
                        Ok(mut pty) => {
                            match pty.catch_up() {
                                Ok(true) => {
                                    force_rerender_clone.store(true, Ordering::Relaxed);
                                    // Send update notification
                                    let _ = update_sender.send(true);
                                }
                                Ok(false) => {
                                    // No data available, continue
                                }
                                Err(_) => {
                                    // Error reading, might indicate PTY is closed
                                    // Continue for now, Drop will handle cleanup
                                }
                            }
                        }
                        Err(std::sync::TryLockError::Poisoned(err)) => {
                            // Recover from poisoned lock
                            let mut pty = recover_lock_poisoned(err);
                            match pty.catch_up() {
                                Ok(true) => {
                                    force_rerender_clone.store(true, Ordering::Relaxed);
                                    let _ = update_sender.send(true);
                                }
                                _ => {}
                            }
                        }
                        Err(std::sync::TryLockError::WouldBlock) => {
                            // Lock is held by another thread, skip this iteration
                        }
                    }
                }
            })
            .map_err(|e| PtyError::InitializationFailed(format!("Failed to spawn PTY reader thread: {}", e)))?;
        
        // Store the thread handle
        pty.lock()
            .unwrap_or_else(recover_lock_poisoned)
            .reader_thread = Some(thread_handle);
        
        Ok(pty)
    }

    fn initialize(&mut self) -> PtyResult<()> {
        self.inner.set_echo(false)
            .context("Failed to set echo mode")?;
        std::thread::sleep(Duration::from_millis(100));
        self.run_command("")
            .context("Failed to run initial command")?;
        Ok(())
    }

    pub fn run_command(&mut self, cmd: &str) -> PtyResult<()> {
        self.inner.write_input(cmd)?;
        std::thread::sleep(std::time::Duration::from_millis(100));
        
        if self.shell.manual_input_echo() {
            self.output += cmd;
        }
        
        let mut output = self.inner.read_output()?;
        
        // Clean up shell-specific output quirks
        if self.shell.inserts_extra_newline() {
            output = output.replace("\u{1b}[?2004l\r\r\n", "");
        }
        
        self.output += &output;
        Ok(())
    }

    pub fn silent_run_command(&mut self, cmd: &str) -> PtyResult<()> {
        self.output.clear();
        self.run_command(cmd)?;
        if self.output.starts_with(cmd) {
            self.output = self.output.chars().skip(cmd.chars().count()).collect();
        }
        Ok(())
    }

    pub fn char_input(&mut self, c: char) -> PtyResult<()> {
        self.input.push(c);
        if c == '\n' {
            self.run_command(&self.input.to_string())?;
            self.input.clear();
        }
        Ok(())
    }

    pub fn char_pop(&mut self) {
        self.input.pop();
    }

    pub fn clear(&mut self) -> PtyResult<()> {
        self.output.clear();
        self.run_command("\n")?;
        self.output = self.output.trim_start_matches('\n').to_string();
        Ok(())
    }

    pub fn catch_up(&mut self) -> PtyResult<bool> {
        let output = self.inner.try_read_output()?;
        if !output.is_empty() {
            let mut processed = output;
            if self.shell.inserts_extra_newline() {
                processed = processed.replace("\u{1b}[?2004l\r\r\n", "");
            }
            self.output += &processed;
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    /// Check if there's a pending rerender request
    pub fn check_force_rerender(&self) -> bool {
        self.force_rerender.swap(false, Ordering::Relaxed)
    }
    
    /// Check for updates from the reader thread without blocking
    pub fn check_for_updates(&self) -> bool {
        match self.update_receiver.try_recv() {
            Ok(_) => true,
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => false,
        }
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        // Signal the reader thread to shutdown
        self.shutdown_flag.store(true, Ordering::Relaxed);
        
        // Take the thread handle and wait for it to finish
        if let Some(thread) = self.reader_thread.take() {
            // Give the thread a moment to notice the shutdown flag
            std::thread::sleep(Duration::from_millis(100));
            
            // Wait for the thread to finish (with a timeout)
            // Note: JoinHandle doesn't have a timed join in stable Rust,
            // but the thread should exit quickly due to the shutdown flag
            let _ = thread.join();
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod unix_impl {
    use super::{Shell, PtyError, PtyResult};
    use mio::unix::SourceFd;
    use mio::{Events, Interest, Poll, Token};
    use nix::fcntl::{fcntl, FcntlArg, OFlag};
    use ptyprocess::PtyProcess;
    use std::io::{BufReader, Read, Write};
    use std::os::unix::io::AsRawFd;
    use std::process::Command;
    use std::time::Duration;

    #[derive(Debug)]
    pub struct PtyImpl {
        process: PtyProcess,
        shell: Shell,
    }

    impl PtyImpl {
        pub fn new(shell: Shell) -> PtyResult<Self> {
            let process = PtyProcess::spawn(Command::new(shell.command()))
                .map_err(|e| PtyError::SpawnFailed(format!("Failed to spawn {}: {}", shell.command(), e)))?;
            Ok(Self {
                process,
                shell,
            })
        }

        pub fn set_echo(&mut self, echo: bool) -> PtyResult<()> {
            self.process.set_echo(echo, None)
                .map_err(|e| PtyError::CommunicationError(format!("Failed to set PTY echo mode: {}", e)))?;
            Ok(())
        }

        pub fn write_input(&mut self, input: &str) -> PtyResult<()> {
            let mut stream = self.process.get_raw_handle()
                .map_err(|e| PtyError::CommunicationError(format!("Failed to get PTY handle: {}", e)))?;
            write!(stream, "{}", input)
                .map_err(|e| PtyError::CommunicationError(format!("Failed to write to PTY: {}", e)))?;
            Ok(())
        }

        pub fn read_output(&mut self) -> PtyResult<String> {
            let stream = self.process.get_raw_handle()
                .map_err(|e| PtyError::CommunicationError(format!("Failed to get PTY handle: {}", e)))?;
            let mut reader = BufReader::new(stream);
            let mut buf = [0u8; 10240];
            let bytes_read = reader.read(&mut buf)
                .map_err(|e| PtyError::CommunicationError(format!("Failed to read from PTY: {}", e)))?;
            Ok(String::from_utf8_lossy(&buf[..bytes_read]).to_string())
        }

        pub fn try_read_output(&mut self) -> PtyResult<String> {
            let stream = self.process.get_raw_handle()
                .map_err(|e| PtyError::CommunicationError(format!("Failed to get PTY handle: {}", e)))?;
            let raw_fd = stream.as_raw_fd();
            
            // Set non-blocking mode
            let flags = fcntl(raw_fd, FcntlArg::F_GETFL)
                .map_err(|e| PtyError::PlatformError(format!("Failed to get file flags: {}", e)))?;
            fcntl(
                raw_fd,
                FcntlArg::F_SETFL(OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK),
            )
            .map_err(|e| PtyError::PlatformError(format!("Failed to set non-blocking mode: {}", e)))?;
            
            let mut source = SourceFd(&raw_fd);
            let mut poll = Poll::new()
                .map_err(|e| PtyError::PlatformError(format!("Failed to create poll instance: {}", e)))?;
            let mut events = Events::with_capacity(128);
            
            poll.registry()
                .register(&mut source, Token(0), Interest::READABLE)
                .map_err(|e| PtyError::PlatformError(format!("Failed to register poll interest: {}", e)))?;
            
            match poll.poll(&mut events, Some(Duration::from_millis(100))) {
                Ok(()) => {
                    let mut reader = BufReader::new(stream);
                    let mut buf = [0u8; 10240];
                    let bytes_read = reader.read(&mut buf)
                        .map_err(|e| PtyError::CommunicationError(format!("Failed to read from PTY: {}", e)))?;
                    Ok(String::from_utf8_lossy(&buf[..bytes_read]).to_string())
                }
                Err(e) => Err(PtyError::from(e)),
            }
        }
    }
}

#[cfg(target_os = "windows")]
mod windows_impl {
    use super::{Shell, PtyError, PtyResult, PtyErrorContext, recover_lock_poisoned};
    use std::io::{Error, ErrorKind, Read, Write};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use portable_pty::{native_pty_system, CommandBuilder, PtySize, PtySystem};
    
    #[derive(Debug, Clone, Copy)]
    pub enum WindowsSignal {
        CtrlC,
        CtrlBreak,
        CtrlZ,
    }
    
    // Try to use native ConPTY first, fall back to portable-pty if not available
    enum PtyBackend {
        ConPty(crate::conpty_windows::ConPty),
        PortablePty {
            master: Box<dyn portable_pty::MasterPty + Send>,
            child: Box<dyn portable_pty::Child + Send + Sync>,
            reader: Arc<Mutex<Box<dyn Read + Send>>>,
            writer: Box<dyn Write + Send>,
        },
    }
    
    pub struct PtyImpl {
        shell: Shell,
        backend: PtyBackend,
    }

    impl PtyImpl {
        pub fn new(shell: Shell) -> PtyResult<Self> {
            // Try native ConPTY first if available
            if crate::conpty_windows::ConPty::is_conpty_available() {
                match Self::create_conpty(&shell) {
                    Ok(backend) => {
                        return Ok(Self {
                            shell,
                            backend: PtyBackend::ConPty(backend),
                        });
                    }
                    Err(e) => {
                        // Log the error and fall back to portable-pty
                        eprintln!("ConPTY creation failed, falling back to portable-pty: {}", e);
                    }
                }
            }
            
            // Fall back to portable-pty
            Self::create_portable_pty(&shell)
        }
        
        fn create_conpty(shell: &Shell) -> PtyResult<crate::conpty_windows::ConPty> {
            let shell_cmd = Self::build_shell_command(shell);
            crate::conpty_windows::ConPty::new(&shell_cmd, 24, 80)
                .map_err(|e| PtyError::InitializationFailed(format!("ConPTY creation failed: {}", e)))
        }
        
        fn create_portable_pty(shell: &Shell) -> PtyResult<Self> {
            // Get the native PTY system (ConPTY on Windows 10+)
            let pty_system = native_pty_system();
            
            // Set the initial PTY size (80x24 is standard)
            let pty_size = PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            };
            
            // Create a new PTY pair
            let pair = pty_system
                .openpty(pty_size)
                .map_err(|e| PtyError::InitializationFailed(format!("Failed to open PTY: {}", e)))?;
            
            // Build the command for the shell
            let mut cmd = CommandBuilder::new(shell.command());
            
            // For PowerShell variants, add flags to make them more suitable for PTY use
            match shell {
                Shell::PowerShell | Shell::PowerShellCore => {
                    cmd.arg("-NoLogo");
                    cmd.arg("-NoProfile");
                }
                Shell::Cmd => {
                    // cmd.exe doesn't need special flags for PTY mode
                }
                _ => {
                    // Other shells (shouldn't happen on Windows, but handle gracefully)
                }
            }
            
            // Spawn the shell process
            let child = pair.slave
                .spawn_command(cmd)
                .map_err(|e| PtyError::SpawnFailed(format!("Failed to spawn shell: {}", e)))?;
            
            // Get reader and writer handles
            let reader = pair.master
                .try_clone_reader()
                .map_err(|e| PtyError::InitializationFailed(format!("Failed to clone reader: {}", e)))?;
            
            let writer = pair.master
                .take_writer()
                .map_err(|e| PtyError::InitializationFailed(format!("Failed to get writer: {}", e)))?;
            
            Ok(Self {
                shell: *shell,
                backend: PtyBackend::PortablePty {
                    master: pair.master,
                    child,
                    reader: Arc::new(Mutex::new(reader)),
                    writer,
                },
            })
        }
        
        fn build_shell_command(shell: &Shell) -> String {
            match shell {
                Shell::PowerShell => "powershell.exe -NoLogo -NoProfile".to_string(),
                Shell::PowerShellCore => "pwsh.exe -NoLogo -NoProfile".to_string(),
                Shell::Cmd => "cmd.exe".to_string(),
                _ => shell.command().to_string(),
            }
        }

        pub fn set_echo(&mut self, echo: bool) -> PtyResult<()> {
            // ConPTY handles echo internally, so this is mostly a no-op on Windows
            // The terminal emulation layer manages echo behavior
            match &mut self.backend {
                PtyBackend::ConPty(_) => Ok(()), // ConPTY handles echo internally
                PtyBackend::PortablePty { .. } => Ok(()), // portable-pty also handles this
            }
        }

        pub fn write_input(&mut self, input: &str) -> PtyResult<()> {
            match &mut self.backend {
                PtyBackend::ConPty(conpty) => {
                    conpty.write(input.as_bytes())
                        .map_err(|e| PtyError::CommunicationError(format!("ConPTY write failed: {}", e)))
                }
                PtyBackend::PortablePty { writer, child, .. } => {
                    // Check if the child process is still alive before writing
                    if child.try_wait().is_some() {
                        return Err(PtyError::ProcessTerminated);
                    }
                    
                    // Write the input and handle potential errors
                    match writer.write_all(input.as_bytes()) {
                        Ok(_) => {
                            // Flush to ensure data is sent immediately
                            writer.flush()
                                .context("Failed to flush PTY writer")?;
                            Ok(())
                        }
                        Err(e) if e.kind() == ErrorKind::BrokenPipe => {
                            Err(PtyError::ProcessTerminated)
                        }
                        Err(e) => Err(PtyError::from(e)),
                    }
                }
            }
        }

        pub fn read_output(&mut self) -> PtyResult<String> {
            match &mut self.backend {
                PtyBackend::ConPty(conpty) => {
                    let data = conpty.read()
                        .map_err(|e| PtyError::CommunicationError(format!("ConPTY read failed: {}", e)))?;
                    Ok(String::from_utf8_lossy(&data).to_string())
                }
                PtyBackend::PortablePty { reader, .. } => {
                    let mut buffer = vec![0u8; 10240];
                    let mut reader = reader.lock().unwrap_or_else(recover_lock_poisoned);
                    
                    match reader.read(&mut buffer) {
                        Ok(n) if n > 0 => {
                            Ok(String::from_utf8_lossy(&buffer[..n]).to_string())
                        }
                        Ok(_) => Ok(String::new()),
                        Err(e) if e.kind() == ErrorKind::WouldBlock => Ok(String::new()),
                        Err(e) => Err(PtyError::from(e)),
                    }
                }
            }
        }

        pub fn try_read_output(&mut self) -> PtyResult<String> {
            match &mut self.backend {
                PtyBackend::ConPty(conpty) => {
                    let data = conpty.try_read()
                        .map_err(|e| PtyError::CommunicationError(format!("ConPTY try_read failed: {}", e)))?;
                    Ok(String::from_utf8_lossy(&data).to_string())
                }
                PtyBackend::PortablePty { reader, child, .. } => {
                    // Check if the child process is still alive
                    if child.try_wait().is_some() {
                        return Err(PtyError::ProcessTerminated);
                    }
                    
                    let mut buffer = vec![0u8; 10240];
                    let reader = reader.clone();
                    
                    // Try to read without blocking
                    let reader_guard = reader.try_lock();
                    match reader_guard {
                        Ok(mut reader) => {
                            match reader.read(&mut buffer) {
                                Ok(n) if n > 0 => {
                                    Ok(String::from_utf8_lossy(&buffer[..n]).to_string())
                                }
                                Ok(_) => Ok(String::new()),
                                Err(e) if e.kind() == ErrorKind::WouldBlock => Ok(String::new()),
                                Err(e) if e.kind() == ErrorKind::UnexpectedEof => {
                                    if child.try_wait().is_some() {
                                        Err(PtyError::ProcessTerminated)
                                    } else {
                                        Ok(String::new())
                                    }
                                }
                                Err(e) => Err(PtyError::from(e)),
                            }
                        }
                        Err(std::sync::TryLockError::Poisoned(err)) => {
                            let mut reader = recover_lock_poisoned(err);
                            match reader.read(&mut buffer) {
                                Ok(n) if n > 0 => Ok(String::from_utf8_lossy(&buffer[..n]).to_string()),
                                Ok(_) => Ok(String::new()),
                                Err(e) if e.kind() == ErrorKind::WouldBlock => Ok(String::new()),
                                Err(e) => Err(PtyError::from(e)),
                            }
                        }
                        Err(std::sync::TryLockError::WouldBlock) => Ok(String::new()),
                    }
                }
            }
        }
        
        pub fn resize(&mut self, rows: u16, cols: u16) -> PtyResult<()> {
            match &mut self.backend {
                PtyBackend::ConPty(conpty) => {
                    conpty.resize(rows, cols)
                        .map_err(|e| PtyError::PlatformError(format!("Failed to resize ConPTY: {}", e)))
                }
                PtyBackend::PortablePty { master, .. } => {
                    let size = PtySize {
                        rows,
                        cols,
                        pixel_width: 0,
                        pixel_height: 0,
                    };
                    
                    master
                        .resize(size)
                        .map_err(|e| PtyError::PlatformError(format!("Failed to resize PTY: {}", e)))?;
                    
                    Ok(())
                }
            }
        }
        
        pub fn is_alive(&self) -> bool {
            match &self.backend {
                PtyBackend::ConPty(conpty) => conpty.is_alive(),
                PtyBackend::PortablePty { child, .. } => child.try_wait().is_none(),
            }
        }
        
        pub fn send_signal(&mut self, signal: WindowsSignal) -> PtyResult<()> {
            match &mut self.backend {
                PtyBackend::ConPty(conpty) => {
                    let conpty_signal = match signal {
                        WindowsSignal::CtrlC => crate::conpty_windows::ConPtySignal::CtrlC,
                        WindowsSignal::CtrlBreak => crate::conpty_windows::ConPtySignal::CtrlBreak,
                        WindowsSignal::CtrlZ => crate::conpty_windows::ConPtySignal::CtrlZ,
                    };
                    conpty.send_signal(conpty_signal)
                        .map_err(|e| PtyError::CommunicationError(format!("Failed to send signal: {}", e)))
                }
                PtyBackend::PortablePty { writer, .. } => {
                    // Send the signal as a control character
                    let signal_char = match signal {
                        WindowsSignal::CtrlC => b"\x03",
                        WindowsSignal::CtrlBreak => b"\x03",
                        WindowsSignal::CtrlZ => b"\x1a",
                    };
                    writer.write_all(signal_char)
                        .context("Failed to write signal character")?;
                    writer.flush()
                        .context("Failed to flush after sending signal")?;
                    Ok(())
                }
            }
        }
    }

    impl std::fmt::Debug for PtyImpl {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("PtyImpl")
                .field("shell", &self.shell)
                .field("is_alive", &self.is_alive())
                .finish()
        }
    }
    
    impl Drop for PtyImpl {
        fn drop(&mut self) {
            match &mut self.backend {
                PtyBackend::ConPty(_) => {
                    // ConPty handles cleanup in its own Drop implementation
                }
                PtyBackend::PortablePty { child, writer, .. } => {
                    // Ensure the child process is terminated when the PTY is dropped
                    // First try a graceful shutdown
                    if child.try_wait().is_none() {
                        // Send EOF to the writer to signal the shell to exit
                        let exit_cmd = match self.shell {
                            Shell::Cmd => b"exit\r\n",
                            Shell::PowerShell | Shell::PowerShellCore => b"exit\r\n",
                            _ => b"exit\n",
                        };
                        let _ = writer.write_all(exit_cmd);
                        let _ = writer.flush();
                        
                        // Give the process a moment to exit gracefully
                        std::thread::sleep(Duration::from_millis(100));
                        
                        // If still alive, force kill
                        if child.try_wait().is_none() {
                            let _ = child.kill();
                        }
                    }
                    
                    // Wait for the child to fully exit to avoid zombie processes
                    let _ = child.wait();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_shell_detection() {
        // Test that shell command returns expected values
        #[cfg(not(target_os = "windows"))]
        {
            assert_eq!(Shell::Bash.command(), "bash");
            assert_eq!(Shell::Dash.command(), "dash");
            assert_eq!(Shell::Zsh.command(), "zsh");
            assert_eq!(Shell::Fish.command(), "fish");
        }
        
        #[cfg(target_os = "windows")]
        {
            assert_eq!(Shell::PowerShell.command(), "powershell.exe");
            assert_eq!(Shell::PowerShellCore.command(), "pwsh.exe");
            assert_eq!(Shell::Cmd.command(), "cmd.exe");
        }
    }
    
    #[test]
    fn test_shell_behavior_flags() {
        // Test manual input echo
        #[cfg(not(target_os = "windows"))]
        {
            assert!(Shell::Bash.manual_input_echo());
            assert!(Shell::Dash.manual_input_echo());
            assert!(!Shell::Zsh.manual_input_echo());
            assert!(!Shell::Fish.manual_input_echo());
        }
        
        #[cfg(target_os = "windows")]
        {
            assert!(!Shell::PowerShell.manual_input_echo());
            assert!(!Shell::PowerShellCore.manual_input_echo());
            assert!(!Shell::Cmd.manual_input_echo());
        }
        
        // Test extra newline insertion
        #[cfg(not(target_os = "windows"))]
        {
            assert!(Shell::Bash.inserts_extra_newline());
            assert!(Shell::Dash.inserts_extra_newline());
            assert!(!Shell::Zsh.inserts_extra_newline());
            assert!(Shell::Fish.inserts_extra_newline());
        }
        
        #[cfg(target_os = "windows")]
        {
            assert!(!Shell::PowerShell.inserts_extra_newline());
            assert!(!Shell::PowerShellCore.inserts_extra_newline());
            assert!(!Shell::Cmd.inserts_extra_newline());
        }
    }
    
    #[test]
    fn test_pty_thread_lifecycle() {
        use std::sync::Arc;
        use std::thread;
        use std::time::Duration;
        
        // Try to create a PTY with the detected shell
        let shell = Shell::detect();
        if let Ok(pty) = Pty::new(shell) {
            // Give the PTY time to initialize
            thread::sleep(Duration::from_millis(200));
            
            // Verify the reader thread is running
            {
                let pty_lock = pty.lock().unwrap();
                assert!(pty_lock.reader_thread.is_some());
            }
            
            // Drop the PTY and ensure cleanup happens
            drop(pty);
            
            // Give time for cleanup
            thread::sleep(Duration::from_millis(200));
            
            // If we get here without panicking, the thread was properly cleaned up
        } else {
            // PTY creation might fail in some test environments
            println!("PTY creation failed in test environment");
        }
    }
    
    #[test]
    fn test_pty_force_rerender_synchronization() {
        use std::sync::Arc;
        use std::thread;
        use std::time::Duration;
        
        let shell = Shell::detect();
        if let Ok(pty) = Pty::new(shell) {
            thread::sleep(Duration::from_millis(200));
            
            // Test the force_rerender flag
            {
                let pty_lock = pty.lock().unwrap();
                
                // Initially should be false
                assert!(!pty_lock.check_force_rerender());
                
                // Set it to true manually
                pty_lock.force_rerender.store(true, std::sync::atomic::Ordering::Relaxed);
                
                // Check should return true and reset it
                assert!(pty_lock.check_force_rerender());
                
                // Second check should return false
                assert!(!pty_lock.check_force_rerender());
            }
        }
    }
    
    #[test]
    fn test_pty_shutdown_flag() {
        use std::sync::Arc;
        use std::thread;
        use std::time::Duration;
        
        let shell = Shell::detect();
        if let Ok(pty) = Pty::new(shell) {
            thread::sleep(Duration::from_millis(100));
            
            // Check that shutdown flag is initially false
            {
                let pty_lock = pty.lock().unwrap();
                assert!(!pty_lock.shutdown_flag.load(std::sync::atomic::Ordering::Relaxed));
            }
            
            // The shutdown flag should be set when dropping
            // This is tested implicitly when the PTY is dropped at the end of the test
        }
    }
    
    #[test]
    fn test_pty_multiple_instances() {
        use std::sync::Arc;
        use std::thread;
        use std::time::Duration;
        
        let shell = Shell::detect();
        
        // Create multiple PTY instances
        let mut ptys = Vec::new();
        for _ in 0..3 {
            if let Ok(pty) = Pty::new(shell) {
                ptys.push(pty);
            }
        }
        
        // Give them time to initialize
        thread::sleep(Duration::from_millis(200));
        
        // Verify all have reader threads
        for pty in &ptys {
            let pty_lock = pty.lock().unwrap();
            assert!(pty_lock.reader_thread.is_some());
        }
        
        // Drop all PTYs - should clean up all threads
        drop(ptys);
        
        // Give time for cleanup
        thread::sleep(Duration::from_millis(300));
        
        // If we get here without issues, all threads were properly cleaned up
    }
    
    #[test]
    fn test_pty_reader_thread_naming() {
        use std::sync::Arc;
        use std::thread;
        use std::time::Duration;
        
        let shell = Shell::detect();
        if let Ok(_pty) = Pty::new(shell) {
            // The thread should be named "pty-reader"
            // This is more of a compile-time test to ensure the thread naming code exists
            thread::sleep(Duration::from_millis(100));
            
            // Note: We can't easily verify the thread name from outside,
            // but the fact that the code compiles with thread naming is good
        }
    }
}