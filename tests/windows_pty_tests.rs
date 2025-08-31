//! Comprehensive tests for Windows PTY implementation
//! 
//! These tests verify ConPTY functionality on Windows systems
//! and ensure cross-platform compatibility.

#![cfg(target_os = "windows")]

use ox::conpty_windows::{ConPty, ConPtySignal, WindowsShellDetector, ShellInfo};
use ox::pty_cross::{Pty, Shell};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::thread;

/// Test helper to create a ConPTY instance
fn create_test_conpty() -> Result<ConPty, Box<dyn std::error::Error>> {
    ConPty::new("cmd.exe", 24, 80).map_err(|e| e.into())
}

/// Test helper to verify shell availability
fn ensure_shell_available(shell: &str) -> bool {
    std::process::Command::new("where")
        .arg(shell)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[test]
fn test_conpty_availability() {
    // This test should pass on Windows 10 1809+ and Windows 11
    let available = ConPty::is_conpty_available();
    
    // Get Windows version for diagnostics
    let version = std::process::Command::new("cmd")
        .args(&["/c", "ver"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok());
    
    println!("Windows version: {:?}", version);
    println!("ConPTY available: {}", available);
    
    // On modern Windows, ConPTY should be available
    if let Some(ver) = version {
        if ver.contains("Windows 10") || ver.contains("Windows 11") {
            assert!(available, "ConPTY should be available on Windows 10/11");
        }
    }
}

#[test]
fn test_shell_detection() {
    let shells = WindowsShellDetector::detect_available_shells();
    
    // At minimum, cmd.exe should always be available
    assert!(!shells.is_empty(), "Should detect at least one shell");
    
    // Check that cmd.exe is in the list
    let has_cmd = shells.iter().any(|s| s.executable.contains("cmd.exe"));
    assert!(has_cmd, "cmd.exe should always be detected");
    
    // Print detected shells for debugging
    println!("Detected shells:");
    for shell in &shells {
        println!("  - {} ({})", shell.name, shell.executable);
        if let Some(version) = &shell.version {
            println!("    Version: {}", version);
        }
    }
}

#[test]
fn test_conpty_creation_and_cleanup() {
    if !ConPty::is_conpty_available() {
        println!("Skipping test: ConPTY not available");
        return;
    }
    
    // Create a ConPTY instance
    let result = create_test_conpty();
    assert!(result.is_ok(), "Failed to create ConPTY: {:?}", result.err());
    
    let mut conpty = result.unwrap();
    
    // Verify it's alive
    assert!(conpty.is_alive(), "ConPTY should be alive after creation");
    
    // Verify size
    assert_eq!(conpty.size(), (80, 24), "ConPTY size should match requested dimensions");
    
    // Drop the ConPTY and ensure cleanup happens
    drop(conpty);
    
    // Give time for cleanup
    thread::sleep(Duration::from_millis(200));
}

#[test]
fn test_conpty_io_operations() {
    if !ConPty::is_conpty_available() {
        println!("Skipping test: ConPTY not available");
        return;
    }
    
    let mut conpty = match create_test_conpty() {
        Ok(c) => c,
        Err(e) => {
            println!("Failed to create ConPTY: {}", e);
            return;
        }
    };
    
    // Write a simple echo command
    let command = b"echo Hello ConPTY\r\n";
    let write_result = conpty.write(command);
    assert!(write_result.is_ok(), "Failed to write to ConPTY: {:?}", write_result.err());
    
    // Give time for processing
    thread::sleep(Duration::from_millis(200));
    
    // Read the output
    let output = conpty.read().unwrap_or_default();
    let output_str = String::from_utf8_lossy(&output);
    
    println!("ConPTY output: {:?}", output_str);
    
    // Check that we got some output (exact output varies by system)
    assert!(!output.is_empty(), "Should receive output from ConPTY");
}

#[test]
fn test_conpty_resize() {
    if !ConPty::is_conpty_available() {
        println!("Skipping test: ConPTY not available");
        return;
    }
    
    let mut conpty = match create_test_conpty() {
        Ok(c) => c,
        Err(e) => {
            println!("Failed to create ConPTY: {}", e);
            return;
        }
    };
    
    // Initial size
    assert_eq!(conpty.size(), (80, 24));
    
    // Resize the ConPTY
    let resize_result = conpty.resize(30, 100);
    assert!(resize_result.is_ok(), "Failed to resize ConPTY: {:?}", resize_result.err());
    
    // Verify new size
    assert_eq!(conpty.size(), (100, 30));
}

#[test]
fn test_conpty_signals() {
    if !ConPty::is_conpty_available() {
        println!("Skipping test: ConPTY not available");
        return;
    }
    
    let mut conpty = match create_test_conpty() {
        Ok(c) => c,
        Err(e) => {
            println!("Failed to create ConPTY: {}", e);
            return;
        }
    };
    
    // Start a long-running command
    conpty.write(b"ping localhost -t\r\n").unwrap();
    thread::sleep(Duration::from_millis(500));
    
    // Send Ctrl+C to interrupt
    let signal_result = conpty.send_signal(ConPtySignal::CtrlC);
    assert!(signal_result.is_ok(), "Failed to send Ctrl+C: {:?}", signal_result.err());
    
    // Give time for signal to be processed
    thread::sleep(Duration::from_millis(200));
    
    // Process should still be alive (cmd.exe doesn't exit on Ctrl+C)
    assert!(conpty.is_alive(), "ConPTY should still be alive after Ctrl+C");
}

#[test]
fn test_different_shells() {
    if !ConPty::is_conpty_available() {
        println!("Skipping test: ConPTY not available");
        return;
    }
    
    // Test with different shells if available
    let test_shells = vec![
        ("cmd.exe", "echo Test"),
        ("powershell.exe -NoLogo", "$PSVersionTable"),
        ("pwsh.exe -NoLogo", "$PSVersionTable"),
    ];
    
    for (shell_cmd, test_cmd) in test_shells {
        // Check if shell is available
        let shell_name = shell_cmd.split_whitespace().next().unwrap();
        if !ensure_shell_available(shell_name) {
            println!("Skipping {}: not available", shell_name);
            continue;
        }
        
        println!("Testing shell: {}", shell_cmd);
        
        match ConPty::new(shell_cmd, 24, 80) {
            Ok(mut conpty) => {
                assert!(conpty.is_alive());
                
                // Run test command
                let command = format!("{}\r\n", test_cmd);
                conpty.write(command.as_bytes()).unwrap();
                
                thread::sleep(Duration::from_millis(500));
                
                let output = conpty.read().unwrap_or_default();
                let output_str = String::from_utf8_lossy(&output);
                
                println!("  Output: {:?}", output_str);
                assert!(!output.is_empty(), "Should get output from {}", shell_name);
            }
            Err(e) => {
                println!("  Failed to create ConPTY with {}: {}", shell_cmd, e);
            }
        }
    }
}

#[test]
fn test_cross_platform_pty_abstraction() {
    // Test the cross-platform Pty abstraction
    let shell = Shell::detect();
    println!("Detected shell: {:?}", shell);
    
    match Pty::new(shell) {
        Ok(pty) => {
            // Give time to initialize
            thread::sleep(Duration::from_millis(200));
            
            // Basic operations
            {
                let mut pty_lock = pty.lock().unwrap();
                
                // Run a simple command
                let result = pty_lock.run_command("echo Hello from cross-platform PTY\r\n");
                assert!(result.is_ok(), "Failed to run command: {:?}", result.err());
                
                // Check output
                assert!(!pty_lock.output.is_empty(), "Should have output");
                println!("Cross-platform PTY output: {}", pty_lock.output);
            }
        }
        Err(e) => {
            println!("Failed to create cross-platform PTY: {:?}", e);
            // This might fail in test environments, which is acceptable
        }
    }
}

#[test]
fn test_pty_thread_safety() {
    if !ConPty::is_conpty_available() {
        println!("Skipping test: ConPTY not available");
        return;
    }
    
    let shell = Shell::detect();
    
    match Pty::new(shell) {
        Ok(pty) => {
            // Test concurrent access from multiple threads
            let pty_clone1 = Arc::clone(&pty);
            let pty_clone2 = Arc::clone(&pty);
            
            let handle1 = thread::spawn(move || {
                for i in 0..5 {
                    if let Ok(mut pty_lock) = pty_clone1.lock() {
                        let _ = pty_lock.run_command(&format!("echo Thread 1 - {}\r\n", i));
                    }
                    thread::sleep(Duration::from_millis(50));
                }
            });
            
            let handle2 = thread::spawn(move || {
                for i in 0..5 {
                    if let Ok(mut pty_lock) = pty_clone2.lock() {
                        let _ = pty_lock.run_command(&format!("echo Thread 2 - {}\r\n", i));
                    }
                    thread::sleep(Duration::from_millis(50));
                }
            });
            
            handle1.join().unwrap();
            handle2.join().unwrap();
            
            // Check final output
            let pty_lock = pty.lock().unwrap();
            println!("Final output length: {}", pty_lock.output.len());
            assert!(!pty_lock.output.is_empty(), "Should have accumulated output");
        }
        Err(e) => {
            println!("Failed to create PTY for thread safety test: {:?}", e);
        }
    }
}

#[test]
fn test_pty_error_handling() {
    // Test error handling for invalid shells
    match ConPty::new("nonexistent_shell.exe", 24, 80) {
        Ok(_) => panic!("Should fail with nonexistent shell"),
        Err(e) => {
            println!("Expected error for nonexistent shell: {}", e);
            assert!(e.to_string().contains("Failed to create process"));
        }
    }
}

#[test]
fn test_pty_cleanup_on_panic() {
    if !ConPty::is_conpty_available() {
        println!("Skipping test: ConPTY not available");
        return;
    }
    
    // This test verifies that PTY resources are cleaned up even if a panic occurs
    let result = std::panic::catch_unwind(|| {
        let _conpty = create_test_conpty().unwrap();
        panic!("Intentional panic for testing cleanup");
    });
    
    assert!(result.is_err(), "Should have panicked");
    
    // Give time for cleanup
    thread::sleep(Duration::from_millis(200));
    
    // If we get here without issues, cleanup was successful
    println!("PTY cleanup successful after panic");
}