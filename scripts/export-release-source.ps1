param([string]$OutputDirectory='',[string]$SourceRef='HEAD')
$ErrorActionPreference='Stop'
$svlWorkspace=(Get-Location).Path
$svlVersion=(Get-Content -LiteralPath (Join-Path $svlWorkspace 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json).version
if(-not $OutputDirectory){$OutputDirectory='release/'+$svlVersion}
$svlCommit=(& git rev-parse --verify ($SourceRef+'^{commit}')).Trim()
if($LASTEXITCODE -ne 0){throw 'Cannot resolve release source revision.'}
if(& git status --porcelain){throw 'Commit intended changes before exporting release source materials.'}
& git diff --quiet $svlCommit -- .
if($LASTEXITCODE -ne 0){throw 'SourceRef must match the current checked-out source.'}
$svlStage=Join-Path $svlWorkspace ('.tools/source-materials-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $svlStage -Force | Out-Null
$svlOwnArchive=Join-Path $svlStage ('SubtitleVocabularyList-'+$svlCommit+'.zip')
& git archive --format=zip ('--output='+$svlOwnArchive) $svlCommit
if($LASTEXITCODE -ne 0){throw 'Application source archive failed.'}
$svlToolSources=Join-Path $svlStage 'media-ocr-asr'
New-Item -ItemType Directory -Path $svlToolSources -Force | Out-Null
$svlRequired=@('FFmpeg-n9.0.2.zip','leptonica-1.85.0.zip','libjpeg-turbo-3.0.1.zip','libpng-1.6.43.zip','tesseract-5.5.3.zip','tiff-4.6.0.zip','whisper.cpp-1.8.3.zip','zlib-1.3.1.zip')
foreach($svlName in $svlRequired){Copy-Item -LiteralPath (Join-Path $svlWorkspace ('.tools/release-cache/sources/'+$svlName)) -Destination $svlToolSources}
Copy-Item -LiteralPath (Join-Path $svlWorkspace '.tools/release-resources/ffmpeg/build-config.txt') -Destination $svlToolSources
$svlMetadataText=& cargo metadata --manifest-path src-tauri/Cargo.toml --locked --format-version 1 --filter-platform x86_64-pc-windows-msvc
if($LASTEXITCODE -ne 0){throw 'Cannot inspect Rust dependency source.'}
$svlMetadata=$svlMetadataText | ConvertFrom-Json
$svlIds=$svlMetadata.resolve.nodes.id
$svlCrates=Join-Path $svlStage 'cargo'
New-Item -ItemType Directory -Path $svlCrates -Force | Out-Null
$svlChecksums=@{}
$svlLock=Get-Content -LiteralPath (Join-Path $svlWorkspace 'src-tauri/Cargo.lock') -Raw
foreach($svlEntry in [regex]::Split($svlLock,'(?m)^\[\[package\]\]\r?$')){
    $svlName=[regex]::Match($svlEntry,'(?m)^name = "([^"]+)"').Groups[1].Value
    $svlVersion=[regex]::Match($svlEntry,'(?m)^version = "([^"]+)"').Groups[1].Value
    $svlChecksum=[regex]::Match($svlEntry,'(?m)^checksum = "([a-f0-9]{64})"').Groups[1].Value
    if($svlChecksum){$svlChecksums[$svlName+'@'+$svlVersion]=$svlChecksum}
}
foreach($svlPackage in $svlMetadata.packages | Where-Object {$_.source -and $_.id -in $svlIds}){
    if($svlPackage.source -notlike 'registry+*'){throw ('Review nonregistry dependency source: '+$svlPackage.name)}
    $svlSource=Split-Path $svlPackage.manifest_path
    $svlRegistry=Split-Path $svlSource
    $svlRegistryName=Split-Path $svlRegistry -Leaf
    $svlCache=Join-Path (Split-Path (Split-Path $svlRegistry)) ('cache/'+$svlRegistryName+'/'+$svlPackage.name+'-'+$svlPackage.version+'.crate')
    $svlExpected=$svlChecksums[$svlPackage.name+'@'+$svlPackage.version]
    if(-not $svlExpected){throw ('Missing locked dependency checksum: '+$svlPackage.name)}
    if((Get-FileHash -LiteralPath $svlCache).Hash.ToLowerInvariant() -ne $svlExpected){throw ('Dependency source checksum mismatch: '+$svlPackage.name)}
    Copy-Item -LiteralPath $svlCache -Destination $svlCrates
}
$svlGraphText=& pnpm list --prod --depth Infinity --json
if($LASTEXITCODE -ne 0){throw 'Cannot inspect frontend dependency source.'}
$svlGraph=$svlGraphText | ConvertFrom-Json
$svlQueue=[Collections.Generic.Queue[object]]::new()
foreach($svlDependency in $svlGraph[0].dependencies.PSObject.Properties){$svlQueue.Enqueue($svlDependency.Value)}
$svlSeen=@{}
while($svlQueue.Count){
    $svlPackage=$svlQueue.Dequeue()
    $svlKey=$svlPackage.from+'@'+$svlPackage.version
    if($svlSeen.ContainsKey($svlKey)){continue}
    $svlSeen[$svlKey]=$true
    $svlTarget=Join-Path $svlStage ('npm/'+$svlKey.Replace('/','_'))
    New-Item -ItemType Directory -Path $svlTarget -Force | Out-Null
    Get-ChildItem -LiteralPath $svlPackage.path -Force | ForEach-Object {
        if($_.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Unexpected link in frontend source package.'}
        Copy-Item -LiteralPath $_.FullName -Destination $svlTarget -Recurse
    }
    if($svlPackage.dependencies){foreach($svlDependency in $svlPackage.dependencies.PSObject.Properties){$svlQueue.Enqueue($svlDependency.Value)}}
}
Copy-Item -LiteralPath (Join-Path $svlWorkspace '.tools/release-resources/licenses') -Destination $svlStage -Recurse
Copy-Item -LiteralPath (Join-Path $svlWorkspace 'LICENSE'),(Join-Path $svlWorkspace 'THIRD_PARTY_NOTICES.md') -Destination $svlStage
$svlReadme=@"
SubtitleVocabularyList $svlVersion 源码与构建材料

应用源码提交：$svlCommit
应用 ZIP 包含固定依赖锁文件、数据库迁移和 scripts/build-offline-*.ps1。
media-ocr-asr 包含构建时使用的八份上游原始源码包及实际 FFmpeg 配置。
构建脚本记录 Windows/MSVC 适配改动；从原始包重新应用这些改动后构建。
cargo 包含锁定版本的原始 .crate 源码归档；摘要与 Cargo.lock 校验值核对。
npm 包含前端运行依赖的已安装发行源码；各自版权与许可保留。
licenses 包含 Rust 与前端依赖的许可、版本和补取许可的固定上游来源。
媒体工具许可和模型来源还可在安装目录 tools 中查看。

解压应用 ZIP 后按 docs/development/windows-environment.md 准备开发工具，
运行 scripts/package-release.ps1。模型下载地址及摘要在对应构建脚本中。
微软 WebView2、Windows SDK、编译器和系统声音按微软许可另行准备。
本资料包不含个人词库、剧集、原声片段、截图或同步凭据。
"@
[IO.File]::WriteAllText((Join-Path $svlStage 'README.txt'),$svlReadme,[Text.UTF8Encoding]::new($false))
$svlManifest=Get-ChildItem -LiteralPath $svlStage -Recurse -File | ForEach-Object {@{path=$_.FullName.Substring($svlStage.Length+1).Replace('\','/');size=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName).Hash.ToLowerInvariant()}}
[IO.File]::WriteAllText((Join-Path $svlStage 'source-manifest.json'),(@{commit=$svlCommit;files=@($svlManifest)}|ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))
$svlOutput=if([IO.Path]::IsPathRooted($OutputDirectory)){[IO.Path]::GetFullPath($OutputDirectory)}else{[IO.Path]::GetFullPath((Join-Path $svlWorkspace $OutputDirectory))}
New-Item -ItemType Directory -Path $svlOutput -Force | Out-Null
$svlArchive=Join-Path $svlOutput ('SubtitleVocabularyList_'+$svlVersion+'_source-materials.zip')
# npm archives can carry Unix epoch timestamps, outside ZIP's 1980-2107 range.
# Adjust copied metadata only; dependency source bytes and hashes stay intact.
foreach($svlFile in Get-ChildItem -LiteralPath $svlStage -Recurse -File){
    if($svlFile.LastWriteTime.Year -lt 1980 -or $svlFile.LastWriteTime.Year -gt 2107){$svlFile.LastWriteTime=[datetime]::new(1980,1,2)}
}
Compress-Archive -Path (Join-Path $svlStage '*') -DestinationPath $svlArchive -Force
if(-not $svlStage.StartsWith((Join-Path $svlWorkspace '.tools/source-materials-'),[StringComparison]::OrdinalIgnoreCase)){throw 'Unexpected generated staging directory.'}
$svlGenerated=@(Get-ChildItem -LiteralPath $svlStage -Recurse -Force)
if($svlGenerated | Where-Object {$_.Attributes -band [IO.FileAttributes]::ReparsePoint}){throw 'Unexpected link in generated staging directory.'}
$svlGenerated | Where-Object {-not $_.PSIsContainer} | Remove-Item
$svlGenerated | Where-Object {$_.PSIsContainer} | Sort-Object {$_.FullName.Length} -Descending | ForEach-Object {Remove-Item -LiteralPath $_.FullName}
Remove-Item -LiteralPath $svlStage
Write-Output ('Release source materials: '+$svlArchive)
