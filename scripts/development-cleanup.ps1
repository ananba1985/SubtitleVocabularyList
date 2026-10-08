function Invoke-SvlDevelopmentCleanup {
    param([string]$Workspace, [string]$DataDirectory, [switch]$CheckOnly)
    $svlRoot = (Resolve-Path -LiteralPath $Workspace).Path.TrimEnd('\', '/')
    $svlData = [IO.Path]::GetFullPath($DataDirectory).TrimEnd('\', '/')
    $svlComparison = [StringComparison]::OrdinalIgnoreCase
    if (@(Get-Process -Name 'SubtitleVocabularyList', 'subtitle-vocabulary-list' -ErrorAction SilentlyContinue).Count) {
        Write-Host 'Temporary cleanup deferred while a vocabulary client is running.'
        return
    }
    if ((Get-Item -LiteralPath $svlRoot -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) {
        Write-Warning 'Temporary cleanup skipped: workspace is a reparse point.'
        return
    }
    $svlTargets = @()
    foreach ($svlScope in @(
        @{ Parent = '.local'; Prefixes = @('startup-dependency-cache-', 'startup-webview-') },
        @{ Parent = 'node_modules'; Prefixes = @('.vite-startup-validation-') },
        @{ Parent = '.tools'; Prefixes = @() }
    )) {
        $svlParent = Join-Path $svlRoot $svlScope.Parent
        if (-not (Test-Path -LiteralPath $svlParent -PathType Container)) { continue }
        if ((Get-Item -LiteralPath $svlParent -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) {
            Write-Warning "Temporary cleanup skipped: parent is a reparse point ($svlParent)."
            continue
        }
        foreach ($svlPrefix in $svlScope.Prefixes) {
            foreach ($svlCandidate in @(Get-ChildItem -LiteralPath $svlParent -Directory -Force -Filter "$svlPrefix*")) {
                $svlId = [guid]::Empty
                if ([guid]::TryParseExact($svlCandidate.Name.Substring($svlPrefix.Length), 'D', [ref]$svlId)) {
                    $svlTargets += $svlCandidate.FullName
                }
            }
        }
        if ($svlScope.Parent -ne 'node_modules') {
            $svlTemporary = Join-Path $svlParent 'tmp'
            if (Test-Path -LiteralPath $svlTemporary -PathType Container) { $svlTargets += $svlTemporary }
        }
    }
    $svlRemoved = 0
    foreach ($svlTarget in $svlTargets) {
        $svlResolved = (Resolve-Path -LiteralPath $svlTarget).Path.TrimEnd('\', '/')
        if (-not $svlResolved.StartsWith($svlRoot + '\', $svlComparison)) {
            Write-Warning "Temporary cleanup skipped: target is outside workspace ($svlTarget)."
            continue
        }
        if ($svlData.Equals($svlResolved, $svlComparison) -or
            $svlData.StartsWith($svlResolved + '\', $svlComparison) -or
            $svlResolved.StartsWith($svlData + '\', $svlComparison)) {
            Write-Warning "Temporary cleanup skipped: target overlaps the selected data directory ($svlTarget)."
            continue
        }
        try {
            if (((Get-Item -LiteralPath $svlResolved -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -or
                @(Get-ChildItem -LiteralPath $svlResolved -Recurse -Force -Attributes ReparsePoint -ErrorAction Stop).Count) {
                Write-Warning "Temporary cleanup skipped: target contains a reparse point ($svlTarget)."
                continue
            }
            if ($CheckOnly) { Write-Host "Plan: remove disposable temporary directory $svlResolved"; continue }
            Remove-Item -LiteralPath $svlResolved -Recurse -Force -ErrorAction Stop
            $svlRemoved++
        } catch {
            Write-Warning "Temporary cleanup deferred: $svlResolved ($($_.Exception.Message))"
        }
    }
    if (-not $CheckOnly) { Write-Host "Development temporary directories removed: $svlRemoved" }
}
