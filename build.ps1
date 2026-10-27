# Builds the Tunepeek installer. Run from the repo root:  .\build.ps1
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "Rust was not found. Install it from https://rustup.rs (with the MSVC build tools) and re-run."
}
if (-not (cargo install --list | Select-String "tauri-cli")) {
    Write-Host "Installing tauri-cli (one time)..." -ForegroundColor Cyan
    cargo install tauri-cli --version "^2" --locked
}

Push-Location src-tauri
try { cargo tauri build } finally { Pop-Location }

$setup = Get-ChildItem "src-tauri\target\release\bundle\nsis\*-setup.exe" |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $setup) { throw "Build finished but no installer was found." }

Write-Host "`nInstaller: $($setup.FullName)" -ForegroundColor Green
if ((Read-Host "Run it now? (y/N)") -eq "y") { Start-Process $setup.FullName }
