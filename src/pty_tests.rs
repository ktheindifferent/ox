//! Tests for the cross-platform PTY implementation

use super::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[test]
fn test_shell_detection() {
    let shell = Shell::detect();
    
    #[cfg(target_os = "windows")]
    {
        assert!(matches!(shell, Shell::PowerShell | Shell::Cmd));
    }
    
    #[cfg(not(target_os = "windows"))]
    {
        assert!(matches!(
            shell,
            Shell::Bash | Shell::Zsh | Shell::Fish | Shell::Dash
        ));
    }
}

#[test]
fn test_shell_command() {
    #[cfg(target_os = "windows")]
    {
        assert_eq!(Shell::PowerShell.command(), "powershell.exe");
        assert_eq!(Shell::Cmd.command(), "cmd.exe");
    }
    
    #[cfg(not(target_os = "windows"))]
    {
        assert_eq!(Shell::Bash.command(), "bash");
        assert_eq!(Shell::Zsh.command(), "zsh");
        assert_eq!(Shell::Fish.command(), "fish");
        assert_eq!(Shell::Dash.command(), "dash");
    }
}

#[test]
fn test_pty_creation() {
    let shell = Shell::detect();
    let result = Pty::new(shell);
    
    assert!(result.is_ok(), "Failed to create PTY: {:?}", result.err());
    
    let pty = result.unwrap();
    let locked = pty.lock().unwrap();
    assert_eq!(locked.shell.command(), shell.command());
}

#[test]
fn test_pty_echo_command() {
    let shell = Shell::detect();
    let pty = Pty::new(shell).expect("Failed to create PTY");
    
    std::thread::sleep(Duration::from_millis(500));
    
    let mut locked = pty.lock().unwrap();
    
    // Test simple echo command
    #[cfg(target_os = "windows")]
    let test_cmd = "echo Hello World\n";
    #[cfg(not(target_os = "windows"))]
    let test_cmd = "echo Hello World\n";
    
    let result = locked.run_command(test_cmd);
    assert!(result.is_ok(), "Failed to run command: {:?}", result.err());
    
    // Give some time for output to be processed
    std::thread::sleep(Duration::from_millis(200));
    
    // Check that output contains our message
    assert!(
        locked.output.contains("Hello World"),
        "Output doesn't contain expected text. Got: {}",
        locked.output
    );
}

#[test]
fn test_pty_clear() {
    let shell = Shell::detect();
    let pty = Pty::new(shell).expect("Failed to create PTY");
    
    std::thread::sleep(Duration::from_millis(500));
    
    let mut locked = pty.lock().unwrap();
    
    // Add some output
    locked.output = "Test output".to_string();
    
    // Clear the PTY
    let result = locked.clear();
    assert!(result.is_ok(), "Failed to clear PTY: {:?}", result.err());
    
    // Output should be cleared or contain only prompt
    assert!(
        locked.output.is_empty() || !locked.output.contains("Test output"),
        "Output was not cleared properly"
    );
}

#[test]
fn test_pty_char_input() {
    let shell = Shell::detect();
    let pty = Pty::new(shell).expect("Failed to create PTY");
    
    std::thread::sleep(Duration::from_millis(500));
    
    let mut locked = pty.lock().unwrap();
    
    // Test character input
    locked.char_input('l').unwrap();
    locked.char_input('s').unwrap();
    assert_eq!(locked.input, "ls");
    
    // Test backspace
    locked.char_pop();
    assert_eq!(locked.input, "l");
    
    // Clear input
    locked.input.clear();
}

#[test]
fn test_pty_multiple_commands() {
    let shell = Shell::detect();
    let pty = Pty::new(shell).expect("Failed to create PTY");
    
    std::thread::sleep(Duration::from_millis(500));
    
    let mut locked = pty.lock().unwrap();
    
    // Run multiple commands
    #[cfg(target_os = "windows")]
    {
        locked.run_command("echo First\n").unwrap();
        std::thread::sleep(Duration::from_millis(200));
        locked.run_command("echo Second\n").unwrap();
        std::thread::sleep(Duration::from_millis(200));
    }
    
    #[cfg(not(target_os = "windows"))]
    {
        locked.run_command("echo First\n").unwrap();
        std::thread::sleep(Duration::from_millis(200));
        locked.run_command("echo Second\n").unwrap();
        std::thread::sleep(Duration::from_millis(200));
    }
    
    // Check that both outputs are present
    assert!(
        locked.output.contains("First"),
        "First command output missing"
    );
    assert!(
        locked.output.contains("Second"),
        "Second command output missing"
    );
}

#[cfg(target_os = "windows")]
#[test]
fn test_windows_specific_shells() {
    // Test PowerShell creation
    let ps_pty = Pty::new(Shell::PowerShell);
    assert!(ps_pty.is_ok(), "Failed to create PowerShell PTY");
    
    // Test CMD creation
    let cmd_pty = Pty::new(Shell::Cmd);
    assert!(cmd_pty.is_ok(), "Failed to create CMD PTY");
}

#[cfg(target_os = "windows")]
#[test]
fn test_windows_pty_resize() {
    use super::platform::PtyImpl;
    
    let shell = Shell::PowerShell;
    let mut pty = PtyImpl::new(shell).expect("Failed to create Windows PTY");
    
    // Test resize functionality
    let result = pty.resize(30, 100);
    assert!(result.is_ok(), "Failed to resize PTY: {:?}", result.err());
}

#[test]
fn test_pty_catch_up() {
    let shell = Shell::detect();
    let pty = Pty::new(shell).expect("Failed to create PTY");
    
    std::thread::sleep(Duration::from_millis(500));
    
    // Run a command that produces output
    {
        let mut locked = pty.lock().unwrap();
        #[cfg(target_os = "windows")]
        locked.run_command("dir\n").unwrap();
        #[cfg(not(target_os = "windows"))]
        locked.run_command("ls\n").unwrap();
    }
    
    // Wait for background thread to catch up
    std::thread::sleep(Duration::from_millis(300));
    
    // Check that catch_up works
    {
        let mut locked = pty.lock().unwrap();
        let result = locked.catch_up();
        assert!(result.is_ok(), "catch_up failed: {:?}", result.err());
    }
}

#[test]
fn test_shell_from_lua() {
    use mlua::Lua;
    
    let lua = Lua::new();
    
    // Test conversion from Lua string to Shell
    #[cfg(target_os = "windows")]
    {
        let ps_str = lua.create_string("powershell").unwrap();
        let shell = Shell::from_lua(mlua::Value::String(ps_str), &lua).unwrap();
        assert!(matches!(shell, Shell::PowerShell));
        
        let cmd_str = lua.create_string("cmd").unwrap();
        let shell = Shell::from_lua(mlua::Value::String(cmd_str), &lua).unwrap();
        assert!(matches!(shell, Shell::Cmd));
    }
    
    #[cfg(not(target_os = "windows"))]
    {
        let bash_str = lua.create_string("bash").unwrap();
        let shell = Shell::from_lua(mlua::Value::String(bash_str), &lua).unwrap();
        assert!(matches!(shell, Shell::Bash));
        
        let zsh_str = lua.create_string("zsh").unwrap();
        let shell = Shell::from_lua(mlua::Value::String(zsh_str), &lua).unwrap();
        assert!(matches!(shell, Shell::Zsh));
    }
}

#[test]
fn test_shell_to_lua() {
    use mlua::Lua;
    
    let lua = Lua::new();
    
    #[cfg(target_os = "windows")]
    {
        let ps_val = Shell::PowerShell.into_lua(&lua).unwrap();
        if let mlua::Value::String(s) = ps_val {
            assert_eq!(s.to_str().unwrap(), "powershell.exe");
        } else {
            panic!("Expected string value");
        }
        
        let cmd_val = Shell::Cmd.into_lua(&lua).unwrap();
        if let mlua::Value::String(s) = cmd_val {
            assert_eq!(s.to_str().unwrap(), "cmd.exe");
        } else {
            panic!("Expected string value");
        }
    }
    
    #[cfg(not(target_os = "windows"))]
    {
        let bash_val = Shell::Bash.into_lua(&lua).unwrap();
        if let mlua::Value::String(s) = bash_val {
            assert_eq!(s.to_str().unwrap(), "bash");
        } else {
            panic!("Expected string value");
        }
    }
}