@echo off
cd /d "%~dp0"

set "PURGATORY_DT_SHOWN=%TEMP%\purgatory-dt-shown"
del "%PURGATORY_DT_SHOWN%" 2>nul

powershell.exe ^
  -NoLogo ^
  -NoProfile ^
  -STA ^
  -ExecutionPolicy Bypass ^
  -WindowStyle Hidden ^
  -File "%~dp0tools\dev\dev_launcher.ps1"

if errorlevel 1 (
  if not exist "%PURGATORY_DT_SHOWN%" (
    echo.
    echo PURGATORY Developer Tools failed to start.
    echo This is the launcher bootstrap, not a Cargo build failure.
    pause
  )
)
