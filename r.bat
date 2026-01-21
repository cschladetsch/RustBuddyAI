@echo off
setlocal
if not defined BUDDY_CUDA set "BUDDY_CUDA=0"
if defined CUDA_PATH (
  if exist "%CUDA_PATH%\bin" set "PATH=%CUDA_PATH%\bin;%PATH%"
)
call "%~dp0\b.bat"
if errorlevel 1 exit /b %ERRORLEVEL%
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0\scripts\ensure_deepseek.ps1" -ConfigPath "%~dp0\buddy\config.toml"
if errorlevel 1 exit /b %ERRORLEVEL%
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0\scripts\ensure_whisper_model.ps1" -ConfigPath "%~dp0\buddy\config.toml"
if errorlevel 1 exit /b %ERRORLEVEL%
set "BUDDY_EXE=%~dp0\buddy\target\x86_64-pc-windows-msvc\release\buddy.exe"
if not exist "%BUDDY_EXE%" (
    echo Buddy executable not found at %BUDDY_EXE%
    exit /b 1
)
"%BUDDY_EXE%" "%~dp0\buddy\config.toml" %*
