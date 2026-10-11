@echo off
setlocal
for /f "tokens=2 delims=:" %%c in ('chcp') do set "svlCodePage=%%c"
chcp 65001 >nul
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0本地调试运行.ps1" %*
set "svlExitCode=%errorlevel%"
chcp %svlCodePage% >nul
exit /b %svlExitCode%
