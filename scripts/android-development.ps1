param(
    [ValidateSet('Check', 'Init', 'Dev', 'Studio', 'Build')][string]$Mode = 'Dev',
    [string]$Device,
    [string]$ConfigPath
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'android-environment.ps1')
$svlAndroid = Get-SvlAndroidEnvironment -ConfigPath $ConfigPath
Show-SvlAndroidEnvironment $svlAndroid
if ($Mode -eq 'Check') { exit 0 }
$svlNames = @('JAVA_HOME', 'ANDROID_HOME', 'ANDROID_SDK_ROOT', 'NDK_HOME', 'RUSTUP_TOOLCHAIN', 'PATH')
$svlPrevious = @{}
foreach ($svlName in $svlNames) { $svlPrevious[$svlName] = [Environment]::GetEnvironmentVariable($svlName, 'Process') }
Push-Location $svlAndroid.Workspace
try {
    $env:JAVA_HOME = $svlAndroid.JavaHome
    $env:ANDROID_HOME = $svlAndroid.SdkRoot
    $env:ANDROID_SDK_ROOT = $svlAndroid.SdkRoot
    $env:NDK_HOME = $svlAndroid.NdkHome
    $env:RUSTUP_TOOLCHAIN = $svlAndroid.RustToolchain
    $env:PATH = "$env:JAVA_HOME\bin;$env:ANDROID_HOME\platform-tools;$env:PATH"
    if ($Mode -ne 'Init') {
        & node (Join-Path $PSScriptRoot 'prepare-android-assets.mjs')
        if ($LASTEXITCODE -ne 0) { throw 'Android offline resources are not ready.' }
    }
    if ($Mode -eq 'Init') {
        if (Test-Path -LiteralPath (Join-Path $svlAndroid.GeneratedProject 'settings.gradle')) { throw 'Android project already exists; initialization does not overwrite maintained sources.' }
        $svlArgs = @('android', 'init', '--skip-targets-install', '--ci')
    } elseif ($Mode -eq 'Build') {
        $svlArgs = @('android', 'build', '--debug', '--target', 'aarch64', '--apk', '--features', 'mobile')
    } else {
        # USB reverse is a development transport only. The installed app is independent.
        $svlAdb = Join-Path $svlAndroid.SdkRoot 'platform-tools/adb.exe'
        if (-not $Device) {
            $svlDevices = @(& $svlAdb devices | Select-String '^\S+\s+device$' | ForEach-Object { ($_.Line -split '\s+')[0] })
            if ($svlDevices.Count -eq 1) { $Device = $svlDevices[0] }
        }
        if ($Device) {
            & $svlAdb -s $Device reverse tcp:1420 tcp:1420
            if ($LASTEXITCODE -ne 0) { throw 'Unable to prepare USB development connection.' }
        } elseif ($Mode -eq 'Dev') { throw 'Choose a connected device with -Device before starting Android development.' }
        $svlArgs = @('android', 'dev', '--features', 'mobile', '--host', '127.0.0.1')
        if ($Mode -eq 'Studio') { $svlArgs += '--open' }
        if ($Device) { $svlArgs += $Device }
    }
    & pnpm tauri @svlArgs
    if ($LASTEXITCODE -ne 0) { throw "Android $Mode failed (exit $LASTEXITCODE)." }
} finally {
    Pop-Location
    foreach ($svlName in $svlNames) { [Environment]::SetEnvironmentVariable($svlName, $svlPrevious[$svlName], 'Process') }
}
