#!/usr/bin/env pwsh
# Core-duplication drift check (ADR-0003 / issue #7).
# Comments are ASCII-only on purpose: Windows PowerShell 5.1 reads a BOM-less script in the
# system ANSI codepage, which corrupts non-ASCII bytes and breaks parsing. Keeping this file
# ASCII makes it parse under any locale (PowerShell 5.1 or 7). The prose rationale lives in
# docs/decisions/0003-core-duplication-sync.md.
#
# This script is the AUTHORITATIVE list of shared files (the ADR-0003 table mirrors it for
# human rationale; keep the two in sync). Two classes:
#   Class A (mirror)   - must stay content-identical; drift fails the run (exit 1)
#   Class B (diverged) - divergence is intentional; informational only (manual reconcile)
# Assumes both repos exist locally (this is a pre-push local gate, not a CI check).
#
# Comparison normalizes line endings (CRLF -> LF) before hashing, so a pure EOL difference
# between the two working trees does NOT show up as drift (avoids false positives).
#
# Usage:
#   pwsh scripts/check-core-drift.ps1
#   pwsh scripts/check-core-drift.ps1 -DesktopRepo D:\path\to\simpomo
#
# Exit codes: 0 = mirrors identical / 1 = mirror drift or missing / 2 = desktop repo not found

[CmdletBinding()]
param(
    [string]$DesktopRepo = "C:\Dev\simpomo"
)

$ErrorActionPreference = "Stop"
$androidRepo = Split-Path -Parent $PSScriptRoot   # parent of scripts/ = repo root

# Class A: mirrors kept content-identical (authoritative list; ADR-0003 table must match this).
$mirror = @(
    "src-tauri/src/stats.rs",
    "src/lib/sounds.ts",
    "src/lib/bgm.ts",
    "src/lib/timer.ts",
    "src/lib/stats.ts",
    "src/lib/audio.ts",
    "src/lib/color.ts"
)
# Class B: diverged by design (differences allowed; shared core logic reconciled by hand).
$diverged = @(
    "src-tauri/src/timer.rs",
    "src-tauri/src/settings.rs",
    "src/lib/settings.ts",
    "src/lib/notify.ts"
)

# SHA256 of the file content with line endings normalized to LF. Returns $null if missing.
function Get-NormalizedSha($path) {
    if (-not (Test-Path -LiteralPath $path)) { return $null }
    $text = [System.IO.File]::ReadAllText($path)
    $text = $text -replace "`r`n", "`n"
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($text)
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        return [System.BitConverter]::ToString($sha.ComputeHash($bytes)).Replace("-", "")
    } finally {
        $sha.Dispose()
    }
}

if (-not (Test-Path -LiteralPath $DesktopRepo)) {
    Write-Host "Desktop repo not found: $DesktopRepo  (override with -DesktopRepo <path>)" -ForegroundColor Red
    exit 2
}

$drift = 0
Write-Host "== Class A: mirror (must be content-identical) ==" -ForegroundColor Cyan
foreach ($f in $mirror) {
    $ha = Get-NormalizedSha (Join-Path $androidRepo $f)
    $hd = Get-NormalizedSha (Join-Path $DesktopRepo $f)
    if ($null -eq $ha -or $null -eq $hd) {
        Write-Host ("  MISSING   {0}" -f $f) -ForegroundColor Red
        $drift++
    } elseif ($ha -eq $hd) {
        Write-Host ("  OK        {0}" -f $f) -ForegroundColor Green
    } else {
        Write-Host ("  DRIFT     {0}" -f $f) -ForegroundColor Red
        $drift++
    }
}

Write-Host ""
Write-Host "== Class B: diverged by design (informational) ==" -ForegroundColor Cyan
foreach ($f in $diverged) {
    $ha = Get-NormalizedSha (Join-Path $androidRepo $f)
    $hd = Get-NormalizedSha (Join-Path $DesktopRepo $f)
    if ($null -eq $ha -or $null -eq $hd) {
        Write-Host ("  MISSING   {0}" -f $f) -ForegroundColor Yellow
    } elseif ($ha -eq $hd) {
        Write-Host ("  same      {0}" -f $f)
    } else {
        Write-Host ("  diff      {0}  (reconcile shared core logic manually)" -f $f) -ForegroundColor Yellow
    }
}

Write-Host ""
if ($drift -gt 0) {
    Write-Host "FAIL: $drift mirror file(s) drifted from $DesktopRepo. Copy them across to restore identity (ADR-0003)." -ForegroundColor Red
    exit 1
}
Write-Host "OK: all Class A mirror files are content-identical with $DesktopRepo." -ForegroundColor Green
exit 0
