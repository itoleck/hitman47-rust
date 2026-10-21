@echo off
rem Imports the Hitman binaries into a Ghidra project and exports decompiled C.
rem Usage: ghidra_analyze.bat [projectDir projectName]  (default C:\Users\itole HM1)
set GHIDRA=C:\Users\itole\Source\Repos\others\ghidra\release-12_1_4
set GAME=E:\Games\SteamRoot\Steam\steamapps\common\Hitman Codename 47
set OUT=%~dp0..\.claude\ghidra_out
set PDIR=%~1
set PNAME=%~2
if "%PDIR%"=="" set PDIR=C:\Users\itole
if "%PNAME%"=="" set PNAME=HM1
if not exist "%PDIR%" mkdir "%PDIR%"
set LOG=%~dp0..\.claude\logs\ghidra.log
if not exist "%OUT%" mkdir "%OUT%"
echo started %date% %time% > "%LOG%"
for %%F in (Hitman.Exe System.dll Globals.dll RenderD3D.dll RenderOpenGL.dll EngineData.dll Sound.dll) do (
  echo ==== %%F >> "%LOG%"
  call "%GHIDRA%\support\analyzeHeadless.bat" "%PDIR%" %PNAME% -import "%GAME%\%%F" -overwrite -scriptPath "%~dp0ghidra_scripts" -postScript ExportAll.java "%OUT%" >> "%LOG%" 2>&1
)
echo finished %date% %time% >> "%LOG%"
