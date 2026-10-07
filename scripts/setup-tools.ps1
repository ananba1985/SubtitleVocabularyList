param([string]$RuntimeDirectory = '.tools\runtime')
$ErrorActionPreference = 'Stop'
$taskRuntimeRoot = if ([IO.Path]::IsPathRooted($RuntimeDirectory)) { [IO.Path]::GetFullPath($RuntimeDirectory) } else { [IO.Path]::GetFullPath((Join-Path (Get-Location) $RuntimeDirectory)) }
New-Item -ItemType Directory -Path $taskRuntimeRoot -Force | Out-Null
$taskWhisperRoot = Join-Path $taskRuntimeRoot 'whisper'
$taskWhisperBinary = Join-Path $taskWhisperRoot 'Release\whisper-cli.exe'
if (-not (Test-Path -LiteralPath $taskWhisperBinary)) {
    $taskArchive = Join-Path $taskRuntimeRoot 'whisper-bin-x64.zip'
    Invoke-WebRequest -UseBasicParsing -Uri 'https://github.com/ggml-org/whisper.cpp/releases/download/v1.8.3/whisper-bin-x64.zip' -OutFile $taskArchive -TimeoutSec 120
    Expand-Archive -LiteralPath $taskArchive -DestinationPath $taskWhisperRoot -Force
}
$taskModelPath = Join-Path $taskRuntimeRoot 'ggml-base.en.bin'
$taskExpectedHash = 'A03779C86DF3323075F5E796CB2CE5029F00EC8869EEE3FDFB897AFE36C6D002'
if (-not (Test-Path -LiteralPath $taskModelPath)) {
    Invoke-WebRequest -UseBasicParsing -Uri 'https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin' -OutFile $taskModelPath -TimeoutSec 180
}
if ((Get-FileHash -LiteralPath $taskModelPath -Algorithm SHA256).Hash -ne $taskExpectedHash) {
    throw 'Whisper model checksum mismatch. The file was retained for inspection; replace it before running offline recognition.'
}
if (-not (Test-Path -LiteralPath $taskWhisperBinary)) { throw 'Whisper executable was not found after extraction.' }
Write-Output ('Prepared local Whisper runtime: ' + $taskWhisperRoot)
Write-Output ('Verified English model SHA256: ' + $taskExpectedHash)
foreach ($taskToolName in @('ffmpeg','ffprobe','tesseract')) {
    $taskToolCommand = Get-Command $taskToolName -ErrorAction Stop
    Write-Output ($taskToolName + ': ' + $taskToolCommand.Source)
}
