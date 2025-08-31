/// Lazy-compiled regex patterns for better performance and error handling
use once_cell::sync::Lazy;
use regex::Regex;
use std::sync::Arc;

/// Error type for regex compilation failures
#[derive(Debug, Clone)]
pub struct RegexError {
    pub pattern: String,
    pub error: String,
}

impl std::fmt::Display for RegexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Failed to compile regex '{}': {}", self.pattern, self.error)
    }
}

impl std::error::Error for RegexError {}

/// Macro to create lazy regex with proper error handling
macro_rules! lazy_regex {
    ($name:ident, $pattern:expr) => {
        pub static $name: Lazy<Arc<Regex>> = Lazy::new(|| {
            Arc::new(Regex::new($pattern).unwrap_or_else(|e| {
                eprintln!("WARNING: Failed to compile regex '{}': {}", $pattern, e);
                eprintln!("Using fallback regex that matches nothing");
                // Return a regex that never matches as a safe fallback
                Regex::new("(?!)").expect("Fallback regex should always compile")
            }))
        });
    };
}

#[cfg(not(target_os = "windows"))]
pub mod ansi {
    use super::*;
    
    // ANSI pattern constants
    const CURSORS: &str = r"\x1b\[(s|u|H|\d+;?\d*[A-J])";
    const ERASING: &str = r"\x1b\[\d*[KJ]";
    const DISPLAY: &str = r"\x1b\[\??(?:\d+)?(?:;\d+)*[mlhABCDHJKSTfXiubsn=\?]";
    const SC_MODE: &str = r"\x1b\[(?:=|\?)[0-9]{1,4}(?:h|l)";
    const AC_JUNK: &str = r"(\a|\b|\n|\v|\f|\r)";
    
    // Create the global pattern dynamically
    fn global_pattern() -> String {
        format!("({CURSORS}|{ERASING}|{DISPLAY}|{SC_MODE}|{AC_JUNK})")
    }
    
    // Lazy-compiled regex patterns
    lazy_regex!(GLOBAL_ANSI, &global_pattern());
    lazy_regex!(DISPLAY_ONLY, DISPLAY);
    lazy_regex!(WEIRD_NEWLINE, r"⏎\s*⏎\s?");
    lazy_regex!(LONG_SPACES, r"%(?:\x1b\[1m)?\s{5,}");
    lazy_regex!(TOTAL_RESET, r"\x1b\[0m");
    lazy_regex!(RESET_BG, r"\x1b\[49m");
    lazy_regex!(RESET_FG, r"\x1b\[39m");
}

/// Try to compile a regex pattern, returning a Result
pub fn try_compile(pattern: &str) -> Result<Regex, RegexError> {
    Regex::new(pattern).map_err(|e| RegexError {
        pattern: pattern.to_string(),
        error: e.to_string(),
    })
}