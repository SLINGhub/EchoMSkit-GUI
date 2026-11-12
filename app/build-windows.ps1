[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$RepoRoot = Split-Path -Parent $PSScriptRoot
$TauriDir = Join-Path $PSScriptRoot "src-tauri"
$SidecarDir = Join-Path $TauriDir "binaries"

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Program,
        [Parameter(ValueFromRemainingArguments = $true)]
        [string[]] $Arguments
    )

    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "'$Program $($Arguments -join ' ')' failed with exit code $LASTEXITCODE."
    }
}

if ($env:OS -ne "Windows_NT") {
    throw "This script must be run on Windows."
}

foreach ($Program in @("cargo", "rustc")) {
    if (-not (Get-Command $Program -ErrorAction SilentlyContinue)) {
        throw "Required program '$Program' was not found. Install Rust and try again."
    }
}

$HostLine = (& rustc -Vv | Select-String -Pattern "^host:").Line
if (-not $HostLine) {
    throw "Could not determine the Rust host target."
}
$HostTarget = ($HostLine -split ":", 2)[1].Trim()
if ($HostTarget -notin @("x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc")) {
    throw "Unsupported Windows Rust target '$HostTarget'."
}

if (-not (Get-Command "cargo-tauri" -ErrorAction SilentlyContinue)) {
    Write-Host "Installing Tauri CLI 2..."
    Invoke-Checked cargo install tauri-cli --version "^2" --locked
}

Write-Host "Building the EchoMSkit processing engine..."
# static c runtime so the worker runs without the vc++ redistributable, tauri already does this for the gui
$PreviousRustFlags = $env:RUSTFLAGS
$env:RUSTFLAGS = "$PreviousRustFlags -C target-feature=+crt-static".Trim()
try {
    Invoke-Checked cargo build --release --manifest-path (Join-Path $RepoRoot "Cargo.toml")
}
finally {
    $env:RUSTFLAGS = $PreviousRustFlags
}

New-Item -ItemType Directory -Path $SidecarDir -Force | Out-Null
$WorkerSource = Join-Path $RepoRoot "target\release\EchoMSkit.exe"
$WorkerDestination = Join-Path $SidecarDir "EchoMSkit-worker-$HostTarget.exe"
if (-not (Test-Path -LiteralPath $WorkerSource -PathType Leaf)) {
    throw "The EchoMSkit processing engine was not produced at '$WorkerSource'."
}
Copy-Item -LiteralPath $WorkerSource -Destination $WorkerDestination -Force

Write-Host "Generating application icons..."
Invoke-Checked cargo tauri icon (Join-Path $PSScriptRoot "app-icon.svg") --output (Join-Path $TauriDir "icons")

Write-Host "Building the lightweight Windows installer..."
Push-Location $TauriDir
try {
    Invoke-Checked cargo tauri build --bundles nsis
}
finally {
    Pop-Location
}

$BundleDir = Join-Path $TauriDir "target\release\bundle\nsis"
$Installer = Get-ChildItem -LiteralPath $BundleDir -Filter "*-setup.exe" -File |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
if (-not $Installer) {
    throw "The expected Windows setup executable was not found in '$BundleDir'."
}

Write-Host ""
Write-Host "Installer created: $($Installer.FullName)"
Write-Host "WebView2 is downloaded only if it is missing."
