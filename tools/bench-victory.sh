#!/usr/bin/env bash
# Timed, watchdog-guarded run of the game under a victory fork.
#   bench-victory.sh <label> <repo-dir-name> <data-name> <seconds> [VAR=value ...]
# Output: C:\GTAVI\Tools\victory-data\bench\<label>\  (caption samples, window shots, GPU csv, stdout).
# The game folder is only read. Always ends with no victory.exe left running.
set -u
T=/c/GTAVI/Tools
label=${1:?label}; repo=${2:?repo}; data=${3:?data}; secs=${4:?seconds}; shift 4
export VICTORY_GAME_DIR='C:\GTAVI\GTA6'
export VICTORY_CACHE_DIR="C:\\GTAVI\\Tools\\victory-data\\$data\\cache"
export VICTORY_DATA_DIR="C:\\GTAVI\\Tools\\victory-data\\$data\\data"
for kv in "$@"; do export "$kv"; done
out="$T/victory-data/bench/$label"; mkdir -p "$out/shots"
printf '%s\n' "$@" > "$out/env.txt"

# refuse to start on top of a leftover run
if tasklist //FI "IMAGENAME eq victory.exe" 2>/dev/null | grep -qi victory.exe; then
  echo "a victory.exe is already running - not starting"; exit 1
fi

# fresh start every run: remove the game's saves so it always begins at the opening cinematic
# (settings and caches are kept; KEEP_SAVE=1 skips this)
if [ "${KEEP_SAVE:-0}" != "1" ]; then
  for s in "$T/victory-data/$data/data"/*/saves; do [ -d "$s" ] && rm -rf "$s" && echo "saves removed: $s"; done
fi

nvidia-smi --query-gpu=utilization.gpu,memory.used --format=csv,noheader,nounits -l 2 > "$out/gpu.csv" &
smi=$!
"$T/$repo/target/${VBIN:-release}/victory.exe" run 'C:\GTAVI\GTA6\game_scarlett_master.exe' --execute > "$out/stdout.txt" 2>&1 &

# watchdog + sampler: caption every 2 s, a shot of the game window every 30 s,
# stop at <secs>, or early when the process is gone or has stood still for 90 s
powershell -NoProfile -ExecutionPolicy Bypass -File "$(cygpath -w "$T/watch-window.ps1")" \
  -ProcessName victory -Seconds "$secs" -OutDir "$(cygpath -w "$out")" -StallSeconds 90 > "$out/watch.txt" 2>&1

for i in 1 2 3 4 5; do taskkill //F //T //IM victory.exe >/dev/null 2>&1; sleep 2; tasklist //FI "IMAGENAME eq victory.exe" 2>/dev/null | grep -qi victory.exe || break; done
kill $smi 2>/dev/null; taskkill //F //IM nvidia-smi.exe >/dev/null 2>&1
sleep 2
echo "== $label ($repo, $secs s) $*"
echo "-- ended: $(tail -1 "$out/watch.txt")"
echo "-- caption samples (first, then every ~30 s):"
awk 'NR==1 || NR%15==0' "$out/captions.txt" 2>/dev/null | tail -16 | cut -c1-200
echo "-- GPU util/VRAM last 60 s:"
tail -30 "$out/gpu.csv" | awk -F', ' '{u+=$1; if($2>m)m=$2; n++} END{if(n) printf "avg util %.0f %%, peak VRAM %d MiB\n", u/n, m}'
echo "-- peak VRAM whole run: $(awk -F', ' '$2>m{m=$2} END{print m}' "$out/gpu.csv") MiB"
echo "-- warnings/errors (top):"
sed -E 's/\x1b\[[0-9;]*m//g' "$out/stdout.txt" | grep -a -E " (WARN|ERROR) " | sed -E 's/^[^ ]+ +//; s/[0-9a-f]{16,}/H/g; s/[0-9]+/N/g' | cut -c1-150 | sort | uniq -c | sort -rn | head -8
echo "-- still running: $(tasklist //FI 'IMAGENAME eq victory.exe' 2>/dev/null | grep -ci victory.exe)"
