param([string]$CacheDirectory='.tools\release-cache',[string]$ResourcesDirectory='.tools\release-resources')
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$svlCache=if([IO.Path]::IsPathRooted($CacheDirectory)){[IO.Path]::GetFullPath($CacheDirectory)}else{[IO.Path]::GetFullPath((Join-Path (Get-Location) $CacheDirectory))}
$svlResources=if([IO.Path]::IsPathRooted($ResourcesDirectory)){[IO.Path]::GetFullPath($ResourcesDirectory)}else{[IO.Path]::GetFullPath((Join-Path (Get-Location) $ResourcesDirectory))}
$svlArchive=Join-Path $svlCache 'sources\FFmpeg-n9.0.2.zip'
New-Item -ItemType Directory -Path (Split-Path $svlArchive) -Force | Out-Null
if(-not(Test-Path -LiteralPath $svlArchive)){Invoke-WebRequest -Uri https://github.com/FFmpeg/FFmpeg/archive/refs/tags/n9.0.2.zip -OutFile $svlArchive -TimeoutSec 300}
if((Get-FileHash -LiteralPath $svlArchive -Algorithm SHA256).Hash -ne '6441B27421B2F06CDB7DAFA4FF38325D97E1B477A730B853185AE1A4F6817AD6'){throw 'FFmpeg source checksum mismatch.'}
$svlSource=Join-Path $svlCache 'sources\FFmpeg-n9.0.2'
if(-not(Test-Path -LiteralPath (Join-Path $svlSource 'configure'))){Expand-Archive -LiteralPath $svlArchive -DestinationPath (Join-Path $svlCache 'sources')}
# MSVC without its optional English language pack prefixes the Microsoft banner
# with localized text. Match the real compiler brand anywhere in that banner.
$svlConfigure=Join-Path $svlSource 'configure'
$svlConfigureText=[IO.File]::ReadAllText($svlConfigure).Replace('grep -q ^Microsoft',"grep -q 'Microsoft'").Replace('grep ^Microsoft',"grep 'Microsoft'")
[IO.File]::WriteAllText($svlConfigure,$svlConfigureText,[Text.UTF8Encoding]::new($false))
# Source archives have no Git metadata; keep the actual upstream RELEASE value
# instead of allowing version.sh to discover the parent application's Git HEAD.
Copy-Item -LiteralPath (Join-Path $svlSource 'RELEASE') -Destination (Join-Path $svlSource 'VERSION') -Force
$svlVsWhere=Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$svlVsRoot=(& $svlVsWhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath | Select-Object -First 1)
$svlVsCmd=Join-Path $svlVsRoot 'Common7\Tools\VsDevCmd.bat'
$svlEnvironment=& cmd.exe /d /c ('call "'+$svlVsCmd+'" -no_logo -arch=x64 >nul && set')
if($LASTEXITCODE -ne 0){throw 'C++ build environment could not be initialized.'}
foreach($svlLine in $svlEnvironment){$svlEquals=$svlLine.IndexOf('=');if($svlEquals -gt 0){$svlName=$svlLine.Substring(0,$svlEquals);if($svlName -in @('PATH','INCLUDE','LIB','LIBPATH','VSINSTALLDIR','VCToolsInstallDir','WindowsSdkDir','WindowsSDKVersion','UCRTVersion')){[Environment]::SetEnvironmentVariable($svlName,$svlLine.Substring($svlEquals+1),'Process')}}}
$svlBash=Join-Path $env:ProgramFiles 'Git\bin\bash.exe'
$svlBuildTools=Join-Path $svlCache 'build-tools'
$svlMsys=Join-Path $svlBuildTools 'msys2'
New-Item -ItemType Directory -Path $svlMsys -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $svlMsys 'etc'),(Join-Path $svlMsys 'tmp') -Force | Out-Null
[IO.File]::WriteAllText((Join-Path $svlMsys 'etc\fstab'),"none / cygdrive binary,posix=0,noacl,user 0 0`n",[Text.UTF8Encoding]::new($false))
$env:MSYSTEM='MSYS'
foreach($svlPackage in @(@{name='make.pkg.tar.zst';url='https://repo.msys2.org/msys/x86_64/make-4.4.1-3-x86_64.pkg.tar.zst';hash='AF0BDBA17F06FE037F0194069ADAA31A8FE45F1A11381501896AEA1FAE37BD5D'},@{name='msys2-runtime.pkg.tar.zst';url='https://repo.msys2.org/msys/x86_64/msys2-runtime-3.6.10-6-x86_64.pkg.tar.zst';hash='B2DB5BAE3826F15CACB536F4C7C6C7E31D3FABEC00AAC8F127473ABA7E994D22'})){
    $svlPackagePath=Join-Path $svlBuildTools $svlPackage.name
    if(-not(Test-Path -LiteralPath $svlPackagePath)){Invoke-WebRequest -Uri $svlPackage.url -OutFile $svlPackagePath -TimeoutSec 180}
    if((Get-FileHash -LiteralPath $svlPackagePath).Hash -ne $svlPackage.hash){throw 'MSYS build helper checksum mismatch.'}
    & "$env:WINDIR\System32\tar.exe" -xf $svlPackagePath -C $svlMsys
    if($LASTEXITCODE -ne 0){throw 'MSYS build helper extraction failed.'}
}
foreach($svlDll in @('msys-intl-8.dll','msys-iconv-2.dll')){Copy-Item -LiteralPath (Join-Path $env:ProgramFiles ('Git\usr\bin\'+$svlDll)) -Destination (Join-Path $svlMsys 'usr\bin') -Force}
Copy-Item -LiteralPath (Join-Path $env:ProgramFiles 'Git\usr\bin\sh.exe') -Destination (Join-Path $svlMsys 'usr\bin') -Force
$svlMake=Join-Path $svlMsys 'usr\bin\make.exe'
$svlZlibPrefix=Join-Path $svlCache 'ocr-install'
if(-not(Test-Path -LiteralPath (Join-Path $svlZlibPrefix 'lib\zlibstatic.lib'))){throw 'Build the offline OCR dependencies first.'}
$svlStaticLibraryDirectory=Join-Path $svlCache 'ffmpeg-static-libs'
New-Item -ItemType Directory -Path $svlStaticLibraryDirectory -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $svlZlibPrefix 'lib\zlibstatic.lib') -Destination (Join-Path $svlStaticLibraryDirectory 'zlib.lib') -Force
Copy-Item -LiteralPath (Join-Path $svlZlibPrefix 'lib\zlibstatic.lib') -Destination (Join-Path $svlStaticLibraryDirectory 'z.lib') -Force
$svlZlibHeaders=Join-Path $svlCache 'ffmpeg-zlib-headers'
New-Item -ItemType Directory -Path $svlZlibHeaders -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $svlZlibPrefix 'include\zlib.h'),(Join-Path $svlZlibPrefix 'include\zconf.h') -Destination $svlZlibHeaders -Force
# FFmpeg's HAVE_UNISTD_H can activate zlib's optional POSIX header. MSVC's
# static build must omit that include, as required by FFmpeg's Windows guide.
$svlZconf=Join-Path $svlZlibHeaders 'zconf.h'
[IO.File]::WriteAllText($svlZconf,[IO.File]::ReadAllText($svlZconf).Replace('#    include <unistd.h>','/* POSIX unistd.h is not used in the MSVC media build. */'),[Text.UTF8Encoding]::new($false))
$env:INCLUDE=$svlZlibHeaders+';'+$env:INCLUDE
$env:LIB=$svlStaticLibraryDirectory+';'+$env:LIB
$env:VSLANG='1033'
$env:PATH=(Join-Path $env:ProgramFiles 'Git\usr\bin')+';'+$env:PATH
$svlBuild=Join-Path $svlCache 'ffmpeg-build'
New-Item -ItemType Directory -Path $svlBuild -Force | Out-Null
$svlScript=Join-Path $svlBuild 'build.sh'
$svlBuildScript=@'
set -eu
source_path=$(cygpath -u "$1")
make_path=$(cygpath -m "$2")
shell_path=$(cygpath -m "$(command -v sh)")
if [ ! -f ffbuild/config.mak ]; then
  "$source_path/configure" --toolchain=msvc --arch=x86_64 --target-os=win64 \
    --disable-autodetect --disable-network --disable-shared --enable-static \
    --disable-x86asm --disable-doc --disable-debug --disable-avdevice --disable-ffplay \
    --enable-zlib > configure-output.log 2>&1
fi
echo "Building offline media tools (2 workers); diagnostics: $PWD/build.log"
'@
[IO.File]::WriteAllText($svlScript,$svlBuildScript.Replace("`r`n","`n"),[Text.UTF8Encoding]::new($false))
Push-Location $svlBuild
try{
    & $svlBash ./build.sh $svlSource $svlMake
    if($LASTEXITCODE -ne 0){throw 'Offline FFmpeg configure failed.'}
    # Restore POSIX source paths if this directory was prepared by native make.
    $svlUnixSource=(& $svlBash -c 'cygpath -u "$1"' -- $svlSource).Trim()
    foreach($svlMakeFile in @('Makefile','ffbuild/config.mak')){
        $svlGenerated=Join-Path $svlBuild $svlMakeFile
        $svlMakeText=[IO.File]::ReadAllText($svlGenerated).Replace($svlSource.Replace('\','/'),$svlUnixSource)
        [IO.File]::WriteAllText($svlGenerated,$svlMakeText,[Text.UTF8Encoding]::new($false))
    }
    $svlDependencyScript=@'
match($0, /[A-Za-z]:[\\\/]/) {
    path=substr($0,RSTART)
    gsub(/\\/,"/",path)
    sub(/[\r ]+$/,"",path)
    if (path ~ /\.(h|hpp|hxx|inc)$/ && path !~ / /) print target ":", path
}
'@
    [IO.File]::WriteAllText((Join-Path $svlBuild 'ffbuild/msvc-deps.awk'),$svlDependencyScript.Replace("`r`n","`n"),[Text.UTF8Encoding]::new($false))
    $svlConfigFile=Join-Path $svlBuild 'ffbuild/config.mak'
    $svlConfigLines=[IO.File]::ReadAllLines($svlConfigFile)
    for($svlIndex=0;$svlIndex -lt $svlConfigLines.Length;$svlIndex++){
        if($svlConfigLines[$svlIndex] -match '^(CCDEP|CXXDEP|ASDEP|HOSTCCDEP)='){
            $svlKey=$Matches[1]
            $svlConfigLines[$svlIndex]=$svlKey+'=$(DEP$(1)) $(DEP$(1)FLAGS) $($(1)DEP_FLAGS) $< 2>&1 | awk -v target="$@" -f ffbuild/msvc-deps.awk > $(@:.o=.d)'
        }
    }
    [IO.File]::WriteAllText($svlConfigFile,($svlConfigLines -join "`n")+"`n",[Text.UTF8Encoding]::new($false))
    $svlShell=(Join-Path $svlMsys 'usr\bin\sh.exe').Replace('\','/')
    & $svlMake -j2 -W fftools/ffmpeg.o -W fftools/ffprobe.o "SHELL=$svlShell" ffmpeg.exe ffprobe.exe *> (Join-Path $svlBuild 'build.log')
    if($LASTEXITCODE -ne 0){throw 'Offline FFmpeg build failed.'}
}finally{Pop-Location}
$svlOutput=Join-Path $svlResources 'ffmpeg'
New-Item -ItemType Directory -Path (Join-Path $svlOutput 'licenses') -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $svlBuild 'ffmpeg.exe'),(Join-Path $svlBuild 'ffprobe.exe') -Destination $svlOutput
Copy-Item -LiteralPath (Join-Path $svlSource 'COPYING.LGPLv2.1') -Destination (Join-Path $svlOutput 'licenses\FFmpeg-LICENSE.txt')
Copy-Item -LiteralPath (Join-Path $svlBuild 'ffbuild/config.mak') -Destination (Join-Path $svlOutput 'build-config.txt')
Write-Output "Built offline media tools: $svlOutput"
