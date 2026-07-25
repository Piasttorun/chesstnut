@echo off
REM Double-click launcher for Chesstnut. The Rust/Cargo toolchain lives in
REM WSL, not on the Windows host, so this just forwards into it rather than
REM requiring `wsl.exe -e bash -lc "cd ... && cargo tauri dev"` to be typed
REM by hand every time. Keeps the console window open (via the final pause)
REM so build errors or a crash are visible instead of the window vanishing.
title Chesstnut
wsl.exe -e bash -lc "cd /mnt/c/Users/mnmat/work_dir/chesstnut && cargo tauri dev"
pause
