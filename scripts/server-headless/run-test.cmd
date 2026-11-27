@echo off
setlocal
cd /d "%~dp0"
echo.
echo openOMSI native headless TEST host - CPU only
echo This package is EXPERIMENTAL. AI, timetables, passengers, uploads and map sharing are disabled.
echo.
if not exist "%~dp0omsi-server.exe" (
    echo ERROR: omsi-server.exe is missing from this package.
    pause
    exit /b 1
)
if not exist "%~dp0server-test.cfg" (
    echo ERROR: server-test.cfg is missing.
    pause
    exit /b 1
)
if not "%~1"=="" (
    set "OMSI_ROOT=%~1"
) else (
    set /p "OMSI_ROOT=Enter the full OMSI 2 installation directory: "
)
set "OMSI_ROOT=%OMSI_ROOT:"=%"
if not exist "%OMSI_ROOT%\Maps" (
    echo ERROR: Could not find the Maps directory under "%OMSI_ROOT%".
    echo Select your OMSI 2 install folder, not a map folder.
    pause
    exit /b 1
)
echo.
echo Starting isolated LAN test. Type stop in the console to shut down cleanly.
echo Clients need the same installed map and buses. No files are shared.
echo.
"%~dp0omsi-server.exe" "%OMSI_ROOT%" "%~dp0server-test.cfg" --experimental-free-roam
set "CODE=%ERRORLEVEL%"
echo.
echo Server exited with code %CODE%.
pause
exit /b %CODE%
