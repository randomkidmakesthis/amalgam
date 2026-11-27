#!/bin/bash
# run-freezes.sh <label> [min_ms=400]: attribute each long frame to its largest measured part.
cd /c/GTAVI/Tools/victory-data/bench/$1; min=${2:-400}
[ -f plain.txt ] || sed -E 's/\x1b\[[0-9;]*m//g' stdout.txt > plain.txt
grep -a "framelog: long frame" plain.txt | awk -v min=$min '{
  ms=0; for(i=1;i<=NF;i++){ if($i=="s:"){ms=$(i+1)+0}
    if($i=="preparing"){prep=$(i+1)+0} if($i=="card;"&&$(i-1)=="the"){wait=$(i-0)} if($i=="for"&&$(i+1)=="the"&&$(i+2)~/^card/){wait=$(i+3)+0}
    if($i=="copy-back"){back=$(i+1)+0} if($i=="idle"&&$(i-1)=="card:"){idle=$(i+1)+0}
    if($i=="refresh"&&$(i-1)=="textures:"){tex=$(i+1)+0} if($i=="remapping:"){rcalls=$(i+1)+0; remap=$(i+3)+0}
    if($i=="waited"&&$(i-1)=="ms"){pipe=$(i-2)+0} if($i=="ran"&&$(i-1)=="thread:"){ran=$(i+1)+0}
    if($i=="present"&&$(i+1)=="call"){pres=$(i+2)+0} if($i=="threads:"&&$(i-1)=="all"&&$(i-2)=="recording,"){rec=$(i+1)+0}
    if($i=="draws"&&$(i-1)+0>0){d=$(i-1)+0} }
  if(ms<min) next; n++; tot+=ms;
  outside=ms-pres; if(outside<0)outside=0;
  best="outside the present call (game threads: loading, mapping calls, waits)"; bv=outside;
  if(wait>bv){bv=wait;best="waiting for the card"} if(prep>bv){bv=prep;best="preparing draws/copies"}
  if(tex>bv){bv=tex;best="texture refresh/uploads"} if(pipe>bv){bv=pipe;best="pipeline compile wait"}
  c[best]++; s[best]+=ms; sr[best]+=remap; sc[best]+=rcalls; si[best]+=idle
} END{ printf "%d frames over %d ms, %.1f s in all\n", n, min, tot/1000; for(k in c) printf "  %-70s %3d frames, %5.1f s (mean %4.0f ms); in them: remapping %.0f ms in %.0f calls, card idle %.0f ms on average\n", k, c[k], s[k]/1000, s[k]/c[k], sr[k]/c[k], sc[k]/c[k], si[k]/c[k] }'
