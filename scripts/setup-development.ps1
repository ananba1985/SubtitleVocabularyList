param([string]$WorkspaceDirectory = 'C:\Projects\codex\SubtitleVocabularyList')
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$taskAdministrator = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $taskAdministrator) { throw 'Run this development setup in an administrator PowerShell.' }
$taskCache = Join-Path $env:LOCALAPPDATA 'SubtitleVocabularyList\development-setup'
New-Item -ItemType Directory -Path $taskCache -Force | Out-Null

function Get-SetupDownload([string]$Uri, [string]$Name, [string]$ExpectedHash = '') {
    $taskDownload = Join-Path $taskCache $Name
    if (-not (Test-Path -LiteralPath $taskDownload)) {
        Write-Host "Downloading $Name"
        Invoke-WebRequest -UseBasicParsing -Uri $Uri -OutFile $taskDownload -TimeoutSec 900
    }
    if ($ExpectedHash -and (Get-FileHash -LiteralPath $taskDownload -Algorithm SHA256).Hash -ne $ExpectedHash) {
        throw "Checksum mismatch: $taskDownload"
    }
    return $taskDownload
}

function Assert-SetupSignature([string]$Path, [string]$Publisher) {
    $taskSignature = Get-AuthenticodeSignature -LiteralPath $Path
    if ($taskSignature.Status -ne 'Valid' -or $taskSignature.SignerCertificate.Subject -notmatch $Publisher) {
        throw "Installer signature did not match the expected publisher: $Path"
    }
}

function Invoke-SetupInstaller([string]$Path, [string[]]$Arguments) {
    $taskInstaller = Start-Process -FilePath $Path -ArgumentList $Arguments -WindowStyle Hidden -PassThru -Wait
    if ($taskInstaller.ExitCode -notin @(0, 3010)) { throw "Installer failed ($($taskInstaller.ExitCode)): $Path" }
    if ($taskInstaller.ExitCode -eq 3010) { Write-Output 'Installer requested a restart; the script will not restart Windows.' }
}

$taskGit = Join-Path $env:ProgramFiles 'Git\cmd\git.exe'
if (-not (Test-Path -LiteralPath $taskGit)) {
    $taskGitInstaller = Get-SetupDownload 'https://github.com/git-for-windows/git/releases/download/v2.56.0.windows.2/Git-2.56.0.2-64-bit.exe' 'Git-2.56.0.2-64-bit.exe' '52188F917B378F00C70EC136BCF090005F30D44FBC4EBA0BCE759CC6592D60F6'
    Invoke-SetupInstaller $taskGitInstaller @('/VERYSILENT','/NORESTART','/SP-','/NOCANCEL')
}
$taskNode = Join-Path $env:ProgramFiles 'nodejs\node.exe'
if (-not (Test-Path -LiteralPath $taskNode)) {
    $taskNodeName = 'node-v22.23.3-x64.msi'
    $taskNodeSums = Invoke-WebRequest -UseBasicParsing 'https://nodejs.org/dist/v22.23.3/SHASUMS256.txt' -TimeoutSec 60
    $taskNodeHash = ($taskNodeSums.Content -split "`n" | Where-Object { $_.Trim().EndsWith("  $taskNodeName") }) -split '\s+' | Select-Object -First 1
    if ($taskNodeHash -notmatch '^[0-9a-f]{64}$') { throw 'Node.js checksum was not found.' }
    $taskNodeInstaller = Get-SetupDownload "https://nodejs.org/dist/v22.23.3/$taskNodeName" $taskNodeName $taskNodeHash
    Invoke-SetupInstaller 'msiexec.exe' @('/i',('"' + $taskNodeInstaller + '"'),'/qn','/norestart')
}
$env:Path = "$env:ProgramFiles\Git\cmd;$env:ProgramFiles\nodejs;$env:APPDATA\npm;$env:USERPROFILE\.cargo\bin;" + $env:Path

$taskWorkspace = [IO.Path]::GetFullPath($WorkspaceDirectory)
if (-not (Test-Path -LiteralPath $taskWorkspace)) {
    New-Item -ItemType Directory -Path (Split-Path $taskWorkspace -Parent) -Force | Out-Null
    & $taskGit clone 'https://github.com/ananba1985/SubtitleVocabularyList.git' $taskWorkspace
    if ($LASTEXITCODE -ne 0) { throw 'Project checkout failed.' }
} elseif (-not (Test-Path -LiteralPath (Join-Path $taskWorkspace '.git'))) {
    throw "The requested workspace already exists and is not a Git checkout: $taskWorkspace"
}
Write-Output "Workspace: $taskWorkspace"
& $taskGit -C $taskWorkspace log -1 --oneline

$taskVsWhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$taskVs = if (Test-Path -LiteralPath $taskVsWhere) { & $taskVsWhere -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath }
if (-not $taskVs) {
    $taskVsInstaller = Get-SetupDownload 'https://aka.ms/vs/17/release/vs_BuildTools.exe' 'vs_BuildTools.exe'
    Assert-SetupSignature $taskVsInstaller 'Microsoft Corporation'
    Write-Output 'Installing Microsoft C++ Build Tools and Windows SDK'
    Invoke-SetupInstaller $taskVsInstaller @('--quiet','--wait','--norestart','--nocache','--add','Microsoft.VisualStudio.Workload.VCTools','--includeRecommended')
}

$taskRustup = Join-Path $env:USERPROFILE '.cargo\bin\rustup.exe'
if (-not (Test-Path -LiteralPath $taskRustup)) {
    $taskRustUri = 'https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe'
    $taskRustResponse = (Invoke-WebRequest -UseBasicParsing "$taskRustUri.sha256" -TimeoutSec 60).Content
    $taskRustText = if ($taskRustResponse -is [byte[]]) { [Text.Encoding]::UTF8.GetString($taskRustResponse) } else { [string]$taskRustResponse }
    $taskRustHash = ($taskRustText.Trim() -split '\s+')[0]
    if ($taskRustHash -notmatch '^[0-9a-f]{64}$') { throw 'Rustup checksum was not found.' }
    $taskRustInstaller = Get-SetupDownload $taskRustUri 'rustup-init.exe' $taskRustHash
    Invoke-SetupInstaller $taskRustInstaller @('-y','--profile','minimal','--default-host','x86_64-pc-windows-msvc','--default-toolchain','1.95.0')
}
& $taskRustup toolchain install 1.95.0 --profile minimal --component clippy --component rustfmt
if ($LASTEXITCODE -ne 0) { throw 'Rust toolchain installation failed.' }
& $taskRustup default 1.95.0
if ($LASTEXITCODE -ne 0) { throw 'Rust default toolchain selection failed.' }
& npm.cmd install -g pnpm@10.6.5
if ($LASTEXITCODE -ne 0) { throw 'pnpm installation failed.' }
Push-Location $taskWorkspace
try {
    & pnpm.cmd install --frozen-lockfile
    if ($LASTEXITCODE -ne 0) { throw 'Locked project dependency installation failed.' }
    & $taskGit --version
    & $taskNode --version
    & pnpm.cmd --version
    & cargo --version
    & rustc --version
    Write-Output 'Development dependencies are ready. Media recognition tools are prepared separately.'
} finally { Pop-Location }
