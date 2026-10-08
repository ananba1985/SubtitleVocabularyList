function Get-SvlDevelopmentEnvironment {
    param([string]$DataDirectory = '.local\dev-data')
    $svlRoot = Split-Path $PSScriptRoot -Parent
    $svlData = if ([IO.Path]::IsPathRooted($DataDirectory)) { [IO.Path]::GetFullPath($DataDirectory) } else { [IO.Path]::GetFullPath((Join-Path $svlRoot $DataDirectory)) }
    $svlRuntime = Join-Path $svlRoot '.tools/runtime'
    $svlPaths = @((Join-Path $env:USERPROFILE '.cargo/bin'), (Join-Path $env:ProgramFiles 'nodejs'), (Join-Path $env:APPDATA 'npm'), (Join-Path $env:ProgramFiles 'Tesseract-OCR'))
    $svlFfmpeg = Join-Path $svlRuntime 'ffmpeg'
    if (Test-Path -LiteralPath $svlFfmpeg) {
        $svlPaths += Join-Path $svlFfmpeg 'bin'
        $svlPaths += Get-ChildItem -LiteralPath $svlFfmpeg -Directory | ForEach-Object { Join-Path $_.FullName 'bin' }
    }
    $svlPaths = @($svlPaths | Where-Object { Test-Path -LiteralPath $_ -PathType Container })
    [pscustomobject]@{
        Workspace = $svlRoot
        DataDirectory = $svlData
        RuntimeDirectory = $svlRuntime
        Path = ($svlPaths -join ';') + ';' + $env:Path
        Config = Join-Path $svlRoot 'src-tauri/tauri.development.conf.json'
    }
}

function Show-SvlDevelopmentEnvironment {
    param($Environment)
    Write-Host "Workspace: $($Environment.Workspace)"
    Write-Host "Development data: $($Environment.DataDirectory)"
    Write-Host "Development media: $($Environment.RuntimeDirectory) and the local PATH"
    foreach ($svlTool in @('pnpm', 'cargo', 'ffmpeg', 'ffprobe', 'tesseract')) {
        $svlCommand = Get-Command $svlTool -ErrorAction SilentlyContinue
        if ($svlCommand) { Write-Host "${svlTool}: $($svlCommand.Source)" }
        elseif ($svlTool -in @('pnpm', 'cargo')) { throw "Missing development tool: $svlTool" }
        else { Write-Host "${svlTool}: not found; prepare the local media environment when needed." }
    }
    foreach ($svlFile in @('whisper/Release/whisper-cli.exe', 'ggml-base.en.bin')) {
        Write-Host "$svlFile available: $(Test-Path -LiteralPath (Join-Path $Environment.RuntimeDirectory $svlFile))"
    }
}
