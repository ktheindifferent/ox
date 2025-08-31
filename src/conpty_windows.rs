//! Native Windows ConPTY implementation
//! 
//! This module provides direct integration with Windows ConPTY API
//! for improved terminal emulation on Windows 10 1809 and later.

use std::io::{self, Read, Write, Error, ErrorKind};
use std::mem::{self, MaybeUninit};
use std::ptr;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::io::{AsRawHandle, FromRawHandle, RawHandle};
use std::collections::VecDeque;
use std::path::PathBuf;

use winapi::ctypes::c_void;
use winapi::shared::minwindef::{DWORD, FALSE, TRUE};
use winapi::shared::winerror::S_OK;
use winapi::um::fileapi::{ReadFile, WriteFile};
use winapi::um::handleapi::{CloseHandle, SetHandleInformation, INVALID_HANDLE_VALUE};
use winapi::um::namedpipeapi::CreatePipe;
use winapi::um::processthreadsapi::{
    CreateProcessW, GetExitCodeProcess, TerminateProcess,
    InitializeProcThreadAttributeList, UpdateProcThreadAttribute, DeleteProcThreadAttributeList,
    PROCESS_INFORMATION, STARTUPINFOW
};
use winapi::um::synchapi::WaitForSingleObject;
use winapi::um::winbase::{
    CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT, 
    HANDLE_FLAG_INHERIT, INFINITE, WAIT_TIMEOUT, STARTUPINFOEXW
};
use winapi::um::wincon::COORD;
use winapi::um::wincontypes::HPCON;

const PIPE_BUFFER_SIZE: usize = 65536;
const READ_TIMEOUT_MS: u32 = 50;
const STILL_ACTIVE: DWORD = 259;
const PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE: usize = 0x00020016;

// ConPTY function declarations (Windows 10 1809+)
#[link(name = "kernel32")]
extern "system" {
    fn CreatePseudoConsole(
        size: COORD,
        hInput: winapi::um::winnt::HANDLE,
        hOutput: winapi::um::winnt::HANDLE,
        dwFlags: DWORD,
        phPC: *mut HPCON,
    ) -> winapi::shared::minwindef::HRESULT;
    
    fn ResizePseudoConsole(
        hPC: HPCON,
        size: COORD,
    ) -> winapi::shared::minwindef::HRESULT;
    
    fn ClosePseudoConsole(hPC: HPCON);
}

/// Represents a Windows ConPTY instance
pub struct ConPty {
    hpc: HPCON,
    process_info: PROCESS_INFORMATION,
    input_pipe: RawHandle,
    output_pipe: RawHandle,
    is_alive: Arc<AtomicBool>,
    output_buffer: Arc<Mutex<VecDeque<u8>>>,
    reader_thread: Option<thread::JoinHandle<()>>,
    size: (u16, u16),
}

impl ConPty {
    /// Create a new ConPTY instance with the specified shell
    pub fn new(shell_cmd: &str, rows: u16, cols: u16) -> io::Result<Self> {
        unsafe {
            // Check if ConPTY is available (Windows 10 1809+)
            if !Self::is_conpty_available() {
                return Err(Error::new(
                    ErrorKind::Unsupported,
                    "ConPTY requires Windows 10 version 1809 or later"
                ));
            }

            // Create pipes for ConPTY communication
            let (input_read, input_write) = Self::create_pipe_pair()?;
            let (output_read, output_write) = Self::create_pipe_pair()?;

            // Set pipe inheritance
            SetHandleInformation(input_read, HANDLE_FLAG_INHERIT, 0);
            SetHandleInformation(output_write, HANDLE_FLAG_INHERIT, 0);

            // Create the pseudo console
            let size = COORD {
                X: cols as i16,
                Y: rows as i16,
            };

            let mut hpc: HPCON = ptr::null_mut();
            let hr = CreatePseudoConsole(
                size,
                input_read,
                output_write,
                0,
                &mut hpc
            );

            if hr != S_OK {
                CloseHandle(input_read);
                CloseHandle(input_write);
                CloseHandle(output_read);
                CloseHandle(output_write);
                return Err(Error::new(
                    ErrorKind::Other,
                    format!("Failed to create pseudo console: HRESULT {:#x}", hr)
                ));
            }

            // Close the handles we passed to ConPTY (it has its own references now)
            CloseHandle(input_read);
            CloseHandle(output_write);

            // Prepare startup info for the shell process
            let mut startup_info: STARTUPINFOEXW = mem::zeroed();
            startup_info.StartupInfo.cb = mem::size_of::<STARTUPINFOEXW>() as u32;

            // Initialize the process thread attribute list
            let mut attr_size: usize = 0;
            InitializeProcThreadAttributeList(
                ptr::null_mut(),
                1,
                0,
                &mut attr_size
            );

            let mut attr_list = vec![0u8; attr_size];
            startup_info.lpAttributeList = attr_list.as_mut_ptr() as *mut c_void;

            if InitializeProcThreadAttributeList(
                startup_info.lpAttributeList,
                1,
                0,
                &mut attr_size
            ) == FALSE
            {
                ClosePseudoConsole(hpc);
                CloseHandle(input_write);
                CloseHandle(output_read);
                return Err(Error::last_os_error());
            }

            // Associate the ConPTY with the attribute list
            if UpdateProcThreadAttribute(
                startup_info.lpAttributeList,
                0,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
                hpc as *mut c_void,
                mem::size_of::<HPCON>(),
                ptr::null_mut(),
                ptr::null_mut()
            ) == FALSE
            {
                DeleteProcThreadAttributeList(startup_info.lpAttributeList);
                ClosePseudoConsole(hpc);
                CloseHandle(input_write);
                CloseHandle(output_read);
                return Err(Error::last_os_error());
            }

            // Parse the shell command and arguments
            let (shell_path, shell_args) = Self::parse_shell_command(shell_cmd)?;
            
            // Build the command line
            let cmd_line = if shell_args.is_empty() {
                shell_path.clone()
            } else {
                format!("{} {}", shell_path, shell_args)
            };
            
            let mut cmd_line_wide: Vec<u16> = OsStr::new(&cmd_line)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();

            // Create the shell process
            let mut process_info: PROCESS_INFORMATION = mem::zeroed();
            
            let creation_flags = EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT;
            
            if CreateProcessW(
                ptr::null(),
                cmd_line_wide.as_mut_ptr(),
                ptr::null_mut(),
                ptr::null_mut(),
                FALSE,
                creation_flags,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut startup_info.StartupInfo as *mut STARTUPINFOW,
                &mut process_info
            ) == FALSE
            {
                let err = Error::last_os_error();
                DeleteProcThreadAttributeList(startup_info.lpAttributeList);
                ClosePseudoConsole(hpc);
                CloseHandle(input_write);
                CloseHandle(output_read);
                return Err(Error::new(
                    ErrorKind::Other,
                    format!("Failed to create process '{}': {}", shell_cmd, err)
                ));
            }

            // Clean up the attribute list
            DeleteProcThreadAttributeList(startup_info.lpAttributeList);

            // Create the ConPty instance
            let is_alive = Arc::new(AtomicBool::new(true));
            let output_buffer = Arc::new(Mutex::new(VecDeque::new()));
            
            let mut conpty = ConPty {
                hpc,
                process_info,
                input_pipe: input_write,
                output_pipe: output_read,
                is_alive: is_alive.clone(),
                output_buffer: output_buffer.clone(),
                reader_thread: None,
                size: (cols, rows),
            };

            // Start the reader thread
            conpty.start_reader_thread();

            Ok(conpty)
        }
    }

    /// Check if ConPTY is available on this system
    pub fn is_conpty_available() -> bool {
        // ConPTY is available on Windows 10 version 1809 (build 17763) and later
        // Try to dynamically check if the ConPTY APIs are available
        unsafe {
            use std::ffi::CString;
            use winapi::um::libloaderapi::{GetModuleHandleA, GetProcAddress};
            
            // Check if we can load the ConPTY functions from kernel32.dll
            let kernel32_name = CString::new("kernel32.dll").unwrap();
            let kernel32 = GetModuleHandleA(kernel32_name.as_ptr());
            
            if kernel32.is_null() {
                return false;
            }
            
            // Check if CreatePseudoConsole is available
            let fn_name = CString::new("CreatePseudoConsole").unwrap();
            let create_pty_fn = GetProcAddress(kernel32, fn_name.as_ptr());
            
            !create_pty_fn.is_null()
        }
    }

    /// Parse a shell command into executable and arguments
    fn parse_shell_command(cmd: &str) -> io::Result<(String, String)> {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return Err(Error::new(ErrorKind::InvalidInput, "Empty shell command"));
        }
        
        let shell = parts[0].to_string();
        let args = parts[1..].join(" ");
        
        // Resolve the full path for common shells if needed
        let shell_path = if shell.ends_with(".exe") {
            shell
        } else {
            match shell {
                "cmd" => "cmd.exe".to_string(),
                "powershell" => "powershell.exe".to_string(),
                "pwsh" => "pwsh.exe".to_string(),
                "wsl" => "wsl.exe".to_string(),
                "bash" => "bash.exe".to_string(),
                _ => shell,
            }
        };
        
        Ok((shell_path, args))
    }

    /// Create a pipe pair for ConPTY communication
    unsafe fn create_pipe_pair() -> io::Result<(RawHandle, RawHandle)> {
        let mut read_pipe: RawHandle = INVALID_HANDLE_VALUE;
        let mut write_pipe: RawHandle = INVALID_HANDLE_VALUE;
        
        let mut security_attrs = winapi::um::minwinbase::SECURITY_ATTRIBUTES {
            nLength: mem::size_of::<winapi::um::minwinbase::SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: ptr::null_mut(),
            bInheritHandle: TRUE,
        };
        
        if CreatePipe(
            &mut read_pipe,
            &mut write_pipe,
            &mut security_attrs,
            PIPE_BUFFER_SIZE as u32
        ) == FALSE
        {
            return Err(Error::last_os_error());
        }
        
        Ok((read_pipe, write_pipe))
    }

    /// Start the background thread that reads from the output pipe
    fn start_reader_thread(&mut self) {
        let output_pipe = self.output_pipe;
        let buffer = self.output_buffer.clone();
        let is_alive = self.is_alive.clone();
        
        let handle = thread::spawn(move || {
            let mut temp_buffer = vec![0u8; PIPE_BUFFER_SIZE];
            
            while is_alive.load(Ordering::Relaxed) {
                unsafe {
                    let mut bytes_read: DWORD = 0;
                    let result = ReadFile(
                        output_pipe,
                        temp_buffer.as_mut_ptr() as *mut c_void,
                        PIPE_BUFFER_SIZE as u32,
                        &mut bytes_read,
                        ptr::null_mut()
                    );
                    
                    if result != FALSE && bytes_read > 0 {
                        let mut buffer_lock = buffer.lock().unwrap();
                        buffer_lock.extend(&temp_buffer[..bytes_read as usize]);
                    } else {
                        // Check if the process is still alive
                        thread::sleep(Duration::from_millis(10));
                    }
                }
            }
        });
        
        self.reader_thread = Some(handle);
    }

    /// Write input to the ConPTY
    pub fn write(&mut self, data: &[u8]) -> io::Result<()> {
        if !self.is_alive() {
            return Err(Error::new(ErrorKind::BrokenPipe, "Process has terminated"));
        }
        
        unsafe {
            let mut bytes_written: DWORD = 0;
            let result = WriteFile(
                self.input_pipe,
                data.as_ptr() as *const c_void,
                data.len() as u32,
                &mut bytes_written,
                ptr::null_mut()
            );
            
            if result == FALSE {
                Err(Error::last_os_error())
            } else {
                Ok(())
            }
        }
    }

    /// Read available output from the ConPTY
    pub fn read(&mut self) -> io::Result<Vec<u8>> {
        let mut buffer_lock = self.output_buffer.lock().unwrap();
        let data: Vec<u8> = buffer_lock.drain(..).collect();
        Ok(data)
    }

    /// Try to read output without blocking
    pub fn try_read(&mut self) -> io::Result<Vec<u8>> {
        if let Ok(mut buffer_lock) = self.output_buffer.try_lock() {
            let data: Vec<u8> = buffer_lock.drain(..).collect();
            Ok(data)
        } else {
            Ok(Vec::new())
        }
    }

    /// Resize the ConPTY
    pub fn resize(&mut self, rows: u16, cols: u16) -> io::Result<()> {
        unsafe {
            let size = COORD {
                X: cols as i16,
                Y: rows as i16,
            };
            
            let hr = ResizePseudoConsole(self.hpc, size);
            if hr != S_OK {
                Err(Error::new(
                    ErrorKind::Other,
                    format!("Failed to resize pseudo console: HRESULT {:#x}", hr)
                ))
            } else {
                self.size = (cols, rows);
                Ok(())
            }
        }
    }

    /// Check if the process is still alive
    pub fn is_alive(&self) -> bool {
        unsafe {
            let mut exit_code: DWORD = 0;
            if GetExitCodeProcess(self.process_info.hProcess, &mut exit_code) != FALSE {
                exit_code == STILL_ACTIVE
            } else {
                false
            }
        }
    }

    /// Get the process exit code
    pub fn exit_code(&self) -> Option<u32> {
        unsafe {
            let mut exit_code: DWORD = 0;
            if GetExitCodeProcess(self.process_info.hProcess, &mut exit_code) != FALSE {
                if exit_code != STILL_ACTIVE {
                    Some(exit_code)
                } else {
                    None
                }
            } else {
                None
            }
        }
    }

    /// Send a signal to the process (Ctrl+C, Ctrl+Break, etc.)
    pub fn send_signal(&mut self, signal: ConPtySignal) -> io::Result<()> {
        let signal_bytes = match signal {
            ConPtySignal::CtrlC => b"\x03",
            ConPtySignal::CtrlBreak => b"\x03", // Ctrl+Break is also 0x03
            ConPtySignal::CtrlZ => b"\x1a",
            ConPtySignal::CtrlD => b"\x04",
            ConPtySignal::CtrlBackslash => b"\x1c",
        };
        
        self.write(signal_bytes)
    }

    /// Kill the process
    pub fn kill(&mut self) -> io::Result<()> {
        unsafe {
            if TerminateProcess(self.process_info.hProcess, 1) == FALSE {
                Err(Error::last_os_error())
            } else {
                self.is_alive.store(false, Ordering::Relaxed);
                Ok(())
            }
        }
    }

    /// Wait for the process to exit
    pub fn wait(&self) -> io::Result<u32> {
        unsafe {
            let result = WaitForSingleObject(self.process_info.hProcess, INFINITE);
            if result == WAIT_TIMEOUT || result == 0xFFFFFFFF {
                Err(Error::last_os_error())
            } else {
                self.exit_code().ok_or_else(|| {
                    Error::new(ErrorKind::Other, "Failed to get exit code")
                })
            }
        }
    }

    /// Get the current ConPTY size
    pub fn size(&self) -> (u16, u16) {
        self.size
    }
}

impl Drop for ConPty {
    fn drop(&mut self) {
        // Signal the reader thread to stop
        self.is_alive.store(false, Ordering::Relaxed);
        
        // Wait for reader thread to finish
        if let Some(handle) = self.reader_thread.take() {
            let _ = handle.join();
        }
        
        unsafe {
            // Close the process handles
            if !self.process_info.hProcess.is_null() {
                // Try graceful shutdown first
                let _ = self.send_signal(ConPtySignal::CtrlC);
                thread::sleep(Duration::from_millis(100));
                
                // Force kill if still alive
                if self.is_alive() {
                    let _ = TerminateProcess(self.process_info.hProcess, 1);
                    let _ = WaitForSingleObject(self.process_info.hProcess, 5000);
                }
                
                CloseHandle(self.process_info.hProcess);
                CloseHandle(self.process_info.hThread);
            }
            
            // Close the ConPTY
            if !self.hpc.is_null() {
                ClosePseudoConsole(self.hpc);
            }
            
            // Close pipe handles
            if self.input_pipe != INVALID_HANDLE_VALUE {
                CloseHandle(self.input_pipe);
            }
            if self.output_pipe != INVALID_HANDLE_VALUE {
                CloseHandle(self.output_pipe);
            }
        }
    }
}

/// Signals that can be sent to the ConPTY process
#[derive(Debug, Clone, Copy)]
pub enum ConPtySignal {
    CtrlC,
    CtrlBreak,
    CtrlZ,
    CtrlD,
    CtrlBackslash,
}

/// Helper to detect available shells on Windows
pub struct WindowsShellDetector;

impl WindowsShellDetector {
    /// Detect all available shells on the system
    pub fn detect_available_shells() -> Vec<ShellInfo> {
        let mut shells = Vec::new();
        
        // Check for PowerShell Core (pwsh)
        if let Some(info) = Self::check_powershell_core() {
            shells.push(info);
        }
        
        // Check for Windows PowerShell
        if let Some(info) = Self::check_windows_powershell() {
            shells.push(info);
        }
        
        // Check for Command Prompt (always available on Windows)
        shells.push(Self::get_cmd_info());
        
        // Check for WSL
        if let Some(info) = Self::check_wsl() {
            shells.push(info);
        }
        
        // Check for Git Bash
        if let Some(info) = Self::check_git_bash() {
            shells.push(info);
        }
        
        shells
    }
    
    fn check_powershell_core() -> Option<ShellInfo> {
        if Self::command_exists("pwsh.exe") {
            Some(ShellInfo {
                name: "PowerShell Core".to_string(),
                executable: "pwsh.exe".to_string(),
                args: vec!["-NoLogo".to_string()],
                version: Self::get_command_version("pwsh.exe", "--version"),
            })
        } else {
            None
        }
    }
    
    fn check_windows_powershell() -> Option<ShellInfo> {
        if Self::command_exists("powershell.exe") {
            Some(ShellInfo {
                name: "Windows PowerShell".to_string(),
                executable: "powershell.exe".to_string(),
                args: vec!["-NoLogo".to_string()],
                version: Self::get_command_version("powershell.exe", "-Version 2>&1"),
            })
        } else {
            None
        }
    }
    
    fn get_cmd_info() -> ShellInfo {
        ShellInfo {
            name: "Command Prompt".to_string(),
            executable: "cmd.exe".to_string(),
            args: vec![],
            version: Self::get_windows_version(),
        }
    }
    
    fn check_wsl() -> Option<ShellInfo> {
        if Self::command_exists("wsl.exe") {
            Some(ShellInfo {
                name: "Windows Subsystem for Linux".to_string(),
                executable: "wsl.exe".to_string(),
                args: vec![],
                version: Self::get_command_version("wsl.exe", "--version"),
            })
        } else {
            None
        }
    }
    
    fn check_git_bash() -> Option<ShellInfo> {
        // Common Git Bash locations
        let possible_paths = vec![
            r"C:\Program Files\Git\bin\bash.exe",
            r"C:\Program Files (x86)\Git\bin\bash.exe",
            r"C:\Git\bin\bash.exe",
        ];
        
        for path in possible_paths {
            if std::path::Path::new(path).exists() {
                return Some(ShellInfo {
                    name: "Git Bash".to_string(),
                    executable: path.to_string(),
                    args: vec![],
                    version: Self::get_command_version(path, "--version"),
                });
            }
        }
        
        None
    }
    
    fn command_exists(cmd: &str) -> bool {
        std::process::Command::new("where")
            .arg(cmd)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
    
    fn get_command_version(cmd: &str, version_flag: &str) -> Option<String> {
        std::process::Command::new(cmd)
            .args(version_flag.split_whitespace())
            .output()
            .ok()
            .and_then(|output| {
                String::from_utf8(output.stdout)
                    .or_else(|_| String::from_utf8(output.stderr))
                    .ok()
            })
            .map(|s| s.lines().next().unwrap_or("").to_string())
    }
    
    fn get_windows_version() -> Option<String> {
        std::process::Command::new("cmd")
            .args(&["/c", "ver"])
            .output()
            .ok()
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|s| s.trim().to_string())
    }
}

/// Information about an available shell
#[derive(Debug, Clone)]
pub struct ShellInfo {
    pub name: String,
    pub executable: String,
    pub args: Vec<String>,
    pub version: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_conpty_available() {
        // This should pass on Windows 10 1809+ and Windows 11
        let available = ConPty::is_conpty_available();
        println!("ConPTY available: {}", available);
        
        // We can't assert true here as it depends on the Windows version
        // but the function should not panic
    }
    
    #[test]
    fn test_shell_detection() {
        let shells = WindowsShellDetector::detect_available_shells();
        
        // At minimum, cmd.exe should always be available
        assert!(!shells.is_empty());
        
        // Check that cmd.exe is in the list
        let has_cmd = shells.iter().any(|s| s.executable.contains("cmd.exe"));
        assert!(has_cmd, "cmd.exe should always be detected");
        
        // Print detected shells for debugging
        for shell in &shells {
            println!("Found shell: {} ({})", shell.name, shell.executable);
            if let Some(version) = &shell.version {
                println!("  Version: {}", version);
            }
        }
    }
    
    #[test]
    fn test_parse_shell_command() {
        let test_cases = vec![
            ("cmd", ("cmd.exe".to_string(), "".to_string())),
            ("powershell", ("powershell.exe".to_string(), "".to_string())),
            ("pwsh -NoLogo", ("pwsh.exe".to_string(), "-NoLogo".to_string())),
            ("wsl bash", ("wsl.exe".to_string(), "bash".to_string())),
        ];
        
        for (input, expected) in test_cases {
            let result = ConPty::parse_shell_command(input).unwrap();
            assert_eq!(result, expected);
        }
    }
    
    #[test]
    #[ignore] // This test requires Windows 10 1809+
    fn test_conpty_creation() {
        if !ConPty::is_conpty_available() {
            println!("Skipping ConPTY test: not available on this system");
            return;
        }
        
        let result = ConPty::new("cmd.exe", 24, 80);
        if let Ok(mut conpty) = result {
            assert!(conpty.is_alive());
            assert_eq!(conpty.size(), (80, 24));
            
            // Try to write a simple command
            let _ = conpty.write(b"echo test\r\n");
            thread::sleep(Duration::from_millis(100));
            
            // Try to read output
            let output = conpty.read().unwrap_or_default();
            println!("Output: {:?}", String::from_utf8_lossy(&output));
            
            // Test resize
            let _ = conpty.resize(30, 100);
            assert_eq!(conpty.size(), (100, 30));
        } else {
            println!("Failed to create ConPTY: {:?}", result.err());
        }
    }
    
    #[test]
    #[ignore] // This test requires Windows 10 1809+
    fn test_conpty_signals() {
        if !ConPty::is_conpty_available() {
            return;
        }
        
        if let Ok(mut conpty) = ConPty::new("cmd.exe", 24, 80) {
            // Test sending Ctrl+C
            let result = conpty.send_signal(ConPtySignal::CtrlC);
            assert!(result.is_ok());
            
            // Test that the process is still alive after Ctrl+C
            thread::sleep(Duration::from_millis(100));
            assert!(conpty.is_alive());
        }
    }
}