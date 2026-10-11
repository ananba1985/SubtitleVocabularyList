param([Parameter(Mandatory=$true)][string]$SourceDirectory, [string]$DataDirectory='D:\SubtitleVocabularyList\Data')
$ErrorActionPreference='Stop'
$svlSource=(Resolve-Path -LiteralPath $SourceDirectory).Path
$svlTarget=[IO.Path]::GetFullPath($DataDirectory)
if ($svlSource -eq $svlTarget -or $svlTarget.StartsWith($svlSource+'\',[StringComparison]::OrdinalIgnoreCase) -or $svlSource.StartsWith($svlTarget+'\',[StringComparison]::OrdinalIgnoreCase)) { throw 'Source and target must be separate directories.' }
if (Test-Path -LiteralPath $svlTarget) { throw 'Target already exists; no data was overwritten.' }
if (Get-Process -Name 'SubtitleVocabularyList','subtitle-vocabulary-list' -ErrorAction SilentlyContinue) { throw 'Exit all clients before migrating.' }
$svlLinks=@(Get-Item -LiteralPath $svlSource -Force; Get-ChildItem -LiteralPath $svlSource -Force -Recurse) | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }
if ($svlLinks) { throw 'Source contains reparse points; no directory was moved.' }
$svlBackupRoot=Join-Path (Split-Path $svlTarget -Parent) ('migration-'+[Guid]::NewGuid().ToString())
$svlBackup=Join-Path $svlBackupRoot 'before.sqlite3'
& python (Join-Path $PSScriptRoot 'data-migration.py') snapshot (Join-Path $svlSource 'vocabulary.sqlite3') $svlBackup
if ($LASTEXITCODE -ne 0) { throw 'Cannot snapshot the source database.' }
# Absolute paths and link checks above precede the directory move.
Move-Item -LiteralPath $svlSource -Destination $svlTarget
& python (Join-Path $PSScriptRoot 'data-migration.py') retarget (Join-Path $svlTarget 'vocabulary.sqlite3') $svlBackup --old-root $svlSource --new-root $svlTarget
if ($LASTEXITCODE -ne 0) { throw "Migration verification failed. Source snapshot remains at $svlBackup" }
$svlBackups=Join-Path $svlTarget 'backups'
New-Item -ItemType Directory -Path $svlBackups -Force | Out-Null
$svlFinalBackup=Join-Path $svlBackups (Split-Path $svlBackupRoot -Leaf)
Move-Item -LiteralPath $svlBackupRoot -Destination $svlFinalBackup
Write-Output "Data directory: $svlTarget"
Write-Output "Original database snapshot: $svlFinalBackup\before.sqlite3"
