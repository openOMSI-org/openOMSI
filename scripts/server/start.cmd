@echo off
rem Start the openBus dedicated server: start.cmd C:\path\to\OMSI2 [server.cfg]
rem (the OMSI 2 folder: a complete original installation, as the players have it; mods
rem go into this folder's Vehicles, maps, Sceneryobjects ... as in the game's content folder)
setlocal
set "HERE=%~dp0"
if "%~1"=="" (
  echo usage: start.cmd C:\path\to\OMSI2 [server.cfg]
  exit /b 1
)
set "CFG=%~2"
if "%CFG%"=="" set "CFG=%HERE%server.cfg"
rem (the output stays in this window, as start.sh shows it)
"%HERE%openbus.exe" --root "%~1" --server "%CFG%"
