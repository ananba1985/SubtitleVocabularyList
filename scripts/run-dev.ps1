param([string]$DataDirectory = '.local\dev-data', [switch]$CheckOnly)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'development-environment.ps1')
$svlEnvironment = Get-SvlDevelopmentEnvironment -DataDirectory $DataDirectory
$svlPreviousPath = $env:Path
$svlPreviousData = $env:SVL_DATA_DIR
Push-Location $svlEnvironment.Workspace
try {
    $env:Path = $svlEnvironment.Path
    $env:SVL_DATA_DIR = $svlEnvironment.DataDirectory
    Show-SvlDevelopmentEnvironment $svlEnvironment
    if ($CheckOnly) { Write-Host 'Plan: pnpm tauri dev --config src-tauri/tauri.development.conf.json'; return }
    & pnpm tauri dev --config $svlEnvironment.Config
    if ($LASTEXITCODE -ne 0) { throw "Development run failed ($LASTEXITCODE)." }
} finally {
    $env:Path = $svlPreviousPath
    $env:SVL_DATA_DIR = $svlPreviousData
    Pop-Location
}
