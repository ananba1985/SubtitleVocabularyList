function Invoke-SvlCargoCacheMaintenance {
    param([switch]$CheckOnly)
    $svlCacheArguments = @('-X', 'utf8', (Join-Path $PSScriptRoot 'cargo-cache.py'))
    if ($CheckOnly) { $svlCacheArguments += '--check-only' }
    & python @svlCacheArguments
    if ($LASTEXITCODE -ne 0) { Write-Warning 'Cargo cache maintenance deferred; existing development work is retained.' }
}
