param([string]$ResourcesDirectory='.tools\release-resources',[switch]$SkipToolBuild,[switch]$CheckOnly,[switch]$NoPause)
$ErrorActionPreference='Stop'
$svlWorkspace=$PSScriptRoot
$svlScripts=Join-Path $svlWorkspace 'scripts'
$svlExitCode=0
$svlPreviousPath=$env:Path
$svlTranscriptStarted=$false
$svlLocationPushed=$false
$svlLogPath=Join-Path $svlWorkspace ('.local/package-logs/package-'+(Get-Date -Format 'yyyyMMdd-HHmmss-fff')+'-'+$PID+'.log')
try {
New-Item -ItemType Directory -Path (Split-Path $svlLogPath -Parent) -Force | Out-Null
Start-Transcript -LiteralPath $svlLogPath -Force | Out-Null
$svlTranscriptStarted=$true
Write-Host "打包执行记录：$svlLogPath"
. (Join-Path $svlScripts 'development-environment.ps1')
. (Join-Path $svlScripts 'build-preflight.ps1')
. (Join-Path $svlScripts 'cargo-cache.ps1')
. (Join-Path $svlScripts 'package-preflight.ps1')
$svlEnvironment=Get-SvlDevelopmentEnvironment
$env:Path=$svlEnvironment.Path
Push-Location $svlWorkspace
$svlLocationPushed=$true
& {
$svlPreflight=Assert-SvlPackageReady -Workspace $svlWorkspace -ResourcesDirectory $ResourcesDirectory -SkipToolBuild:$SkipToolBuild
$svlVersion=$svlPreflight.Version
$svlResources=$svlPreflight.Resources
$svlRequired=$svlPreflight.RequiredResources
$svlDelivery=Join-Path $svlWorkspace ('release/'+$svlVersion)
if($CheckOnly){
    Write-Output ('预检结束，退出码 0；正式打包输出目录：'+$svlDelivery)
    Write-Output '本次只检查前置条件并保存执行记录，没有清理、编译、下载、启动或安装程序。'
    return
}
$svlDictionaries=Join-Path $svlWorkspace '.tools/release-dictionaries'
& python -X utf8 (Join-Path $svlScripts 'prepare-base-dictionary.py') --release-dir $svlDictionaries
if($LASTEXITCODE -ne 0){throw 'Reference dictionary preparation failed; installer compilation was not started.'}
$svlDictionaryManifest=Get-Content -LiteralPath (Join-Path $svlDictionaries 'manifest.json') -Raw | ConvertFrom-Json
$svlDictionaryFiles=@($svlDictionaryManifest.files)+@(@{path='manifest.json';sha256=(Get-FileHash -LiteralPath (Join-Path $svlDictionaries 'manifest.json')).Hash.ToLowerInvariant()})
& python -X utf8 (Join-Path $svlScripts 'prepare-mpv-release.py') --resources-directory $svlResources
if($LASTEXITCODE -ne 0){throw 'Player resource preparation failed; installer compilation was not started.'}
Invoke-SvlCargoCacheMaintenance
if(-not $SkipToolBuild){
    & (Join-Path $svlScripts 'build-offline-ocr.ps1') -ResourcesDirectory $ResourcesDirectory
    & (Join-Path $svlScripts 'build-offline-whisper.ps1') -ResourcesDirectory $ResourcesDirectory
    & (Join-Path $svlScripts 'build-offline-ffmpeg.ps1') -ResourcesDirectory $ResourcesDirectory
}
$svlManifest=@()
foreach($svlRelative in $svlRequired){
    $svlPath=Join-Path $svlResources $svlRelative
    if(-not(Test-Path -LiteralPath $svlPath -PathType Leaf)){throw "Missing offline resource: $svlRelative"}
    $svlManifest+=@{path=$svlRelative;sha256=(Get-FileHash -LiteralPath $svlPath).Hash.ToLowerInvariant();size=(Get-Item -LiteralPath $svlPath).Length}
}
[IO.File]::WriteAllText((Join-Path $svlResources 'manifest.json'),(@{version=$svlVersion;files=$svlManifest}|ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))
Copy-Item -LiteralPath (Join-Path $svlWorkspace 'LICENSE') -Destination (Join-Path $svlResources 'SubtitleVocabularyList-LICENSE.txt')
& (Join-Path $svlScripts 'prepare-release-licenses.ps1') -ResourcesDirectory $ResourcesDirectory
& pnpm tauri build
if($LASTEXITCODE -ne 0){throw 'Installer build failed.'}
foreach($svlItem in $svlManifest){
    if((Get-FileHash -LiteralPath (Join-Path $svlResources $svlItem.path)).Hash.ToLowerInvariant() -ne $svlItem.sha256){throw 'Offline resources changed during packaging; rebuild from the verified snapshot.'}
}
foreach($svlItem in $svlDictionaryFiles){
    if((Get-FileHash -LiteralPath (Join-Path $svlDictionaries $svlItem.path)).Hash.ToLowerInvariant() -ne $svlItem.sha256){throw 'Reference dictionaries changed during packaging; prepare them again.'}
}
$svlInstallers=@(Get-ChildItem -LiteralPath (Join-Path $svlWorkspace 'src-tauri/target/release/bundle/nsis') -Filter ('SubtitleVocabularyList_'+$svlVersion+'_*-setup.exe'))
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
& (Join-Path $svlScripts 'copy-release-dictionaries.ps1') -InstallDirectory $svlStandalone
[IO.File]::WriteAllText((Join-Path $svlStandalone 'README.txt'),
    "Run SubtitleVocabularyList.exe from this directory. Keep the tools and Data/dictionaries directories beside it.`r`nUser data and caches are stored in the Data subdirectory beside the executable. Reference dictionary updates replace only the packaged dictionary files, not vocabulary.sqlite3 or learning records. This standalone directory is separate from an installed application's Data directory.`r`n",
    [Text.UTF8Encoding]::new($false))

& (Join-Path $svlScripts 'export-release-source.ps1') -OutputDirectory $svlDelivery
$svlOutputs=@($svlInstallers[0].Name,('SubtitleVocabularyList_'+$svlVersion+'_source-materials.zip'))
$svlOutputs+=Get-ChildItem -LiteralPath $svlStandalone -Recurse -File | ForEach-Object {$_.FullName.Substring($svlDelivery.Length+1).Replace('\','/')}
$svlSums=foreach($svlName in $svlOutputs){(Get-FileHash -LiteralPath (Join-Path $svlDelivery $svlName)).Hash.ToLowerInvariant()+'  '+$svlName}
$svlSums | Set-Content -LiteralPath (Join-Path $svlDelivery 'SHA256SUMS.txt') -Encoding utf8
Write-Output "Release installer: $svlDelivery"
Write-Output "Standalone executable: $(Join-Path $svlStandalone 'SubtitleVocabularyList.exe')"
} | Out-Host
} catch {
    $svlExitCode=1
    $svlFailureLabel=if($CheckOnly){'打包前检查失败：'}else{'安装包生成失败：'}
    Write-Host ($svlFailureLabel+$_.Exception.Message) -ForegroundColor Red
    if($_.InvocationInfo.PositionMessage){Write-Host $_.InvocationInfo.PositionMessage}
    if($_.ScriptStackTrace){Write-Host $_.ScriptStackTrace}
    Write-Host "请保留执行记录：$svlLogPath"
} finally {
    if($svlLocationPushed){Pop-Location}
    $env:Path=$svlPreviousPath
    if($svlTranscriptStarted){Stop-Transcript | Out-Null}
    if(-not $CheckOnly -and -not $NoPause){
        Read-Host '执行结束，按 Enter 关闭窗口' | Out-Null
    }
}
exit $svlExitCode
