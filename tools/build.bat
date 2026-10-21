@echo off
rem Builds HitMan-Rust.exe and HitManVK\ddraw.dll (32-bit, release) into dist\
rem Every run bumps the patch number in version.txt (file and product version of
rem HitMan-Rust.exe and HitManVK\ddraw.dll); a failed build puts the old number back.
setlocal
set ROOT=%~dp0..
cd /d "%ROOT%"
set CARGO=%USERPROFILE%\.cargo\bin\cargo.exe
set RUSTUP=%USERPROFILE%\.cargo\bin\rustup.exe

rem --- version: major.minor.patch, patch + 1 ---
set VERFILE=%ROOT%\version.txt
if not exist "%VERFILE%" (echo 0.0.0)> "%VERFILE%"
set OLDVER=
set /p OLDVER=< "%VERFILE%"
set MAJ=& set MIN=& set PAT=
for /f "tokens=1-3 delims=. " %%a in ("%OLDVER%") do (
  set MAJ=%%a
  set MIN=%%b
  set PAT=%%c
)
if "%PAT%"=="" (
  echo version.txt must contain major.minor.patch, found "%OLDVER%"
  exit /b 1
)
set /a PAT=%PAT%+1
set NEWVER=%MAJ%.%MIN%.%PAT%
(echo %NEWVER%)> "%VERFILE%"
echo Version %OLDVER% -^> %NEWVER%

rem Hitman is a 32-bit game: build for i686 with a static CRT (no VC++ redistributable needed).
set RUSTFLAGS=-C target-feature=+crt-static
"%RUSTUP%" target add i686-pc-windows-msvc || goto :failed
"%CARGO%" build --release --target i686-pc-windows-msvc || goto :failed
if not exist dist\HitManVK mkdir dist\HitManVK
copy /y target\i686-pc-windows-msvc\release\HitMan-Rust.exe dist\ >nul || goto :failed
copy /y target\i686-pc-windows-msvc\release\ddraw.dll dist\HitManVK\ >nul || goto :failed
copy /y dist-files\HitMan-Rust.ini dist\ >nul
copy /y dist-files\hm1vk.ini dist\HitManVK\ >nul
echo Build OK (HitMan-Rust.exe %NEWVER%)
dir /b /s dist
exit /b 0

:failed
(echo %OLDVER%)> "%VERFILE%"
echo Build FAILED, version.txt left at %OLDVER%
exit /b 1
