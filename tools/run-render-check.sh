#!/bin/bash
# Render-completeness guard: run-render-check.sh <label> [<baseline-label>]
# Reads victory-data/bench/<label>/stdout.txt (read-only) and prints every counted rendering loss,
# next to the baseline's, with WORSE where a loss counter is >20 % above baseline and above a floor.
# Units: /kf = per 1,000 frames (frame-weighted for per-frame log lines; totals / frames for
# "since the start" counters); avg/max/last = gauges; n = plain count of log lines.
# A counter whose log line is absent prints n/a. See docs/render-completeness.md for what each means.
T=/c/GTAVI/Tools
B=$T/victory-data/bench
[ -z "$1" ] && { echo "usage: run-render-check.sh <label> [<baseline-label>]"; exit 2; }
run=$1; base=$2
[ -f "$B/$run/stdout.txt" ] || { echo "no $B/$run/stdout.txt"; exit 2; }
[ -n "$base" ] && [ ! -f "$B/$base/stdout.txt" ] && { echo "no $B/$base/stdout.txt"; exit 2; }

collect() {
  sed -E 's/\x1b\[[0-9;]*m//g' "$B/$1/stdout.txt" | awk '
  function secs(t,  a) { if (split(t, a, /[T:Z]/) < 4) return -1; return a[2]*3600 + a[3]*60 + a[4] }
  function num(re,  m) { return match($0, re, m) ? m[1] + 0 : "" }
  function sw(key,  dt) {            # frames since this kind of line was last seen (about 1 s cadence)
    dt = (key in seen) ? now - seen[key] : 1; if (dt <= 0 || dt > 5) dt = 1
    seen[key] = now; return dt * (fps > 0 ? fps : 1)
  }
  function gauge(k, v) { if (v == "") return; gs[k] += v; gn[k]++; if (v > gm[k]) gm[k] = v; gl[k] = v }
  function last(k, v) { if (v != "") L[k] = v }
  {
    t = secs($1); if (t >= 0) { if (t < prevt - 43200) day += 86400; prevt = t; now = t + day
      if (first == "") first = now; lastt = now }
  }
  / (WARN|ERROR) / { if ($0 ~ / ERROR /) errors++; else warns++ }
  /victory_gpu::host: [0-9.]+ fps, / {
    fps = num("host: ([0-9.]+) fps"); dt = (fpst != "") ? now - fpst : 5; if (dt <= 0 || dt > 15) dt = 5
    fpst = now; fw = fps * dt; FW += fw; fl++
    A["drop"] += num("dropped ([0-9.]+) without a host pipeline") * fw
    A["dropind"] += num("being made\\) and ([0-9.]+) indirect") * fw
    A["draws"] += num(", ([0-9.]+) draws/frame") * fw
    gauge("making", num("\\(([0-9]+) being made\\)"))
    gauge("twait", num("([0-9]+) waiting, [0-9]+ passed over unmapped"))
    gauge("tunmapped", num("waiting, ([0-9]+) passed over unmapped"))
    last("nullviews", num("left null as the host would refuse them: ([0-9]+)"))
    last("uavrefused", num("unordered views refused ([0-9]+)x"))
    last("notwin", num("([0-9]+) depth textures left without one"))
    next
  }
  /without a host pipeline: / {
    r = num("dropped ([0-9.]+)/frame without"); w = (fw > 0 ? fw : 1)
    if ($0 ~ /ray-tracing structure builder/) A["drop_rt"] += r * w
    else if ($0 ~ /: being made/) A["drop_making"] += r * w
    else A["drop_other"] += r * w
    havewhy = 1; next
  }
  /frame presented \(since the last line/ {
    ASK += num("draws=([0-9]+)") + num("dispatches=([0-9]+)"); RAN += num("drawn=([0-9]+)") + num("dispatched=([0-9]+)"); PF = num("frame=([0-9]+)"); next }
  /indirect mesh dispatches: the largest group count/ { gauge("meshmax", num("asked for ([0-9]+)")); next }
  /frame times: [0-9]+ frames/ { FR += num("frame times: ([0-9]+) frames"); LONG += num("; ([0-9]+) long \\("); next }
  /pipeline creation method that is not implemented/ {
    s = match($0, /slot=(0x[0-9a-f]+)/, m) ? m[1] : "?"; c = num("calls=([0-9]+)"); if (c > unimpl[s]) unimpl[s] = c; haveun = 1; next }
  /compute dispatches of more than 65535/ {
    w = sw("cd"); SW += w
    S["whole"] += num("([0-9.]+) run whole past") * w; S["clamped"] += num("([0-9.]+) records still clamped") * w; havecd = 1; next }
  /descriptor tables that found no free slots/ {
    w = sw("ts"); S["tblwait"] += num("per frame: ([0-9.]+) waited") * w; S["tblgone"] += num("([0-9.]+) given up") * w; next }
  /indirect records gathered for their list/ { w = sw("ig"); S["nogather"] += num("none for ([0-9]+)") * w; havegather = 1; next }
  /texture refreshes that did not wait for title memory/ { w = sw("nw"); S["nowait"] += num("([0-9.]+) left for the next refresh") * w; next }
  /fills of upload memory that read title memory flights may write, not waited/ {
    last("fillchanged", num("then changed, since the start: ([0-9]+)")); next }
  /textures picked by index, the queue:/ {
    gauge("qslots", num("queue: ([0-9]+) slots")); gauge("qdue", num("slots, ([0-9]+) of them due"))
    gauge("qlong", num("longest due ([0-9]+) ms")); AGE += num("([0-9]+) served ahead for age")
    gauge("qmem", num("([0-9]+) waiting for their memory")); gauge("qpart", num("([0-9]+) uploaded in part"))
    last("partup", num("([0-9]+) uploads of textures not all mapped")); last("nofirst", num("\\(([0-9]+) of them without their first page"))
    last("stale", num("([0-9]+) slots made stale")); last("madenull", num("([0-9]+) made slots found unmapped and made null"))
    haveq = 1; next }
  /placed texture views: / {
    last("keptsame", num("([0-9]+) writes of another view of the texture")); last("keptother", num("([0-9]+) writes of a view of another texture")); next }
  /memory of textures: / {
    gauge("nocopy", num("([0-9]+) title textures have no host copy yet"))
    last("letgo_mb", num("[0-9]+ let go ([0-9]+) MB and")); last("again_n", num("and ([0-9]+) made again")); last("again_mb", num("made again ([0-9]+) MB since"))
    last("noroom", num("lack of room: ([0-9]+)")); next }
  /memory housekeeping: / {
    last("room_mb", num("let go for room ([0-9]+) MB")); last("early_mb", num("let go for room early [0-9]+ \\(([0-9]+) MB"))
    last("oom", num("out-of-memory fallback ran ([0-9]+) times")); next }
  /textures with levels on demand: / { last("nolevel", num("([0-9]+) uploads left a held level without memory")); next }
  /stores through views picked by index, per frame/ { last("firstmarks", num("([0-9]+) first marks")); next }
  /copies left out since the start because both sides/ { last("leftout", num("one host texture: ([0-9]+)")); next }
  /changed after its upload with nothing to tell/ { c = num("changed=([0-9]+)"); if (c > chg) chg = c; havechg = 1; next }
  /a program uses a buffer view the host refused \(it reads zeros\)/ { N["zeros"]++; next }
  /out of card memory: an allocation/ { N["oomwarn"]++; next }
  /little free RAM/ { N["lowram"]++; next }
  /shader does not translate/ { N["notrans"]++; next }
  /host (mesh )?pipeline refused|host root signature refused/ { N["refused"]++; next }
  /a frame waited for a pipeline past its budget/ { N["pastbudget"]++; next }
  /host pipeline wait budget per frame/ { L["waitms"] = num("millis=([0-9]+)"); next }
  /stale after its refresh, three times/ { N["stale3"]++; next }
  /shaders touched pages that had no copy/ { N["nopage"]++; next }
  # Losses said once only (their draws or views are not counted after the first).
  /draw is dropped|indexed rectangles not drawn|indirect mesh dispatches dropped|indirect execution dropped|placed unordered texture view not mirrored|host view heap capped|not uploaded$|levels skipped|title-format texels skipped|a root SRV not on 16 bytes reads zeros|no argument clamp|the card has no room for more; they wait|buffer view that is not there/ { once++ }
  END {
    if (FR == 0) FR = FW
    el = lastt - first
    printf "seconds\t%s\n", (el > 0 ? sprintf("%.0f", el) : "n/a")
    printf "frames\t%s\n", (FR > 0 ? sprintf("%.0f", FR) : "n/a")
    printf "fps\t%s\n", (FR > 0 && el > 0 ? sprintf("%.1f", FR / el) : "n/a")
    printf "long\t%s\n", (FR > 0 ? sprintf("%.1f", LONG / FR * 1000) : "n/a")
    printf "draws\t%s\n", (FW > 0 ? sprintf("%.0f", A["draws"] / FW) : "n/a")
    split("drop dropind", fk, " "); for (i = 1; i <= 2; i++) printf "%s\t%s\n", fk[i], (FW > 0 ? sprintf("%.0f", A[fk[i]] / FW * 1000) : "n/a")
    split("drop_rt drop_making drop_other", fk, " ")
    for (i = 1; i <= 3; i++) printf "%s\t%s\n", fk[i], (FW > 0 && fl > 0 ? sprintf("%.0f", A[fk[i]] / FW * 1000) : "n/a")
    un = (A["drop"] - A["drop_rt"] - A["drop_making"] - A["drop_other"]) / (FW > 0 ? FW : 1) * 1000; if (un < 50) un = 0
    printf "drop_none\t%s\n", (FW > 0 && fl > 0 ? sprintf("%.0f", un) : "n/a")
    printf "askrun\t%s\n", (PF > 0 ? sprintf("%.0f", (ASK - RAN) / PF * 1000) : "n/a")
    printf "meshmax\t%s\n", (gn["meshmax"] ? gm["meshmax"] : "n/a")
    u = 0; for (s in unimpl) u += unimpl[s]; printf "unimpl\t%s\n", (haveun ? u : (fl > 0 ? 0 : "n/a"))
    split("whole clamped tblwait tblgone nowait", fk, " ")
    for (i = 1; i <= 5; i++) printf "%s\t%s\n", fk[i], (havecd && SW > 0 ? sprintf("%.1f", S[fk[i]] / SW * 1000) : "n/a")
    printf "nogather\t%s\n", (havegather && SW > 0 ? sprintf("%.0f", S["nogather"] / SW * 1000) : "n/a")
    split("making twait tunmapped qslots qdue qmem qpart nocopy", fk, " ")
    for (i = 1; i <= 8; i++) printf "%s\t%s\n", fk[i], (gn[fk[i]] ? sprintf("%.0f", gs[fk[i]] / gn[fk[i]]) : "n/a")
    printf "qduemax\t%s\n", (gn["qdue"] ? gm["qdue"] : "n/a")
    printf "qlong\t%s\n", (gn["qlong"] ? gm["qlong"] : "n/a")
    printf "age\t%s\n", (haveq && FR > 0 ? sprintf("%.1f", AGE / FR * 1000) : "n/a")
    split("partup nofirst stale madenull keptsame keptother again_n again_mb letgo_mb room_mb early_mb nolevel leftout", fk, " ")
    for (i = 1; i <= 13; i++) printf "%s\t%s\n", fk[i], ((fk[i] in L) && FR > 0 ? sprintf("%.0f", L[fk[i]] / FR * 1000) : "n/a")
    split("noroom oom nullviews uavrefused notwin fillchanged firstmarks waitms", fk, " ")
    for (i = 1; i <= 8; i++) printf "%s\t%s\n", fk[i], ((fk[i] in L) ? L[fk[i]] : "n/a")
    printf "chg\t%s\n", (havechg ? sprintf("%.1f", chg / FR * 1000) : (haveq ? "0.0" : "n/a"))
    split("zeros oomwarn lowram notrans refused pastbudget stale3 nopage", fk, " ")
    for (i = 1; i <= 8; i++) printf "%s\t%s\n", fk[i], (fl > 0 ? N[fk[i]] + 0 : "n/a")
    printf "once\t%s\n", (fl > 0 ? once + 0 : "n/a")
    printf "warns\t%s\n", warns + 0; printf "errors\t%s\n", errors + 0
  }'
}

# key | unit | floor (0 = information only, never marked) | what
ROWS='seconds|s|0|run length in the log
frames|n|0|frames timed (frame times lines)
fps|avg|0|frames / seconds
long|/kf|0|long frames
draws|avg|0|draws + dispatches asked per frame
-|||DRAWS AND DISPATCHES NOT RUN
drop|/kf|500|dropped without a host pipeline, all reasons
drop_rt|/kf|500|  of them: ray-tracing structure builder off
drop_making|/kf|5|  of them: pipeline still being made (wait budget ran out)
drop_other|/kf|5|  of them: any other reason (untranslated, refused)
drop_none|/kf|50|  of them: no reason line (table slots, view heap, root view)
dropind|/kf|5|indirect executions dropped
askrun|/kf|5000|asked minus run on the host, to the last power-of-2 frame
meshmax|max|0|largest indirect mesh group count asked (65,535 run)
making|avg|2|pipelines being made at the frame line
pastbudget|n|0|frames that waited past the budget (load wait; first only)
waitms|last|0|pipeline wait budget in force, ms
unimpl|n|100|pipeline creation calls answered "not implemented"
notrans|n|0.5|shaders that do not translate
refused|n|0.5|host pipelines / root signatures refused
whole|/kf|1|compute dispatches run whole past 65,535 groups
clamped|/kf|1|indirect compute records still clamped
tblwait|/kf|5|descriptor tables that waited for slots
tblgone|/kf|0.5|descriptor tables given up (draws not drawn)
nogather|/kf|0|indirect records not gathered (clamped singly; no loss)
-|||TEXTURES LATE, IN PART OR NULL
twait|avg|500|textures picked by index waiting (frame line)
tunmapped|avg|500|  passed over unmapped
qdue|avg|100|queue: slots due
qduemax|max|500|queue: slots due, worst reading
qlong|max|500|queue: longest due, ms, worst reading
age|/kf|5|queue: served ahead for age (over MAX_AGE_MS late)
qmem|avg|500|queue: waiting for their memory
qpart|avg|200|queue: uploaded in part
partup|/kf|500|uploads of textures not all mapped
nofirst|/kf|200|  of them without their first page
madenull|/kf|50|made slots found unmapped and made null
stale|/kf|5000|slots made stale by a newer view
keptsame|/kf|5000|slot kept the old view of its texture until remade
keptother|/kf|5000|slot kept showing ANOTHER (old) texture until remade
chg|/kf|5|textures changed after upload, uploaded again
nowait|/kf|50|texture refreshes left for the next refresh (NO_WAIT)
nocopy|avg|5000|title textures with no host copy yet
nolevel|/kf|2|uploads that left a held level without memory
-|||TEXTURE COPIES RELEASED AND REMADE
letgo_mb|/kf|500|host texture copies let go, MB
early_mb|/kf|200|  let go early for room (VRAM_EARLY), MB
room_mb|/kf|50|  let go by the out-of-memory fallback, MB
again_n|/kf|500|host texture copies made again
again_mb|/kf|200|host texture copies made again, MB
noroom|last|5|texture passes that made none for lack of room
oom|last|0.5|out-of-memory fallback runs
oomwarn|n|0.5|"out of card memory" warnings
lowram|n|10|"little free RAM" warnings
-|||VIEWS AND BUFFERS
nullviews|last|5|buffer views left null (host would refuse)
zeros|n|5|programs named with a refused view ("reads zeros")
uavrefused|last|0.5|unordered views refused
notwin|last|0.5|depth textures left without a twin
fillchanged|last|2|fills that read bytes a flight then changed
firstmarks|last|5|first marks (stores not copied back)
leftout|/kf|10|copies left out (both sides one host texture)
stale3|n|0.5|host copy stale after refresh, three times running
nopage|n|0.5|shaders touched pages that had no copy
once|n|0.5|losses said once only (kinds seen; their repeats are silent)
warns|n|0|WARN lines
errors|n|0.5|ERROR lines'

tmp=$(mktemp -d 2>/dev/null || echo "${TMPDIR:-/tmp}/rrc.$$"); mkdir -p "$tmp"
collect "$run" > "$tmp/a"
if [ -n "$base" ]; then collect "$base" > "$tmp/b"; else : > "$tmp/b"; fi

echo "render check: $run${base:+   baseline: $base}"
printf '%s\n' "$ROWS" | awk -F'|' -v fa="$tmp/a" -v fb="$tmp/b" -v hb="${base:-}" -v ra="$run" '
  BEGIN { while ((getline l < fa) > 0) { split(l, p, "\t"); a[p[1]] = p[2] }
          while ((getline l < fb) > 0) { split(l, p, "\t"); b[p[1]] = p[2] }
          printf "%-61s %-5s %12s", "counter", "unit", substr(ra, 1, 12); if (hb != "") printf " %12s  %s", substr(hb, 1, 12), ""; printf "\n" }
  $1 == "-" { printf "-- %s\n", $4; next }
  { va = ($1 in a) ? a[$1] : "n/a"; vb = ($1 in b) ? b[$1] : "n/a"; mark = ""
    if (hb != "" && $3 + 0 > 0 && va != "n/a" && vb != "n/a" && va + 0 > (vb + 0) * 1.2 && va - vb > $3 + 0) { mark = "WORSE"; worse++ }
    printf "%-61s %-5s %12s", $4, $2, va; if (hb != "") printf " %12s  %s", vb, mark; printf "\n" }
  END { if (hb != "") printf "%d counter(s) WORSE than baseline (over +20 %% and over the floor)\n", worse }'
rm -rf "$tmp"
echo "shots (one every 30 s): $(cygpath -w "$B/$run/shots" 2>/dev/null || echo "$B/$run/shots") ($(ls "$B/$run/shots" 2>/dev/null | wc -l) files)"
[ -n "$base" ] && echo "baseline shots:         $(cygpath -w "$B/$base/shots" 2>/dev/null || echo "$B/$base/shots") ($(ls "$B/$base/shots" 2>/dev/null | wc -l) files)"
exit 0
