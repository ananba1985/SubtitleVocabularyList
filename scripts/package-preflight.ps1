function Assert-SvlPackageReady {
    param(
        [Parameter(Mandatory=$true)][string]$Workspace,
        [Parameter(Mandatory=$true)][string]$ResourcesDirectory,
        [switch]$SkipToolBuild
    )

    $svlProblems = [Collections.Generic.List[string]]::new()
    $svlCommands = @{}
    Write-Host '正在检查打包前置条件（不清理、不编译、不下载）……'
    foreach ($svlTool in @('git', 'node', 'pnpm', 'cargo', 'python', '7z')) {
        $svlCommands[$svlTool] = Get-Command $svlTool -ErrorAction SilentlyContinue
        if (-not $svlCommands[$svlTool]) {
            $svlProblems.Add("未找到开发命令 $svlTool，请按 docs/development/windows-environment.md 准备开发环境。")
        }
    }

    $svlVersion = $null
    try {
        $svlConfig = Get-Content -LiteralPath (Join-Path $Workspace 'src-tauri/tauri.conf.json') -Raw -Encoding UTF8 | ConvertFrom-Json
        $svlVersion = $svlConfig.version
        if (-not $svlVersion) { throw '未找到应用版本。' }
        if ($svlCommands['node']) {
            $svlVersionOutput = & node (Join-Path $Workspace 'scripts/version.mjs') check
            if ($LASTEXITCODE -ne 0) { throw '版本不一致，请先执行 pnpm version:check 查看并修正。' }
            Write-Host "通过：应用版本一致（$svlVersion）。"
        }
    } catch {
        $svlProblems.Add('版本／项目配置检查失败：' + $_.Exception.Message)
    }

    if ($svlCommands['git']) {
        try {
            $svlChanges = @(& git -c core.quotepath=false -C $Workspace status --porcelain=v1 --untracked-files=all)
            if ($LASTEXITCODE -ne 0) { throw '无法读取 Git 状态，请确认当前目录是完整的项目仓库。' }
            if ($svlChanges.Count) {
                throw ("源码存在未提交的文件（含已暂存、未暂存和未跟踪文件）：`n" +
                    ($svlChanges -join "`n") +
                    "`n请先核对并提交需要打包的修改，确保 git status 干净后重试；脚本不会自动提交、还原或删除这些文件。")
            }
            Write-Host '通过：源码工作区干净。'
        } catch {
            $svlProblems.Add($_.Exception.Message)
        }
    }

    if ($svlCommands['cargo']) {
        try {
            $svlTarget = Assert-SvlCargoBuildIdle -Workspace $Workspace -PassThru -ReadOnly
            $svlExpectedTarget = [IO.Path]::GetFullPath((Join-Path $Workspace 'src-tauri/target'))
            if (-not $svlTarget.Equals($svlExpectedTarget, [StringComparison]::OrdinalIgnoreCase)) {
                throw "Cargo 构建目录为 $svlTarget；安装包入口要求使用 $svlExpectedTarget，请检查 CARGO_TARGET_DIR 或 Cargo 配置。"
            }
            Write-Host "通过：Cargo 构建目录内没有运行中的程序（$svlTarget）。"
        } catch {
            $svlProblems.Add('构建目录／程序占用检查失败：' + $_.Exception.Message)
        }
    }

    foreach ($svlTool in @('tauri', 'tsc', 'vite')) {
        if (-not (Test-Path -LiteralPath (Join-Path $Workspace "node_modules/.bin/$svlTool.cmd") -PathType Leaf)) {
            $svlProblems.Add("前端依赖缺失：$svlTool。请先在项目根目录执行 pnpm install --frozen-lockfile。")
        }
    }

    $svlVsWhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if (-not (Test-Path -LiteralPath $svlVsWhere -PathType Leaf)) {
        $svlProblems.Add('未找到 Visual Studio 安装查询工具，请安装 Visual Studio 2022 Build Tools 的 C++ 开发工具。')
    } else {
        $svlVsRoot = & $svlVsWhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath | Select-Object -First 1
        if ($LASTEXITCODE -ne 0 -or -not $svlVsRoot) {
            $svlProblems.Add('未找到 Visual Studio C++ 工具链，请安装 Visual Studio 2022 Build Tools 的 C++ 开发工具。')
        } elseif (-not $SkipToolBuild) {
            foreach ($svlFile in @('Common7/Tools/VsDevCmd.bat', 'Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin/cmake.exe')) {
                if (-not (Test-Path -LiteralPath (Join-Path $svlVsRoot $svlFile) -PathType Leaf)) {
                    $svlProblems.Add("离线工具重建依赖缺失：$svlFile。请补齐 Visual Studio 的 C++／CMake 组件。")
                }
            }
        }
    }
    if (-not $SkipToolBuild -and -not (Test-Path -LiteralPath (Join-Path $env:ProgramFiles 'Git/bin/bash.exe') -PathType Leaf)) {
        $svlProblems.Add('未找到 FFmpeg 构建所需的 Git for Windows bash.exe，请按开发环境文档安装 Git。')
    }

    $svlResources = $null
    $svlRequired = @('ffmpeg/ffmpeg.exe', 'ffmpeg/ffprobe.exe', 'tesseract/tesseract.exe', 'tesseract/tessdata/eng.traineddata', 'whisper/whisper-cli.exe', 'whisper/ggml-base.en.bin', 'mpv/libmpv-2.dll', 'mpv/mpv-runtime.json', 'mpv/NOTICE.md', 'mpv/Copyright', 'mpv/LICENSE.GPL', 'mpv/LICENSE.LGPL')
    try {
        $svlResources = if ([IO.Path]::IsPathRooted($ResourcesDirectory)) {
            [IO.Path]::GetFullPath($ResourcesDirectory)
        } else {
            [IO.Path]::GetFullPath((Join-Path $Workspace $ResourcesDirectory))
        }
        $svlExpectedResources = [IO.Path]::GetFullPath((Join-Path $Workspace '.tools/release-resources'))
        if (-not $svlResources.TrimEnd('\', '/').Equals($svlExpectedResources, [StringComparison]::OrdinalIgnoreCase)) {
            throw "安装配置只映射 $svlExpectedResources，请使用默认资源目录。"
        }
        if ($SkipToolBuild) {
            foreach ($svlFile in $svlRequired | Where-Object { -not $_.StartsWith('mpv/') }) {
                if (-not (Test-Path -LiteralPath (Join-Path $svlResources $svlFile) -PathType Leaf)) {
                    $svlProblems.Add("复用的离线资源缺失：$svlFile。请去掉 -SkipToolBuild，使用默认入口准备完整资源。")
                }
            }
            Write-Host '打包模式：复用已有离线工具；已检查六个关键工具／模型文件是否存在。'
        } else {
            Write-Host '打包模式：重建 OCR、Whisper 和 FFmpeg；资源和底库由正式流程准备，预检不会下载或生成。'
        }
        Write-Host '播放器由正式流程按固定摘要准备、检查 x64 架构和 DLL 依赖；-SkipToolBuild 不跳过播放器与底库。'
    } catch {
        $svlProblems.Add('资源目录检查失败：' + $_.Exception.Message)
    }

    if ($svlProblems.Count) {
        $svlDetails = for ($svlIndex = 0; $svlIndex -lt $svlProblems.Count; $svlIndex++) {
            '{0}. {1}' -f ($svlIndex + 1), $svlProblems[$svlIndex]
        }
        throw ("打包前检查未通过：`n" + ($svlDetails -join "`n`n") + "`n`n本次未开始清理、编译或下载，请处理上述问题后重试。")
    }

    Write-Host '打包前检查通过。此结果不代替实际编译、资源摘要／许可校验或安装验收。'
    [pscustomobject]@{ Version = $svlVersion; Resources = $svlResources; RequiredResources = $svlRequired }
}
