# Cross-Platform Path Handling Guide for Ox Plugin Developers

## Overview
When developing plugins for Ox, it's crucial to ensure your code works correctly across Windows, macOS, and Linux. The main challenge is handling file paths, which use different separators on different platforms.

## Platform Differences
- **Unix/Linux/macOS**: Use forward slashes (`/`) as path separators
- **Windows**: Uses backslashes (`\`) as path separators

## Best Practices

### 1. Use the `build_path()` Function
Ox provides a built-in `build_path()` function in `bootstrap.lua` that automatically uses the correct path separator for the current platform.

```lua
-- ❌ Don't do this - hardcoded separator
local config_file = home .. "/.oxrc"
local script_path = plugin_path .. "/myscript.py"

-- ✅ Do this instead - cross-platform
local config_file = build_path(home, ".oxrc")
local script_path = build_path(plugin_path, "myscript.py")
```

### 2. Use `path_sep` Variable
The global `path_sep` variable contains the correct path separator for the current platform:

```lua
-- Get the platform-specific separator
local separator = path_sep  -- Will be "/" on Unix, "\" on Windows
```

### 3. Use `plugin_path` Variable
The global `plugin_path` variable is automatically set to the correct configuration directory for the current platform:
- Windows: `%APPDATA%\ox` or `%USERPROFILE%\ox`
- Unix/Linux: `$XDG_CONFIG_HOME/ox` or `~/.config/ox`
- macOS: `~/.config/ox`

### 4. Platform Detection
You can detect the current platform using:

```lua
-- Using package.config (first character is path separator)
local is_windows = package.config:sub(1,1) == '\\'

-- Using shell.is_windows
if shell.is_windows then
    -- Windows-specific code
else
    -- Unix/Linux/macOS code
end
```

## Common Patterns

### Creating Plugin Files
```lua
-- Create a plugin-specific file
local my_plugin_file = build_path(plugin_path, "my_plugin_data.txt")
local file = io.open(my_plugin_file, "w")
if file then
    file:write("data")
    file:close()
end
```

### Checking File Existence
```lua
-- Use the provided file_exists function
local script_path = build_path(plugin_path, "script.py")
if not file_exists(script_path) then
    -- Create the file
end
```

### Working with Git Paths
```lua
-- When combining repository paths with file paths
local repo_path = "/home/user/myrepo"  -- From git command
local file_path = "src/main.rs"        -- From git status
local full_path = build_path(repo_path, file_path)
```

### Executing External Commands
```lua
-- Quote paths properly for shell commands
local script_path = build_path(plugin_path, "script.py")
local command = string.format('python "%s" "%s"', script_path, argument)
local output = shell:output(command)
```

## Directory Operations

### Creating Directories
```lua
-- Create a directory cross-platform
if not dir_exists(plugin_path) then
    local command
    if shell.is_windows then
        command = 'mkdir "' .. plugin_path .. '"'
    else
        command = 'mkdir -p "' .. plugin_path .. '"'
    end
    shell:run(command)
end
```

### Listing Directory Contents
The `dir_exists()` function is provided and works cross-platform.

## Examples from Core Plugins

### From emmet.lua
```lua
-- Correct cross-platform approach
local script_path = build_path(plugin_path, "oxemmet.py")
local command = string.format('python "%s" "%s"', script_path, unexpanded)
```

### From git.lua
```lua
-- Combining paths from git output
local file_name = build_path(repo_path, line:sub(4))
```

### From live_html.lua
```lua
-- Creating plugin scripts
local livehtml_script_path = build_path(plugin_path, "livehtml.py")
if not file_exists(livehtml_script_path) then
    local file = io.open(livehtml_script_path, "w")
    -- Write script content
end
```

## Testing Your Plugin

To ensure your plugin works on all platforms:

1. **Test path construction**: Verify paths are built correctly
2. **Test file operations**: Ensure files can be created, read, and deleted
3. **Test external commands**: Verify shell commands work with proper quoting
4. **Test on different platforms**: If possible, test on Windows, macOS, and Linux

## Migration Checklist

When updating existing plugins for cross-platform compatibility:

- [ ] Replace all hardcoded `/` or `\` in paths with `build_path()`
- [ ] Replace string concatenation for paths (e.g., `path .. "/file"`) with `build_path(path, "file")`
- [ ] Use `plugin_path` instead of hardcoded config directories
- [ ] Test file operations on Windows and Unix-like systems
- [ ] Properly quote file paths in shell commands
- [ ] Use platform detection for OS-specific features

## Summary

By following these guidelines and using the provided utility functions, your Ox plugins will work seamlessly across all supported platforms. The key is to never hardcode path separators and always use the provided cross-platform utilities.