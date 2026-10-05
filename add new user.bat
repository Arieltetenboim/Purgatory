@echo off
setlocal

cd /d "%~dp0"

echo.
echo === PURGATORY - Add Development User ===
echo Rules: 2-32 chars, lowercase a-z, 0-9, _ and .
echo No leading/trailing dot and no consecutive dots.
echo.

set /p "USERNAME=Username: "

powershell -NoProfile -Command ^
  "$u='%USERNAME%'; if ($u -notmatch '^(?!\.)(?!.*\.\.)[a-z0-9_.]{2,32}(?<!\.)$') { exit 1 }"

if errorlevel 1 (
    echo.
    echo ERROR: Invalid username.
    pause
    exit /b 1
)

echo.
echo Creating user "%USERNAME%"...
echo.

cargo run -p purgatory-server -- --database-add-user --user "%USERNAME%"

if errorlevel 1 (
    echo.
    echo ERROR: User creation failed.
    pause
    exit /b 1
)

echo.
echo User "%USERNAME%" created successfully.
pause