@echo off
setlocal EnableExtensions
cd /d "%~dp0"

echo.
echo ================================
echo   PURGATORY - COMMIT AND PUSH
echo ================================
echo.

rem =================================================
rem Verify repository
rem =================================================
git rev-parse --is-inside-work-tree >nul 2>&1
if errorlevel 1 goto :not_repo

echo Current status:
git status --short
if errorlevel 1 goto :status_failed
echo.

rem =================================================
rem Resolve current branch
rem =================================================
set "BRANCH="
for /f "delims=" %%B in ('git branch --show-current') do set "BRANCH=%%B"

if not defined BRANCH goto :detached_head

echo Branch: %BRANCH%
echo.

rem =================================================
rem Fetch origin
rem =================================================
echo [1/7] Fetching origin...
git fetch origin
if errorlevel 1 goto :fetch_failed

rem =================================================
rem Resolve upstream
rem =================================================
set "UPSTREAM="
for /f "delims=" %%U in ('git rev-parse --abbrev-ref --symbolic-full-name "@{u}" 2^>nul') do set "UPSTREAM=%%U"

if not defined UPSTREAM goto :no_upstream

echo       Upstream: %UPSTREAM%

rem =================================================
rem Refuse if remote is ahead
rem =================================================
set "BEHIND="
for /f "delims=" %%C in ('git rev-list --count "HEAD..%UPSTREAM%"') do set "BEHIND=%%C"

if not defined BEHIND goto :compare_failed
if not "%BEHIND%"=="0" goto :remote_ahead

echo       Branch is synchronized.
echo.

rem =================================================
rem Choose commit mode
rem =================================================
echo ================================
echo   SELECT COMMIT MODE
echo ================================
echo.
echo   [1] SAFE
echo       Run full PURGATORY quality gate.
echo       Use for code, tests, Cargo, scripts,
echo       protocol, schemas, runtime changes, etc.
echo.
echo   [2] FAST
echo       Skip the full quality gate.
echo       Use only for small changes such as docs,
echo       README, reports, wording, or safe metadata.
echo.

set "MODE="
set /p "MODE=Choose [1/2]: "

if "%MODE%"=="1" goto :mode_safe
if "%MODE%"=="2" goto :mode_fast

echo.
echo ERROR: Invalid selection.
goto :failed


:mode_safe
set "RUN_GATE=1"
echo.
echo SAFE mode selected.
goto :stage_changes


:mode_fast
set "RUN_GATE=0"

echo.
echo ================================
echo   FAST MODE WARNING
echo ================================
echo.
echo The full PURGATORY quality gate will NOT run.
echo.
echo Git sync checks and staged-diff checks still run.
echo Use FAST only when the change does not require
echo build, runtime, lint, or test validation.
echo.

set "CONFIRM_FAST="
set /p "CONFIRM_FAST=Continue with FAST mode? [Y/N]: "

if /I "%CONFIRM_FAST%"=="Y" goto :stage_changes
if /I "%CONFIRM_FAST%"=="YES" goto :stage_changes

echo.
echo FAST mode cancelled.
goto :cancelled


:stage_changes
rem =================================================
rem Stage changes
rem =================================================
echo.
echo [2/7] Staging changes...
git add -A
if errorlevel 1 goto :stage_failed

rem =================================================
rem Verify staged diff
rem =================================================
echo.
echo [3/7] Checking staged diff...
git diff --cached --check
if errorlevel 1 goto :diff_failed

rem Detect whether anything is staged.
git diff --cached --quiet
if not errorlevel 1 goto :nothing_to_commit

echo.
echo Staged changes:
echo --------------------------------
git status --short
echo --------------------------------
echo.

rem =================================================
rem Quality gate
rem =================================================
if "%RUN_GATE%"=="1" goto :run_quality_gate

echo [4/7] Full quality gate skipped - FAST mode.
goto :after_quality_gate


:run_quality_gate
echo [4/7] Running PURGATORY quality gate...
echo.

powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\check.ps1"
if errorlevel 1 goto :quality_failed

echo.
echo       Quality gate passed.


:after_quality_gate
rem =================================================
rem Commit message
rem =================================================
echo.
set "MSG="
set /p "MSG=Commit message: "

if not defined MSG goto :empty_message

rem =================================================
rem Final confirmation
rem =================================================
echo.
echo ================================
echo   READY TO COMMIT
echo ================================
echo.
echo Branch:  %BRANCH%
echo Upstream: %UPSTREAM%

if "%RUN_GATE%"=="1" (
    echo Mode:    SAFE
) else (
    echo Mode:    FAST
)

echo Message: %MSG%
echo.

set "CONFIRM_COMMIT="
set /p "CONFIRM_COMMIT=Commit and push these staged changes? [Y/N]: "

if /I "%CONFIRM_COMMIT%"=="Y" goto :commit_changes
if /I "%CONFIRM_COMMIT%"=="YES" goto :commit_changes

echo.
echo Commit cancelled.
echo Changes remain staged.
goto :cancelled


:commit_changes
rem =================================================
rem Commit
rem =================================================
echo.
echo [5/7] Creating commit...
git commit -m "%MSG%"
if errorlevel 1 goto :commit_failed

rem =================================================
rem Push
rem =================================================
echo.
echo [6/7] Pushing to %UPSTREAM%...
git push
if errorlevel 1 goto :push_failed

rem =================================================
rem Final status
rem =================================================
echo.
echo [7/7] Final repository status...
git status --short
if errorlevel 1 goto :final_status_failed

echo.
echo ================================
echo   DONE - COMMITTED AND PUSHED
echo ================================
echo.

if "%RUN_GATE%"=="1" (
    echo Mode: SAFE - full quality gate passed.
) else (
    echo Mode: FAST - full quality gate was skipped.
)

echo.
pause
exit /b 0


rem =================================================
rem Safe exits / errors
rem =================================================

:not_repo
echo.
echo ERROR: This BAT is not running inside a Git repository.
goto :failed

:status_failed
echo.
echo ERROR: Could not read Git status.
goto :failed

:detached_head
echo.
echo ERROR: Git is in detached HEAD state.
echo No commit or push was performed.
goto :failed

:fetch_failed
echo.
echo ERROR: git fetch origin failed.
echo Check network access and the origin remote.
goto :failed

:no_upstream
echo.
echo ERROR: Branch "%BRANCH%" has no upstream configured.
echo.
echo Configure it explicitly first, for example:
echo   git push -u origin %BRANCH%
echo.
echo No commit or push was performed.
goto :failed

:compare_failed
echo.
echo ERROR: Could not compare HEAD with "%UPSTREAM%".
goto :failed

:remote_ahead
echo.
echo ERROR: Remote contains %BEHIND% commit(s)
echo that are not present locally.
echo.
echo Sync/rebase manually before committing or pushing.
echo No automatic rebase, reset, stash, force push,
echo or conflict resolution was attempted.
goto :failed

:stage_failed
echo.
echo ERROR: git add -A failed.
goto :failed

:diff_failed
echo.
echo ERROR: git diff --cached --check found problems.
echo Fix the reported whitespace/conflict-marker issue first.
goto :failed

:nothing_to_commit
echo.
echo ================================
echo   NOTHING TO COMMIT
echo ================================
echo.
pause
exit /b 0

:quality_failed
echo.
echo ERROR: PURGATORY quality gate failed.
echo.
echo No commit was created.
echo The changes remain staged so you can inspect/fix them.
goto :failed

:empty_message
echo.
echo ERROR: Commit message cannot be empty.
echo Changes remain staged.
goto :failed

:commit_failed
echo.
echo ERROR: git commit failed.
echo.
git status --short
goto :failed

:push_failed
echo.
echo ERROR: Commit was created locally, but git push failed.
echo.
echo IMPORTANT:
echo The local commit was NOT reset, removed, amended,
echo rebased, or force-pushed.
echo.
git status --short
goto :failed

:final_status_failed
echo.
echo WARNING: Commit and push succeeded,
echo but final git status could not be read.
goto :failed

:cancelled
echo.
echo ================================
echo   OPERATION CANCELLED
echo ================================
echo.
pause
exit /b 0

:failed
echo.
echo ================================
echo   OPERATION STOPPED SAFELY
echo ================================
echo.
pause
exit /b 1