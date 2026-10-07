param([string]$ResourcesDirectory='.tools\release-resources',[switch]$SkipToolBuild)
$ErrorActionPreference='Stop'
$svlWorkspace=(Get-Location).Path
if(-not(Test-Path -LiteralPath (Join-Path $svlWorkspace 'src-tauri/tauri.conf.json'))){throw 'Run this script from the SubtitleVocabularyList workspace.'}
if(& git status --porcelain){throw 'Commit intended changes before packaging a source-matched release.'}
$svlResources=if([IO.Path]::IsPathRooted($ResourcesDirectory)){[IO.Path]::GetFullPath($ResourcesDirectory)}else{[IO.Path]::GetFullPath((Join-Path $svlWorkspace $ResourcesDirectory))}
if($svlResources -ne [IO.Path]::GetFullPath((Join-Path $svlWorkspace '.tools/release-resources'))){throw 'The installer configuration maps .tools/release-resources; use that resource directory.'}
if(-not $SkipToolBuild){
    & (Join-Path $PSScriptRoot 'build-offline-ocr.ps1') -ResourcesDirectory $ResourcesDirectory
    & (Join-Path $PSScriptRoot 'build-offline-whisper.ps1') -ResourcesDirectory $ResourcesDirectory
    & (Join-Path $PSScriptRoot 'build-offline-ffmpeg.ps1') -ResourcesDirectory $ResourcesDirectory
}
$svlRequired=@('ffmpeg/ffmpeg.exe','ffmpeg/ffprobe.exe','tesseract/tesseract.exe','tesseract/tessdata/eng.traineddata','whisper/whisper-cli.exe','whisper/ggml-base.en.bin')
$svlManifest=@()
foreach($svlRelative in $svlRequired){
    $svlPath=Join-Path $svlResources $svlRelative
    if(-not(Test-Path -LiteralPath $svlPath -PathType Leaf)){throw "Missing offline resource: $svlRelative"}
    $svlManifest+=@{path=$svlRelative;sha256=(Get-FileHash -LiteralPath $svlPath).Hash.ToLowerInvariant();size=(Get-Item -LiteralPath $svlPath).Length}
}
[IO.File]::WriteAllText((Join-Path $svlResources 'manifest.json'),(@{version='0.1.0';files=$svlManifest}|ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))
Copy-Item -LiteralPath (Join-Path $svlWorkspace 'LICENSE') -Destination (Join-Path $svlResources 'SubtitleVocabularyList-LICENSE.txt')
& (Join-Path $PSScriptRoot 'prepare-release-licenses.ps1') -ResourcesDirectory $ResourcesDirectory
& pnpm tauri build
if($LASTEXITCODE -ne 0){throw 'Installer build failed.'}
foreach($svlItem in $svlManifest){
    if((Get-FileHash -LiteralPath (Join-Path $svlResources $svlItem.path)).Hash.ToLowerInvariant() -ne $svlItem.sha256){throw 'Offline resources changed during packaging; rebuild from the verified snapshot.'}
}
$svlInstallers=Get-ChildItem -LiteralPath (Join-Path $svlWorkspace 'src-tauri/target/release/bundle/nsis') -Filter '*-setup.exe'
if($svlInstallers.Count -ne 1){throw 'Expected one NSIS installer.'}
$svlDelivery=Join-Path $svlWorkspace 'release/0.1.0'
New-Item -ItemType Directory -Path $svlDelivery -Force | Out-Null
Copy-Item -LiteralPath $svlInstallers[0].FullName -Destination $svlDelivery
& (Join-Path $PSScriptRoot 'export-release-source.ps1') -OutputDirectory $svlDelivery
$svlOutputs=@($svlInstallers[0].Name,'SubtitleVocabularyList_0.1.0_source-materials.zip')
$svlSums=foreach($svlName in $svlOutputs){(Get-FileHash -LiteralPath (Join-Path $svlDelivery $svlName)).Hash.ToLowerInvariant()+'  '+$svlName}
$svlSums | Set-Content -LiteralPath (Join-Path $svlDelivery 'SHA256SUMS.txt') -Encoding utf8
Write-Output "Release installer: $svlDelivery"
