#!/bin/bash
# input-check.sh <label>: did the run reach the opening scene by itself?
B=/c/GTAVI/Tools/victory-data/bench/$1
scene=$(awk -F'|' '{t=$1+0; for(i=1;i<=NF;i++) if($i~/draws/){d=$i+0}; if(d>1000){print t; exit}}' $B/captions.txt 2>/dev/null)
keys=$(grep -c "  down  key (Enter|Arrow|Space)" $B/input.txt 2>/dev/null); keys=${keys:-n/a}
gave=$(sed -E 's/\x1b\[[0-9;]*m//g' $B/stdout.txt | grep -ac "auto input: gave up")
if [ -z "$scene" ]; then echo "INPUT CHECK $1: FAILED - the opening scene was never reached (script gave up: $gave, real key presses: $keys)" | tee $B/input-check.txt
elif [ "$scene" -gt 120 ] || { [ "$keys" != "0" ] && [ "$keys" != "n/a" ]; } || [ "$gave" != "0" ]; then echo "INPUT CHECK $1: SUSPECT - scene at ${scene} s, real key presses: $keys, script gave up: $gave (manual help or a late start; numbers still usable, input script needs a look)" | tee $B/input-check.txt
else echo "INPUT CHECK $1: ok - scene at ${scene} s, no real key presses" | tee $B/input-check.txt; fi
# A sleeping or switched-off display throttles presentation: menus (which run at the 30 fps cap, 33 ms) then show ~150 ms.
menu=$(awk -F'|' '{for(i=1;i<=NF;i++){if($i~/ ms/){m=$i+0} if($i~/draws/){d=$i+0}} if(d>0&&d<100&&m>0){n++; v[n]=m}} END{if(n>4){asort(v); print int(v[int((n+1)/2)])}}' $B/captions.txt 2>/dev/null)
if [ -n "$menu" ] && [ "$menu" -gt 50 ]; then echo "DISPLAY CHECK $1: FAILED - menus ran at ${menu} ms a frame instead of 33: the display was asleep or off, the run is not valid" | tee -a $B/input-check.txt; fi
