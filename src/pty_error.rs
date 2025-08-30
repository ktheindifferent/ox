//! Error types for PTY operations
//!
//! This module defines custom error types for pseudo-terminal operations,
//! providing better error handling and recovery mechanisms for PTY-related failures.

use std::fmt;
use std::io;
use std::sync::{Arc, PoisonError};

/// Custom error type for PTY operations
#[derive(Debug)]
pub enum PtyError {
    /// I/O operation failed
    Io(io::Error),
    
    /// Failed to acquire a lock
    LockPoisoned(String),
    
    /// Lock acquisition timed out
    LockTimeout,
    
    /// PTY initialization failed
    InitializationFailed(String),
    
    /// Child process has terminated
    ProcessTerminated,
    
    /// Failed to spawn shell
    SpawnFailed(String),
    
    /// PTY read/write operation failed
    CommunicationError(String),
    
    /// Shell command execution failed
    CommandFailed(String),
    
    /// Invalid shell type
    InvalidShell(String),
    
    /// Platform-specific error
    PlatformError(String),
}

impl fmt::Display for PtyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PtyError::Io(err) => write!(f, "PTY I/O error: {}", err),
            PtyError::LockPoisoned(msg) => write!(f, "PTY lock poisoned: {}", msg),
            PtyError::LockTimeout => write!(f, "PTY lock acquisition timed out"),
            PtyError::InitializationFailed(msg) => write!(f, "PTY initialization failed: {}", msg),
            PtyError::ProcessTerminated => write!(f, "PTY child process has terminated"),
            PtyError::SpawnFailed(msg) => write!(f, "Failed to spawn shell: {}", msg),
            PtyError::CommunicationError(msg) => write!(f, "PTY communication error: {}", msg),
            PtyError::CommandFailed(msg) => write!(f, "Shell command failed: {}", msg),
            PtyError::InvalidShell(msg) => write!(f, "Invalid shell: {}", msg),
            PtyError::PlatformError(msg) => write!(f, "Platform-specific PTY error: {}", msg),
        }
    }
}

impl std::error::Error for PtyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PtyError::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for PtyError {
    fn from(err: io::Error) -> Self {
        match err.kind() {
            io::ErrorKind::BrokenPipe => PtyError::ProcessTerminated,
            io::ErrorKind::UnexpectedEof => PtyError::ProcessTerminated,
            _ => PtyError::Io(err),
        }
    }
}

impl<T> From<PoisonError<T>> for PtyError {
    fn from(err: PoisonError<T>) -> Self {
        PtyError::LockPoisoned(format!("Mutex guard poisoned: {}", err))
    }
}

/// Conversion to mlua Error for Lua integration
impl From<PtyError> for mlua::Error {
    fn from(err: PtyError) -> Self {
        mlua::Error::ExternalError(Arc::new(err))
    }
}

/// Result type for PTY operations
pub type PtyResult<T> = Result<T, PtyError>;

/// Helper function to recover from a poisoned lock
pub fn recover_lock_poisoned<T>(err: PoisonError<T>) -> T {
    eprintln!("Warning: Recovering from poisoned lock: {}", err);
    err.into_inner()
}

/// Helper trait for better error context
pub trait PtyErrorContext<T> {
    /// Add context to a PTY error
    fn context(self, msg: &str) -> PtyResult<T>;
}

impl<T> PtyErrorContext<T> for PtyResult<T> {
    fn context(self, msg: &str) -> PtyResult<T> {
        self.map_err(|e| match e {
            PtyError::Io(io_err) => PtyError::CommunicationError(
                format!("{}: {}", msg, io_err)
            ),
            other => other,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::io;
    
    #[test]
    fn test_pty_error_display() {
        let err = PtyError::ProcessTerminated;
        assert_eq!(format!("{}", err), "PTY child process has terminated");
        
        let err = PtyError::LockPoisoned("test lock".to_string());
        assert_eq!(format!("{}", err), "PTY lock poisoned: test lock");
        
        let err = PtyError::InitializationFailed("init failed".to_string());
        assert_eq!(format!("{}", err), "PTY initialization failed: init failed");
    }
    
    #[test]
    fn test_pty_error_from_io() {
        let io_err = io::Error::new(io::ErrorKind::BrokenPipe, "pipe broken");
        let pty_err: PtyError = io_err.into();
        assert!(matches!(pty_err, PtyError::ProcessTerminated));
        
        let io_err = io::Error::new(io::ErrorKind::UnexpectedEof, "eof");
        let pty_err: PtyError = io_err.into();
        assert!(matches!(pty_err, PtyError::ProcessTerminated));
        
        let io_err = io::Error::new(io::ErrorKind::Other, "other error");
        let pty_err: PtyError = io_err.into();
        assert!(matches!(pty_err, PtyError::Io(_)));
    }
    
    #[test]
    fn test_lock_poisoning_recovery() {
        // Create a mutex that will be poisoned
        let mutex = Arc::new(Mutex::new(42));
        let mutex_clone = Arc::clone(&mutex);
        
        // Spawn a thread that will panic while holding the lock
        let handle = thread::spawn(move || {
            let _guard = mutex_clone.lock().unwrap();
            panic!("Intentionally poisoning the lock");
        });
        
        // Wait for the thread to panic
        let _ = handle.join();
        
        // Now the mutex is poisoned
        assert!(mutex.lock().is_err());
        
        // Test recovery
        let result = mutex.lock();
        match result {
            Err(poison_err) => {
                let recovered = recover_lock_poisoned(poison_err);
                assert_eq!(*recovered, 42);
            }
            Ok(_) => panic!("Expected poisoned lock"),
        }
    }
}