#!/bin/bash
# run-compare.sh <label> [...]: scene-independent comparison of runs.
# Frame time is compared at equal load (median ms in draw-count bands), hitches are counted, and texture
# loading is shown as a curve over time since the opening scene started (so different timings line up).
cd /c/GTAVI/Tools/victory-data/bench
for r in "$@"; do
  [ -f $r/plain.txt ] || sed -E 's/\x1b\[[0-9;]*m//g' $r/stdout.txt > $r/plain.txt
  scene=$(awk -F'|' '{t=$1+0; for(i=1;i<=NF;i++) if($i~/draws/){d=$i+0}; if(d>1000){print t; exit}}' $r/captions.txt)
  echo "== $r (scene at ${scene:-?} s; $(cat $r/watch.txt 2>/dev/null))"
  grep -a "usual frame" $r/plain.txt | awk '{for(i=1;i<=NF;i++){if($i=="long):"){f=$(i+1)+0} if($i=="draws"&&$(i-1)+0>0){d=$(i-1)+0}} b=(d<1000)?"a <1k":(d<2500)?"b 1-2.5k":(d<4000)?"c 2.5-4k":"d >4k"; print b "\t" f}' | sort -k1,1 -k3,3n | awk -F'\t' '{if($1!=p){if(p!="")printf "   draws %-9s median %3.0f ms (n=%d)\n",substr(p,3),v[int((n+1)/2)],n; n=0; p=$1} v[++n]=$2} END{if(p!="")printf "   draws %-9s median %3.0f ms (n=%d)\n",substr(p,3),v[int((n+1)/2)],n}'
  grep -a "frame times:" $r/plain.txt | awk '{for(i=1;i<=NF;i++){if($i=="frames"&&$(i+1)=="in"){fr+=$(i-1)} if($i=="long"){lg+=$(i-1)} if($i=="longest"){x=$(i+1)+0; if(x>mx)mx=x; if(x>=500)h5++}}} END{printf "   hitches: %d long frames of %d (%.1f %%); 5-s periods with a frame over 500 ms: %d; worst %d ms\n", lg, fr, (fr?100*lg/fr:0), h5, mx}'
  t0=$(grep -a "usual frame" $r/plain.txt | head -1 | cut -c12-19)
  grep -a "memory of textures" $r/plain.txt | awk -v s="${scene:-60}" 'BEGIN{split("60 120 180 240",T," ")} {split(substr($1,12,8),h,":"); t=h[1]*3600+h[2]*60+h[3]; if(!t0)t0=t; e=t-t0; for(i=1;i<=NF;i++){if($i=="alive"&&$(i+2)=="MB"&&c++%2==1){al=$(i+1)} if($i=="again"){ag=$(i+1)}} c=0; for(k=1;k<=4;k++) if(!done[k]&&e>=T[k]){done[k]=1; printf "   +%3d s: title-filled copies alive %s MB, made again %s MB\n",T[k],al,ag}}'
  grep -a "the queue:" $r/plain.txt | awk '{for(i=1;i<=NF;i++){if($i=="waiting"&&$(i+1)=="for"){w=$(i-1)+0} if($i=="uploaded"&&$(i+1)=="in"&&$(i+2)~/part/){u=$(i-1)+0} if($i=="due"){d=$(i-3)+0}} n++; sw+=w; su+=u; sd+=d} END{if(n)printf "   texture queue (mean over run): %d waiting for the game to map them, %d uploaded in part, %d due\n", sw/n, su/n, sd/n}'
done
