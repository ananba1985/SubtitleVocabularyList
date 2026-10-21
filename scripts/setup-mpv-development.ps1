param()
$ErrorActionPreference = 'Stop'
$svlRoot = Split-Path $PSScriptRoot -Parent
$svlRuntime = Join-Path $svlRoot '.tools/runtime/mpv'
$svlArchive = Join-Path $svlRoot '.tools/runtime/mpv-dev-x86_64-20261021-git-e471496c56.7z'
$svlExpected = 'EFC719EFF3DC5954B4A5EFC800632808BCEC01CC9034940DD14E5EB7FE9EA633'
$svlUrl = 'https://github.com/shinchiro/mpv-winbuild-cmake/releases/download/20261021/mpv-dev-x86_64-20261021-git-e471496c56.7z'
New-Item -ItemType Directory -Path $svlRuntime -Force | Out-Null
if (-not (Test-Path -LiteralPath $svlArchive)) { Invoke-WebRequest -Uri $svlUrl -OutFile $svlArchive }
if ((Get-FileHash -LiteralPath $svlArchive -Algorithm SHA256).Hash -ne $svlExpected) { throw 'libmpv archive checksum mismatch.' }
& 7z x -y "-o$svlRuntime" $svlArchive | Out-Null
if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath (Join-Path $svlRuntime 'libmpv-2.dll'))) { throw 'libmpv extraction failed; install 7-Zip and retry.' }
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'mpv-runtime-notice.md') -Destination (Join-Path $svlRuntime 'NOTICE.md') -Force
Write-Host "libmpv development runtime ready: $svlRuntime"
