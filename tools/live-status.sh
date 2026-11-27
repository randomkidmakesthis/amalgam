#!/bin/bash
# live-status.sh [label]: what is limiting the game RIGHT NOW, from the running (or last) run's log.
# Reads the newest per-frame meters and names the tightest bound, plus memory pressure and loading stalls.
cd /c/GTAVI/Tools/victory-data/bench
r=${1:-$(ls -td run-*/ | head -1 | tr -d /)}
L=$(tail -c 6000000 $r/stdout.txt | sed -E 's/\x1b\[[0-9;]*m//g')
cap=$(tail -1 $r/captions.txt | cut -d'|' -f4-6)
echo "run $r - now:$cap"
echo "$L" | grep -a "framelog: usual frame" | tail -1 | awk '{for(i=1;i<=NF;i++){if($i=="long):"){f=$(i+1)+0} if($i=="preparing"){p=$(i+1)+0} if($i=="card"&&$(i-1)=="the"&&$(i-2)=="for"){w=$(i+1)+0} if($i=="work"&&$(i-1)=="at"){s=$(i+1)+0} if($i=="idle"&&$(i-1)=="card:"){id=$(i+1)+0} if($i=="lists"&&$(i-1)=="other"){g=$(i+1)+0} if($i=="draws"&&$(i-1)+0>0){d=$(i-1)+0} if($i=="uploads,"){u=$(i-1)+0} if($i=="remapping:"){rc=$(i+1)+0; rm=$(i+3)+0}}
  printf "  frame %.0f ms (%.1f fps) at %d draws\n", f, 1000/f, d
  printf "  our submit thread busy %.0f ms (preparing %.0f, waiting for the card %.0f)\n", s, p, w
  printf "  card: game lists %.0f ms, idle %.0f ms\n", g, id
  printf "  loading: %d texture uploads a frame, %d page-mapping calls (%.0f ms)\n", u, rc, rm }'
R=$(echo "$L" | grep -a "title timeline per frame over" | tail -1 | awk '{for(i=1;i<=NF;i++) if($i=="wall"&&$(i+1)=="R"){print $(i+2)+0}}')
[ -n "$R" ] && echo "  game's recording phase $R ms"
echo "$L" | grep -a "framelog: usual frame" | tail -1 | awk -v R="${R:-0}" '{for(i=1;i<=NF;i++){if($i=="long):"){f=$(i+1)+0} if($i=="work"&&$(i-1)=="at"){s=$(i+1)+0} if($i=="idle"&&$(i-1)=="card:"){id=$(i+1)+0} if($i=="card"&&$(i-1)=="the"&&$(i-2)=="for"){w=$(i+1)+0}}
  card=f-id; best="the card (busy " int(card) " of " int(f) " ms)"; v=card
  if(s>v){v=s; best="our submit thread (busy " int(s) " of " int(f) " ms)"}
  if(R+4>v){v=R+4; best="the game recording its draws through our layer (" int(R) " ms)"}
  if(f<36){best="nothing: at the 30 fps cap"}
  print "  TIGHTEST LIMIT NOW: " best }'
echo "$L" | grep -a "memory housekeeping" | tail -1 | grep -o "card [0-9]* MB in use ([0-9]* MB counting free heap space) of [0-9]* MB\|texture copies alive [0-9]* MB\|made again [0-9]* MB ([0-9]* MB in the last 5 s)\|out-of-memory fallback ran [0-9]* times\|free RAM [0-9]* MB" | sed 's/^/  memory: /'
echo "$L" | grep -a "frame times:" | tail -3 | grep -o "longest [0-9]*" | awk '{if($2+0>m)m=$2+0} END{if(m>400) print "  FREEZE in the last 15 s: longest frame " m " ms"; else print "  no freeze in the last 15 s (longest frame " m " ms)"}'
echo "$L" | grep -a "hangwatch: hang watch: no frame" | tail -1 | cut -c60-300
