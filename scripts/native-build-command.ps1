function Invoke-SvlNativeBuild {
    param(
        [string]$FilePath,
        [string[]]$Arguments,
        [string]$LogPath,
        [string]$FailureMessage
    )
    if(-not(Test-Path -LiteralPath $FilePath -PathType Leaf)){throw "Build tool not found: $FilePath"}
    $svlPreviousPreference=$ErrorActionPreference
    try {
        # Windows PowerShell 5.1 turns redirected native stderr into errors,
        # including successful CMake warnings. Keep the log and check the exit code.
        $ErrorActionPreference='Continue'
        & $FilePath @Arguments *> $LogPath
        $svlNativeExitCode=$LASTEXITCODE
    } finally {
        $ErrorActionPreference=$svlPreviousPreference
    }
    if($svlNativeExitCode -ne 0){throw "$FailureMessage (exit $svlNativeExitCode). Diagnostics: $LogPath"}
}
