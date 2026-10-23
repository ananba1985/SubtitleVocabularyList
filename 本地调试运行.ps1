param([string]$DataDirectory = 'D:\SubtitleVocabularyList\Data', [switch]$CheckOnly, [string]$Configuration = '', [switch]$Incremental)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'scripts/development-environment.ps1')
. (Join-Path $PSScriptRoot 'scripts/development-cleanup.ps1')
. (Join-Path $PSScriptRoot 'scripts/build-preflight.ps1')
. (Join-Path $PSScriptRoot 'scripts/cargo-cache.ps1')
$svlEnvironment = Get-SvlDevelopmentEnvironment -DataDirectory $DataDirectory
$svlDebugConfig = if ($Configuration) { (Resolve-Path -LiteralPath $Configuration).Path } else { $svlEnvironment.Config }
$svlPreviousPath = $env:Path
$svlPreviousData = $env:SVL_DATA_DIR
Push-Location $svlEnvironment.Workspace
try {
    $env:Path = $svlEnvironment.Path
    $env:SVL_DATA_DIR = $svlEnvironment.DataDirectory
    Show-SvlDevelopmentEnvironment $svlEnvironment
    if (-not $CheckOnly) { Assert-SvlCargoBuildIdle -Workspace $svlEnvironment.Workspace }
    try {
        Invoke-SvlDevelopmentCleanup -Workspace $svlEnvironment.Workspace -DataDirectory $svlEnvironment.DataDirectory -CheckOnly:$CheckOnly
    } catch {
        Write-Warning "Temporary cleanup deferred: $($_.Exception.Message)"
    }
    if ($CheckOnly) {
        Write-Host 'Plan: prepare the pinned libmpv runtime if it is missing.'
        Write-Host 'Plan: verify no process is running from this Cargo build directory.'
        Write-Host 'Plan: retire previous-version application caches; reuse current caches and dependencies.'
        Write-Host "Plan: pnpm tauri dev --config $svlDebugConfig"
        return
    }
    if (-not (Test-Path -LiteralPath (Join-Path $PSScriptRoot '.tools/runtime/mpv/libmpv-2.dll'))) {
        & (Join-Path $PSScriptRoot 'scripts/setup-mpv-development.ps1')
    }
    # Incremental builds are now the default; keep -Incremental for old launchers.
    Invoke-SvlCargoCacheMaintenance
    & pnpm tauri dev --config $svlDebugConfig
    if ($LASTEXITCODE -ne 0) { throw "Development run failed ($LASTEXITCODE)." }
} finally {
    $env:Path = $svlPreviousPath
    $env:SVL_DATA_DIR = $svlPreviousData
    Pop-Location
}
