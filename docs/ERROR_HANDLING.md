# Error Handling Best Practices for Ox Editor

## Overview
Proper error handling is crucial for creating a robust and reliable text editor. This document outlines the error handling practices used in the Ox editor codebase.

## Core Principles

### 1. Never Use `unwrap()` in Production Code
- **DON'T**: `let doc = self.files.get(ptr).unwrap();`
- **DO**: `let doc = self.files.get(ptr).ok_or(OxError::DocumentNotFound { index: 0 })?;`

### 2. Use Descriptive Error Messages with `expect()`
When `expect()` is necessary (rare cases):
- **DON'T**: `config.borrow().expect("")`
- **DO**: `config.borrow().expect("Failed to borrow terminal config - config should be initialized")`

### 3. Prefer `?` Operator for Error Propagation
The `?` operator provides clean error propagation:
```rust
pub fn render(&mut self) -> Result<()> {
    self.terminal.start()?;
    self.draw_content()?;
    self.terminal.flush()?;
    Ok(())
}
```

## Error Type System

### Custom Error Type
Ox uses a custom error type defined in `src/error.rs`:

```rust
use error_set::error_set;

error_set! {
    OxError = {
        #[display("Document not found at index {}", index)]
        DocumentNotFound { index: usize },
        
        #[display("Invalid color format: {}", color)]
        InvalidColor { color: String },
        
        #[display("Terminal configuration error: {}", msg)]
        TerminalConfig { msg: String },
        
        // ... other error variants
    };
}

pub type Result<T> = std::result::Result<T, OxError>;
```

### Error Conversion
Use `map_err` to convert between error types:
```rust
let cfg = self.config.borrow::<TerminalConfig>()
    .map_err(|_| OxError::TerminalConfig { 
        msg: "Failed to borrow terminal config".to_string() 
    })?;
```

## Lazy Initialization for Regex

### Problem
Regex compilation can panic if the pattern is invalid:
```rust
// BAD: This will panic at runtime if pattern is invalid
let re = Regex::new(pattern).expect("Invalid regex");
```

### Solution
Use `once_cell` for lazy, safe initialization:
```rust
use once_cell::sync::Lazy;

static ANSI_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(PATTERN).unwrap_or_else(|e| {
        eprintln!("WARNING: Failed to compile regex: {}", e);
        // Return a regex that never matches as fallback
        Regex::new("(?!)").expect("Fallback regex should compile")
    })
});
```

## Handling Option Types

### When Methods Return Option
```rust
// For methods that might not find an element
pub fn get_highlighter(&mut self, idx: usize) -> Option<&mut Highlighter> {
    self.files.get_atom_mut(self.ptr.clone())
        .and_then(|(fcs, _)| fcs.get_mut(idx))
        .map(|fc| &mut fc.highlighter)
}

// Usage
if let Some(highlighter) = editor.get_highlighter(0) {
    highlighter.run(&lines);
}
```

### Converting Option to Result
When you need to convert `Option` to `Result`:
```rust
let doc = self.try_doc()
    .ok_or_else(|| OxError::DocumentNotFound { index: 0 })?;
```

## Safe Fallbacks

### Provide Sensible Defaults
```rust
// Instead of panicking on missing value
let line_count = doc.lines.len();

// Use unwrap_or_default for safe defaults
let current_line = doc.line(y).unwrap_or_default();
```

### Graceful Degradation
For non-critical features, log and continue:
```rust
if let Err(e) = self.update_syntax_highlighting() {
    eprintln!("Warning: Syntax highlighting failed: {}", e);
    // Continue without syntax highlighting
}
```

## Testing Error Paths

### Unit Tests for Error Cases
```rust
#[test]
fn test_document_not_found() {
    let mut editor = Editor::new();
    let result = editor.get_document(999);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), OxError::DocumentNotFound { .. }));
}
```

### Integration Tests with Invalid Input
```rust
#[test]
fn test_malformed_config() {
    let config = "invalid { json }";
    let result = Config::parse(config);
    assert!(result.is_err());
}
```

## CI/CD Integration

### Automated Checks
The project includes a GitHub Actions workflow (`.github/workflows/check-unwrap.yml`) that:
1. Scans for `unwrap()` calls in non-test code
2. Checks for `expect()` calls with insufficient context
3. Fails the build if unwrap() is found in production code

### Pre-commit Hooks
Consider adding a pre-commit hook:
```bash
#!/bin/bash
# .git/hooks/pre-commit

# Check for unwrap() in staged files
if git diff --cached --name-only | grep '\.rs$' | xargs grep '\.unwrap()' | grep -v test; then
    echo "Error: Found unwrap() in non-test code"
    exit 1
fi
```

## Common Patterns

### Pattern 1: Document Access
```rust
// Safe document access with proper error handling
let fc = self.files.get(ptr)
    .ok_or_else(|| OxError::DocumentNotFound { index: 0 })?;
let doc = &fc.doc;
```

### Pattern 2: Configuration Access
```rust
// Safe configuration borrowing
let cfg = self.config.borrow::<TerminalConfig>()
    .map_err(|e| OxError::TerminalConfig { 
        msg: format!("Config borrow failed: {}", e) 
    })?;
```

### Pattern 3: File Operations
```rust
// Safe file reading with context
let contents = std::fs::read_to_string(&path)
    .map_err(|e| OxError::Io(e))?;
```

## Migration Guide

### Step 1: Identify Unwraps
```bash
# Find all unwrap calls
grep -r "\.unwrap()" src/ --include="*.rs" | grep -v test
```

### Step 2: Categorize by Risk
- **Critical**: UI rendering, file operations, user input handling
- **Medium**: Configuration parsing, plugin loading
- **Low**: Test code, build scripts

### Step 3: Replace with Proper Handling
1. For functions returning `Result`, use `?` operator
2. For `Option` types, use `ok_or_else` with descriptive errors
3. For initialization, use lazy_static or once_cell
4. For non-critical operations, use `unwrap_or_default`

## Performance Considerations

### Error Path Performance
- Error creation should be cheap (avoid heavy allocations)
- Use `&'static str` for error messages when possible
- Consider using error codes for high-frequency operations

### Lazy Initialization Benefits
- Compile regex patterns once at startup
- Cache expensive computations
- Avoid repeated error handling for the same operation

## Conclusion

Following these error handling practices ensures:
- No unexpected panics in production
- Clear error messages for debugging
- Graceful degradation when possible
- Maintainable and testable code

Remember: **A panic in a text editor means potential data loss for users**. Always handle errors properly!