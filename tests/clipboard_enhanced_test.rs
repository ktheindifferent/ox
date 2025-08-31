// Enhanced clipboard tests with error handling and retry logic

use ox::clipboard::{Clipboard, ClipboardError, ClipboardMethod, ClipboardStatus};
use std::time::{Duration, Instant};

#[test]
fn test_clipboard_error_types() {
    // Test that ClipboardError types are properly defined
    let errors = vec![
        ClipboardError::NativeClipboardFailed("test error".to_string()),
        ClipboardError::ToolNotFound("xclip not found".to_string()),
        ClipboardError::Timeout,
        ClipboardError::Locked,
        ClipboardError::TextTooLarge(1024),
        ClipboardError::InvalidFormat("bad format".to_string()),
        ClipboardError::PlatformError("platform issue".to_string()),
        ClipboardError::OSC52Failed("osc52 error".to_string()),
    ];
    
    for error in errors {
        // Test that Display trait is implemented
        let err_str = format!("{}", error);
        assert!(!err_str.is_empty());
        
        // Test conversion to IoError
        let io_err: std::io::Error = error.into();
        assert!(!io_err.to_string().is_empty());
    }
}

#[test]
fn test_clipboard_status_tracking() {
    let mut clipboard = Clipboard::new();
    let status = clipboard.get_status();
    
    // Check that status contains expected fields
    assert_eq!(status.method, ClipboardMethod::Native);
    assert!(!status.platform_info.is_empty());
    assert!(status.last_error.is_none());
    
    // Test with OSC52 fallback enabled
    let mut clipboard_osc = Clipboard::new().with_osc52_fallback();
    let status_osc = clipboard_osc.get_status();
    assert!(status_osc.osc52_enabled);
}

#[test]
fn test_clipboard_retry_logic() {
    let mut clipboard = Clipboard::new()
        .with_max_retries(3)
        .with_verbose_logging();
    
    // Test that retries are configurable
    let test_text = "Testing retry logic";
    let start = Instant::now();
    
    // This may succeed or fail depending on the environment
    let _ = clipboard.set_text(test_text);
    
    // If it failed and retried, it should have taken some time
    // (Each retry has a delay)
    let elapsed = start.elapsed();
    
    // Just verify the operation completes without panic
    assert!(elapsed < Duration::from_secs(5), "Clipboard operation took too long");
}

#[test]
fn test_clipboard_method_detection() {
    let mut clipboard = Clipboard::new().with_osc52_fallback();
    
    // Test setting text and checking the method used
    let test_text = "Method detection test";
    match clipboard.set_text(test_text) {
        Ok(()) => {
            // Check which method was used
            let method = clipboard.current_method;
            assert!(
                matches!(
                    method,
                    ClipboardMethod::Native | ClipboardMethod::OSC52 | ClipboardMethod::Cached
                ),
                "Unexpected clipboard method: {:?}",
                method
            );
        }
        Err(_) => {
            // If clipboard is not available, that's okay in test environment
            println!("Clipboard not available in test environment");
        }
    }
}

#[test]
fn test_clipboard_fallback_chain() {
    let mut clipboard = Clipboard::new()
        .with_osc52_fallback()
        .with_max_retries(2);
    
    let test_text = "Fallback chain test";
    
    // Try to set text - should use fallback chain if native fails
    match clipboard.set_text(test_text) {
        Ok(()) => {
            // Verify last_copied is set regardless of method
            assert_eq!(clipboard.last_copied(), test_text);
            
            // Try to get text
            match clipboard.get_text() {
                Ok(text) => {
                    // Should return either native clipboard or cached text
                    if clipboard.current_method == ClipboardMethod::Cached {
                        assert_eq!(text, test_text);
                    }
                }
                Err(_) => {
                    // In test environment, this is acceptable
                }
            }
        }
        Err(_) => {
            // Even if set_text fails, last_copied should be updated
            assert_eq!(clipboard.last_copied(), test_text);
        }
    }
}

#[test]
fn test_clipboard_info_output() {
    let clipboard = Clipboard::new();
    let info = clipboard.get_clipboard_info();
    
    // Verify info contains expected information
    assert!(info.contains("OSC52 fallback:"));
    
    #[cfg(target_os = "windows")]
    assert!(info.contains("Windows"));
    
    #[cfg(target_os = "macos")]
    assert!(info.contains("macOS"));
    
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    assert!(info.contains("Linux"));
}

#[test]
fn test_clipboard_error_clearing() {
    let mut clipboard = Clipboard::new();
    
    // Clipboard might not have an error initially
    assert!(clipboard.last_error().is_none());
    
    // After operations, errors can be cleared
    clipboard.clear_error();
    assert!(clipboard.last_error().is_none());
}

#[test]
fn test_clipboard_large_text_handling() {
    let mut clipboard = Clipboard::new();
    
    // Test with very large text (10MB)
    let large_text = "x".repeat(10 * 1024 * 1024);
    
    match clipboard.set_text(&large_text) {
        Ok(()) => {
            // If it succeeds, verify the text was stored
            assert_eq!(clipboard.last_copied(), large_text);
        }
        Err(_) => {
            // Large text might fail, which is acceptable
            // But last_copied should still be updated
            assert_eq!(clipboard.last_copied(), large_text);
        }
    }
}

#[test]
fn test_clipboard_special_characters() {
    let mut clipboard = Clipboard::new();
    
    let special_texts = vec![
        "Line 1\nLine 2\nLine 3",      // Newlines
        "Tab\there\ttest",              // Tabs
        "Emoji 🎉 🚀 ⭐",               // Emojis
        "Unicode: αβγδ ΑΒΓΔ",          // Greek letters
        "Quotes: \"double\" 'single'",  // Quotes
        "Special: <>&|\\",              // Shell special chars
        "",                             // Empty string
    ];
    
    for text in special_texts {
        let _ = clipboard.set_text(text);
        assert_eq!(clipboard.last_copied(), text);
    }
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
mod linux_tests {
    use super::*;
    use ox::clipboard::Selection;
    
    #[test]
    fn test_linux_selection_types() {
        let mut clipboard_primary = Clipboard::new()
            .with_selection(Selection::Primary);
        
        let mut clipboard_clipboard = Clipboard::new()
            .with_selection(Selection::Clipboard);
        
        // Test that different selections can be configured
        let test_text = "Selection test";
        
        // These may fail in headless environment, but shouldn't panic
        let _ = clipboard_primary.set_text(test_text);
        let _ = clipboard_clipboard.set_text(test_text);
    }
    
    #[test]
    fn test_linux_clipboard_info() {
        // Test that clipboard info properly reports Linux-specific information
        let clipboard = Clipboard::new();
        let info = clipboard.get_clipboard_info();
        
        // On Linux, the info should contain session type and tool information
        assert!(info.contains("Linux session:"));
        
        // Just verify the info function doesn't panic
        println!("Linux clipboard info: {}", info);
    }
}

#[test]
fn test_osc52_always_succeeds() {
    let clipboard = Clipboard::new();
    
    // OSC52 should always succeed as it just prints escape sequences
    let result = clipboard.set_text_osc52("OSC52 test");
    assert!(result.is_ok());
}

#[test]
fn test_concurrent_clipboard_access() {
    use std::sync::{Arc, Mutex};
    use std::thread;
    
    let clipboard = Arc::new(Mutex::new(Clipboard::new()));
    let mut handles = vec![];
    
    // Spawn multiple threads trying to access clipboard
    for i in 0..5 {
        let clipboard_clone = Arc::clone(&clipboard);
        let handle = thread::spawn(move || {
            let text = format!("Thread {} text", i);
            if let Ok(mut cb) = clipboard_clone.lock() {
                let _ = cb.set_text(&text);
            }
        });
        handles.push(handle);
    }
    
    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }
    
    // Verify clipboard is still in valid state
    let clipboard_final = Arc::try_unwrap(clipboard).unwrap().into_inner().unwrap();
    assert!(!clipboard_final.last_copied().is_empty());
}