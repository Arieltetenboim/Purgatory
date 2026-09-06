@echo off
setlocal
cd /d "%~dp0"

echo.
echo ================================
echo   PURGATORY - COMMIT AND PUSH
echo ================================
echo.

rem ensure we are inside a git working tree
git rev-parse --is-inside-work-tree >nul 2>&1
if errorlevel 1 (
  echo ERROR: Not a git repository here.
  pause
  exit /b 1
)

echo.
echo Current status:
git status --short
echo.

rem get current branch
for /f "usebackq delims=" %%b in (`git rev-parse --abbrev-ref HEAD`) do set "BRANCH=%%b"
echo On branch: %BRANCH%
echo.

rem fetch origin and determine upstream to compare against
echo Fetching origin...
git fetch origin
if errorlevel 1 (
  echo ERROR: git fetch origin failed.
  pause
  exit /b 1
)

set "UPSTREAM="
for /f "usebackq delims=" %%u in (`git rev-parse --abbrev-ref --symbolic-full-name @{u} 2^>nul`) do set "UPSTREAM=%%u"
if "%UPSTREAM%"=="" set "UPSTREAM=origin/master"
echo Comparing local %BRANCH% to %UPSTREAM%...
for /f "tokens=1,2" %%i in ('git rev-list --left-right --count %BRANCH%...%UPSTREAM%') do (
  set "LOCAL_UNIQ=%%i"
  set "REMOTE_UNIQ=%%j"
)
if "%REMOTE_UNIQ%"=="" set "REMOTE_UNIQ=0"
if not "%REMOTE_UNIQ%"=="0" (
  echo.
  echo ERROR: Remote has %REMOTE_UNIQ% commit(s) not present locally.
  echo Please sync (pull/rebase) and resolve any conflicts before pushing.
  pause
  exit /b 1
)

rem prompt for commit message
set /p MSG="Commit message: "
if "%MSG%"=="" (
  echo.
  echo ERROR: Commit message cannot be empty.
  pause
  exit /b 1
)

echo.
echo [1/4] Running local quality gate (scripts/check.ps1)...
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\check.ps1"
if errorlevel 1 (
  echo.
  echo ERROR: Quality gate failed. Fix issues before committing.
  pause
  exit /b 1
)

echo.
echo [2/4] Staging changes...
git add -A
if errorlevel 1 (
  echo ERROR: git add failed.
  pause
  exit /b 1
)

rem check for any staged problems (whitespace, conflict markers)
echo.
echo [3/4] Checking staged changes...
git diff --cached --check
if errorlevel 1 (
  echo.
  echo ERROR: git diff --cached --check found issues. Fix before committing.
  pause
  exit /b 1
)

rem detect if there is anything to commit
for /f "usebackq delims=" %%f in (`git diff --cached --name-only`) do set "STAGED=1"
if not defined STAGED (
  echo No changes staged / nothing to commit.
  pause
  exit /b 0
)

echo.
echo [4/4] Creating commit...
git commit -m "%MSG%"
if errorlevel 1 (
  echo.
  echo ERROR: git commit failed.
  git status --short
  pause
  exit /b 1
)

echo.
echo Pushing to origin (no force)...
git push
if errorlevel 1 (
  echo.
  echo ERROR: git push failed. Please resolve and push manually.
  git status --short
  pause
  exit /b 1
)

echo.
echo ================================
echo   DONE - COMMITTED AND PUSHED
echo ================================
echo.
pause
exit /b 0
