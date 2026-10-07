param([string]$RuntimeDirectory = '.tools\runtime')
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$taskRuntime = if ([IO.Path]::IsPathRooted($RuntimeDirectory)) { [IO.Path]::GetFullPath($RuntimeDirectory) } else { [IO.Path]::GetFullPath((Join-Path (Get-Location) $RuntimeDirectory)) }
New-Item -ItemType Directory -Path $taskRuntime -Force | Out-Null
function Get-MediaDownload([string]$Uri, [string]$Name, [string]$Hash) {
    $taskPath = Join-Path $taskRuntime $Name
    if (-not (Test-Path -LiteralPath $taskPath)) {
        Write-Host "Downloading $Name"
        Invoke-WebRequest -UseBasicParsing -Uri $Uri -OutFile $taskPath -TimeoutSec 900
    }
    if ((Get-FileHash -LiteralPath $taskPath -Algorithm SHA256).Hash -ne $Hash) { throw "Checksum mismatch: $taskPath" }
    return $taskPath
}
$taskFfmpegRoot = Join-Path $taskRuntime 'ffmpeg\ffmpeg-9.0.2-essentials_build'
if (-not (Test-Path -LiteralPath (Join-Path $taskFfmpegRoot 'bin\ffmpeg.exe'))) {
    $taskArchive = Get-MediaDownload 'https://github.com/GyanD/codexffmpeg/releases/download/9.0.2/ffmpeg-9.0.2-essentials_build.zip' 'ffmpeg-9.0.2-essentials_build.zip' '60F467265B1E312373DBCD92200C2618A74850F98D3D078E94296BB3FA2047BA'
    Expand-Archive -LiteralPath $taskArchive -DestinationPath (Join-Path $taskRuntime 'ffmpeg') -Force
}
$taskTesseractRoot = Join-Path $env:ProgramFiles 'Tesseract-OCR'
if (-not (Test-Path -LiteralPath (Join-Path $taskTesseractRoot 'tesseract.exe'))) {
    $taskInstaller = Get-MediaDownload 'https://github.com/tesseract-ocr/tesseract/releases/download/5.5.3/tesseract-ocr-w64-setup-5.5.3.20260724.exe' 'tesseract-ocr-w64-setup-5.5.3.20260724.exe' 'BEE9E3434BD94FD65387D9BE28CD467A41F61B1275383B55B0F59A1331270AE4'
    $taskInstalled = Start-Process -FilePath $taskInstaller -ArgumentList '/S' -WindowStyle Hidden -Wait -PassThru
    if ($taskInstalled.ExitCode -ne 0) { throw "Tesseract installation failed ($($taskInstalled.ExitCode))." }
}
$env:Path = "$taskFfmpegRoot\bin;$taskTesseractRoot;" + $env:Path
& (Join-Path $PSScriptRoot 'setup-tools.ps1') -RuntimeDirectory $taskRuntime
if ($LASTEXITCODE -ne 0) { throw 'Media runtime preparation failed.' }
& ffmpeg -version | Select-Object -First 1
& ffprobe -version | Select-Object -First 1
& tesseract --version 2>&1 | Select-Object -First 2
& tesseract --list-langs
if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath (Join-Path $taskTesseractRoot 'tessdata\eng.traineddata'))) { throw 'English OCR data is not available.' }
Write-Output "Media runtime: $taskRuntime"
