param([string]$DataDirectory = '.local\dev-data', [string]$OutputDirectory = '.local\build', [switch]$CheckOnly)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'development-environment.ps1')
$svlEnvironment = Get-SvlDevelopmentEnvironment -DataDirectory $DataDirectory
$svlOutput = if ([IO.Path]::IsPathRooted($OutputDirectory)) { [IO.Path]::GetFullPath($OutputDirectory) } else { [IO.Path]::GetFullPath((Join-Path $svlEnvironment.Workspace $OutputDirectory)) }
$svlPreviousPath = $env:Path
$svlPreviousData = $env:SVL_DATA_DIR
Push-Location $svlEnvironment.Workspace
try {
    $env:Path = $svlEnvironment.Path
    $env:SVL_DATA_DIR = $svlEnvironment.DataDirectory
    Show-SvlDevelopmentEnvironment $svlEnvironment
    Write-Host "Local executable output: $svlOutput"
    if ($CheckOnly) { Write-Host 'Plan: pnpm tauri build --debug --no-bundle --config src-tauri/tauri.development.conf.json'; return }
    & pnpm tauri build --debug --no-bundle --config $svlEnvironment.Config
    if ($LASTEXITCODE -ne 0) { throw "Local executable build failed ($LASTEXITCODE)." }
    $svlMetadata = (& cargo metadata --manifest-path src-tauri/Cargo.toml --no-deps --format-version 1 | ConvertFrom-Json)
    if ($LASTEXITCODE -ne 0) { throw 'Cannot locate the Cargo build output.' }
    $svlExecutable = Join-Path $svlMetadata.target_directory 'debug/SubtitleVocabularyList.exe'
    New-Item -ItemType Directory -Path $svlOutput -Force | Out-Null
    Copy-Item -LiteralPath $svlExecutable -Destination $svlOutput -Force
    $svlLauncher = @"
`$ErrorActionPreference = 'Stop'
. '$((Join-Path $PSScriptRoot 'development-environment.ps1').Replace("'", "''"))'
`$svlEnvironment = Get-SvlDevelopmentEnvironment -DataDirectory '$($svlEnvironment.DataDirectory.Replace("'", "''"))'
`$env:Path = `$svlEnvironment.Path
`$env:SVL_DATA_DIR = `$svlEnvironment.DataDirectory
& (Join-Path `$PSScriptRoot 'SubtitleVocabularyList.exe')
"@
    [IO.File]::WriteAllText((Join-Path $svlOutput 'run-local.ps1'), $svlLauncher, [Text.UTF8Encoding]::new($false))
    Write-Host "Local executable: $(Join-Path $svlOutput 'SubtitleVocabularyList.exe')"
    Write-Host "Local launcher: $(Join-Path $svlOutput 'run-local.ps1')"
} finally {
    $env:Path = $svlPreviousPath
    $env:SVL_DATA_DIR = $svlPreviousData
    Pop-Location
}
