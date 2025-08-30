//! User friendly interface for dealing with pseudo terminals

use mio::unix::SourceFd;
use mio::{Events, Interest, Poll, Token};
use mlua::prelude::*;
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use ptyprocess::PtyProcess;
use std::io::{BufReader, Read, Write};
use std::os::unix::io::AsRawFd;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::pty_error::{PtyError, PtyResult, PtyErrorContext, recover_lock_poisoned};

#[derive(Debug)]
pub struct Pty {
    pub process: PtyProcess,
    pub output: String,
    pub input: String,
    pub shell: Shell,
    pub force_rerender: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum Shell {
    Bash,
    Dash,
    Zsh,
    Fish,
}

impl Shell {
    pub fn manual_input_echo(self) -> bool {
        matches!(self, Self::Bash | Self::Dash)
    }

    pub fn inserts_extra_newline(self) -> bool {
        !matches!(self, Self::Zsh)
    }

    pub fn command(&self) -> &str {
        match self {
            Self::Bash => "bash",
            Self::Dash => "dash",
            Self::Zsh => "zsh",
            Self::Fish => "fish",
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
                    _ => Self::Bash,
                }
            } else {
                Self::Bash
            }
        } else {
            Self::Bash
        })
    }
}

impl Pty {
    pub fn new(shell: Shell) -> PtyResult<Arc<Mutex<Self>>> {
        let process = PtyProcess::spawn(Command::new(shell.command()))
            .map_err(|e| PtyError::SpawnFailed(format!("Failed to spawn {}: {}", shell.command(), e)))?;
        
        let pty = Arc::new(Mutex::new(Self {
            process,
            output: String::new(),
            input: String::new(),
            shell,
            force_rerender: false,
        }));
        
        // Initialize PTY with proper error handling
        {
            let mut pty_guard = pty.lock()
                .unwrap_or_else(recover_lock_poisoned);
            pty_guard.process.set_echo(false, None)
                .map_err(|e| PtyError::InitializationFailed(format!("Failed to set PTY echo mode: {}", e)))?;
        }
        
        std::thread::sleep(Duration::from_millis(100));
        
        {
            let mut pty_guard = pty.lock()
                .unwrap_or_else(recover_lock_poisoned);
            pty_guard.run_command("")
                .map_err(|e| PtyError::InitializationFailed(format!("Failed to run initial command: {:?}", e)))?;
        }
        
        // Spawn thread to constantly read from the terminal
        let pty_clone = Arc::clone(&pty);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(100));
                
                // Try to acquire lock with timeout
                match pty_clone.try_lock() {
                    Ok(mut pty) => {
                        pty.force_rerender = matches!(pty.catch_up(), Ok(true));
                    }
                    Err(std::sync::TryLockError::Poisoned(err)) => {
                        // Recover from poisoned lock
                        let mut pty = recover_lock_poisoned(err);
                        pty.force_rerender = matches!(pty.catch_up(), Ok(true));
                    }
                    Err(std::sync::TryLockError::WouldBlock) => {
                        // Lock is held by another thread, skip this iteration
                        // Lock is held by another thread, skip this iteration
                    }
                }
            }
        });
        
        Ok(pty)
    }

    pub fn run_command(&mut self, cmd: &str) -> PtyResult<()> {
        let mut stream = self.process.get_raw_handle()
            .map_err(|e| PtyError::CommunicationError(format!("Failed to get PTY handle: {}", e)))?;
        // Write the command
        write!(stream, "{cmd}")
            .map_err(|e| PtyError::CommunicationError(format!("Failed to write command to PTY: {}", e)))?;
        std::thread::sleep(Duration::from_millis(100));
        if self.shell.manual_input_echo() {
            // println!("Adding (pre-cmd) {:?}", cmd);
            self.output += cmd;
        }
        // Read the output
        let mut reader = BufReader::new(stream);
        let mut buf = [0u8; 10240];
        let bytes_read = reader.read(&mut buf)
            .map_err(|e| PtyError::CommunicationError(format!("Failed to read PTY output: {}", e)))?;
        let mut output = String::from_utf8_lossy(&buf[..bytes_read]).to_string();
        // Add on the output
        if self.shell.inserts_extra_newline() {
            output = output.replace("\u{1b}[?2004l\r\r\n", "");
        }
        // println!("Adding (aftercmd) \"{:?}\"", output);
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
            // Return key pressed, send the input
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
        let stream = self.process.get_raw_handle()
            .map_err(|e| PtyError::CommunicationError(format!("Failed to get PTY handle: {}", e)))?;
        let raw_fd = stream.as_raw_fd();
        
        let flags = fcntl(raw_fd, FcntlArg::F_GETFL)
            .map_err(|e| PtyError::PlatformError(format!("Failed to get file flags: {}", e)))?;
        fcntl(
            raw_fd,
            FcntlArg::F_SETFL(OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK),
        )
        .map_err(|e| PtyError::PlatformError(format!("Failed to set non-blocking mode: {}", e)))?;
        
        let mut source = SourceFd(&raw_fd);
        // Set up mio Poll and register the raw_fd
        let mut poll = Poll::new()
            .map_err(|e| PtyError::PlatformError(format!("Failed to create poll instance: {}", e)))?;
        let mut events = Events::with_capacity(128);
        poll.registry()
            .register(&mut source, Token(0), Interest::READABLE)
            .map_err(|e| PtyError::PlatformError(format!("Failed to register poll interest: {}", e)))?;
            
        match poll.poll(&mut events, Some(Duration::from_millis(100))) {
            Ok(()) => {
                // Data is available to read
                let mut reader = BufReader::new(stream);
                let mut buf = [0u8; 10240];
                let bytes_read = reader.read(&mut buf)
                    .map_err(|e| PtyError::CommunicationError(format!("Failed to read from PTY: {}", e)))?;

                // Process the read data
                let mut output = String::from_utf8_lossy(&buf[..bytes_read]).to_string();
                if self.shell.inserts_extra_newline() {
                    output = output.replace("\u{1b}[?2004l\r\r\n", "");
                }

                // Append the output to self.output
                // println!("Adding (aftercmd) \"{:?}\"", output);
                self.output += &output;
                Ok(!output.is_empty())
            }
            Err(e) => Err(PtyError::from(e)),
        }
    }
}
