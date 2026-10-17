function Assert-SvlCargoBuildIdle {
    param([Parameter(Mandatory=$true)][string]$Workspace)

    $svlManifest = Join-Path $Workspace 'src-tauri/Cargo.toml'
    $svlMetadataJson = & cargo metadata --manifest-path $svlManifest --no-deps --format-version 1
    if ($LASTEXITCODE -ne 0) { throw '无法确定 Cargo 构建目录，未开始清理。' }
    $svlMetadata = $svlMetadataJson | ConvertFrom-Json
    $svlTarget = [IO.Path]::GetFullPath($svlMetadata.target_directory).TrimEnd([char[]]'\/')
    $svlTargetPrefix = $svlTarget + [IO.Path]::DirectorySeparatorChar
    $svlRunning = @(Get-CimInstance Win32_Process | Where-Object {
        $_.ExecutablePath -and $_.ExecutablePath.StartsWith($svlTargetPrefix, [StringComparison]::OrdinalIgnoreCase)
    })
    if ($svlRunning.Count -eq 0) { return }

    $svlDetails = ($svlRunning | ForEach-Object { "PID $($_.ProcessId): $($_.ExecutablePath)" }) -join [Environment]::NewLine
    throw @"
本项目的编译产物仍在运行，Windows 无法在 cargo clean 时删除被占用的 exe。
$svlDetails
请保存输入，从程序托盘选择“退出”，并在本地调试终端按 Ctrl+C 结束调试后重新运行本脚本。
本次尚未开始清理或编译；不会自动终止进程，也不会操作此 Cargo 目录之外的安装版。
"@
}
