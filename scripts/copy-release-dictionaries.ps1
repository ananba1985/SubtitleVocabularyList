param([Parameter(Mandatory=$true)][string]$InstallDirectory)
$ErrorActionPreference='Stop'
$svlWorkspace=Split-Path $PSScriptRoot
$svlConfig=Get-Content -LiteralPath (Join-Path $svlWorkspace 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json
$svlResources=$svlConfig.bundle.resources.PSObject.Properties | Where-Object {$_.Name.StartsWith('../.tools/release-dictionaries/')}
# Reuse the installer's explicit file mapping; never copy or remove the Data tree.
foreach($svlResource in $svlResources){
    if(-not $svlResource.Value.StartsWith('Data/dictionaries/')){throw 'Dictionary resources must stay in Data/dictionaries.'}
    $svlSource=[IO.Path]::GetFullPath((Join-Path (Join-Path $svlWorkspace 'src-tauri') $svlResource.Name))
    $svlTarget=Join-Path $InstallDirectory $svlResource.Value
    New-Item -ItemType Directory -Path (Split-Path $svlTarget) -Force | Out-Null
    Copy-Item -LiteralPath $svlSource -Destination $svlTarget -Force
}
Write-Output ('Prepared reference dictionaries: '+(Join-Path $InstallDirectory 'Data/dictionaries'))
