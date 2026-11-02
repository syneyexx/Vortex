@echo off
rem Vortex installeren: omgeving + alle dependencies (zie scripts\install.ps1).
rem Opties: install.bat -Gpu  of  install.bat -NoGpu
cd /d "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\install.ps1" %*
echo.
pause
