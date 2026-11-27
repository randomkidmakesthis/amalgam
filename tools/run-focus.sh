#!/bin/bash
# run-focus.sh <label|captions file>: game window focused (fg=1) vs not (fg=0), from the watchdog's caption samples.
# In the scene only (draws >= 1000): median ms a frame per draw band, sample counts, mean process CPU % (100 = one core).
# Needs captions written by watch-window.ps1 with the ` | fg=.. cpu=..% prio=..` field (older runs have none).
f=$1; [ -f "$f" ] || f=/c/GTAVI/Tools/victory-data/bench/$1/captions.txt
[ -f "$f" ] || { echo "no captions: $1"; exit 1; }
awk -F'|' '
{ ms=""; d=""; fg=""; cpu=""; pr=""
  for(i=1;i<=NF;i++){
    if($i~/ ms *$/ && $i!~/MB/) ms=$i+0
    if($i~/ draws *$/) d=$i+0
    if($i~/fg=/){ n=split($i,w," "); for(k=1;k<=n;k++){ split(w[k],kv,"="); if(kv[1]=="fg")fg=kv[2]; if(kv[1]=="cpu"){c=kv[2]; sub("%","",c); cpu=c}; if(kv[1]=="prio")pr=kv[2] } }
  }
  if(fg==""){ nofg++; next }
  if(pr!="") prio[pr]++
  if(d=="" || d<1000 || ms=="" || ms<=0) next
  b=(d<2500)?"1-2.5k":(d<4000)?"2.5-4k":">=4k"
  key=fg SUBSEP b; cnt[key]++; v[key, cnt[key]]=ms
  all[fg]++; va[fg, all[fg]]=ms
  if(cpu!="" && cpu!="-"){ cs[fg]+=cpu; cn[fg]++ }
}
function med(arr, base, n,   i, j, t, a){ for(i=1;i<=n;i++) a[i]=arr[base, i]
  for(i=2;i<=n;i++){ t=a[i]; for(j=i-1;j>=1 && a[j]>t;j--) a[j+1]=a[j]; a[j+1]=t }
  return (n%2)? a[(n+1)/2] : (a[n/2]+a[n/2+1])/2 }
END{
  if(nofg && !length(all)) { printf "no fg= field in %d samples (captions from an older watch-window.ps1)\n", nofg; exit }
  split("1-2.5k 2.5-4k >=4k", B, " ")
  for(g=1; g>=0; g--){
    printf "fg=%d (%s): %d scene samples", g, g?"game focused":"game NOT focused", all[g]+0
    if(all[g]) printf ", median %.1f ms", med(va, g, all[g])
    if(cn[g]) printf ", mean cpu %.0f %%", cs[g]/cn[g]
    printf "\n"
    for(k=1;k<=3;k++){ key=g SUBSEP B[k]; n=cnt[key]+0; if(n) printf "   draws %-7s median %6.1f ms (n=%d)\n", B[k], med(v, key, n), n; else printf "   draws %-7s -\n", B[k] }
  }
  if(nofg) printf "(%d samples without the fg field skipped)\n", nofg
  for(p in prio) pl=pl " " p "=" prio[p]; if(pl!="") print "priority class seen (samples):" pl
}' "$f"
