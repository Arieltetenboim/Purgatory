@echo off
cd /d "%~dp0"

rem Slice 1.1: build, then launch purgatory-dev-hub.exe independently.
rem cargo run is NOT used: it would keep this console alive and put the Hub
rem (and its children) in cargo's process job, so closing the shell kills the Hub
rem and can kill the dedicated server.
rem DEV.BAT remains the PowerShell fallback. Do not drive both shells at once.

set "EXE=%~dp0target\debug\purgatory-dev-hub.exe"

rem Developer Hub loads config\local\database.env itself, including when this
rem script is double-clicked. Do not put database passwords in this script.
if not exist "%~dp0config\local\database.env" (
  echo.
  echo Local database configuration is missing:
  echo   config\local\database.env
  echo Copy config\local\database.env.example and edit the passwords once.
  echo Check and Start Server need that file. Create and Reset ask for the
  echo administrator password in Developer Hub.
  echo.
)

echo Building purgatory-dev-hub...
cargo build -p purgatory-dev-hub
if errorlevel 1 (
  echo.
  echo Developer Hub build failed.
  pause
  exit /b 1
)

if not exist "%EXE%" (
  echo.
  echo Expected executable missing:
  echo   %EXE%
  pause
  exit /b 1
)

start "" /D "%~dp0" "%EXE%"
exit /b 0
