param([string]$ConfigPath)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
. (Join-Path $PSScriptRoot 'android-environment.ps1')
. (Join-Path $PSScriptRoot 'native-build-command.ps1')
$svlAndroid = Get-SvlAndroidEnvironment -ConfigPath $ConfigPath
$svlRoot = $svlAndroid.Workspace
$svlSources = Join-Path $svlRoot '.tools/release-cache/sources'
$svlOutput = Join-Path $svlRoot '.local/android-ocr'
$svlPrefix = (Join-Path $svlOutput 'install').Replace('\','/')
$svlVsWhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
$svlVsRoot = (& $svlVsWhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath | Select-Object -First 1)
if (-not $svlVsRoot) { throw 'Visual Studio bundled CMake/Ninja is required for this local Android build.' }
$svlCmake = Join-Path $svlVsRoot 'Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin/cmake.exe'
$svlNinja = Join-Path $svlVsRoot 'Common7/IDE/CommonExtensions/Microsoft/CMake/Ninja/ninja.exe'
New-Item -ItemType Directory -Force $svlSources,$svlOutput,$svlPrefix | Out-Null
function Get-SvlAndroidOcrSource([string]$Name,[string]$Uri,[string]$Hash) {
    $svlZip = Join-Path $svlSources ($Name + '.zip')
    if (-not (Test-Path -LiteralPath $svlZip)) { Invoke-WebRequest -Uri $Uri -OutFile $svlZip -TimeoutSec 180 }
    if ((Get-FileHash -LiteralPath $svlZip -Algorithm SHA256).Hash -ne $Hash) { throw "Source checksum mismatch: $Name" }
    $svlSource = Join-Path $svlSources $Name
    if (-not (Test-Path -LiteralPath (Join-Path $svlSource 'CMakeLists.txt'))) { Expand-Archive -LiteralPath $svlZip -DestinationPath $svlSources }
    return $svlSource
}
$svlLeptonica = Get-SvlAndroidOcrSource 'leptonica-1.85.0' 'https://github.com/DanBloomberg/leptonica/archive/refs/tags/1.85.0.zip' '59D37884ED57988309F8C000519D4024DF51949EA332F7A6FF53A6C29B4E1DB8'
$svlTesseract = Get-SvlAndroidOcrSource 'tesseract-5.5.3' 'https://github.com/tesseract-ocr/tesseract/archive/refs/tags/5.5.3.zip' '697D7BF55B53A6C90F5041FFA548F7085AEE921DA45342C42D57B7CBCB2FA16D'
$svlCommon = @('-G','Ninja',"-DCMAKE_MAKE_PROGRAM=$svlNinja","-DCMAKE_TOOLCHAIN_FILE=$($svlAndroid.NdkHome.Replace('\','/'))/build/cmake/android.toolchain.cmake",'-DANDROID_ABI=arm64-v8a','-DANDROID_PLATFORM=android-29','-DANDROID_STL=c++_shared','-DANDROID_SUPPORT_FLEXIBLE_PAGE_SIZES=ON','-DCMAKE_BUILD_TYPE=Release','-DCMAKE_POSITION_INDEPENDENT_CODE=ON','-DBUILD_SHARED_LIBS=OFF',"-DCMAKE_INSTALL_PREFIX=$svlPrefix",'-DCMAKE_INSTALL_LIBDIR=lib',"-DCMAKE_PREFIX_PATH=$svlPrefix",'-DCMAKE_POLICY_VERSION_MINIMUM=3.10')
function Build-SvlAndroidOcr([string]$Name,[string]$Source,[string[]]$Options) {
    $svlBuild = Join-Path $svlOutput $Name
    New-Item -ItemType Directory -Force $svlBuild | Out-Null
    Invoke-SvlNativeBuild -FilePath $svlCmake -Arguments (@('-S',$Source,'-B',$svlBuild)+$svlCommon+$Options) -LogPath (Join-Path $svlBuild 'configure.log') -FailureMessage "Android OCR configure failed: $Name"
    Write-Output "Building Android $Name (2 workers); logs: $svlBuild"
    Invoke-SvlNativeBuild -FilePath $svlCmake -Arguments @('--build',$svlBuild,'--parallel','2') -LogPath (Join-Path $svlBuild 'build.log') -FailureMessage "Android OCR build failed: $Name"
    Invoke-SvlNativeBuild -FilePath $svlCmake -Arguments @('--install',$svlBuild) -LogPath (Join-Path $svlBuild 'install.log') -FailureMessage "Android OCR install failed: $Name"
}
# Android decodes PNG/JPEG/etc. into pixels; Leptonica only needs raw image operations.
Build-SvlAndroidOcr 'cpufeatures' (Join-Path $PSScriptRoot 'android-ocr-cpufeatures') @()
Build-SvlAndroidOcr 'leptonica' $svlLeptonica @('-DBUILD_PROG=OFF','-DENABLE_ZLIB=OFF','-DENABLE_PNG=OFF','-DENABLE_GIF=OFF','-DENABLE_JPEG=OFF','-DENABLE_TIFF=OFF','-DENABLE_WEBP=OFF','-DENABLE_OPENJPEG=OFF')
Build-SvlAndroidOcr 'tesseract' $svlTesseract @('-DSW_BUILD=OFF','-DBUILD_TRAINING_TOOLS=OFF','-DBUILD_TESTS=OFF','-DOPENMP_BUILD=OFF','-DGRAPHICS_DISABLED=ON','-DDISABLED_LEGACY_ENGINE=ON','-DDISABLE_ARCHIVE=ON','-DDISABLE_CURL=ON','-DDISABLE_TIFF=ON','-DENABLE_NATIVE=OFF','-DENABLE_LTO=OFF',"-DLeptonica_DIR=$svlPrefix/lib/cmake/leptonica", "-DCpuFeaturesNdkCompat_DIR=$svlPrefix/lib/cmake/CpuFeaturesNdkCompat")
$svlRuntime = Join-Path $svlAndroid.NdkHome 'toolchains/llvm/prebuilt/windows-x86_64/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so'
Copy-Item -LiteralPath $svlRuntime -Destination (Join-Path $svlPrefix 'lib/libc++_shared.so')
$svlManifest = @{ schemaVersion=1; abi='arm64-v8a'; ndk=(Split-Path $svlAndroid.NdkHome -Leaf); tesseract='5.5.3'; leptonica='1.85.0'; rawPixels=$true } | ConvertTo-Json
[IO.File]::WriteAllText((Join-Path $svlOutput 'manifest.json'), $svlManifest, [Text.UTF8Encoding]::new($false))
Write-Output "Android OCR static libraries ready: $svlPrefix/lib"
