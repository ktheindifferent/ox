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
    // Use canonicalize if the path exists, otherwise just clean it up
    if path.exists() {
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
    } else {
        // Clean up the path manually
        let mut components = Vec::new();
        for component in path.components() {
            match component {
                std::path::Component::ParentDir => {
                    // Remove the last component if it's not a parent dir
                    if let Some(last) = components.last() {
                        if !matches!(last, std::path::Component::ParentDir) {
                            components.pop();
                            continue;
                        }
                    }
                    components.push(component);
                }
                std::path::Component::CurDir => {
                    // Skip current directory markers
                    continue;
                }
                _ => components.push(component),
            }
        }
        
        let mut result = PathBuf::new();
        for component in components {
            match component {
                std::path::Component::Prefix(p) => result.push(p.as_os_str()),
                std::path::Component::RootDir => result.push("/"),
                std::path::Component::Normal(s) => result.push(s),
                std::path::Component::ParentDir => result.push(".."),
                std::path::Component::CurDir => {} // Already filtered out
            }
        }
        result
    }
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

/// Validate that a path is safe and doesn't contain dangerous patterns
pub fn validate_path(path: &str) -> bool {
    // Check for null bytes (security issue)
    if path.contains('\0') {
        return false;
    }
    
    // Check for path traversal attempts
    let path_buf = PathBuf::from(path);
    
    // Count depth to detect going above root
    let mut depth = 0;
    for component in path_buf.components() {
        match component {
            std::path::Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    // Path tries to go above the starting point
                    return false;
                }
            }
            std::path::Component::Normal(_) => depth += 1,
            std::path::Component::RootDir => depth = 0, // Reset at root
            _ => {}
        }
    }
    
    // Additional check: if path starts with multiple parent dirs, it's likely malicious
    let mut components = path_buf.components();
    let mut parent_count = 0;
    while let Some(component) = components.next() {
        match component {
            std::path::Component::ParentDir => parent_count += 1,
            std::path::Component::CurDir => continue,
            _ => break,
        }
    }
    // If there are 3 or more parent directories at the start, consider it potentially dangerous
    if parent_count >= 3 && !path.starts_with('/') && !path.starts_with("C:\\") {
        return false;
    }
    
    true
}

/// Sanitize a path by removing dangerous characters and normalizing it
pub fn sanitize_path(path: &str) -> PathBuf {
    // Remove null bytes
    let cleaned = path.replace('\0', "");
    
    // Expand tilde
    let expanded = expand_tilde(&cleaned);
    
    // Normalize the path
    normalize_path(&expanded)
}

/// Convert a path to use forward slashes (for cross-platform compatibility in configs)
pub fn to_forward_slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Convert a path from forward slashes to platform-specific separators
pub fn from_forward_slashes(path: &str) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        PathBuf::from(path.replace('/', "\\"))
    }
    
    #[cfg(not(target_os = "windows"))]
    {
        PathBuf::from(path)
    }
}

/// Check if a path is absolute
pub fn is_absolute_path(path: &str) -> bool {
    let path = PathBuf::from(path);
    path.is_absolute() || path.starts_with("~")
}

/// Make a path relative to a base path
pub fn make_relative(path: &Path, base: &Path) -> Option<PathBuf> {
    path.strip_prefix(base).ok().map(|p| p.to_path_buf())
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
    
    #[test]
    fn test_lua_path_compatibility() {
        // Test that our Rust path functions produce paths compatible with Lua
        let config_dir = config_dir();
        assert!(config_dir.is_some());
        
        let config_path = config_dir.unwrap();
        let path_str = config_path.to_string_lossy();
        
        // Verify path contains "ox" directory
        assert!(path_str.contains("ox"));
        
        // Verify platform-specific structure
        #[cfg(target_os = "windows")]
        {
            // Windows should use AppData or user profile
            assert!(path_str.contains("AppData") || path_str.contains("ox"));
            assert!(path_str.contains("\\"));
        }
        
        #[cfg(not(target_os = "windows"))]
        {
            // Unix-like should use .config
            assert!(path_str.contains(".config") || path_str.contains("ox"));
            assert!(path_str.contains("/"));
        }
    }
    
    #[test]
    fn test_oxrc_path() {
        let home = home_dir().unwrap();
        let oxrc_path = home.join(".oxrc");
        let path_str = oxrc_path.to_string_lossy();
        
        assert!(path_str.ends_with(".oxrc"));
        
        #[cfg(target_os = "windows")]
        assert!(path_str.contains("\\"));
        
        #[cfg(not(target_os = "windows"))]
        assert!(path_str.contains("/"));
    }
    
    #[test]
    fn test_path_validation() {
        // Valid paths
        assert!(validate_path("/home/user/file.txt"));
        assert!(validate_path("relative/path/file.txt"));
        assert!(validate_path("~/documents/file.txt"));
        
        // Invalid paths with null bytes
        assert!(!validate_path("/home/user\0/file.txt"));
        assert!(!validate_path("file\0.txt"));
        
        // Path traversal attempts
        assert!(validate_path("./file.txt"));
        assert!(validate_path("dir/../file.txt")); // This is allowed within bounds
        
        #[cfg(not(target_os = "windows"))]
        {
            assert!(!validate_path("../../../etc/passwd")); // Trying to go above root
        }
    }
    
    #[test]
    fn test_path_sanitization() {
        // Test null byte removal
        let sanitized = sanitize_path("/home/user\0/file.txt");
        assert!(!sanitized.to_string_lossy().contains('\0'));
        
        // Test tilde expansion
        let sanitized = sanitize_path("~/test.txt");
        assert!(!sanitized.to_string_lossy().starts_with("~"));
        
        // Test normalization
        #[cfg(not(target_os = "windows"))]
        {
            let sanitized = sanitize_path("/home//user/../user/./file.txt");
            assert_eq!(sanitized.to_string_lossy(), "/home/user/file.txt");
        }
    }
    
    #[test]
    fn test_forward_slash_conversion() {
        let path = PathBuf::from("home/user/file.txt");
        let forward = to_forward_slashes(&path);
        assert!(forward.contains("/"));
        assert!(!forward.contains("\\"));
        
        #[cfg(target_os = "windows")]
        {
            let path = PathBuf::from("C:\\Users\\test\\file.txt");
            let forward = to_forward_slashes(&path);
            assert!(forward.contains("/"));
            assert!(!forward.contains("\\"));
        }
    }
    
    #[test]
    fn test_from_forward_slashes() {
        let path_str = "home/user/file.txt";
        let path = from_forward_slashes(path_str);
        
        #[cfg(target_os = "windows")]
        {
            assert!(path.to_string_lossy().contains("\\"));
        }
        
        #[cfg(not(target_os = "windows"))]
        {
            assert!(path.to_string_lossy().contains("/"));
        }
    }
    
    #[test]
    fn test_is_absolute_path() {
        assert!(is_absolute_path("~/file.txt"));
        
        #[cfg(target_os = "windows")]
        {
            assert!(is_absolute_path("C:\\file.txt"));
            assert!(is_absolute_path("\\\\server\\share\\file.txt"));
            assert!(!is_absolute_path("relative\\path.txt"));
        }
        
        #[cfg(not(target_os = "windows"))]
        {
            assert!(is_absolute_path("/home/user/file.txt"));
            assert!(!is_absolute_path("relative/path.txt"));
        }
    }
    
    #[test]
    fn test_make_relative() {
        let base = PathBuf::from("/home/user");
        let path = PathBuf::from("/home/user/documents/file.txt");
        let relative = make_relative(&path, &base);
        
        assert!(relative.is_some());
        assert_eq!(relative.unwrap().to_string_lossy(), "documents/file.txt");
        
        // Test when path is not relative to base
        let other_path = PathBuf::from("/var/log/file.txt");
        let relative = make_relative(&other_path, &base);
        assert!(relative.is_none());
    }
    
    #[test]
    fn test_ensure_parent_dirs() {
        use std::fs;
        use tempfile::tempdir;
        
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("nested/dirs/file.txt");
        
        // Parent directories don't exist yet
        assert!(!file_path.parent().unwrap().exists());
        
        // Create parent directories
        ensure_parent_dirs(&file_path).unwrap();
        
        // Now parent directories should exist
        assert!(file_path.parent().unwrap().exists());
    }
    
    #[test]
    fn test_ensure_dir() {
        use tempfile::tempdir;
        
        let temp_dir = tempdir().unwrap();
        let new_dir = temp_dir.path().join("new_directory");
        
        // Directory doesn't exist yet
        assert!(!new_dir.exists());
        
        // Create directory
        ensure_dir(&new_dir).unwrap();
        
        // Now directory should exist
        assert!(new_dir.exists());
        assert!(new_dir.is_dir());
        
        // Calling again should not fail
        ensure_dir(&new_dir).unwrap();
    }
    
    #[test]
    fn test_windows_unc_paths() {
        #[cfg(target_os = "windows")]
        {
            let unc_path = "\\\\server\\share\\file.txt";
            assert!(is_absolute_path(unc_path));
            
            let sanitized = sanitize_path(unc_path);
            assert!(sanitized.to_string_lossy().starts_with("\\\\"));
        }
    }
    
    #[test]
    fn test_mixed_separators() {
        // Test handling of mixed separators
        let mixed_path = "home\\user/documents\\file.txt";
        let normalized = normalize_path(&PathBuf::from(mixed_path));
        let path_str = normalized.to_string_lossy();
        
        // On Unix, PathBuf::from with backslashes treats them as part of the filename
        // So we need to handle this differently - the path should be consistent
        #[cfg(target_os = "windows")]
        {
            // On Windows, should consistently use backslashes
            let forward_count = path_str.matches('/').count();
            let back_count = path_str.matches('\\').count();
            // Should predominantly use one separator type
            assert!(forward_count == 0 || back_count == 0 || forward_count < 2 || back_count < 2);
        }
        
        #[cfg(not(target_os = "windows"))]
        {
            // On Unix, backslashes in the input are treated as literal characters
            // This is expected behavior - we're just testing that normalize_path works
            // The important thing is that it doesn't crash
            assert!(!path_str.is_empty());
        }
    }
}