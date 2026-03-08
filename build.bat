@echo off
echo Building Daemon V1...

cargo build --release

if %errorlevel% equ 0 (
    echo.
    echo Build successful!
    echo Executable: target\release\daemon_v1.exe
    echo.
    echo Run with: target\release\daemon_v1.exe
) else (
    echo.
    echo Build failed!
    exit /b 1
)
