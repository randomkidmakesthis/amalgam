#!/bin/bash
# usage: gd.sh addr... ; decompiles into ghidra-scripts/out/dec/<addr>.c using project ghidra-proj2
export GHIDRA_HEADLESS_MAXMEM=4G
cd /c/GTAVI/Tools
cmd //c "ghidra_12.1.4_PUBLIC\support\analyzeHeadless.bat C:\GTAVI\Tools\ghidra-proj2 scarlett2 -process game_scarlett_master.exe -noanalysis -scriptPath C:\GTAVI\Tools\ghidra-scripts -postScript Decomp.java C:\GTAVI\Tools\ghidra-scripts\out\dec $*" > ghidra-proj2/last-run.txt 2>&1
grep -E "Decomp.java>|ERROR|Exception" ghidra-proj2/last-run.txt | cut -c1-200
