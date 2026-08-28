@echo off
cd /d "%~dp0"

powershell.exe ^
  -NoLogo ^
  -NoProfile ^
  -STA ^
  -ExecutionPolicy Bypass ^
  -WindowStyle Hidden ^
  -File "%~dp0tools\dev_launcher.ps1"
