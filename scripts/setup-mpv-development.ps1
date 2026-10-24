param()
$ErrorActionPreference = 'Stop'
$svlRoot = Split-Path $PSScriptRoot -Parent
$svlMpv = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'mpv-runtime.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$svlRuntime = Join-Path $svlRoot '.tools/runtime/mpv'
$svlArchive = Join-Path $svlRoot ('.tools/runtime/' + $svlMpv.archive)
New-Item -ItemType Directory -Path $svlRuntime -Force | Out-Null
if (-not (Test-Path -LiteralPath $svlArchive)) { Invoke-WebRequest -Uri $svlMpv.archiveUrl -OutFile $svlArchive }
if ((Get-FileHash -LiteralPath $svlArchive -Algorithm SHA256).Hash -ne $svlMpv.archiveSha256) { throw 'libmpv archive checksum mismatch.' }
if (-not (Test-Path -LiteralPath (Join-Path $svlRuntime $svlMpv.library)) -or
    (Get-FileHash -LiteralPath (Join-Path $svlRuntime $svlMpv.library) -Algorithm SHA256).Hash -ne $svlMpv.librarySha256) {
    $svlExtractorJson = & python -X utf8 (Join-Path $PSScriptRoot 'archive_tools.py')
    if ($LASTEXITCODE -ne 0) { throw 'Unable to locate the player archive extraction tool.' }
    $svlExtractor = ($svlExtractorJson | ConvertFrom-Json).path
    if (-not $svlExtractor) { throw 'Player extraction requires 7-Zip; existing standard and Scoop installations are detected automatically.' }
    & $svlExtractor x -y "-o$svlRuntime" $svlArchive | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'libmpv extraction failed.' }
}
if ((Get-FileHash -LiteralPath (Join-Path $svlRuntime $svlMpv.library) -Algorithm SHA256).Hash -ne $svlMpv.librarySha256) { throw 'libmpv DLL checksum mismatch.' }
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'mpv-runtime-notice.md') -Destination (Join-Path $svlRuntime 'NOTICE.md') -Force
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'mpv-runtime.json') -Destination $svlRuntime -Force
Write-Host "libmpv development runtime ready: $svlRuntime"
