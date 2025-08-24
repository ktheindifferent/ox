//! Cross-platform PTY abstraction layer

use mlua::prelude::*;
use std::io::Result;
use std::sync::{Arc, Mutex};

#[cfg(test)]
#[path = "pty_tests.rs"]
mod tests;

// Platform-specific implementations are defined inline below

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
            // Check COMSPEC environment variable first (standard Windows shell variable)
            if let Ok(comspec) = std::env::var("COMSPEC") {
                if comspec.to_lowercase().contains("cmd.exe") {
                    return Self::Cmd;
                }
            }
            
            // Check if PowerShell is the default shell
            // Check for PowerShell in PATH or use it as default on modern Windows
            if let Ok(path) = std::env::var("PATH") {
                if path.contains("PowerShell") || path.contains("pwsh") {
                    return Self::PowerShell;
                }
            }
            
            // Default to PowerShell on modern Windows (Windows 10+)
            // as it's more feature-rich than cmd.exe
            Self::PowerShell
        }
    }

    pub fn manual_input_echo(self) -> bool {
        #[cfg(not(target_os = "windows"))]
        {
            matches!(self, Self::Bash | Self::Dash)
        }
        #[cfg(target_os = "windows")]
        {
            // Windows shells handle echo differently through ConPTY
            false
        }
    }

    pub fn inserts_extra_newline(self) -> bool {
        #[cfg(not(target_os = "windows"))]
        {
            !matches!(self, Self::Zsh)
        }
        #[cfg(target_os = "windows")]
        {
            // Windows ConPTY handles newlines differently
            false
        }
    }

    pub fn command(&self) -> &str {
        match self {
            Self::Bash => "bash",
            Self::Dash => "dash",
            Self::Zsh => "zsh",
            Self::Fish => "fish",
            #[cfg(target_os = "windows")]
            Self::PowerShell => "powershell.exe",
            #[cfg(target_os = "windows")]
            Self::Cmd => "cmd.exe",
        }
    }
}

impl IntoLua for Shell {
    fn into_lua(self, lua: &Lua) -> LuaResult<LuaValue> {
        let string = lua.create_string(self.command())?;
        Ok(LuaValue::String(string))
    }
}

impl FromLua for Shell {
    fn from_lua(val: LuaValue, _: &Lua) -> LuaResult<Self> {
        Ok(if let LuaValue::String(inner) = val {
            if let Ok(s) = inner.to_str() {
                match s.to_owned().as_str() {
                    "dash" => Self::Dash,
                    "zsh" => Self::Zsh,
                    "fish" => Self::Fish,
                    #[cfg(target_os = "windows")]
                    "powershell" | "powershell.exe" => Self::PowerShell,
                    #[cfg(target_os = "windows")]
                    "cmd" | "cmd.exe" => Self::Cmd,
                    _ => Self::Bash,
                }
            } else {
                Shell::detect()
            }
        } else {
            Shell::detect()
        })
    }
}

#[derive(Debug)]
pub struct Pty {
    inner: platform::PtyImpl,
    pub output: String,
    pub input: String,
    pub shell: Shell,
    pub force_rerender: bool,
}

impl Pty {
    pub fn new(shell: Shell) -> Result<Arc<Mutex<Self>>> {
        let inner = platform::PtyImpl::new(shell)?;
        let pty = Arc::new(Mutex::new(Self {
            inner,
            output: String::new(),
            input: String::new(),
            shell,
            force_rerender: false,
        }));
        
        // Initialize the PTY
        pty.lock().unwrap().initialize()?;
        
        // Spawn thread to constantly read from the terminal
        let pty_clone = Arc::clone(&pty);
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let mut pty = pty_clone.lock().unwrap();
            pty.force_rerender = matches!(pty.catch_up(), Ok(true));
            std::mem::drop(pty);
        });
        
        Ok(pty)
    }

    fn initialize(&mut self) -> Result<()> {
        self.inner.set_echo(false)?;
        std::thread::sleep(std::time::Duration::from_millis(100));
        self.run_command("")?;
        Ok(())
    }

    pub fn run_command(&mut self, cmd: &str) -> Result<()> {
        self.inner.write_input(cmd)?;
        std::thread::sleep(std::time::Duration::from_millis(100));
        
        if self.shell.manual_input_echo() {
            self.output += cmd;
        }
        
        let mut output = self.inner.read_output()?;
        
        if self.shell.inserts_extra_newline() {
            output = output.replace("\u{1b}[?2004l\r\r\n", "");
        }
        
        self.output += &output;
        Ok(())
    }

    pub fn silent_run_command(&mut self, cmd: &str) -> Result<()> {
        self.output.clear();
        self.run_command(cmd)?;
        if self.output.starts_with(cmd) {
            self.output = self.output.chars().skip(cmd.chars().count()).collect();
        }
        Ok(())
    }

    pub fn char_input(&mut self, c: char) -> Result<()> {
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

    pub fn clear(&mut self) -> Result<()> {
        self.output.clear();
        self.run_command("\n")?;
        self.output = self.output.trim_start_matches('\n').to_string();
        Ok(())
    }

    pub fn catch_up(&mut self) -> Result<bool> {
        match self.inner.try_read_output() {
            Ok(output) if !output.is_empty() => {
                let mut processed = output;
                if self.shell.inserts_extra_newline() {
                    processed = processed.replace("\u{1b}[?2004l\r\r\n", "");
                }
                self.output += &processed;
                Ok(true)
            }
            Ok(_) => Ok(false),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(false),
            Err(e) => Err(e),
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod unix_impl {
    use super::Shell;
    use mio::unix::SourceFd;
    use mio::{Events, Interest, Poll, Token};
    use nix::fcntl::{fcntl, FcntlArg, OFlag};
    use ptyprocess::PtyProcess;
    use std::io::{BufReader, Read, Result, Write};
    use std::os::unix::io::AsRawFd;
    use std::process::Command;
    use std::time::Duration;

    pub struct PtyImpl {
        process: PtyProcess,
        shell: Shell,
    }

    impl PtyImpl {
        pub fn new(shell: Shell) -> Result<Self> {
            Ok(Self {
                process: PtyProcess::spawn(Command::new(shell.command()))?,
                shell,
            })
        }

        pub fn set_echo(&mut self, echo: bool) -> Result<()> {
            self.process.set_echo(echo, None)?;
            Ok(())
        }

        pub fn write_input(&mut self, input: &str) -> Result<()> {
            let mut stream = self.process.get_raw_handle()?;
            write!(stream, "{}", input)?;
            Ok(())
        }

        pub fn read_output(&mut self) -> Result<String> {
            let mut stream = self.process.get_raw_handle()?;
            let mut reader = BufReader::new(stream);
            let mut buf = [0u8; 10240];
            let bytes_read = reader.read(&mut buf)?;
            Ok(String::from_utf8_lossy(&buf[..bytes_read]).to_string())
        }

        pub fn try_read_output(&mut self) -> Result<String> {
            let stream = self.process.get_raw_handle()?;
            let raw_fd = stream.as_raw_fd();
            
            // Set non-blocking mode
            let flags = fcntl(raw_fd, FcntlArg::F_GETFL).unwrap();
            fcntl(
                raw_fd,
                FcntlArg::F_SETFL(OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK),
            )
            .unwrap();
            
            let mut source = SourceFd(&raw_fd);
            let mut poll = Poll::new()?;
            let mut events = Events::with_capacity(128);
            
            poll.registry()
                .register(&mut source, Token(0), Interest::READABLE)?;
            
            match poll.poll(&mut events, Some(Duration::from_millis(100))) {
                Ok(()) if !events.is_empty() => {
                    let mut reader = BufReader::new(stream);
                    let mut buf = [0u8; 10240];
                    match reader.read(&mut buf) {
                        Ok(bytes_read) => Ok(String::from_utf8_lossy(&buf[..bytes_read]).to_string()),
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(String::new()),
                        Err(e) => Err(e),
                    }
                }
                Ok(_) => Ok(String::new()), // No events, return empty string
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(String::new()),
                Err(e) => Err(e),
            }
        }
    }

    impl std::fmt::Debug for PtyImpl {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("PtyImpl")
                .field("shell", &self.shell)
                .finish()
        }
    }
}

#[cfg(target_os = "windows")]
mod windows_impl {
    use super::Shell;
    use portable_pty::{native_pty_system, Child, CommandBuilder, PtyPair, PtySize, PtySystem};
    use std::io::{Error, ErrorKind, Read, Result, Write};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    
    pub struct PtyImpl {
        shell: Shell,
        pty_pair: Box<PtyPair>,
        reader: Arc<Mutex<Box<dyn Read + Send>>>,
        writer: Box<dyn Write + Send>,
        child: Box<dyn Child + Send + Sync>,
    }

    impl PtyImpl {
        pub fn new(shell: Shell) -> Result<Self> {
            // Get the native PTY system (ConPTY on Windows)
            let pty_system = native_pty_system();
            
            // Create a PTY with standard terminal dimensions
            let pty_pair = pty_system
                .openpty(PtySize {
                    rows: 24,
                    cols: 80,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|e| Error::new(ErrorKind::Other, format!("Failed to open PTY: {}", e)))?;
            
            // Build the command for the shell
            let mut cmd = CommandBuilder::new(shell.command());
            
            // Add shell-specific arguments
            match shell {
                Shell::PowerShell => {
                    cmd.arg("-NoLogo");
                    cmd.arg("-NoExit");
                    cmd.arg("-Command");
                    cmd.arg("-");
                }
                Shell::Cmd => {
                    // cmd.exe doesn't need special arguments for PTY mode
                }
                _ => {}
            }
            
            // Set environment variables for better terminal compatibility
            cmd.env("TERM", "xterm-256color");
            
            // Spawn the shell process
            let child = pty_pair
                .slave
                .spawn_command(cmd)
                .map_err(|e| Error::new(ErrorKind::Other, format!("Failed to spawn shell: {}", e)))?;
            
            // Get reader and writer from the master side
            let reader = pty_pair
                .master
                .try_clone_reader()
                .map_err(|e| Error::new(ErrorKind::Other, format!("Failed to clone reader: {}", e)))?;
            
            let writer = pty_pair
                .master
                .take_writer()
                .map_err(|e| Error::new(ErrorKind::Other, format!("Failed to get writer: {}", e)))?;
            
            Ok(Self {
                shell,
                pty_pair: Box::new(pty_pair),
                reader: Arc::new(Mutex::new(reader)),
                writer,
                child: Box::new(child),
            })
        }

        pub fn set_echo(&mut self, _echo: bool) -> Result<()> {
            // Echo control is handled differently on Windows ConPTY
            // The ConPTY API doesn't directly expose echo control like Unix PTYs
            // This is typically controlled by the shell itself
            Ok(())
        }

        pub fn write_input(&mut self, input: &str) -> Result<()> {
            self.writer.write_all(input.as_bytes())?;
            self.writer.flush()?;
            Ok(())
        }

        pub fn read_output(&mut self) -> Result<String> {
            // Check if the process is still alive
            if !self.is_alive() {
                return Err(Error::new(ErrorKind::BrokenPipe, "PTY process has terminated"));
            }
            
            let mut reader = self.reader.lock().unwrap();
            let mut buf = vec![0u8; 10240];
            
            // Set a timeout for reading
            let timeout = Duration::from_millis(500);
            let start = std::time::Instant::now();
            
            let mut total_read = 0;
            while start.elapsed() < timeout {
                match reader.read(&mut buf[total_read..]) {
                    Ok(0) => {
                        // Check if process died
                        if !self.is_alive() && total_read == 0 {
                            return Err(Error::new(ErrorKind::BrokenPipe, "PTY process terminated"));
                        }
                        break;
                    }
                    Ok(n) => {
                        total_read += n;
                        if total_read >= buf.len() {
                            break;
                        }
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                    Err(e) if e.kind() == ErrorKind::Interrupted => {
                        // Retry on interrupted system call
                        continue;
                    }
                    Err(e) => return Err(e),
                }
            }
            
            Ok(String::from_utf8_lossy(&buf[..total_read]).to_string())
        }

        pub fn try_read_output(&mut self) -> Result<String> {
            // Check if the process is still alive
            if !self.is_alive() {
                return Err(Error::new(ErrorKind::BrokenPipe, "PTY process has terminated"));
            }
            
            let mut reader = self.reader.lock().unwrap();
            let mut buf = vec![0u8; 10240];
            let mut total_read = 0;
            
            // Non-blocking read attempt
            loop {
                match reader.read(&mut buf[total_read..]) {
                    Ok(0) => {
                        // Check if process died
                        if !self.is_alive() && total_read == 0 {
                            return Err(Error::new(ErrorKind::BrokenPipe, "PTY process terminated"));
                        }
                        break;
                    }
                    Ok(n) => {
                        total_read += n;
                        if total_read >= buf.len() {
                            break;
                        }
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => {
                        break; // No more data available
                    }
                    Err(e) if e.kind() == ErrorKind::Interrupted => {
                        // Retry on interrupted system call
                        continue;
                    }
                    Err(e) => return Err(e),
                }
            }
            
            Ok(String::from_utf8_lossy(&buf[..total_read]).to_string())
        }
        
        pub fn resize(&mut self, rows: u16, cols: u16) -> Result<()> {
            self.pty_pair
                .master
                .resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|e| Error::new(ErrorKind::Other, format!("Failed to resize PTY: {}", e)))?;
            Ok(())
        }
        
        pub fn is_alive(&mut self) -> bool {
            // Check if the child process is still running
            self.child.try_wait().is_none()
        }
        
        pub fn kill(&mut self) -> Result<()> {
            // Attempt to kill the child process gracefully
            self.child
                .kill()
                .map_err(|e| Error::new(ErrorKind::Other, format!("Failed to kill process: {}", e)))?;
            Ok(())
        }
    }

    impl Drop for PtyImpl {
        fn drop(&mut self) {
            // Ensure the child process is terminated when the PTY is dropped
            // This prevents zombie processes on Windows
            if self.is_alive() {
                let _ = self.kill();
            }
        }
    }

    impl std::fmt::Debug for PtyImpl {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("PtyImpl")
                .field("shell", &self.shell)
                .field("status", &"ConPTY")
                .field("alive", &self.child.try_wait().is_none())
                .finish()
        }
    }
}