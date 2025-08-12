//! Cross-platform directory utilities

use std::env;
use std::path::{Path, PathBuf};

/// Get the user's home directory
pub fn home_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        env::var("USERPROFILE")
            .or_else(|_| env::var("HOMEDRIVE").and_then(|drive| {
                env::var("HOMEPATH").map(|path| format!("{}{}", drive, path))
            }))
            .ok()
            .map(PathBuf::from)
    }
    
    #[cfg(not(target_os = "windows"))]
    {
        env::var("HOME")
            .ok()
            .map(PathBuf::from)
    }
}

/// Get the configuration directory for the application
pub fn config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        // On Windows, use %APPDATA%\ox or %USERPROFILE%\ox
        env::var("APPDATA")
            .ok()
            .map(PathBuf::from)
            .or_else(home_dir)
            .map(|mut path| {
                path.push("ox");
                path
            })
    }
    
    #[cfg(target_os = "macos")]
    {
        // On macOS, use ~/Library/Application Support/ox or ~/.config/ox
        home_dir().map(|mut path| {
            // Check if ~/Library/Application Support exists
            let mut app_support = path.clone();
            app_support.push("Library");
            app_support.push("Application Support");
            
            if app_support.exists() {
                app_support.push("ox");
                app_support
            } else {
                // Fall back to ~/.config/ox
                path.push(".config");
                path.push("ox");
                path
            }
        })
    }
    
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        // On Linux and other Unix-like systems, respect XDG Base Directory spec
        env::var("XDG_CONFIG_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                home_dir().map(|mut path| {
                    path.push(".config");
                    path
                })
            })
            .map(|mut path| {
                path.push("ox");
                path
            })
    }
}

/// Get the data directory for the application
pub fn data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        // On Windows, use %LOCALAPPDATA%\ox or config_dir
        env::var("LOCALAPPDATA")
            .ok()
            .map(PathBuf::from)
            .map(|mut path| {
                path.push("ox");
                path
            })
            .or_else(config_dir)
    }
    
    #[cfg(target_os = "macos")]
    {
        // On macOS, use ~/Library/Application Support/ox
        home_dir().map(|mut path| {
            path.push("Library");
            path.push("Application Support");
            path.push("ox");
            path
        })
    }
    
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        // On Linux, respect XDG Base Directory spec
        env::var("XDG_DATA_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                home_dir().map(|mut path| {
                    path.push(".local");
                    path.push("share");
                    path
                })
            })
            .map(|mut path| {
                path.push("ox");
                path
            })
    }
}

/// Get the cache directory for the application
pub fn cache_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        // On Windows, use %TEMP%\ox or %LOCALAPPDATA%\ox\cache
        env::var("TEMP")
            .or_else(|_| env::var("TMP"))
            .ok()
            .map(PathBuf::from)
            .map(|mut path| {
                path.push("ox");
                path
            })
            .or_else(|| {
                data_dir().map(|mut path| {
                    path.push("cache");
                    path
                })
            })
    }
    
    #[cfg(target_os = "macos")]
    {
        // On macOS, use ~/Library/Caches/ox
        home_dir().map(|mut path| {
            path.push("Library");
            path.push("Caches");
            path.push("ox");
            path
        })
    }
    
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        // On Linux, respect XDG Base Directory spec
        env::var("XDG_CACHE_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                home_dir().map(|mut path| {
                    path.push(".cache");
                    path
                })
            })
            .map(|mut path| {
                path.push("ox");
                path
            })
    }
}

/// Expand tilde (~) in paths to the home directory
pub fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with("~/") || path == "~" {
        if let Some(home) = home_dir() {
            if path == "~" {
                return home;
            }
            let path_without_tilde = &path[2..];
            return home.join(path_without_tilde);
        }
    }
    PathBuf::from(path)
}

/// Normalize a path to use the correct separators for the current platform
pub fn normalize_path(path: &Path) -> PathBuf {
    // PathBuf automatically handles platform-specific separators
    path.to_path_buf()
}

/// Join path components using the platform-specific separator
pub fn join_paths<I, P>(components: I) -> PathBuf
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    let mut result = PathBuf::new();
    for component in components {
        result.push(component);
    }
    result
}

/// Get the platform-specific path separator as a string
pub fn path_separator() -> &'static str {
    #[cfg(target_os = "windows")]
    { "\\" }
    
    #[cfg(not(target_os = "windows"))]
    { "/" }
}

/// Create all parent directories for a path if they don't exist
pub fn ensure_parent_dirs(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

/// Create a directory if it doesn't exist
pub fn ensure_dir(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_expand_tilde() {
        let expanded = expand_tilde("~/test/file.txt");
        assert!(!expanded.to_string_lossy().contains("~"));
        
        let expanded_home = expand_tilde("~");
        assert_eq!(expanded_home, home_dir().unwrap());
        
        let no_tilde = expand_tilde("/absolute/path");
        assert_eq!(no_tilde, PathBuf::from("/absolute/path"));
    }
    
    #[test]
    fn test_join_paths() {
        let path = join_paths(&["home", "user", "file.txt"]);
        let path_str = path.to_string_lossy();
        
        #[cfg(target_os = "windows")]
        assert!(path_str.contains("\\"));
        
        #[cfg(not(target_os = "windows"))]
        assert!(path_str.contains("/"));
    }
}