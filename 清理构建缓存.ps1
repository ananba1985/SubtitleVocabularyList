param([switch]$CheckOnly)
$ErrorActionPreference = 'Stop'
$svlPreviousPath = $env:Path
try {
    $svlRoot = (Resolve-Path -LiteralPath $PSScriptRoot).Path.TrimEnd('\', '/')
    $env:Path = (Join-Path $env:USERPROFILE '.cargo/bin') + ';' + $env:Path
    . (Join-Path $svlRoot 'scripts/build-preflight.ps1')
    $svlTarget = Assert-SvlCargoBuildIdle -Workspace $svlRoot -PassThru
    $svlExpected = [IO.Path]::GetFullPath((Join-Path $svlRoot 'src-tauri/target'))
    $svlComparison = [StringComparison]::OrdinalIgnoreCase
    if (-not $svlTarget.Equals($svlExpected, $svlComparison)) {
        throw "仅支持本项目 src-tauri/target，当前 Cargo 目录为 $svlTarget，未清理。"
    }
    foreach ($svlParent in @($svlRoot, (Join-Path $svlRoot 'src-tauri'), $svlTarget)) {
        if ((Test-Path -LiteralPath $svlParent) -and
            ((Get-Item -LiteralPath $svlParent -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw "路径含目录链接，未清理：$svlParent"
        }
    }
    $svlTargets = @()
    foreach ($svlProfile in @('debug', 'release')) {
        $svlProfilePath = Join-Path $svlTarget $svlProfile
        if (-not (Test-Path -LiteralPath $svlProfilePath -PathType Container)) { continue }
        if ((Get-Item -LiteralPath $svlProfilePath -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) {
            throw "路径含目录链接，未清理：$svlProfilePath"
        }
        foreach ($svlCache in @('incremental', 'deps', 'build', '.fingerprint')) {
            $svlPath = Join-Path $svlProfilePath $svlCache
            if (-not (Test-Path -LiteralPath $svlPath -PathType Container)) { continue }
            $svlResolved = (Resolve-Path -LiteralPath $svlPath).Path
            if (-not $svlResolved.StartsWith($svlTarget + '\', $svlComparison)) {
                throw "缓存不在本项目构建目录内，未清理：$svlResolved"
            }
            if ($env:SVL_DATA_DIR) {
                $svlData = [IO.Path]::GetFullPath($env:SVL_DATA_DIR).TrimEnd('\', '/')
                if ($svlData.Equals($svlResolved, $svlComparison) -or
                    $svlData.StartsWith($svlResolved + '\', $svlComparison) -or
                    $svlResolved.StartsWith($svlData + '\', $svlComparison)) {
                    throw "缓存与 SVL_DATA_DIR 相交，未清理：$svlResolved"
                }
            }
            if ((Get-Item -LiteralPath $svlResolved -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw "缓存是目录链接，未清理：$svlResolved"
            }
            $svlChildren = @(Get-ChildItem -LiteralPath $svlResolved -Recurse -Force -ErrorAction Stop)
            if (@($svlChildren | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }).Count) {
                throw "缓存包含目录链接，未清理：$svlResolved"
            }
            $svlBytes = [long](($svlChildren | Where-Object { -not $_.PSIsContainer } | Measure-Object -Property Length -Sum).Sum)
            $svlTargets += [pscustomobject]@{ Path = $svlResolved; Bytes = $svlBytes }
        }
    }
    Write-Host '仅清理 Cargo 的 incremental、deps、build、.fingerprint 缓存，不重新编译。'
    Write-Host '保留源码、工具、词库、原声、学习历史、Data、安装包和验证资料。'
    $svlRemovedBytes = 0L
    foreach ($svlItem in $svlTargets) {
        Write-Host ('{0}  ({1:N2} GiB)' -f $svlItem.Path, ($svlItem.Bytes / 1GB))
        if (-not $CheckOnly) {
            Remove-Item -LiteralPath $svlItem.Path -Recurse -Force -ErrorAction Stop
            $svlRemovedBytes += $svlItem.Bytes
        }
    }
    if ($CheckOnly) { Write-Host '预览完成，没有删除任何文件。' }
    else { Write-Host ('清理完成，移除文件大小合计约 {0:N2} GiB。下次编译需要重新生成缓存。' -f ($svlRemovedBytes / 1GB)) }
    exit 0
} catch {
    Write-Host ('清理未全部完成：' + $_.Exception.Message) -ForegroundColor Red
    exit 1
} finally {
    $env:Path = $svlPreviousPath
}
