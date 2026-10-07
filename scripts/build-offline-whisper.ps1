param([string]$CacheDirectory='.tools\release-cache',[string]$ResourcesDirectory='.tools\release-resources')
$ErrorActionPreference='Stop';$ProgressPreference='SilentlyContinue'
$svlCache=if([IO.Path]::IsPathRooted($CacheDirectory)){[IO.Path]::GetFullPath($CacheDirectory)}else{[IO.Path]::GetFullPath((Join-Path (Get-Location) $CacheDirectory))}
$svlResources=if([IO.Path]::IsPathRooted($ResourcesDirectory)){[IO.Path]::GetFullPath($ResourcesDirectory)}else{[IO.Path]::GetFullPath((Join-Path (Get-Location) $ResourcesDirectory))}
$svlArchive=Join-Path $svlCache 'sources\whisper.cpp-1.8.3.zip'
New-Item -ItemType Directory -Path (Split-Path $svlArchive) -Force | Out-Null
if(-not(Test-Path -LiteralPath $svlArchive)){Invoke-WebRequest -Uri https://github.com/ggml-org/whisper.cpp/archive/refs/tags/v1.8.3.zip -OutFile $svlArchive -TimeoutSec 300}
if((Get-FileHash -LiteralPath $svlArchive).Hash -ne '18901A03ED093A2563CB101BE345C155D40FB904528C03A1B12E4315A0497F1D'){throw 'Whisper source checksum mismatch.'}
$svlSource=Join-Path $svlCache 'sources\whisper.cpp-1.8.3'
if(-not(Test-Path -LiteralPath (Join-Path $svlSource 'CMakeLists.txt'))){Expand-Archive -LiteralPath $svlArchive -DestinationPath (Join-Path $svlCache 'sources')}
$svlVsWhere=Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$svlVsRoot=(& $svlVsWhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath | Select-Object -First 1)
$svlCmake=Join-Path $svlVsRoot 'Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe'
$svlBuild=Join-Path $svlCache 'whisper-build';New-Item -ItemType Directory -Path $svlBuild -Force | Out-Null
& $svlCmake -S $svlSource -B $svlBuild -G 'Visual Studio 17 2022' -A x64 -DBUILD_SHARED_LIBS=OFF -DCMAKE_POLICY_DEFAULT_CMP0091=NEW -DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded -DWHISPER_BUILD_TESTS=OFF -DWHISPER_BUILD_SERVER=OFF -DWHISPER_CURL=OFF -DWHISPER_SDL2=OFF -DGGML_NATIVE=OFF -DGGML_OPENMP=OFF -DGGML_AVX512=OFF *> (Join-Path $svlBuild 'configure.log')
if($LASTEXITCODE -ne 0){throw 'Whisper configure failed.'}
Write-Output "Building Whisper CLI (2 workers); diagnostics: $svlBuild"
& $svlCmake --build $svlBuild --config Release --target whisper-cli --parallel 2 *> (Join-Path $svlBuild 'build.log')
if($LASTEXITCODE -ne 0){throw 'Whisper build failed.'}
$svlOutput=Join-Path $svlResources 'whisper';New-Item -ItemType Directory -Path $svlOutput -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $svlBuild 'bin\Release\whisper-cli.exe') -Destination $svlOutput
Copy-Item -LiteralPath (Join-Path $svlSource 'LICENSE') -Destination (Join-Path $svlOutput 'LICENSE.txt')
Invoke-WebRequest -Uri https://raw.githubusercontent.com/openai/whisper/main/LICENSE -OutFile (Join-Path $svlOutput 'Model-LICENSE.txt') -TimeoutSec 30
$svlModel=Join-Path $svlOutput 'ggml-base.en.bin'
$svlExistingModel=Join-Path (Get-Location) '.tools\runtime\ggml-base.en.bin'
if(-not(Test-Path -LiteralPath $svlModel)){if(Test-Path -LiteralPath $svlExistingModel){Copy-Item -LiteralPath $svlExistingModel -Destination $svlModel}else{Invoke-WebRequest -Uri https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin -OutFile $svlModel -TimeoutSec 600}}
if((Get-FileHash -LiteralPath $svlModel).Hash -ne 'A03779C86DF3323075F5E796CB2CE5029F00EC8869EEE3FDFB897AFE36C6D002'){throw 'Whisper English model checksum mismatch.'}
Write-Output "Built offline English transcription resources: $svlOutput"
