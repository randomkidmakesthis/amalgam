#!/bin/bash
# usage: q.sh x:ADDR d:ADDR c:ADDR ...  (full project, read-only) -> out/q/xrefs.txt (appended), out/q/<entry>.c
# x = xrefs to address, c = callers+callees of containing function, d = decompile containing function.
# Never use `start` here (MSYS mangles it into a Windows error dialog). Priority: ghidra-priority.ps1 (started below).
if tasklist | grep -qi victory.exe; then export GHIDRA_HEADLESS_MAXMEM=4G; else export GHIDRA_HEADLESS_MAXMEM=12G; fi
cd /c/GTAVI/Tools
( sleep 2; powershell -NoProfile -ExecutionPolicy Bypass -File 'C:\GTAVI\Tools\ghidra-priority.ps1' -EverySeconds 1 > /dev/null 2>&1 & )
cmd //c "ghidra_12.1.4_PUBLIC\support\analyzeHeadless.bat C:\GTAVI\Tools\ghidra-proj scarlett -process game_scarlett_master.exe -readOnly -noanalysis -scriptPath C:\GTAVI\Tools\ghidra-scripts -postScript Q.java C:\GTAVI\Tools\ghidra-scripts\out\q $*" > ghidra-scripts/out/q-last.txt 2>&1
grep -E "Q.java>|ERROR|Exception" ghidra-scripts/out/q-last.txt | cut -c1-200
