# Cross-platform build script for Windows
# Run with: .\build.ps1 or powershell -ExecutionPolicy Bypass -File build.ps1

param(
    [Parameter(Position=0)]
    [ValidateSet("build", "test", "package", "install", "clean", "all")]
    [string]$Task = "build",
    
    [Parameter()]
    [ValidateSet("debug", "release")]
    [string]$Configuration = "release",
    
    [Parameter()]
    [switch]$Help
)

# Display help
if ($Help) {
    Write-Host @"
Ox Editor - Windows Build Script

Usage: .\build.ps1 [Task] [-Configuration <debug|release>] [-Help]

Tasks:
  build    - Build the project (default)
  test     - Run tests
  package  - Create distribution packages
  install  - Install ox to system
  clean    - Clean build artifacts
  all      - Build, test, and package

Options:
  -Configuration  Build configuration (debug or release, default: release)
  -Help          Show this help message

Examples:
  .\build.ps1                    # Build release version
  .\build.ps1 build -Configuration debug  # Build debug version
  .\build.ps1 test              # Run tests
  .\build.ps1 all               # Full build pipeline
"@
    exit 0
}

# Set error action preference
$ErrorActionPreference = "Stop"

# Colors for output
function Write-Info { Write-Host $args -ForegroundColor Cyan }
function Write-Success { Write-Host $args -ForegroundColor Green }
function Write-Error { Write-Host $args -ForegroundColor Red }

# Check for required tools
function Test-Command {
    param([string]$Command)
    $null = Get-Command $Command -ErrorAction SilentlyContinue
    return $?
}

function Ensure-Directory {
    param([string]$Path)
    if (!(Test-Path $Path)) {
        New-Item -ItemType Directory -Force -Path $Path | Out-Null
    }
}

# Build function
function Build-Project {
    Write-Info "Building Ox Editor ($Configuration)..."
    
    $buildArgs = @("build")
    if ($Configuration -eq "release") {
        $buildArgs += "--release"
    }
    
    & cargo $buildArgs
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Build failed!"
        exit 1
    }
    
    Write-Success "Build completed successfully!"
    
    # Copy to target directory
    Ensure-Directory "target\dist"
    $exePath = if ($Configuration -eq "release") { 
        "target\release\ox.exe" 
    } else { 
        "target\debug\ox.exe" 
    }
    
    if (Test-Path $exePath) {
        Copy-Item $exePath "target\dist\ox.exe" -Force
        Write-Success "Executable copied to target\dist\ox.exe"
    }
}

# Test function
function Run-Tests {
    Write-Info "Running tests..."
    
    & cargo test --all
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Tests failed!"
        exit 1
    }
    
    Write-Success "All tests passed!"
}

# Package function
function Create-Package {
    Write-Info "Creating distribution packages..."
    
    # Ensure build is up to date
    Build-Project
    
    Ensure-Directory "target\pkgs"
    
    # Create ZIP archive
    $version = (cargo pkgid | Select-String -Pattern "@(.+)$").Matches[0].Groups[1].Value
    $zipName = "ox-$version-windows-x64.zip"
    $zipPath = "target\pkgs\$zipName"
    
    Write-Info "Creating ZIP archive: $zipName"
    
    # Prepare distribution folder
    $distFolder = "target\dist-temp"
    Ensure-Directory $distFolder
    
    # Copy files
    Copy-Item "target\dist\ox.exe" "$distFolder\ox.exe" -Force
    if (Test-Path "README.md") {
        Copy-Item "README.md" "$distFolder\README.md" -Force
    }
    if (Test-Path "LICENSE") {
        Copy-Item "LICENSE" "$distFolder\LICENSE" -Force
    }
    
    # Create config directory structure
    Ensure-Directory "$distFolder\config"
    if (Test-Path "config") {
        Copy-Item "config\*" "$distFolder\config\" -Recurse -Force
    }
    
    # Create ZIP
    Compress-Archive -Path "$distFolder\*" -DestinationPath $zipPath -Force
    
    # Clean up temp folder
    Remove-Item $distFolder -Recurse -Force
    
    Write-Success "Package created: $zipPath"
    
    # Optional: Create MSI installer if WiX is available
    if (Test-Command "candle.exe") {
        Write-Info "WiX detected, creating MSI installer..."
        # MSI creation would go here
    }
    
    # Create Chocolatey package structure
    $chocoFolder = "target\pkgs\chocolatey"
    Ensure-Directory $chocoFolder
    
    # Create nuspec file
    $nuspec = @"
<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://schemas.microsoft.com/packaging/2015/06/nuspec.xsd">
  <metadata>
    <id>ox-editor</id>
    <version>$version</version>
    <title>Ox Editor</title>
    <authors>Curlpipe</authors>
    <projectUrl>https://github.com/curlpipe/ox</projectUrl>
    <description>A simple but flexible text editor</description>
    <tags>editor text-editor terminal cli</tags>
    <licenseUrl>https://github.com/curlpipe/ox/blob/master/LICENSE</licenseUrl>
  </metadata>
  <files>
    <file src="..\..\dist\ox.exe" target="tools" />
  </files>
</package>
"@
    
    $nuspec | Out-File "$chocoFolder\ox-editor.nuspec" -Encoding UTF8
    
    Write-Success "Chocolatey package structure created in $chocoFolder"
}

# Install function
function Install-Ox {
    Write-Info "Installing Ox Editor..."
    
    # Build first if needed
    if (!(Test-Path "target\dist\ox.exe")) {
        Build-Project
    }
    
    # Install location
    $installPath = "$env:LOCALAPPDATA\Programs\ox"
    Ensure-Directory $installPath
    
    # Copy executable
    Copy-Item "target\dist\ox.exe" "$installPath\ox.exe" -Force
    Write-Success "Ox installed to $installPath\ox.exe"
    
    # Add to PATH
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($userPath -notlike "*$installPath*") {
        $newPath = "$userPath;$installPath"
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        Write-Success "Added $installPath to user PATH"
        Write-Info "Please restart your terminal for PATH changes to take effect"
    } else {
        Write-Info "$installPath is already in PATH"
    }
    
    # Create start menu shortcut
    $startMenuPath = "$env:APPDATA\Microsoft\Windows\Start Menu\Programs"
    $shortcutPath = "$startMenuPath\Ox Editor.lnk"
    
    $WshShell = New-Object -ComObject WScript.Shell
    $Shortcut = $WshShell.CreateShortcut($shortcutPath)
    $Shortcut.TargetPath = "$installPath\ox.exe"
    $Shortcut.WorkingDirectory = $installPath
    $Shortcut.Description = "Ox - A simple but flexible text editor"
    $Shortcut.Save()
    
    Write-Success "Start menu shortcut created"
    Write-Success "Installation completed!"
}

# Clean function
function Clean-Build {
    Write-Info "Cleaning build artifacts..."
    
    & cargo clean
    
    if (Test-Path "target\dist") {
        Remove-Item "target\dist" -Recurse -Force
    }
    if (Test-Path "target\pkgs") {
        Remove-Item "target\pkgs" -Recurse -Force
    }
    if (Test-Path "target\dist-temp") {
        Remove-Item "target\dist-temp" -Recurse -Force
    }
    
    Write-Success "Clean completed!"
}

# Main execution
Write-Info "Ox Editor Build Script"
Write-Info "====================="

# Check for Rust/Cargo
if (!(Test-Command "cargo")) {
    Write-Error "Cargo not found! Please install Rust from https://rustup.rs/"
    exit 1
}

# Execute requested task
switch ($Task) {
    "build" {
        Build-Project
    }
    "test" {
        Run-Tests
    }
    "package" {
        Create-Package
    }
    "install" {
        Install-Ox
    }
    "clean" {
        Clean-Build
    }
    "all" {
        Build-Project
        Run-Tests
        Create-Package
        Write-Success "All tasks completed!"
    }
    default {
        Write-Error "Unknown task: $Task"
        exit 1
    }
}

Write-Info "Done!"