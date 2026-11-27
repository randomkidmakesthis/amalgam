#!/bin/bash
# Hands-off measured run: run-auto.sh <label> <seconds> [VAR=value ...]
# Standard settings + the screen-driven start-screen input script. Uses victory-run/target/diag.
# Screens by draw count (caption): logo ~11-21 (7-16 s), loading ~6, TITLE screen ~22 (press A until it goes), LEGAL NOTICE ~39 draws at first and ~46 after the first Down (Down, wait 7 s for its timer, Down, A), then A, Down, A, Down, A, loading ~5, scene >1000.
# After the run an INPUT CHECK line says whether the scene was reached hands-off (no real key presses seen).
T=/c/GTAVI/Tools
AI="VICTORY_AUTO_INPUT=d>=15,rep(A,1,d<10),w3,d>=20,w1,rep(A,1,d>=35),w1.5,Down,w7,Down,w0.6,A,w1.5,A,w0.7,Down,w0.5,A,w0.7,Down,w0.5,A"
label=$1; secs=$2; shift 2
B=$T/victory-data/bench/$label
(powershell -NoProfile -ExecutionPolicy Bypass -File "$(cygpath -w $T/keep-display-awake.ps1)" > /dev/null 2>&1 &)
(powershell -NoProfile -ExecutionPolicy Bypass -File "$(cygpath -w $T/builds-yield.ps1)" -Log "$(cygpath -w $T/victory-data/build-$label.txt)" > /dev/null 2>&1 &)
(powershell -NoProfile -ExecutionPolicy Bypass -File "$(cygpath -w $T/record-input.ps1)" -Out "$(cygpath -w $T/victory-data/input-$label.txt)" > /dev/null 2>&1 &)
bash $T/bench-victory.sh "$label" victory-run coolfox "$secs" "$AI" VBIN=diag VICTORY_JOIN_COPIES=1 VICTORY_REFRESH_ONCE=1 \
  VICTORY_CARD_REHASH_RUNS=1 VICTORY_BACK_OWN_VIEW=1 VICTORY_WIDE_MERGE=1 VICTORY_LEVELS_ON_DEMAND=1 VICTORY_MIRROR_FAST=1 \
  VICTORY_FRAME_LOG=1 VICTORY_CARD_METER=1 VICTORY_PIPELINE_WAIT_MS=20 VICTORY_ASYNC_SUBMIT=1 VICTORY_QUEUE2=1 \
  VICTORY_INDIRECT_WINDOW=1 VICTORY_TEXTURE_NO_WAIT=1 VICTORY_COPY_PLAIN=1 VICTORY_WIDE_TEXTURES=1 \n  VICTORY_INDIRECT_GATHER=1 VICTORY_LATE_WAIT_LISTS=1024 VICTORY_JOIN_PARTS=1 \n  VICTORY_CARD_FILLS_BATCH=0 VICTORY_GATHER_DISPATCH=0 "$@" 2>&1 | tail -3 | cut -c1-140
sleep 1 2>/dev/null || ping -n 3 127.0.0.1 > /dev/null
mv $T/victory-data/input-$label.txt $B/input.txt 2>/dev/null; mv $T/victory-data/build-$label.txt $B/build-activity.txt 2>/dev/null; cat $B/build-activity.txt 2>/dev/null
bash $T/input-check.sh "$label"
