//! Ox Editor Library
//! 
//! This library provides the core functionality of the Ox text editor.

pub mod clipboard;

// Re-export commonly used types
pub use clipboard::{Clipboard, Selection};