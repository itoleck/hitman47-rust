@echo off
rem Builds HitMan-Rust.exe and HitManVK\ddraw.dll (32-bit, release) into dist\
setlocal
set ROOT=%~dp0..
cd /d "%ROOT%"
set CARGO=%USERPROFILE%\.cargo\bin\cargo.exe
set RUSTUP=%USERPROFILE%\.cargo\bin\rustup.exe
rem Hitman is a 32-bit game: build for i686 with a static CRT (no VC++ redistributable needed).
set RUSTFLAGS=-C target-feature=+crt-static
"%RUSTUP%" target add i686-pc-windows-msvc || exit /b 1
"%CARGO%" build --release --target i686-pc-windows-msvc || exit /b 1
if not exist dist\HitManVK mkdir dist\HitManVK
copy /y target\i686-pc-windows-msvc\release\HitMan-Rust.exe dist\ >nul || exit /b 1
copy /y target\i686-pc-windows-msvc\release\ddraw.dll dist\HitManVK\ >nul || exit /b 1
copy /y dist-files\HitMan-Rust.ini dist\ >nul
copy /y dist-files\hm1vk.ini dist\HitManVK\ >nul
echo Build OK
dir /b /s dist
