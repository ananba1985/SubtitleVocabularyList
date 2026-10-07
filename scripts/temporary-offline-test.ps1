param(
    [Parameter(Mandatory=$true)][string]$AppDirectory,
    [Parameter(Mandatory=$true)][string]$SignalDirectory,
    [ValidateRange(60,900)][int]$DurationSeconds=600
)
$ErrorActionPreference='Stop'
$svlPrincipal=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if(-not $svlPrincipal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'This temporary firewall test requires an administrator PowerShell session.'}
$svlWorkspace=[IO.Path]::GetFullPath((Split-Path $PSScriptRoot))
$svlSignals=[IO.Path]::GetFullPath($SignalDirectory)
if(-not $svlSignals.StartsWith((Join-Path $svlWorkspace '.tools')+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Keep test signals inside this workspace .tools directory.'}
$svlInstall=[IO.Path]::GetFullPath($AppDirectory)
$svlRelative=@('SubtitleVocabularyList.exe','tools/ffmpeg/ffmpeg.exe','tools/ffmpeg/ffprobe.exe','tools/tesseract/tesseract.exe','tools/whisper/whisper-cli.exe')
$svlPrograms=foreach($svlName in $svlRelative){(Resolve-Path -LiteralPath (Join-Path $svlInstall $svlName) -ErrorAction Stop).Path}
New-Item -ItemType Directory -Path $svlSignals -Force | Out-Null
$svlReady=Join-Path $svlSignals 'offline-ready.json'
$svlDone=Join-Path $svlSignals 'offline-done'
if((Test-Path -LiteralPath $svlReady) -or (Test-Path -LiteralPath $svlDone)){throw 'Use a fresh signal directory for each test.'}
$svlRules=@()
$svlLease=[guid]::NewGuid().ToString('N')
try{
    for($svlIndex=0;$svlIndex -lt $svlPrograms.Count;$svlIndex++){
        $svlName='SubtitleVocabularyList-offline-test-'+$svlLease+'-'+$svlIndex
        New-NetFirewallRule -Name $svlName -DisplayName 'SubtitleVocabularyList temporary offline test' -Program $svlPrograms[$svlIndex] -Direction Outbound -Action Block -Profile Any -RemoteAddress @('0.0.0.0-126.255.255.255','128.0.0.0-255.255.255.255','::2-ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff') | Out-Null
        $svlRules+=$svlName
    }
    $svlExpires=[DateTime]::UtcNow.AddSeconds($DurationSeconds)
    [IO.File]::WriteAllText($svlReady,(@{pid=$PID;rules=$svlRules;programs=$svlPrograms;expiresUtc=$svlExpires.ToString('o')}|ConvertTo-Json),[Text.UTF8Encoding]::new($false))
    Write-Output 'Only the isolated application and its four tools are blocked; loopback model access and other applications are unchanged.'
    while([DateTime]::UtcNow -lt $svlExpires -and -not(Test-Path -LiteralPath $svlDone)){Start-Sleep -Seconds 1}
}finally{
    foreach($svlName in $svlRules){Remove-NetFirewallRule -Name $svlName -ErrorAction Continue}
    if(Test-Path -LiteralPath $svlReady){Remove-Item -LiteralPath $svlReady}
    [IO.File]::WriteAllText((Join-Path $svlSignals 'offline-restored.json'),(@{pid=$PID;removedRules=$svlRules;restoredUtc=[DateTime]::UtcNow.ToString('o')}|ConvertTo-Json),[Text.UTF8Encoding]::new($false))
}
