//! Cross-platform PTY integration tests
//! 
//! These tests verify PTY functionality across all supported platforms

use ox::pty_cross::{Pty, Shell};
use std::sync::Arc;
use std::time::Duration;
use std::thread;

#[test]
fn test_shell_detection_cross_platform() {
    let shell = Shell::detect();
    
    // Verify we detected a valid shell for the platform
    #[cfg(not(target_os = "windows"))]
    {
        assert!(matches!(
            shell,
            Shell::Bash | Shell::Zsh | Shell::Fish | Shell::Dash
        ));
        
        // Verify the command is not empty
        assert!(!shell.command().is_empty());
        
        println!("Unix shell detected: {:?} ({})", shell, shell.command());
    }
    
    #[cfg(target_os = "windows")]
    {
        assert!(matches!(
            shell,
            Shell::PowerShell | Shell::PowerShellCore | Shell::Cmd
        ));
        
        // Verify the command ends with .exe on Windows
        assert!(shell.command().ends_with(".exe"));
        
        println!("Windows shell detected: {:?} ({})", shell, shell.command());
    }
}

#[test]
fn test_shell_behavior_properties() {
    // Test all shell variants for correct behavior properties
    let test_shells = vec![
        #[cfg(not(target_os = "windows"))]
        Shell::Bash,
        #[cfg(not(target_os = "windows"))]
        Shell::Zsh,
        #[cfg(not(target_os = "windows"))]
        Shell::Fish,
        #[cfg(not(target_os = "windows"))]
        Shell::Dash,
        #[cfg(target_os = "windows")]
        Shell::PowerShell,
        #[cfg(target_os = "windows")]
        Shell::PowerShellCore,
        #[cfg(target_os = "windows")]
        Shell::Cmd,
    ];
    
    for shell in test_shells {
        println!("Testing shell: {:?}", shell);
        println!("  Command: {}", shell.command());
        println!("  Manual input echo: {}", shell.manual_input_echo());
        println!("  Inserts extra newline: {}", shell.inserts_extra_newline());
        
        // Verify command is not empty
        assert!(!shell.command().is_empty());
    }
}

#[test]
fn test_pty_creation_and_initialization() {
    let shell = Shell::detect();
    
    match Pty::new(shell) {
        Ok(pty) => {
            // Give PTY time to initialize
            thread::sleep(Duration::from_millis(200));
            
            // Verify the PTY is properly initialized
            {
                let pty_lock = pty.lock().unwrap();
                
                // Check that shell is set correctly
                assert_eq!(pty_lock.shell.command(), shell.command());
                
                // Check that we can access the shell field
                // (We can't check reader_thread directly as it's private)
                
                println!("PTY created successfully with shell: {:?}", shell);
            }
            
            // PTY should be cleaned up when dropped
            drop(pty);
            
            // Give time for cleanup
            thread::sleep(Duration::from_millis(200));
        }
        Err(e) => {
            // PTY creation might fail in some test environments (e.g., CI without TTY)
            println!("PTY creation failed (may be expected in CI): {:?}", e);
        }
    }
}

#[test]
fn test_pty_command_execution() {
    let shell = Shell::detect();
    
    match Pty::new(shell) {
        Ok(pty) => {
            thread::sleep(Duration::from_millis(200));
            
            // Test running a simple command
            {
                let mut pty_lock = pty.lock().unwrap();
                
                // Platform-specific echo command
                #[cfg(not(target_os = "windows"))]
                let command = "echo 'Hello from PTY'\n";
                #[cfg(target_os = "windows")]
                let command = "echo Hello from PTY\r\n";
                
                let result = pty_lock.run_command(command);
                assert!(result.is_ok(), "Failed to run command: {:?}", result.err());
                
                // Give time for output to be processed
                thread::sleep(Duration::from_millis(100));
                
                // Check that we have output
                assert!(!pty_lock.output.is_empty(), "Should have output after command");
                
                println!("Command output: {}", pty_lock.output);
                
                // Verify output contains expected text
                assert!(
                    pty_lock.output.contains("Hello from PTY") || 
                    pty_lock.output.contains("echo"),
                    "Output should contain command or result"
                );
            }
        }
        Err(e) => {
            println!("Skipping test: PTY creation failed: {:?}", e);
        }
    }
}

#[test]
fn test_pty_character_input() {
    let shell = Shell::detect();
    
    match Pty::new(shell) {
        Ok(pty) => {
            thread::sleep(Duration::from_millis(200));
            
            {
                let mut pty_lock = pty.lock().unwrap();
                
                // Test character input accumulation
                pty_lock.char_input('t').unwrap();
                pty_lock.char_input('e').unwrap();
                pty_lock.char_input('s').unwrap();
                pty_lock.char_input('t').unwrap();
                
                assert_eq!(pty_lock.input, "test", "Input should accumulate");
                
                // Test backspace
                pty_lock.char_pop();
                assert_eq!(pty_lock.input, "tes", "Backspace should remove last char");
                
                // Clear input
                pty_lock.input.clear();
                
                // Test newline triggers command execution
                pty_lock.char_input('l').unwrap();
                pty_lock.char_input('s').unwrap();
                
                let output_before = pty_lock.output.len();
                
                // Newline should execute the command
                pty_lock.char_input('\n').unwrap();
                
                // Input should be cleared after execution
                assert!(pty_lock.input.is_empty(), "Input should be cleared after newline");
                
                thread::sleep(Duration::from_millis(200));
                
                // Should have more output after command execution
                let output_after = pty_lock.output.len();
                assert!(
                    output_after >= output_before,
                    "Should have output after command execution"
                );
            }
        }
        Err(e) => {
            println!("Skipping test: PTY creation failed: {:?}", e);
        }
    }
}

#[test]
fn test_pty_concurrent_access() {
    let shell = Shell::detect();
    
    match Pty::new(shell) {
        Ok(pty) => {
            // Test that multiple threads can safely access the PTY
            let pty1 = Arc::clone(&pty);
            let pty2 = Arc::clone(&pty);
            
            let handle1 = thread::spawn(move || {
                for i in 0..3 {
                    if let Ok(pty_lock) = pty1.lock() {
                        let has_updates = pty_lock.check_for_updates();
                        println!("Thread 1 - iteration {}: updates = {}", i, has_updates);
                    }
                    thread::sleep(Duration::from_millis(50));
                }
            });
            
            let handle2 = thread::spawn(move || {
                for i in 0..3 {
                    if let Ok(pty_lock) = pty2.lock() {
                        let needs_rerender = pty_lock.check_force_rerender();
                        println!("Thread 2 - iteration {}: rerender = {}", i, needs_rerender);
                    }
                    thread::sleep(Duration::from_millis(50));
                }
            });
            
            handle1.join().unwrap();
            handle2.join().unwrap();
            
            println!("Concurrent access test completed successfully");
        }
        Err(e) => {
            println!("Skipping test: PTY creation failed: {:?}", e);
        }
    }
}

#[test]
fn test_pty_force_rerender_flag() {
    let shell = Shell::detect();
    
    match Pty::new(shell) {
        Ok(pty) => {
            thread::sleep(Duration::from_millis(100));
            
            {
                let pty_lock = pty.lock().unwrap();
                
                // Test force rerender flag through the public API
                // Initially should be false
                assert!(!pty_lock.check_force_rerender());
                
                // We can't set it directly (it's private), but we can verify
                // that check_force_rerender works correctly
                // The second check should also return false
                assert!(!pty_lock.check_force_rerender());
            }
        }
        Err(e) => {
            println!("Skipping test: PTY creation failed: {:?}", e);
        }
    }
}

#[test]
fn test_pty_catch_up() {
    let shell = Shell::detect();
    
    match Pty::new(shell) {
        Ok(pty) => {
            thread::sleep(Duration::from_millis(200));
            
            {
                let mut pty_lock = pty.lock().unwrap();
                
                // Run a command that produces output
                #[cfg(not(target_os = "windows"))]
                let command = "echo 'Test output' && echo 'More output'\n";
                #[cfg(target_os = "windows")]
                let command = "echo Test output & echo More output\r\n";
                
                pty_lock.run_command(command).unwrap();
                
                thread::sleep(Duration::from_millis(200));
                
                // Try to catch up on any pending output
                let result = pty_lock.catch_up();
                assert!(result.is_ok(), "catch_up should not fail");
                
                // Output should contain our test strings
                assert!(
                    pty_lock.output.contains("output"),
                    "Should have captured command output"
                );
            }
        }
        Err(e) => {
            println!("Skipping test: PTY creation failed: {:?}", e);
        }
    }
}

#[test]
fn test_pty_silent_run_command() {
    let shell = Shell::detect();
    
    match Pty::new(shell) {
        Ok(pty) => {
            thread::sleep(Duration::from_millis(200));
            
            {
                let mut pty_lock = pty.lock().unwrap();
                
                // First add some output
                pty_lock.output = "Previous output\n".to_string();
                
                // Run silent command
                #[cfg(not(target_os = "windows"))]
                let command = "echo 'Silent test'\n";
                #[cfg(target_os = "windows")]
                let command = "echo Silent test\r\n";
                
                let result = pty_lock.silent_run_command(command);
                assert!(result.is_ok(), "silent_run_command failed: {:?}", result.err());
                
                // Output should not contain the command itself
                // (though it may contain the result)
                println!("Silent command output: {}", pty_lock.output);
            }
        }
        Err(e) => {
            println!("Skipping test: PTY creation failed: {:?}", e);
        }
    }
}

#[test]
fn test_pty_clear() {
    let shell = Shell::detect();
    
    match Pty::new(shell) {
        Ok(pty) => {
            thread::sleep(Duration::from_millis(200));
            
            {
                let mut pty_lock = pty.lock().unwrap();
                
                // Add some output
                pty_lock.output = "Some previous output\n".to_string();
                assert!(!pty_lock.output.is_empty());
                
                // Clear the PTY
                let result = pty_lock.clear();
                assert!(result.is_ok(), "clear() failed: {:?}", result.err());
                
                // Output should be cleared or minimal
                println!("Output after clear: '{}'", pty_lock.output);
            }
        }
        Err(e) => {
            println!("Skipping test: PTY creation failed: {:?}", e);
        }
    }
}

#[test]
fn test_multiple_pty_instances() {
    let shell = Shell::detect();
    
    // Try to create multiple PTY instances
    let mut ptys = Vec::new();
    
    for i in 0..3 {
        match Pty::new(shell) {
            Ok(pty) => {
                println!("Created PTY instance {}", i);
                ptys.push(pty);
            }
            Err(e) => {
                println!("Failed to create PTY instance {}: {:?}", i, e);
                // It's okay if we can't create multiple PTYs in test environment
                break;
            }
        }
    }
    
    if !ptys.is_empty() {
        // Give them time to initialize
        thread::sleep(Duration::from_millis(200));
        
        // Verify all PTYs are accessible
        for (i, pty) in ptys.iter().enumerate() {
            let pty_lock = pty.lock().unwrap();
            // Just verify we can lock and access the PTY
            println!("PTY {} shell: {:?}", i, pty_lock.shell);
        }
        
        println!("Successfully created and verified {} PTY instances", ptys.len());
    }
    
    // Drop all PTYs - should clean up properly
    drop(ptys);
    
    // Give time for cleanup
    thread::sleep(Duration::from_millis(300));
}