param([string]$OutputDirectory = '.tools\probes')
$ErrorActionPreference = 'Stop'
$taskProbeRoot = [System.IO.Path]::GetFullPath((Join-Path (Get-Location) $OutputDirectory))
New-Item -ItemType Directory -Path $taskProbeRoot -Force | Out-Null
$taskSpeechPath = Join-Path $taskProbeRoot 'speech.wav'
$taskVoice = New-Object -ComObject SAPI.SpVoice
$taskVoices = $taskVoice.GetVoices()
for ($taskVoiceIndex = 0; $taskVoiceIndex -lt $taskVoices.Count; $taskVoiceIndex++) {
    $taskToken = $taskVoices.Item($taskVoiceIndex)
    if ($taskToken.GetAttribute('Language') -match '(^|;)409(;|$)') {
        $taskVoice.Voice = $taskToken
        break
    }
}
$taskStream = New-Object -ComObject SAPI.SpFileStream
$taskStream.Open($taskSpeechPath, 3, $false)
try {
    $taskVoice.AudioOutputStream = $taskStream
    $null = $taskVoice.Speak('I was reluctant to ask for help. Learning English takes practice.')
} finally { $taskStream.Close() }
Write-Output ('English voice: ' + $taskVoice.Voice.GetDescription())
$taskCaptionPath = Join-Path $taskProbeRoot 'captions.srt'
$taskCaptions = @"
1
00:00:00,000 --> 00:00:02,600
I was reluctant to ask for help.

2
00:00:02,600 --> 00:00:05,000
Learning English takes practice.
"@
[System.IO.File]::WriteAllText($taskCaptionPath, $taskCaptions, [System.Text.UTF8Encoding]::new($false))
$taskVideoPath = Join-Path $taskProbeRoot 'with-subtitles.mkv'
& ffmpeg -nostdin -y -v error -f lavfi -i 'color=c=0x163e36:s=640x360:r=12:d=5' -i $taskSpeechPath -i $taskCaptionPath -map '0:v' -map '1:a' -map '2:s' -c:v libx264 -preset ultrafast -c:a aac -c:s srt -metadata:s:s:0 language=eng -t 5 $taskVideoPath
if ($LASTEXITCODE -ne 0) { throw 'Synthetic video generation failed.' }
& ffprobe -v error -show_entries stream=index,codec_type:stream_tags=language -of json $taskVideoPath
if ($LASTEXITCODE -ne 0) { throw 'Synthetic video inspection failed.' }
Get-Item -LiteralPath $taskSpeechPath,$taskVideoPath | Select-Object Name,Length
