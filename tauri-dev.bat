@echo off
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1
set PATH=d:\env\rust\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin;C:\Users\Spence\.cargo\bin;%PATH%
set RUSTUP_HOME=d:\env\rust\rustup
set HTTPS_PROXY=socks5://127.0.0.1:10808
cd /d "d:\Users\Spence\Desktop\FileSweep"
echo === Installing Tauri CLI ===
call npm install -g @tauri-apps/cli@latest
echo === Starting Tauri Dev Mode ===
call npx tauri dev
pause
