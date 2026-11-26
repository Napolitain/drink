#Requires -Version 7.3
<#
.SYNOPSIS
Build Drink for Windows.

.DESCRIPTION
Without options, builds target\Drink Dev.exe. -Dev also opens it. -SmokeTest
builds and runs the interactive smoke test, which exits with 77 when the
session cannot show a break.
#>
[CmdletBinding()]
param([switch]$Dev, [switch]$SmokeTest)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
Set-Location (Split-Path -Parent $PSScriptRoot)
# Keep outputs in this checkout even with a global Cargo target dir.
$env:CARGO_TARGET_DIR = Join-Path $PWD 'target'
$target = $env:CARGO_TARGET_DIR

function Install-Executable([string]$From, [string]$To) {
    try {
        Copy-Item -LiteralPath $From -Destination $To -Force
    } catch {
        throw "Cannot replace $To. Quit it first, then build again."
    }
}

if ($SmokeTest) {
    cargo build --locked --release -p drink-windows --bin drink-smoke
    $exe = Join-Path $target 'DrinkSmoke.exe'
    Install-Executable (Join-Path $target 'release\drink-smoke.exe') $exe
    # Pass the smoke test's own exit code through, including 77 for a skip.
    $PSNativeCommandUseErrorActionPreference = $false
    & $exe
    exit $LASTEXITCODE
}

cargo build --locked --release -p drink-windows --bin drink
$exe = Join-Path $target 'Drink Dev.exe'
Install-Executable (Join-Path $target 'release\drink.exe') $exe
$exe
if ($Dev) {
    Start-Process -FilePath $exe
}
