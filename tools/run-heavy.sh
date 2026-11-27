#!/bin/bash
# Heavy-scene summary of runs: run-heavy.sh <label> [<label> ...]
cd /c/GTAVI/Tools/victory-data/bench
for r in "$@"; do
  printf "%s (%s): " "$r" "$(cat $r/watch.txt 2>/dev/null)"
  sed -E 's/\x1b\[[0-9;]*m//g' $r/stdout.txt | grep -a "usual frame" | awk '{for(i=1;i<=NF;i++){if($i=="long):"){f=$(i+1)} if($i=="preparing"){p=$(i+1)+0} if($i=="card"&&$(i-1)=="the"&&$(i-2)=="for"){w=$(i+1)+0} if($i=="copy-back"){b=$(i+1)+0} if($i=="lists"&&$(i-1)=="other"){g=$(i+1)+0} if($i=="draws"&&$(i-1)+0>0){d=$(i-1)}} if(d>3000){n++; sf+=f; sp+=p; sw+=w; sb+=b; sg+=g; sd+=d}} END{if(n)printf "heavy windows %d: frame %.0f ms, preparing %.1f, waiting %.1f, copy-back %.1f, game gpu %.1f, draws %.0f", n,sf/n,sp/n,sw/n,sb/n,sg/n,sd/n}'
  printf ", crashes %s\n" "$(sed -E 's/\x1b\[[0-9;]*m//g' $r/stdout.txt | grep -ac '0x887A\|device removed\|panicked')"
done
