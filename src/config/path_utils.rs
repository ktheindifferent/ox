/// Path utility functions exposed to Lua
use crate::dirs;
use mlua::prelude::*;
use std::path::{Path, PathBuf, MAIN_SEPARATOR};

pub struct PathUtils;

impl LuaUserData for PathUtils {
    fn add_fields<F: LuaUserDataFields<Self>>(_fields: &mut F) {}

    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        // Get the platform path separator
        methods.add_function("separator", |_, ()| {
            Ok(MAIN_SEPARATOR.to_string())
        });

        // Check if running on Windows
        methods.add_function("is_windows", |_, ()| {
            Ok(cfg!(target_os = "windows"))
        });

        // Normalize path separators for the current platform
        methods.add_function("normalize", |_, path: String| {
            let path = PathBuf::from(path);
            Ok(path.to_string_lossy().to_string())
        });

        // Join path components
        methods.add_function("join", |_, parts: Vec<String>| {
            let mut path = PathBuf::new();
            for part in parts {
                path.push(part);
            }
            Ok(path.to_string_lossy().to_string())
        });

        // Get the directory part of a path
        methods.add_function("dirname", |_, path: String| {
            let path = Path::new(&path);
            Ok(path.parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| ".".to_string()))
        });

        // Get the filename part of a path
        methods.add_function("basename", |_, path: String| {
            let path = Path::new(&path);
            Ok(path.file_name()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string_lossy().to_string()))
        });

        // Expand home directory in paths
        methods.add_function("expand_home", |_, path: String| {
            let expanded = dirs::expand_tilde(&path);
            Ok(expanded.to_string_lossy().to_string())
        });

        // Make a path absolute
        methods.add_function("absolute", |_, path: String| {
            let expanded = dirs::expand_tilde(&path);
            let absolute = if expanded.is_absolute() {
                expanded
            } else {
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(expanded)
            };
            Ok(absolute.to_string_lossy().to_string())
        });

        // Check if a path exists
        methods.add_function("exists", |_, path: String| {
            let expanded = dirs::expand_tilde(&path);
            Ok(expanded.exists())
        });

        // Check if a path is a file
        methods.add_function("is_file", |_, path: String| {
            let expanded = dirs::expand_tilde(&path);
            Ok(expanded.is_file())
        });

        // Check if a path is a directory
        methods.add_function("is_dir", |_, path: String| {
            let expanded = dirs::expand_tilde(&path);
            Ok(expanded.is_dir())
        });

        // Get file extension
        methods.add_function("extension", |_, path: String| {
            let path = Path::new(&path);
            Ok(path.extension()
                .and_then(|ext| ext.to_str())
                .map(|s| s.to_string()))
        });

        // Remove file extension
        methods.add_function("remove_extension", |_, path: String| {
            let path = Path::new(&path);
            let without_ext = path.with_extension("");
            Ok(without_ext.to_string_lossy().to_string())
        });

        // Get home directory
        methods.add_function("home_dir", |_, ()| {
            Ok(dirs::home_dir()
                .map(|p| p.to_string_lossy().to_string()))
        });

        // Get config directory
        methods.add_function("config_dir", |_, ()| {
            Ok(dirs::config_dir()
                .map(|p| p.to_string_lossy().to_string()))
        });

        // Get data directory
        methods.add_function("data_dir", |_, ()| {
            Ok(dirs::data_dir()
                .map(|p| p.to_string_lossy().to_string()))
        });

        // Get cache directory
        methods.add_function("cache_dir", |_, ()| {
            Ok(dirs::cache_dir()
                .map(|p| p.to_string_lossy().to_string()))
        });

        // Create directory with all parents
        methods.add_function("ensure_dir", |_, path: String| {
            let expanded = dirs::expand_tilde(&path);
            match dirs::ensure_dir(&expanded) {
                Ok(()) => Ok(true),
                Err(_) => Ok(false),
            }
        });

        // Create parent directories for a file path
        methods.add_function("ensure_parent_dirs", |_, path: String| {
            let expanded = dirs::expand_tilde(&path);
            match dirs::ensure_parent_dirs(&expanded) {
                Ok(()) => Ok(true),
                Err(_) => Ok(false),
            }
        });
    }
}

/// Register path utilities with Lua
pub fn register_path_utils(lua: &Lua) -> LuaResult<()> {
    let globals = lua.globals();
    let path_utils = PathUtils;
    globals.set("path_utils", path_utils)?;
    Ok(())
}