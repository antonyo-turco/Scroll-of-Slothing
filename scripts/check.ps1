# Full local quality gate. Run from anywhere: pwsh scripts/check.ps1 [-Quick] [-NoE2E]
#   -Quick  skip the Tauri app crate (slow to compile) and Playwright
#   -NoE2E  skip Playwright only
param([switch]$Quick, [switch]$NoE2E)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

function Step($name, [scriptblock]$cmd) {
    Write-Host "==> $name" -ForegroundColor Cyan
    & $cmd
    if ($LASTEXITCODE -ne 0) { Write-Host "FAILED: $name" -ForegroundColor Red; exit $LASTEXITCODE }
}

Step 'cargo fmt' { cargo fmt --all -- --check }
if ($Quick) {
    Step 'cargo clippy (crates)' { cargo clippy --all-targets -- -D warnings }
    Step 'cargo test (crates)' { cargo test }
} else {
    Step 'cargo clippy (workspace)' { cargo clippy --workspace --all-targets -- -D warnings }
    Step 'cargo test (workspace)' { cargo test --workspace }
}

Set-Location (Join-Path $root 'app')
if (-not (Test-Path node_modules)) { Step 'npm ci' { npm ci } }
Step 'svelte-check' { npm run check }
Step 'vitest' { npm test }
if (-not ($Quick -or $NoE2E)) { Step 'playwright' { npx playwright test --reporter=line } }

Write-Host 'All checks passed.' -ForegroundColor Green
