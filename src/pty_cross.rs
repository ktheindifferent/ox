//! Cross-platform PTY abstraction layer

use mlua::prelude::*;
use std::io::{Read, Result, Write};
use std::sync::{Arc, Mutex};

#[cfg(not(target_os = "windows"))]
mod unix_impl;
#[cfg(target_os = "windows")]
mod windows_impl;

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
            Self::PowerShell
        }
    }

    pub fn manual_input_echo(self) -> bool {
        matches!(self, Self::Bash | Self::Dash)
    }

    pub fn inserts_extra_newline(self) -> bool {
        #[cfg(not(target_os = "windows"))]
        {
            !matches!(self, Self::Zsh)
        }
        #[cfg(target_os = "windows")]
        {
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
                Ok(()) => {
                    let mut reader = BufReader::new(stream);
                    let mut buf = [0u8; 10240];
                    let bytes_read = reader.read(&mut buf)?;
                    Ok(String::from_utf8_lossy(&buf[..bytes_read]).to_string())
                }
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
    use std::io::{Result, Error, ErrorKind};
    
    pub struct PtyImpl {
        shell: Shell,
        // TODO: Implement using Windows ConPTY API or portable-pty crate
    }

    impl PtyImpl {
        pub fn new(shell: Shell) -> Result<Self> {
            // For now, return an error indicating PTY is not yet supported on Windows
            Err(Error::new(
                ErrorKind::Unsupported,
                "PTY support on Windows is not yet implemented. Terminal features are currently unavailable."
            ))
        }

        pub fn set_echo(&mut self, _echo: bool) -> Result<()> {
            Err(Error::new(ErrorKind::Unsupported, "Not implemented on Windows"))
        }

        pub fn write_input(&mut self, _input: &str) -> Result<()> {
            Err(Error::new(ErrorKind::Unsupported, "Not implemented on Windows"))
        }

        pub fn read_output(&mut self) -> Result<String> {
            Err(Error::new(ErrorKind::Unsupported, "Not implemented on Windows"))
        }

        pub fn try_read_output(&mut self) -> Result<String> {
            Err(Error::new(ErrorKind::Unsupported, "Not implemented on Windows"))
        }
    }

    impl std::fmt::Debug for PtyImpl {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("PtyImpl")
                .field("shell", &self.shell)
                .field("status", &"Not implemented")
                .finish()
        }
    }
}