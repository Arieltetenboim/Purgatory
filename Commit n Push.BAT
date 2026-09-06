@echo off
setlocal
cd /d "%~dp0"

echo.
echo ================================
echo   PURGATORY - COMMIT AND PUSH
echo ================================
echo.

git status --short
echo.

set /p MSG="Commit message: "

if "%MSG%"=="" (
echo.
echo ERROR: Commit message cannot be empty.
pause
exit /b 1
)

echo.
echo [1/3] Staging changes...
git add -A
if errorlevel 1 goto :error

echo.
echo [2/3] Creating commit...
git commit -m "%MSG%"
if errorlevel 1 goto :error

echo.
echo [3/3] Pushing...
git push
if errorlevel 1 goto :error

echo.
echo ================================
echo   DONE - COMMITTED AND PUSHED
echo ================================
echo.
pause
exit /b 0

:error
echo.
echo ================================
echo   ERROR - OPERATION STOPPED
echo ================================
echo.
git status --short
pause
exit /b 1
