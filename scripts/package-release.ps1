param([string]$ResourcesDirectory='.tools\release-resources',[switch]$SkipToolBuild,[switch]$CheckOnly)
$ErrorActionPreference='Stop'
$svlWorkspace=Split-Path $PSScriptRoot -Parent
Push-Location $svlWorkspace
try {
& node (Join-Path $PSScriptRoot 'version.mjs') check
if($LASTEXITCODE -ne 0){throw 'Application versions are inconsistent.'}
if(-not(Test-Path -LiteralPath (Join-Path $svlWorkspace 'src-tauri/tauri.conf.json'))){throw 'Run this script from the SubtitleVocabularyList workspace.'}
$svlConfig=Get-Content -LiteralPath (Join-Path $svlWorkspace 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json
$svlVersion=$svlConfig.version
$svlDelivery=Join-Path $svlWorkspace ('release/'+$svlVersion)
if($CheckOnly){
    Write-Output ('Plan: build NSIS installer and standalone executable into '+$svlDelivery)
    Write-Output ('Rebuild offline tools: '+(-not $SkipToolBuild))
    Write-Output 'Release packaging requires committed, clean source; no build or installation was performed.'
    return
}
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
[IO.File]::WriteAllText((Join-Path $svlResources 'manifest.json'),(@{version=$svlVersion;files=$svlManifest}|ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))
Copy-Item -LiteralPath (Join-Path $svlWorkspace 'LICENSE') -Destination (Join-Path $svlResources 'SubtitleVocabularyList-LICENSE.txt')
& (Join-Path $PSScriptRoot 'prepare-release-licenses.ps1') -ResourcesDirectory $ResourcesDirectory
& pnpm tauri build
if($LASTEXITCODE -ne 0){throw 'Installer build failed.'}
foreach($svlItem in $svlManifest){
    if((Get-FileHash -LiteralPath (Join-Path $svlResources $svlItem.path)).Hash.ToLowerInvariant() -ne $svlItem.sha256){throw 'Offline resources changed during packaging; rebuild from the verified snapshot.'}
}
$svlInstallers=Get-ChildItem -LiteralPath (Join-Path $svlWorkspace 'src-tauri/target/release/bundle/nsis') -Filter '*-setup.exe'
if($svlInstallers.Count -ne 1){throw 'Expected one NSIS installer.'}
New-Item -ItemType Directory -Path $svlDelivery -Force | Out-Null
Copy-Item -LiteralPath $svlInstallers[0].FullName -Destination $svlDelivery
$svlStandalone=Join-Path $svlDelivery 'standalone'
New-Item -ItemType Directory -Path $svlStandalone -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $svlWorkspace 'src-tauri/target/release/SubtitleVocabularyList.exe') -Destination $svlStandalone -Force
# Copy contents rather than the directory itself to support repeated manual packaging.
$svlStandaloneTools=Join-Path $svlStandalone 'tools'
New-Item -ItemType Directory -Path $svlStandaloneTools -Force | Out-Null
Get-ChildItem -LiteralPath $svlResources -Force | Copy-Item -Destination $svlStandaloneTools -Recurse -Force
[IO.File]::WriteAllText((Join-Path $svlStandalone 'README.txt'),
    "Run SubtitleVocabularyList.exe from this directory. Keep the tools directory beside it.`r`nUser data remains in the application's data directory; this folder is not a development environment.`r`n",
    [Text.UTF8Encoding]::new($false))

& (Join-Path $PSScriptRoot 'export-release-source.ps1') -OutputDirectory $svlDelivery
$svlOutputs=@($svlInstallers[0].Name,('SubtitleVocabularyList_'+$svlVersion+'_source-materials.zip'))
$svlOutputs+=Get-ChildItem -LiteralPath $svlStandalone -Recurse -File | ForEach-Object {$_.FullName.Substring($svlDelivery.Length+1).Replace('\','/')}
$svlSums=foreach($svlName in $svlOutputs){(Get-FileHash -LiteralPath (Join-Path $svlDelivery $svlName)).Hash.ToLowerInvariant()+'  '+$svlName}
$svlSums | Set-Content -LiteralPath (Join-Path $svlDelivery 'SHA256SUMS.txt') -Encoding utf8
Write-Output "Release installer: $svlDelivery"
Write-Output "Standalone executable: $(Join-Path $svlStandalone 'SubtitleVocabularyList.exe')"
} finally { Pop-Location }
