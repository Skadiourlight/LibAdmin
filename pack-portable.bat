@echo off
setlocal
cd /d "%~dp0app\src-tauri"
rem Always build into this project. A leftover CARGO_TARGET_DIR points at a
rem temporary folder and would copy an older exe into dist.
set "CARGO_TARGET_DIR=%CD%\target"
set "EXE=%CARGO_TARGET_DIR%\release\libadmin.exe"
echo Building portable LibAdmin.exe ...
echo Output: %EXE%
cargo build --release
if errorlevel 1 exit /b 1
if not exist "%EXE%" (
  echo Build finished but the exe is missing:
  echo   %EXE%
  exit /b 1
)
mkdir "%~dp0dist" 2>nul
copy /y "%EXE%" "%~dp0dist\LibAdmin.exe"
if errorlevel 1 exit /b 1
echo.
echo Portable exe:
echo   %~dp0dist\LibAdmin.exe
echo Copy that one file. Cover tools unpack themselves on first run.
endlocal
