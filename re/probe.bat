@echo off
set LOG=%~dp0..\.claude\logs\probe.log
(
echo === cargo
"%USERPROFILE%\.cargo\bin\cargo.exe" --version
"%USERPROFILE%\.cargo\bin\rustup.exe" show
"%USERPROFILE%\.cargo\bin\rustup.exe" target list --installed
echo === vswhere
"%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" -all -products * -format text -property installationPath
echo === vulkan
echo VULKAN_SDK=%VULKAN_SDK%
where vulkan-1.dll
dir /b %SystemRoot%\SysWOW64\vulkan-1.dll %SystemRoot%\System32\vulkan-1.dll
where glslc glslangValidator vulkaninfo
echo === java
dir /b "%ProgramFiles%\Microsoft"
echo === gpu
wmic path win32_VideoController get name,CurrentHorizontalResolution,CurrentVerticalResolution,DriverVersion
echo === node
where node npm git
echo === done
) > "%LOG%" 2>&1
