@echo off
rem The openOMSI dedicated server with questions (a double click): see run-server.ps1.
rem   run-server.cmd           the map (and the rest the first time)
rem   run-server.cmd -Setup    the folder and the settings again
rem (start.cmd stays as it was: start.cmd C:\path\to\OMSI2 [server.cfg])
setlocal
powershell -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0run-server.ps1" %*
set "CODE=%ERRORLEVEL%"
rem (a window opened by a double click stays open to show why it stopped)
if not "%CODE%"=="0" pause
exit /b %CODE%
