#!/usr/bin/env pwsh
# Regenerate the shipping Windows desktop artifact: build the release binary
# and bundle it with the licenses and docs into a .zip, mirroring the Windows
# packaging in .github/workflows/release.yml.
#
# Usage: scripts/package-desktop.ps1 [-Version <string>]
#   -Version defaults to "dev"; releases use the tag name (e.g. v0.4.0).
#
# Requires a stable Rust toolchain. The first build downloads and embeds the
# IPADIC morphological dictionary (needs network, once).
[CmdletBinding()]
param(
    [string]$Version = "dev"
)
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot   # scripts/ -> repo root

# The Cargo workspace lives under software/; the binary lands in
# software/target/release. Licenses and docs stay at the repo root.
Push-Location (Join-Path $root "software")
try {
    cargo build --release -p shiori-gui
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }
} finally {
    Pop-Location
}

$name = "windows-x86_64"
$archive = Join-Path $root "shiori-$Version-$name.zip"
$items = @(
    Join-Path $root "software/target/release/shiori.exe"
    Join-Path $root "LICENSE-MIT"
    Join-Path $root "LICENSE-APACHE"
    Join-Path $root "README.md"
    Join-Path $root "CHANGELOG.md"
)
Compress-Archive -Force -Path $items -DestinationPath $archive

Write-Host "Built $archive"
