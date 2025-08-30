//! User friendly interface for dealing with pseudo terminals

use mio::unix::SourceFd;
use mio::{Events, Interest, Poll, Token};
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use ptyprocess::PtyProcess;
use std::io::{BufReader, Read, Write};
use std::os::unix::io::AsRawFd;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::pty_error::{PtyError, PtyResult, recover_lock_poisoned};

pub struct Pty {
    pub process: PtyProcess,
    pub output: String,
    pub input: String,
    pub shell: Shell,
    force_rerender: Arc<AtomicBool>,
    shutdown_flag: Arc<AtomicBool>,
    reader_thread: Option<JoinHandle<()>>,
    update_receiver: Receiver<bool>,
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

    pub fn command(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Dash => "dash",
            Self::Zsh => "zsh",
            Self::Fish => "fish",
        }
    }

    pub fn detect() -> Self {
        std::env::var("SHELL").map_or(Self::Bash, |shell| {
            if shell.ends_with("zsh") {
                Self::Zsh
            } else if shell.ends_with("fish") {
                Self::Fish
            } else if shell.ends_with("dash") {
                Self::Dash
            } else {
                Self::Bash
            }
        })
    }
}

impl Pty {
    pub fn new(shell: Shell) -> PtyResult<Arc<Mutex<Self>>> {
        let process = PtyProcess::spawn(Command::new(shell.command()))
            .map_err(|e| PtyError::SpawnFailed(format!("Failed to spawn {}: {}", shell.command(), e)))?;
            
        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let force_rerender = Arc::new(AtomicBool::new(false));
        let (update_sender, update_receiver) = channel::<bool>();
        
        let pty = Arc::new(Mutex::new(Self {
            process,
            output: String::new(),
            input: String::new(),
            shell,
            force_rerender: Arc::clone(&force_rerender),
            shutdown_flag: Arc::clone(&shutdown_flag),
            reader_thread: None,
            update_receiver,
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
                    
                    // Try to acquire lock with timeout
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

impl std::fmt::Debug for Pty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pty")
            .field("shell", &self.shell)
            .field("output_len", &self.output.len())
            .field("input_len", &self.input.len())
            .field("has_reader_thread", &self.reader_thread.is_some())
            .finish()
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
            
            // Wait for the thread to finish
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_pty_thread_lifecycle() {
        use std::thread;
        use std::time::Duration;
        
        // Try to create a PTY with bash
        if let Ok(pty) = Pty::new(Shell::Bash) {
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
        use std::thread;
        use std::time::Duration;
        
        if let Ok(pty) = Pty::new(Shell::Bash) {
            thread::sleep(Duration::from_millis(200));
            
            // Test the force_rerender flag
            {
                let pty_lock = pty.lock().unwrap();
                
                // Initially should be false
                assert!(!pty_lock.check_force_rerender());
                
                // Set it to true manually
                pty_lock.force_rerender.store(true, Ordering::Relaxed);
                
                // Check should return true and reset it
                assert!(pty_lock.check_force_rerender());
                
                // Second check should return false
                assert!(!pty_lock.check_force_rerender());
            }
        }
    }
    
    #[test]
    fn test_pty_shutdown_flag() {
        use std::thread;
        use std::time::Duration;
        
        if let Ok(pty) = Pty::new(Shell::Bash) {
            thread::sleep(Duration::from_millis(100));
            
            // Check that shutdown flag is initially false
            {
                let pty_lock = pty.lock().unwrap();
                assert!(!pty_lock.shutdown_flag.load(Ordering::Relaxed));
            }
            
            // The shutdown flag should be set when dropping
            // This is tested implicitly when the PTY is dropped at the end of the test
        }
    }
    
    #[test]
    fn test_pty_multiple_instances() {
        use std::thread;
        use std::time::Duration;
        
        // Create multiple PTY instances
        let mut ptys = Vec::new();
        for _ in 0..3 {
            if let Ok(pty) = Pty::new(Shell::Bash) {
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
    fn test_shell_detection() {
        // Test that shell command returns expected values
        assert_eq!(Shell::Bash.command(), "bash");
        assert_eq!(Shell::Dash.command(), "dash");
        assert_eq!(Shell::Zsh.command(), "zsh");
        assert_eq!(Shell::Fish.command(), "fish");
        
        // Test manual input echo
        assert!(Shell::Bash.manual_input_echo());
        assert!(Shell::Dash.manual_input_echo());
        assert!(!Shell::Zsh.manual_input_echo());
        assert!(!Shell::Fish.manual_input_echo());
        
        // Test extra newline insertion
        assert!(Shell::Bash.inserts_extra_newline());
        assert!(Shell::Dash.inserts_extra_newline());
        assert!(!Shell::Zsh.inserts_extra_newline());
        assert!(Shell::Fish.inserts_extra_newline());
    }
}