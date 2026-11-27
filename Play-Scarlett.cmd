@echo off
rem Starts the title with this branch's current settings (scripts\play-scarlett.ps1).
rem Drag your dump's folder (the one with game_scarlett_master.exe) onto this file the first time;
rem it is remembered in game-folder.txt, so a double-click is enough afterwards.
rem Other options: see the top of scripts\play-scarlett.ps1 (for example -RayTracing, -Hooks, -AutoStart).
setlocal
cd /d "%~dp0"
if "%~1"=="" (
  powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\play-scarlett.ps1"
) else (
  powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\play-scarlett.ps1" -Game "%~1" %2 %3 %4 %5 %6 %7 %8 %9
)
if errorlevel 1 pause
