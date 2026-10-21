@echo off
rem Copies the built files from dist\ into the Hitman: Codename 47 folder.
rem Usage: deploy.bat [game folder]
setlocal
set ROOT=%~dp0..
set GAME=%~1
if "%GAME%"=="" set GAME=E:\Games\SteamRoot\Steam\steamapps\common\Hitman Codename 47
if not exist "%GAME%\System.dll" (
  echo Game folder not found: %GAME%
  exit /b 1
)
if not exist "%GAME%\HitManVK" mkdir "%GAME%\HitManVK"
copy /y "%ROOT%\dist\HitMan-Rust.exe" "%GAME%\" || exit /b 1
copy /y "%ROOT%\dist\HitManVK\ddraw.dll" "%GAME%\HitManVK\" || exit /b 1
if not exist "%GAME%\HitMan-Rust.ini" copy /y "%ROOT%\dist\HitMan-Rust.ini" "%GAME%\" >nul
if not exist "%GAME%\HitManVK\hm1vk.ini" copy /y "%ROOT%\dist\HitManVK\hm1vk.ini" "%GAME%\HitManVK\" >nul
echo Deployed to %GAME%
