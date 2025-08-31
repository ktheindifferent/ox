/// Error handling utilities
use error_set::error_set;
use kaolinite::event::Error as KError;

error_set! {
    OxError = {
        #[display("Error in I/O: {0}")]
        Render(std::io::Error),
        #[display("{}",
            match source {
                KError::NoFileName => "This document has no file name, please use 'save as' instead".to_string(),
                KError::OutOfRange => "Requested operation is out of range".to_string(),
                KError::ReadOnlyFile => "This file is read only and can't be saved or edited".to_string(),
                KError::Rope(rerr) => format!("Backend had an issue processing text: {rerr}"),
                KError::Io(ioerr) => format!("I/O Error: {ioerr}"),
            }
        )]
        Kaolinite(KError),
        #[display("Error in config file: {}", msg)]
        Config {
            msg: String
        },
        #[display("Error in lua: {0}")]
        Lua(mlua::prelude::LuaError),
        #[display("Operation Cancelled")]
        Cancelled,
        #[display("File '{}' is already open", file)]
        AlreadyOpen {
            file: String,
        },
        #[cfg(not(target_os = "windows"))]
        #[display("PTY error: {}", msg)]
        Pty {
            msg: String
        },
        InvalidPath,
        #[display("Regex compilation failed: {}", msg)]
        RegexCompilation {
            msg: String
        },
        #[display("Parse error: {}", msg)]
        Parse {
            msg: String
        },
        #[display("Document not found at index {}", index)]
        DocumentNotFound {
            index: usize
        },
        #[display("Invalid color format: {}", color)]
        InvalidColor {
            color: String
        },
        #[display("Terminal configuration error: {}", msg)]
        TerminalConfig {
            msg: String
        },
        #[display("Clipboard operation failed: {}", msg)]
        Clipboard {
            msg: String
        },
        #[display("Internal error: {}", msg)]
        Internal {
            msg: String
        },
        // None, <--- Needed???
    };
}

/// Easy syntax sugar to have functions return the custom error type
pub type Result<T> = std::result::Result<T, OxError>;

#[cfg(not(target_os = "windows"))]
impl From<crate::pty_error::PtyError> for OxError {
    fn from(err: crate::pty_error::PtyError) -> Self {
        OxError::Pty {
            msg: err.to_string(),
        }
    }
}
