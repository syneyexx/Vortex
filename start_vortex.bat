@echo off
rem Vortex starten: worker, modelservers, kern + GUI (zie scripts\start.ps1).
cd /d "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\start.ps1" %*
if errorlevel 1 pause
