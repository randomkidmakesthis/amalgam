#!/bin/bash
# Batch export for a later review of topics T1-T7. No model needed. Run from Git Bash:
#   bash /c/GTAVI/Tools/ghidra-scripts/export-topics.sh [cap=250] [onlyTopic]
# Requires: the full auto-analysis in ghidra-proj (project "scarlett") has FINISHED and released its lock
# (no java.exe with "ghidra" in its command line; ghidra-proj\scarlett.lock gone). Opens that project read-only.
# Output: ghidra-scripts/out/topics/T1.txt .. T7.txt (+ console log out/topics/export-console.txt)
# and out/topics/raw-*.txt from the capstone tools (these do not need Ghidra at all).
set -u
T=/c/GTAVI/Tools
OUT=$T/ghidra-scripts/out/topics
mkdir -p "$OUT"
if tasklist //FI "IMAGENAME eq java.exe" 2>/dev/null | grep -qi java.exe && [ -e "$T/ghidra-proj/scarlett.lock" ]; then
  echo "ghidra-proj still locked (analysis running?) - not starting"; exit 1
fi
# 4G while the game runs, else 12G
if tasklist | grep -qi victory.exe; then export GHIDRA_HEADLESS_MAXMEM=4G; else export GHIDRA_HEADLESS_MAXMEM=12G; fi
cd "$T"
cmd //c "ghidra_12.1.4_PUBLIC\\support\\analyzeHeadless.bat C:\\GTAVI\\Tools\\ghidra-proj scarlett -process game_scarlett_master.exe -readOnly -noanalysis -scriptPath C:\\GTAVI\\Tools\\ghidra-scripts -postScript ExportTopics.java C:\\GTAVI\\Tools\\ghidra-scripts\\out\\topics ${1:-250} ${2:-}" > "$OUT/export-console.txt" 2>&1
grep -E "ExportTopics.java>|ERROR|Exception" "$OUT/export-console.txt" | cut -c1-200

# Raw tools (independent of Ghidra; ~10 s each)
cd "$T/ghidra-scripts"
python sweep.py 'call qword ptr \[r\w+ \+ 0xe8\]'  > "$OUT/raw-call-0xe8.txt"   # page-mapping batch candidates
python sweep.py 'call qword ptr \[r\w+ \+ 0x1d8\]' > "$OUT/raw-call-0x1d8.txt"  # ExecuteIndirect candidates
python callers.py 145121550 144db7900 141d40f90 1401f6330 1450bb280 141ad0fd0 141ad0ef0 140f2c470 > "$OUT/raw-callers.txt"
python xs.py IndirectArgs DirectStorage Prestream Readback ReadBack > "$OUT/raw-string-xrefs.txt"
ls -la "$OUT"
