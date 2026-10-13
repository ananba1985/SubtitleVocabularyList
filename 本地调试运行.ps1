param([string]$DataDirectory = 'D:\SubtitleVocabularyList\Data', [switch]$CheckOnly)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'scripts/development-environment.ps1')
. (Join-Path $PSScriptRoot 'scripts/development-cleanup.ps1')
$svlEnvironment = Get-SvlDevelopmentEnvironment -DataDirectory $DataDirectory
$svlPreviousPath = $env:Path
$svlPreviousData = $env:SVL_DATA_DIR
Push-Location $svlEnvironment.Workspace
try {
    $env:Path = $svlEnvironment.Path
    $env:SVL_DATA_DIR = $svlEnvironment.DataDirectory
    Show-SvlDevelopmentEnvironment $svlEnvironment
    try {
        Invoke-SvlDevelopmentCleanup -Workspace $svlEnvironment.Workspace -DataDirectory $svlEnvironment.DataDirectory -CheckOnly:$CheckOnly
    } catch {
        Write-Warning "Temporary cleanup deferred: $($_.Exception.Message)"
    }
    if ($CheckOnly) {
        Write-Host 'Plan: cargo clean --manifest-path src-tauri/Cargo.toml'
        Write-Host 'Plan: pnpm tauri dev --config src-tauri/tauri.development.conf.json'
        return
    }
    Write-Host 'Cleaning previous Cargo build artifacts before compilation...'
    & cargo clean --manifest-path (Join-Path $svlEnvironment.Workspace 'src-tauri/Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw 'Cargo cleanup failed; development compilation was not started.' }
    & pnpm tauri dev --config $svlEnvironment.Config
    if ($LASTEXITCODE -ne 0) { throw "Development run failed ($LASTEXITCODE)." }
} finally {
    $env:Path = $svlPreviousPath
    $env:SVL_DATA_DIR = $svlPreviousData
    Pop-Location
}
