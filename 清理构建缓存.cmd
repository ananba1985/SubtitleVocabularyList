@echo off
setlocal
for /f "tokens=2 delims=:" %%c in ('chcp') do set "svlCodePage=%%c"
chcp 65001 >nul
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0清理构建缓存.ps1" %*
set "svlExitCode=%errorlevel%"
echo.
pause
chcp %svlCodePage% >nul
exit /b %svlExitCode%
