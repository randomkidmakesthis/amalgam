#!/bin/bash
# cargo-bg.sh <cargo args...>: the ONLY way agents run cargo in this project.
# Safe to use while the game is running: the build runs at Idle priority on logical processors 12-15 with few jobs,
# and waits if free RAM is short (the game commits ~30 GB of 32). With no game running it uses the whole machine.
# Do not pass -j: it is chosen here (inserted right after the subcommand, so `test ... -- <args>` works). Example: bash /c/GTAVI/Tools/cargo-bg.sh build --profile diag -p victory --features diagnostics
source /c/GTAVI/Tools/env.sh
# TESTS never overlap a game run: test binaries create D3D devices and map multi-GB sections; two runs died with
# "device suspended ... some other application submitted invalid commands" and with a paging storm while tests ran.
# `test` (and `bench`/`run`) therefore waits until no victory.exe is running (up to 20 minutes, then gives up with exit 75).
case "$1" in test|bench|run|nextest|build|check|clippy|rustc)
  n=0; while tasklist | grep -qi victory.exe; do n=$((n+1)); if [ $n -gt 240 ]; then echo "cargo-bg: a game run is still going after 20 minutes; not running tests alongside it (exit 75). Try again later."; exit 75; fi; ping -n 6 127.0.0.1 > /dev/null; done;;
esac
free_mb() { powershell -NoProfile -Command "[int]((Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory/1024)" | tr -d '\r'; }
if tasklist | grep -qi victory.exe; then
  n=0; while [ "$(free_mb)" -lt 2500 ] && [ $n -lt 90 ] && tasklist | grep -qi victory.exe; do ping -n 5 127.0.0.1 > /dev/null; n=$((n+1)); done
  sub=$1; shift
  exec powershell -NoProfile -Command "\$p=[Diagnostics.Process]::GetCurrentProcess(); \$p.PriorityClass='Idle'; \$p.ProcessorAffinity=[IntPtr]0xF000; & cargo $sub -j 4 $*; exit \$LASTEXITCODE"
else
  sub=$1; shift
  exec cargo "$sub" -j 14 "$@"
fi
