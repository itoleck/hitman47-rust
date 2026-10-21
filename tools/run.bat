@echo off
rem Dispatcher: runs ..\.claude\cmd.bat and records its output and exit code.
set ROOT=%~dp0..
powershell -NoProfile -Command "Get-CimInstance Win32_Process -Filter \"Name='cmd.exe'\" | Where-Object { $_.CommandLine -like '*HM1-Rust\tools\run.bat*' -and $_.CreationDate -lt (Get-Date).AddSeconds(-20) } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }" >nul 2>&1
del "%ROOT%\.claude\logs\done.txt" 2>nul
cmd /d /c call "%ROOT%\.claude\cmd.bat" > "%ROOT%\.claude\logs\last.log" 2>&1 < nul
echo %errorlevel% > "%ROOT%\.claude\logs\done.txt"
