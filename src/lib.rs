//! Ox Editor Library
//! 
//! This library provides the core functionality of the Ox text editor.

pub mod clipboard;
pub mod pty_cross;
pub mod pty_error;
#[cfg(target_os = "windows")]
pub mod conpty_windows;

// Re-export commonly used types
pub use clipboard::{Clipboard, Selection};