@echo off
setlocal
cd /d "%~dp0"

if exist "%~dp0.tools\cargo\bin\cargo.exe" (
    set "RUSTUP_HOME=%~dp0.tools\rustup"
    set "CARGO_HOME=%~dp0.tools\cargo"
    set "PATH=%~dp0.tools\cargo\bin;%PATH%"
)

where cargo >nul 2>nul
if %ERRORLEVEL% neq 0 (
    echo [ERRO] Cargo nao encontrado. Por favor, instale o Rust em https://rustup.rs
    pause
    exit /b 1
)

cargo run %*
if %ERRORLEVEL% neq 0 (
    pause
    exit /b %ERRORLEVEL%
)
