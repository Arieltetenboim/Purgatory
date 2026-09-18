@echo off
setlocal EnableExtensions DisableDelayedExpansion
cd /d "%~dp0"
title PURGATORY - Git Helper

rem =================================================
rem Verify repository before showing the menu
rem =================================================
git rev-parse --is-inside-work-tree >nul 2>&1
if errorlevel 1 goto :helper_not_repo

:helper_main_menu
cls

set "BRANCH="
for /f "delims=" %%B in ('git branch --show-current') do set "BRANCH=%%B"
if not defined BRANCH goto :helper_detached_head

set "MENU_FILE=%TEMP%\purgatory_git_menu_%RANDOM%_%RANDOM%.txt"
if exist "%MENU_FILE%" del /q "%MENU_FILE%" >nul 2>&1

powershell.exe -NoProfile -ExecutionPolicy Bypass -Command ^
  "$items=@('PULL','PUSH  (commit + push)','COMPARE BRANCHES');" ^
  "$i=0;" ^
  "[Console]::CursorVisible=$false;" ^
  "try {" ^
  "  while($true){" ^
  "    Clear-Host;" ^
  "    Write-Host 'PURGATORY - GIT HELPER' -ForegroundColor Cyan;" ^
  "    Write-Host ('Current branch: %BRANCH%');" ^
  "    Write-Host '';" ^
  "    Write-Host 'Use Up/Down arrows, Enter to select, Esc to cancel.';" ^
  "    Write-Host '';" ^
  "    for($n=0;$n -lt $items.Count;$n++){" ^
  "      if($n -eq $i){ Write-Host ('> ' + $items[$n]) -ForegroundColor Black -BackgroundColor Gray }" ^
  "      else { Write-Host ('  ' + $items[$n]) }" ^
  "    }" ^
  "    $k=[Console]::ReadKey($true);" ^
  "    switch($k.Key){" ^
  "      'UpArrow'   { if($i -gt 0){$i--} else {$i=$items.Count-1} }" ^
  "      'DownArrow' { if($i -lt $items.Count-1){$i++} else {$i=0} }" ^
  "      'Enter'     { Set-Content -LiteralPath '%MENU_FILE%' -Value ($i+1) -NoNewline -Encoding ASCII; exit 0 }" ^
  "      'Escape'    { exit 20 }" ^
  "    }" ^
  "  }" ^
  "} finally { [Console]::CursorVisible=$true }"

set "MENU_RESULT=%ERRORLEVEL%"
if "%MENU_RESULT%"=="20" goto :helper_cancelled
if not "%MENU_RESULT%"=="0" goto :helper_menu_failed
if not exist "%MENU_FILE%" goto :helper_menu_failed

set "ACTION="
set /p ACTION=<"%MENU_FILE%"
del /q "%MENU_FILE%" >nul 2>&1

if "%ACTION%"=="1" goto :helper_pull_mode
if "%ACTION%"=="2" goto :push_mode
if "%ACTION%"=="3" goto :helper_compare_mode
goto :helper_menu_failed


rem =================================================
rem PULL
rem =================================================
:helper_pull_mode
cls
echo.
echo ============================================
echo   PURGATORY - PULL
echo ============================================
echo.

call :helper_require_clean_worktree
if errorlevel 1 (
    pause
    goto :helper_main_menu
)

echo Fetching origin...
git fetch origin --prune
if errorlevel 1 goto :helper_fetch_failed

rem PULL lists origin branches only, because a pull needs a remote source.
call :helper_select_branch "SELECT BRANCH TO PULL" "0" "1"
if errorlevel 20 goto :helper_main_menu
if errorlevel 1 goto :helper_branch_select_failed

echo.
echo ============================================
echo   CONFIRM PULL
echo ============================================
echo.
echo Current branch : %BRANCH%
echo Target branch  : %TARGET%
echo.
choice /C YN /N /M "Switch to this branch and pull from origin? [Y/N]: "
if errorlevel 2 goto :helper_main_menu

git show-ref --verify --quiet "refs/remotes/origin/%TARGET%"
if errorlevel 1 goto :helper_pull_remote_missing

echo.
echo Switching to "%TARGET%"...

git show-ref --verify --quiet "refs/heads/%TARGET%"
if not errorlevel 1 (
    git switch "%TARGET%"
) else (
    git switch --track "origin/%TARGET%"
)
if errorlevel 1 goto :helper_switch_failed

echo.
echo Pulling latest changes from origin/%TARGET%...
git pull --ff-only origin "%TARGET%"
if errorlevel 1 goto :helper_pull_failed

echo.
echo ============================================
echo   DONE - PULL COMPLETE
echo ============================================
echo.
echo Branch: %TARGET%
git log -1 --oneline
echo.
pause
goto :helper_main_menu


rem =================================================
rem COMPARE BRANCHES
rem =================================================
:helper_compare_mode
cls
echo.
echo ============================================
echo   PURGATORY - COMPARE BRANCHES
echo ============================================
echo.

set "BRANCH="
for /f "delims=" %%B in ('git branch --show-current') do set "BRANCH=%%B"
if not defined BRANCH goto :helper_detached_head

echo Fetching origin so remote branch data is current...
git fetch origin --prune
if errorlevel 1 goto :helper_fetch_failed

rem COMPARE lists the union of local and origin branches, excluding current.
call :helper_select_branch "SELECT BRANCH TO COMPARE" "1" "0"
if errorlevel 20 goto :helper_main_menu
if errorlevel 1 goto :helper_branch_select_failed

set "TARGET_REF="

rem Prefer the local branch if it exists. Otherwise compare to origin/<branch>.
git show-ref --verify --quiet "refs/heads/%TARGET%"
if not errorlevel 1 set "TARGET_REF=%TARGET%"

if not defined TARGET_REF (
    git show-ref --verify --quiet "refs/remotes/origin/%TARGET%"
    if not errorlevel 1 set "TARGET_REF=origin/%TARGET%"
)

if not defined TARGET_REF goto :helper_compare_target_missing

set "AHEAD_COUNT="
set "BEHIND_COUNT="
for /f "tokens=1,2" %%A in ('git rev-list --left-right --count "%BRANCH%...%TARGET_REF%"') do (
    set "AHEAD_COUNT=%%A"
    set "BEHIND_COUNT=%%B"
)

if not defined AHEAD_COUNT goto :helper_compare_failed
if not defined BEHIND_COUNT goto :helper_compare_failed

cls
echo.
echo ============================================
echo   BRANCH COMPARISON
echo ============================================
echo.
echo Current: %BRANCH%
echo Target : %TARGET_REF%
echo.
echo Commits only on current : %AHEAD_COUNT%
echo Commits only on target  : %BEHIND_COUNT%
echo.

echo ============================================
echo   COMMITS ONLY ON CURRENT: %BRANCH%
echo ============================================
git log --oneline --decorate "%TARGET_REF%..%BRANCH%"
if errorlevel 1 goto :helper_compare_failed
echo.

echo ============================================
echo   COMMITS ONLY ON TARGET: %TARGET_REF%
echo ============================================
git log --oneline --decorate "%BRANCH%..%TARGET_REF%"
if errorlevel 1 goto :helper_compare_failed
echo.

echo ============================================
echo   FILE SUMMARY
echo ============================================
git diff --stat "%BRANCH%" "%TARGET_REF%"
if errorlevel 1 goto :helper_compare_failed
echo.

echo ============================================
echo   CHANGED FILES
echo   A=Added  M=Modified  D=Deleted  R=Renamed
echo ============================================
git diff --name-status "%BRANCH%" "%TARGET_REF%"
if errorlevel 1 goto :helper_compare_failed
echo.

echo ============================================
echo   COMPARE COMPLETE
echo ============================================
echo.
echo No branch was switched and no files were changed.
echo.
pause
goto :helper_main_menu


rem =================================================
rem Helper: require clean worktree before switching
rem =================================================
:helper_require_clean_worktree
git diff --quiet
if errorlevel 1 goto :helper_dirty_worktree
git diff --cached --quiet
if errorlevel 1 goto :helper_dirty_worktree
for /f "delims=" %%A in ('git ls-files --others --exclude-standard') do goto :helper_dirty_worktree
exit /b 0

:helper_dirty_worktree
echo.
echo [STOP] Local changes were found.
echo Commit, stash, or discard them before switching branches.
echo.
git status --short
echo.
exit /b 1


rem =================================================
rem Helper: arrow-key branch selector
rem %~1 = title
rem %~2 = exclude current branch? 1=yes
rem %~3 = remote branches only? 1=yes
rem Returns TARGET
rem =================================================
:helper_select_branch
set "TARGET="
set "SELECT_FILE=%TEMP%\purgatory_branch_%RANDOM%_%RANDOM%.txt"
if exist "%SELECT_FILE%" del /q "%SELECT_FILE%" >nul 2>&1

powershell.exe -NoProfile -ExecutionPolicy Bypass -Command ^
  "$ErrorActionPreference='Stop';" ^
  "$out='%SELECT_FILE%';" ^
  "$title='%~1';" ^
  "$excludeCurrent='%~2' -eq '1';" ^
  "$remoteOnly='%~3' -eq '1';" ^
  "$current=(git branch --show-current).Trim();" ^
  "$local=@(git for-each-ref --format='%%(refname:short)' refs/heads);" ^
  "$remote=@(git for-each-ref --format='%%(refname:short)' refs/remotes/origin | Where-Object { $_ -ne 'origin/HEAD' } | ForEach-Object { $_ -replace '^origin/','' });" ^
  "if($remoteOnly){ $branches=@($remote | Sort-Object -Unique) } else { $branches=@($local + $remote | Sort-Object -Unique) };" ^
  "if($excludeCurrent){ $branches=@($branches | Where-Object { $_ -ne $current }) };" ^
  "if($branches.Count -eq 0){ Write-Host 'No eligible branches found.' -ForegroundColor Red; exit 10 };" ^
  "$i=0;" ^
  "[Console]::CursorVisible=$false;" ^
  "try {" ^
  "  while($true){" ^
  "    Clear-Host;" ^
  "    Write-Host $title -ForegroundColor Cyan;" ^
  "    Write-Host ('Current: ' + $current);" ^
  "    Write-Host 'Use Up/Down arrows, Enter to select, Esc to cancel.';" ^
  "    Write-Host '';" ^
  "    for($n=0;$n -lt $branches.Count;$n++){" ^
  "      if($n -eq $i){ Write-Host ('> ' + $branches[$n]) -ForegroundColor Black -BackgroundColor Gray }" ^
  "      else { Write-Host ('  ' + $branches[$n]) }" ^
  "    }" ^
  "    $k=[Console]::ReadKey($true);" ^
  "    switch($k.Key){" ^
  "      'UpArrow'   { if($i -gt 0){$i--} else {$i=$branches.Count-1} }" ^
  "      'DownArrow' { if($i -lt $branches.Count-1){$i++} else {$i=0} }" ^
  "      'Enter'     { Set-Content -LiteralPath $out -Value $branches[$i] -NoNewline -Encoding UTF8; exit 0 }" ^
  "      'Escape'    { exit 20 }" ^
  "    }" ^
  "  }" ^
  "} finally { [Console]::CursorVisible=$true }"

set "SELECT_RESULT=%ERRORLEVEL%"
if "%SELECT_RESULT%"=="20" (
    if exist "%SELECT_FILE%" del /q "%SELECT_FILE%" >nul 2>&1
    exit /b 20
)
if not "%SELECT_RESULT%"=="0" (
    if exist "%SELECT_FILE%" del /q "%SELECT_FILE%" >nul 2>&1
    exit /b 1
)
if not exist "%SELECT_FILE%" exit /b 1

set /p TARGET=<"%SELECT_FILE%"
del /q "%SELECT_FILE%" >nul 2>&1

if not defined TARGET exit /b 1
exit /b 0


rem =================================================
rem Helper errors
rem =================================================
:helper_not_repo
echo.
echo ERROR: This BAT is not running inside a Git repository.
pause
exit /b 1

:helper_detached_head
echo.
echo ERROR: Git is in detached HEAD state.
echo No operation was performed.
pause
exit /b 1

:helper_cancelled
echo.
echo Operation cancelled.
exit /b 0

:helper_menu_failed
echo.
echo ERROR: Could not read the selected operation.
pause
goto :helper_main_menu

:helper_fetch_failed
echo.
echo ERROR: git fetch origin failed.
echo Check network access and the origin remote.
pause
goto :helper_main_menu

:helper_branch_select_failed
echo.
echo ERROR: Branch selection failed.
pause
goto :helper_main_menu

:helper_pull_remote_missing
echo.
echo ERROR: origin/%TARGET% does not exist.
echo Nothing was switched or pulled.
pause
goto :helper_main_menu

:helper_switch_failed
echo.
echo ERROR: Could not switch to "%TARGET%".
pause
goto :helper_main_menu

:helper_pull_failed
echo.
echo ERROR: Pull failed.
echo.
echo No automatic merge, rebase, reset, stash,
echo or conflict resolution was attempted.
echo The pull uses --ff-only.
pause
goto :helper_main_menu

:helper_compare_target_missing
echo.
echo ERROR: Could not resolve branch "%TARGET%".
pause
goto :helper_main_menu

:helper_compare_failed
echo.
echo ERROR: Could not compare "%BRANCH%" with "%TARGET_REF%".
pause
goto :helper_main_menu


rem =================================================
rem Existing PUSH flow starts here.
rem Kept from the uploaded Commit n Push.BAT.
rem =================================================
:push_mode
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