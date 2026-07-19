$ErrorActionPreference = "Stop"

$projectRoot = Split-Path -Parent $PSScriptRoot
$binaryName = if ($IsWindows -or $env:OS -eq "Windows_NT") { "driftguard.exe" } else { "driftguard" }
$binary = Join-Path $projectRoot "target\debug\$binaryName"

Write-Host "Building DriftGuard..." -ForegroundColor Cyan
cargo build --quiet --manifest-path (Join-Path $projectRoot "Cargo.toml")

Write-Host "`n1. Diagnose the fixed example" -ForegroundColor Cyan
Push-Location (Join-Path $projectRoot "examples\fixed-ai-app")
& $binary doctor
Pop-Location

Write-Host "`n2. Show a deployment contract failure" -ForegroundColor Cyan
Push-Location (Join-Path $projectRoot "examples\broken-ai-app")
& $binary check
$brokenStatus = $LASTEXITCODE
Pop-Location
if ($brokenStatus -ne 1) {
    throw "Broken example returned unexpected exit code $brokenStatus"
}

Write-Host "`n3. Show the corrected project passing" -ForegroundColor Cyan
Push-Location (Join-Path $projectRoot "examples\fixed-ai-app")
& $binary check
Pop-Location

Write-Host "`nDemo complete." -ForegroundColor Green
