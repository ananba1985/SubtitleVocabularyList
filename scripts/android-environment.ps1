function Get-SvlAndroidEnvironment {
    param([string]$ConfigPath)
    $svlRoot = Split-Path $PSScriptRoot -Parent
    $svlLock = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'android-toolchain.json') -Raw | ConvertFrom-Json
    if (-not $ConfigPath) { $ConfigPath = Join-Path $svlRoot '.local/android-development.json' }
    $svlLocal = if (Test-Path -LiteralPath $ConfigPath) { Get-Content -LiteralPath $ConfigPath -Raw | ConvertFrom-Json } else { $null }
    $svlSdk = if ($svlLocal -and $svlLocal.sdkRoot) { $svlLocal.sdkRoot } elseif ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { Join-Path $env:LOCALAPPDATA 'Android/Sdk' }
    $svlJava = if ($svlLocal -and $svlLocal.javaHome) { $svlLocal.javaHome } else { $env:JAVA_HOME }
    $svlMissing = [System.Collections.Generic.List[string]]::new()
    foreach ($svlPart in @('platform-tools/adb.exe', 'cmdline-tools/latest/bin/sdkmanager.bat', "platforms/android-$($svlLock.compileSdk)/android.jar", "ndk/$($svlLock.ndkVersion)/toolchains/llvm/prebuilt/windows-x86_64/bin/clang.exe")) {
        if (-not (Test-Path -LiteralPath (Join-Path $svlSdk $svlPart))) { $svlMissing.Add("SDK: $svlPart") }
    }
    if (-not $svlJava -or -not (Test-Path -LiteralPath (Join-Path $svlJava 'bin/java.exe'))) {
        $svlMissing.Add('JDK 21: configure javaHome in .local/android-development.json')
    } else {
        $svlRelease = Get-Content -LiteralPath (Join-Path $svlJava 'release') -Raw
        if ($svlRelease -notmatch ('JAVA_VERSION="' + $svlLock.javaMajor + '\.')) { $svlMissing.Add("JDK: expected major $($svlLock.javaMajor)") }
    }
    $svlTargets = @(& rustup target list --installed --toolchain $svlLock.rustToolchain 2>$null)
    if ($LASTEXITCODE -ne 0 -or $svlLock.rustTarget -notin $svlTargets) { $svlMissing.Add("Rust: rustup target add $($svlLock.rustTarget) --toolchain $($svlLock.rustToolchain)") }
    [pscustomobject]@{
        Workspace = $svlRoot
        SdkRoot = $svlSdk
        JavaHome = $svlJava
        NdkHome = Join-Path $svlSdk "ndk/$($svlLock.ndkVersion)"
        RustToolchain = $svlLock.rustToolchain
        Missing = $svlMissing.ToArray()
        GeneratedProject = Join-Path $svlRoot 'src-tauri/gen/android'
    }
}

function Show-SvlAndroidEnvironment {
    param($Environment)
    Write-Host "Workspace: $($Environment.Workspace)"
    Write-Host "Android SDK: $($Environment.SdkRoot)"
    Write-Host "Gradle JDK: $($Environment.JavaHome)"
    Write-Host "Android NDK: $($Environment.NdkHome)"
    Write-Host "Rust toolchain: $($Environment.RustToolchain)"
    Write-Host "Android project initialized: $(Test-Path -LiteralPath (Join-Path $Environment.GeneratedProject 'settings.gradle'))"
    foreach ($svlItem in $Environment.Missing) { Write-Host "Missing: $svlItem" }
    if ($Environment.Missing.Count) { throw 'Android development environment is incomplete; the check did not install or change tools.' }
}
