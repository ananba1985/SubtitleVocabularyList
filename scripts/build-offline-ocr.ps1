param([string]$CacheDirectory='.tools\release-cache', [string]$ResourcesDirectory='.tools\release-resources')
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$svlCache=if([IO.Path]::IsPathRooted($CacheDirectory)){[IO.Path]::GetFullPath($CacheDirectory)}else{[IO.Path]::GetFullPath((Join-Path (Get-Location) $CacheDirectory))}
$svlResources=if([IO.Path]::IsPathRooted($ResourcesDirectory)){[IO.Path]::GetFullPath($ResourcesDirectory)}else{[IO.Path]::GetFullPath((Join-Path (Get-Location) $ResourcesDirectory))}
$svlSources=Join-Path $svlCache 'sources'
$svlPrefix=(Join-Path $svlCache 'ocr-install').Replace('\','/')
New-Item -ItemType Directory -Path $svlSources,$svlPrefix,$svlResources -Force | Out-Null
$svlVsWhere=Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$svlVsRoot=(& $svlVsWhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath | Select-Object -First 1)
if(-not $svlVsRoot){throw 'Visual Studio C++ toolchain is required for offline resource builds.'}
$svlCmake=Join-Path $svlVsRoot 'Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe'
if(-not(Test-Path -LiteralPath $svlCmake)){throw 'The bundled Visual Studio CMake was not found.'}
function Get-SvlSource([string]$Name,[string]$Uri,[string]$Hash){
    $svlArchive=Join-Path $svlSources ($Name+'.zip')
    if(-not(Test-Path -LiteralPath $svlArchive)){Invoke-WebRequest -Uri $Uri -OutFile $svlArchive -TimeoutSec 300}
    if((Get-FileHash -LiteralPath $svlArchive -Algorithm SHA256).Hash -ne $Hash){throw "Source checksum mismatch: $Name"}
    $svlSource=Join-Path $svlSources $Name
    if(-not(Test-Path -LiteralPath (Join-Path $svlSource 'CMakeLists.txt'))){Expand-Archive -LiteralPath $svlArchive -DestinationPath $svlSources}
    return $svlSource
}
function Invoke-SvlCmake([string]$Name,[string]$Source,[string[]]$Options){
    $svlBuild=Join-Path $svlCache ('ocr-build\'+$Name)
    New-Item -ItemType Directory -Path $svlBuild -Force | Out-Null
    & $svlCmake -S $Source -B $svlBuild -G 'Visual Studio 17 2022' -A x64 "-DCMAKE_INSTALL_PREFIX=$svlPrefix" "-DCMAKE_PREFIX_PATH=$svlPrefix" '-DCMAKE_POLICY_DEFAULT_CMP0091=NEW' '-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded' -DBUILD_SHARED_LIBS=OFF @Options *> (Join-Path $svlBuild 'configure.log')
    if($LASTEXITCODE -ne 0){throw "Configure failed: $Name"}
    Write-Output "Building $Name (2 workers); diagnostics: $svlBuild"
    & $svlCmake --build $svlBuild --config Release --parallel 2 *> (Join-Path $svlBuild 'build.log')
    if($LASTEXITCODE -ne 0){throw "Build failed: $Name"}
    & $svlCmake --install $svlBuild --config Release *> (Join-Path $svlBuild 'install.log')
    if($LASTEXITCODE -ne 0){throw "Install failed: $Name"}
}
$svlZlib=Get-SvlSource 'zlib-1.3.1' 'https://github.com/madler/zlib/archive/refs/tags/v1.3.1.zip' '50B24B47BF19E1F35D2A21FF36D2A366638CDF958219A66F30CE0861201760E6'
Invoke-SvlCmake 'zlib' $svlZlib @('-DZLIB_BUILD_EXAMPLES=OFF',"-DINSTALL_BIN_DIR=$svlPrefix/bin","-DINSTALL_LIB_DIR=$svlPrefix/lib","-DINSTALL_INC_DIR=$svlPrefix/include","-DINSTALL_MAN_DIR=$svlPrefix/share/man","-DINSTALL_PKGCONFIG_DIR=$svlPrefix/share/pkgconfig")
$svlZlibLibrary=(Join-Path $svlPrefix 'lib\zlibstatic.lib').Replace('\','/')
if(-not(Test-Path -LiteralPath $svlZlibLibrary)){throw 'Static zlib was not produced.'}
$svlPng=Get-SvlSource 'libpng-1.6.43' 'https://github.com/pnggroup/libpng/archive/refs/tags/v1.6.43.zip' '5E18474A26814AE479E02CA6432DA32D19DC6E615551D140C954A68D63B3F192'
Invoke-SvlCmake 'png' $svlPng @('-DPNG_SHARED=OFF','-DPNG_STATIC=ON','-DPNG_TESTS=OFF','-DPNG_TOOLS=OFF','-DPNG_EXECUTABLES=OFF',"-DZLIB_LIBRARY=$svlZlibLibrary","-DZLIB_INCLUDE_DIR=$svlPrefix\include")
$svlLeptonica=Get-SvlSource 'leptonica-1.85.0' 'https://github.com/DanBloomberg/leptonica/archive/refs/tags/1.85.0.zip' '59D37884ED57988309F8C000519D4024DF51949EA332F7A6FF53A6C29B4E1DB8'
$svlJpeg=Get-SvlSource 'libjpeg-turbo-3.0.1' 'https://github.com/libjpeg-turbo/libjpeg-turbo/archive/refs/tags/3.0.1.zip' 'D6D99E693366BC03897677650E8B2DFA76B5D6C54E2C9E70C03F0AF821B0A52F'
Invoke-SvlCmake 'jpeg' $svlJpeg @('-DENABLE_SHARED=OFF','-DENABLE_STATIC=ON','-DWITH_SIMD=OFF','-DWITH_JAVA=OFF','-DWITH_TURBOJPEG=OFF')
$svlJpegLibrary=(Join-Path $svlPrefix 'lib\jpeg-static.lib').Replace('\','/')
$svlTiff=Get-SvlSource 'tiff-4.6.0' 'https://download.osgeo.org/libtiff/tiff-4.6.0.zip' '9FC11AD1BF2636BDC97FA88F8C609E484BBA3CF5F9DF73BE31AE5E6D78ED0E20'
Invoke-SvlCmake 'tiff' $svlTiff @('-Dtiff-tools=OFF','-Dtiff-tests=OFF','-Dtiff-contrib=OFF','-Dtiff-docs=OFF','-Djpeg=ON','-Dlibdeflate=OFF','-Dlzma=OFF','-Dzstd=OFF','-Dwebp=OFF','-Djbig=OFF','-Dlerc=OFF',"-DZLIB_LIBRARY=$svlZlibLibrary","-DZLIB_INCLUDE_DIR=$svlPrefix\include","-DJPEG_LIBRARY=$svlJpegLibrary","-DJPEG_INCLUDE_DIR=$svlPrefix\include")
foreach($svlTiffExport in (Get-ChildItem -LiteralPath (Join-Path $svlPrefix 'lib/cmake/tiff') -Filter '*.cmake')){
    $svlTiffText=[IO.File]::ReadAllText($svlTiffExport.FullName).Replace('ZLIB::ZLIB',$svlZlibLibrary).Replace('JPEG::JPEG',$svlJpegLibrary)
    [IO.File]::WriteAllText($svlTiffExport.FullName,$svlTiffText,[Text.UTF8Encoding]::new($false))
}
Invoke-SvlCmake 'leptonica' $svlLeptonica @('-DSW_BUILD=OFF','-DBUILD_PROG=OFF','-DENABLE_GIF=OFF','-DENABLE_JPEG=ON','-DENABLE_TIFF=ON','-DENABLE_WEBP=OFF','-DENABLE_OPENJPEG=OFF',"-DZLIB_LIBRARY=$svlZlibLibrary","-DZLIB_INCLUDE_DIR=$svlPrefix\include","-DJPEG_LIBRARY=$svlJpegLibrary","-DJPEG_INCLUDE_DIR=$svlPrefix\include")
# The exported static TIFF dependency keeps an imported ZLIB target that is not
# re-created by Tesseract's isolated try_run. Bind it to our actual static file.
$svlLeptTargets=Join-Path $svlPrefix 'lib\cmake\leptonica\LeptonicaTargets.cmake'
$svlLeptExports=[IO.File]::ReadAllText($svlLeptTargets).Replace('ZLIB::ZLIB',$svlZlibLibrary.Replace('\','/')).Replace('JPEG::JPEG',$svlJpegLibrary.Replace('\','/'))
[IO.File]::WriteAllText($svlLeptTargets,$svlLeptExports,[Text.UTF8Encoding]::new($false))
$svlTesseract=Get-SvlSource 'tesseract-5.5.3' 'https://github.com/tesseract-ocr/tesseract/archive/refs/tags/5.5.3.zip' '697D7BF55B53A6C90F5041FFA548F7085AEE921DA45342C42D57B7CBCB2FA16D'
Invoke-SvlCmake 'tesseract' $svlTesseract @('-DSW_BUILD=OFF','-DBUILD_TRAINING_TOOLS=OFF','-DBUILD_TESTS=OFF','-DWIN32_MT_BUILD=ON','-DOPENMP_BUILD=OFF','-DGRAPHICS_DISABLED=ON','-DDISABLE_ARCHIVE=ON','-DDISABLE_CURL=ON','-DDISABLE_TIFF=OFF','-DENABLE_NATIVE=OFF')
$svlOcrOutput=Join-Path $svlResources 'tesseract'
New-Item -ItemType Directory -Path (Join-Path $svlOcrOutput 'tessdata'),(Join-Path $svlOcrOutput 'licenses') -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $svlPrefix 'bin\tesseract.exe') -Destination $svlOcrOutput
Copy-Item -LiteralPath (Join-Path $svlTesseract 'LICENSE') -Destination (Join-Path $svlOcrOutput 'licenses\Tesseract-LICENSE.txt')
Copy-Item -LiteralPath (Join-Path $svlLeptonica 'leptonica-license.txt') -Destination (Join-Path $svlOcrOutput 'licenses\Leptonica-LICENSE.txt')
Copy-Item -LiteralPath (Join-Path $svlPng 'LICENSE') -Destination (Join-Path $svlOcrOutput 'licenses\Libpng-LICENSE.txt')
Copy-Item -LiteralPath (Join-Path $svlZlib 'LICENSE') -Destination (Join-Path $svlOcrOutput 'licenses\Zlib-LICENSE.txt')
Copy-Item -LiteralPath (Join-Path $svlTiff 'LICENSE.md') -Destination (Join-Path $svlOcrOutput 'licenses\Libtiff-LICENSE.txt')
Copy-Item -LiteralPath (Join-Path $svlJpeg 'LICENSE.md') -Destination (Join-Path $svlOcrOutput 'licenses\Libjpeg-LICENSE.txt')
Copy-Item -LiteralPath (Join-Path $svlJpeg 'README.ijg') -Destination (Join-Path $svlOcrOutput 'licenses\Libjpeg-IJG.txt')
$svlEnglishData=Join-Path $svlCache 'eng.traineddata'
if(-not(Test-Path -LiteralPath $svlEnglishData)){Invoke-WebRequest -Uri https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/4.1.0/eng.traineddata -OutFile $svlEnglishData -TimeoutSec 180}
if((Get-FileHash -LiteralPath $svlEnglishData -Algorithm SHA256).Hash -ne '7D4322BD2A7749724879683FC3912CB542F19906C83BCC1A52132556427170B2'){throw 'English OCR data checksum mismatch.'}
Copy-Item -LiteralPath $svlEnglishData -Destination (Join-Path $svlOcrOutput 'tessdata\eng.traineddata')
$svlEnglishLicense=Join-Path $svlOcrOutput 'licenses\English-data-LICENSE.txt'
Invoke-WebRequest -Uri https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/4.1.0/LICENSE -OutFile $svlEnglishLicense -TimeoutSec 30
Write-Output "Built offline OCR executable: $svlOcrOutput"
