param([switch]$CheckOnly)
$ErrorActionPreference = 'Stop'
try {
    . (Join-Path $PSScriptRoot 'scripts/cargo-cache.ps1')
    Invoke-SvlCargoCacheMaintenance -CheckOnly:$CheckOnly
    exit $LASTEXITCODE
} catch {
    Write-Host ('缓存维护未完成：' + $_.Exception.Message) -ForegroundColor Red
    exit 1
}