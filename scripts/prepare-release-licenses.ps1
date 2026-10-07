param([string]$ResourcesDirectory='.tools\release-resources')
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$svlWorkspace=(Get-Location).Path
$svlDestination=[IO.Path]::GetFullPath((Join-Path $svlWorkspace $ResourcesDirectory))
$svlLicenses=Join-Path $svlDestination 'licenses'
New-Item -ItemType Directory -Path $svlLicenses -Force | Out-Null
$svlMetadataText=& cargo metadata --manifest-path src-tauri/Cargo.toml --locked --format-version 1 --filter-platform x86_64-pc-windows-msvc
if($LASTEXITCODE -ne 0){throw 'Cannot inspect locked Rust dependencies.'}
$svlMetadata=$svlMetadataText | ConvertFrom-Json
$svlIds=$svlMetadata.resolve.nodes.id
$svlManifest=@()
$svlMissing=@()
foreach($svlPackage in $svlMetadata.packages | Where-Object {$_.source -and $_.id -in $svlIds}){
    $svlSource=Split-Path $svlPackage.manifest_path
    $svlPackageDirectory=Join-Path $svlLicenses ('rust/'+$svlPackage.name+'-'+$svlPackage.version)
    New-Item -ItemType Directory -Path $svlPackageDirectory -Force | Out-Null
    $svlFiles=@(Get-ChildItem -LiteralPath $svlSource -File | Where-Object {$_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|UNLICENSE|Copyright)'})
    foreach($svlFile in $svlFiles){Copy-Item -LiteralPath $svlFile.FullName -Destination $svlPackageDirectory}
    # Some published crates omit their workspace license. Recover the exact
    # upstream revision recorded in that crate, without using a moving branch.
    if($svlFiles.Count -eq 0 -and $svlPackage.license -eq 'MPL-2.0'){
        $svlUrl='https://www.mozilla.org/media/MPL/2.0/index.txt'
        $svlResponse=Invoke-WebRequest -Uri $svlUrl -TimeoutSec 20 -UseBasicParsing
        [IO.File]::WriteAllText((Join-Path $svlPackageDirectory 'LICENSE-MPL-2.0'),$svlResponse.Content,[Text.UTF8Encoding]::new($false))
        [IO.File]::WriteAllText((Join-Path $svlPackageDirectory 'LICENSE-MPL-2.0.source.txt'),$svlUrl,[Text.UTF8Encoding]::new($false))
    }
    elseif($svlFiles.Count -eq 0){
        $svlVcsPath=Join-Path $svlSource '.cargo_vcs_info.json'
        if((Test-Path -LiteralPath $svlVcsPath) -and $svlPackage.repository -match '^https://github.com/([^/]+/[^/]+?)(?:\.git)?$'){
            $svlRepository=$Matches[1]
            $svlVcs=Get-Content -LiteralPath $svlVcsPath -Raw | ConvertFrom-Json
            foreach($svlName in @('LICENSE','LICENSE-MIT','LICENSE-APACHE','COPYING')){
                $svlUrl='https://raw.githubusercontent.com/'+$svlRepository+'/'+$svlVcs.git.sha1+'/'+$svlName
                try{
                    $svlResponse=Invoke-WebRequest -Uri $svlUrl -TimeoutSec 20 -UseBasicParsing
                    [IO.File]::WriteAllText((Join-Path $svlPackageDirectory $svlName),$svlResponse.Content,[Text.UTF8Encoding]::new($false))
                    [IO.File]::WriteAllText((Join-Path $svlPackageDirectory ($svlName+'.source.txt')),$svlUrl,[Text.UTF8Encoding]::new($false))
                    break
                }catch{
                    if(-not $_.Exception.Response -or [int]$_.Exception.Response.StatusCode -ne 404){throw}
                }
            }
        }
    }
    $svlIncluded=@(Get-ChildItem -LiteralPath $svlPackageDirectory -File | Where-Object {$_.Name -notlike '*.source.txt'})
    if($svlIncluded.Count -eq 0){$svlMissing+=$svlPackage.name+'-'+$svlPackage.version}
    $svlManifest+=@{ecosystem='cargo';name=$svlPackage.name;version=$svlPackage.version;license=$svlPackage.license;repository=$svlPackage.repository;files=@($svlIncluded.Name)}
}
$svlGraphText=& pnpm list --prod --depth Infinity --json
if($LASTEXITCODE -ne 0){throw 'Cannot inspect locked frontend runtime dependencies.'}
$svlGraph=$svlGraphText | ConvertFrom-Json
$svlQueue=[Collections.Generic.Queue[object]]::new()
foreach($svlDependency in $svlGraph[0].dependencies.PSObject.Properties){$svlQueue.Enqueue($svlDependency.Value)}
$svlSeen=@{}
while($svlQueue.Count){
        $svlCandidate=$svlQueue.Dequeue()
        $svlPackageFile=Join-Path $svlCandidate.path 'package.json'
        $svlPackage=Get-Content -LiteralPath $svlPackageFile -Raw | ConvertFrom-Json
        $svlKey=$svlPackage.name+'@'+$svlPackage.version
        if($svlSeen.ContainsKey($svlKey)){continue}
        $svlSeen[$svlKey]=$true
        $svlDirectory=Join-Path $svlLicenses ('npm/'+$svlKey.Replace('/','_'))
        New-Item -ItemType Directory -Path $svlDirectory -Force | Out-Null
        $svlFiles=@(Get-ChildItem -LiteralPath $svlCandidate.path -File | Where-Object {$_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|UNLICENSE|Copyright)'})
        foreach($svlFile in $svlFiles){Copy-Item -LiteralPath $svlFile.FullName -Destination $svlDirectory}
        if($svlFiles.Count -eq 0){$svlMissing+=$svlKey}
        $svlManifest+=@{ecosystem='npm';name=$svlPackage.name;version=$svlPackage.version;license=$svlPackage.license;files=@($svlFiles.Name)}
        if($svlCandidate.dependencies){foreach($svlDependency in $svlCandidate.dependencies.PSObject.Properties){$svlQueue.Enqueue($svlDependency.Value)}}
}
[IO.File]::WriteAllText((Join-Path $svlLicenses 'dependencies.json'),($svlManifest|ConvertTo-Json -Depth 6),[Text.UTF8Encoding]::new($false))
Copy-Item -LiteralPath (Join-Path $svlWorkspace 'THIRD_PARTY_NOTICES.md') -Destination (Join-Path $svlDestination 'THIRD_PARTY_NOTICES.md')
if($svlMissing.Count){throw ('Missing dependency license files: '+($svlMissing -join ', '))}
Write-Output ('Prepared license materials for '+$svlManifest.Count+' locked dependencies.')
