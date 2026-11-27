# Memory audit: what holds RAM and VRAM, and what holds it for nothing (2026-11-26)

Read-only audit. Sources: run-121-fox-combined3 (layer counters; menu 10:38:41 = 37 s, mid 10:41:20 = 196 s,
peak 10:43:40 = 336 s), run-120-fox-best118-5min (heavy scene, 10:34:27), run-118-fox-fill-delta2 (address-space
walk at 31 / 190 / 346 s; run-121 had `VICTORY_MEMORY_MAP=0`). **[M]** = a log line says it, **[I]** = inferred.
Printing code: `copystat.rs:578` (map walk), `copystat.rs:765-940` (memory lines), `resource.rs:2479` (housekeeping),
`host.rs:9860` (video memory), `xmem.rs:2515` (pool discard).

## 0. Three findings that change the picture
1. **Every MB the process holds on the card is also a MB of process commit** [I from three exact sums]. The walk's
   "write-combined" kind is 3,367 / 19,130 / 21,141 MB [M]; the card's "in use" is 2,395 / 15,666 / 16,768 MB [M] and
   staging blocks + list scratch 322 / 2,016 / 2,756 MB [M]; the rest is 650 / 1,448 / 1,617 MB. Block counts match too:
   menu "24 of 64.00 MB write-combined" = 21 texture + 3 staging blocks; mid 208 vs 169 + 30 [M]. Render-target sizes
   show up the same way (10 of 14.06 MB = 2560x1440x4, 28 of 15 MB, 12 of 30 MB). Only 3.9-4.0 GB of that kind is "in
   memory" [M]: it is commit, not RAM. So private 33-36 GB is mostly *the card's contents counted again*.
2. **Nothing big is idle by the time rules.** `vram_keep` (`host.rs:152`) keeps copies 10 min only under 88 % net; in
   play net sits on the mark (13,124 of high 13,143 MB [M]), so the 5 s rule runs: 22.9-40 GB let go, 4.6-8.9 GB made
   again (20-22 %) [M]. `KEEP_FREED`'s 64 empty blocks do not exist in play: pressure is on, "texture blocks 164 alive
   (2 empty)" [M]; `ram_idle_ms` (`host.rs:374`) is already 2-10 s on a <= 32 GB PC (`limited_host_ram`).
   Render-target copies: "0 were not used for 30 s and are held by no list and no slot" [M].
3. **What is held for nothing is space, not objects**: holes inside 64 MB texture blocks (2.0-2.9 GB), staging blocks
   that hold 60-240 MB of live staging in 0.7-3.4 GB, window bytes outside anything asked for (1.3 GB), and three
   groups of allocations nobody names (6.2 GB).

## 1. RAM ledger (process private = commit), MB
| Holder | menu | mid | peak | source |
|---|---|---|---|---|
| Title's heaps (private, console addresses; incl. 185 MB graphics heap) | 4,629 | 4,737 | 4,737 | map [M]; 767-1,237 in memory |
| Mirror of card memory: texture blocks (64 MB each) | 1,344 | 10,816 | 11,136 | blocks line x map match [M/I] |
| Mirror of card memory: render targets, card buffers, other card things | 1,051 | 4,850 | 5,632 | card in use - blocks [I] |
| Staging blocks of textures (upload memory, 64 MB each) | 192 | 1,920 | 2,624 | "memory in blocks" [M] |
| List scratch chunks | 130 | 96 | 132 | "what holds it" [M] |
| Other write-combined (send buffers, read-back, 99 x 32 MB group 2,508) | 650 | 1,448 | 1,617 | map rest [I]; owner not named |
| Windows of title memory, 4 MiB (+ 8 MiB group 529-767) | 15 | 2,621 | 3,287 | map [M]; counter: 3,184 / 3,660 alive |
| Process heap = our own allocations (containers 189, translations 96, views 107-135) | 409 | 1,040 | 1,136 | map [M]; "own allocations" 1,434-1,562 |
| 4 allocations of 1,024 MB (committed part) | 1,104 | 2,449 | 2,507 | map [M]; **owner unknown** |
| 40 allocations of 32 MB, not write-combined | 51 | 1,250 | 1,259 | map [M]; **owner unknown** |
| Other private >= 1 MB (401 MB own block, 174 MB twin of graphics heap, ...) | 783 | 1,803 | 1,906 | map [M] |
| **Sum** | 10,358 | 33,030 | 35,973 | |
| **Private as measured** (run-118 memory line) | 10,552 | 33,294 | 36,226 | unaccounted 194 / 264 / 253 |
Not in private (mapped): title's page pool 6,236 MB (section; system commit once; 0 MB "in memory" as a pool view,
1.3-2.5 GB resident via `pool discard` line [M]); views of title pages 4.2-8.6 GB (same pages, no extra commit);
pipeline library file 282 MB (file-backed; 1,129-1,694 MB trimmed from memory so far [M]); driver cache files.
In RAM (working set 18.2 GB at mid [M]): views of title pages 3.8, other private 4.8, write-combined 4.0, windows 2.0,
title heaps 1.2, files 1.8, heap 0.7 GB [M]. Host lists: 1,200 alive, "private bytes grew 1,012 MB while lists
were made" [M] (inside the rows above). Not metered at all: DirectStorage buffers (limit 256 MB floor,
`dstorage.rs:236`), descriptor heaps, movie planes, audio, thread stacks, write-watch bitmaps, send buffers
(`host/send.rs:123` keeps up to 768 MB spare). Meter that would close it: the map's group line printing each group's
first base address + the `book` name of the resource mapped there (one `format!` in `copystat.rs:707`).

## 2. VRAM ledger, MB (run-121; last column heavy scene run-120)
| | menu | mid | peak | heavy | |
|---|---|---|---|---|---|
| **Card's own, in use** / budget | 2,395 / 15,277 | 15,334 / 14,990 | 16,565 / 15,050 | 16,944 / 15,015 | [M] |
| Texture blocks | 1,344 | 10,496 | 10,944 | 11,776 | [M] |
| - copies the title fills, alive | 523 | 7,950 | 7,912 | 8,129 | [M] |
| - free space inside heaps (holes) | 52 | 2,366 | 2,040 | 2,729 | [M] |
| - empty blocks kept | 768 | 128 | 128 | 128 | [M] |
| - resting 2.5 s / retiring / padding | 1 | 52 | 864 | 790 | [I] remainder |
| Copies the host renders to or writes (targets, depth twins 226-251) | 471 | 2,728 | 3,405 | 3,042 | [M] |
| Buffers on the card (cap 2,048) | 174 | 1,254 | 1,249 | 1,248 | [M] |
| Rest: swap chain, descriptor heaps, pipelines | 406 | 856 | 967 | 878 | [I] |
| **PC memory shared with the card** | 1,246 | 7,173 | 9,069 | 8,512 | [M] |
| Windows of title memory | 8 | 3,012 | 3,628 | 3,908 | [M] |
| CPU sides of buffer copies | 174 | 1,067 | 1,368 | 1,065 | [M] |
| Staging blocks + list scratch | 322 | 1,512 | 2,428 | 1,952 | [M] |
| Rest: send buffers, read-back, what Windows moved off the card | 742 | 1,582 | 1,645 | 1,587 | [I] |
gpu.csv (all processes): 3,372 / 15,971 / 15,841 MB; the card has 16,376 MB, other programs hold 0.6-1.0 GB [M/I].
**"17.1 GB against 14.7 GB, 7.4 GB shared"**: the 17.1 is 11.8 GB of texture blocks (8.1 alive, 2.7-2.9 holes, 0.8-1.2
resting), 3.0 GB targets, 1.25 GB buffers, 0.9 GB rest. The room rule counts net (`resource.rs:2498`: raw - free block
room), so raw stays over budget by exactly the holes. The 7.4 GB shared is *not* spilled textures: it is our own
CPU-visible buffers (windows 3.9, buffer CPU sides 1.1, staging 0.6-2.4, scratch 0.2) plus ~1.6 GB unnamed, part of
which Windows demoted because 17.1 + other programs > 16.4 GB physical [I].
Cost of the full card [M]: "passes that made none for lack of room" 180-390 (heavy 858); 1,088 card copies of
buffers turned down; 58-102 thousand title textures without a host copy; soft RAM caps cut 6,558 texture passes.

## 3. Holder by holder: idle, empty-but-kept, duplicated
| Holder | Held for nothing | Rule / constant | Existing switch | Cost of release |
|---|---|---|---|---|
| Holes in texture blocks | 2.0-2.9 GB card + same commit [M] | first block with room, `pool.rs:266`; `BLOCK` 64 MB `pool.rs:30` | `VICTORY_HEAP_PACK=1` (run-107: -370 MB raw only [M]) | none for `place`; `=1` changes who is let go |
| Texture pieces resting | 0.8-1.2 GB at bursts [I] | `REST` 2,500 ms `pool.rs:88` | none | safety rule (device loss); leave |
| Empty texture blocks | 128 MB in play, 768 MB at menu [M] | `SPARE` 2 / `SPARE_KEPT` 64 `pool.rs:32-34`, `RIPE_BLOCKS` 12 | `VICTORY_KEEP_FREED=0` | nothing to gain in play |
| Staging blocks | 0.7-3.4 GB hold 60-240 MB live [M]; follows uploads 130-890 MB/s x 2.5 s rest [I] | same `REST` for staging `pool.rs:338`; first-fit; `STAGING_KEPT` 8 `pool.rs:37` | `VICTORY_STAGING_SPARE_BLOCKS` only drops *empty* blocks (already <= 8, 1 under pressure): **no effect** | code change, see 4 |
| Texture copies (title fills) | none: 5 s rule already runs, 20 % come back [M] | `vram_keep` `host.rs:152` | `VRAM_RAW_PCT`, `UNMAP_EXACT` rejected (made again x2) | - |
| Copies over title memory no longer mapped | 0.6-0.9 GB (estimate line) [M], 0-383 MB still shown | sweep takes idle only | `VICTORY_UNMAP_EXACT=1` (rejected) | re-upload |
| Render-target copies | 0 by the meter [M]; 471 -> 3,405 MB, 938 alive | never swept `resource.rs:2571` | none | meter needed: census by size + age regardless of holder |
| Windows of title memory | 1,312 MB outside any asked range (3,044 alive, 1,732 in span) [M]; 930 MB never copied [M]; 284 MB in two windows [M]; 97 % held by slots | 4 MiB grid `shadow.rs:5334`; 570-633 asked without a length | `VICTORY_WINDOW_KB=1024` + `VICTORY_WINDOW_TAILS=1`; `VICTORY_WINDOW_UNMAP=1` | tails: another buffer per longer ask (UI risk); more windows |
| Host lists | 204-446 idle of 1,200; 427 never reset; ~0.84 MB each [M/I] = 0.2-0.4 GB | `IDLE_MOST` 512 `list.rs:241`; idle time 0 = forever | `VICTORY_LIST_POOL_IDLE_S=10` | ~0.2 ms to make one again |
| List scratch spare | 6-128 MB [M] | `SPARE_MOST` 128 MB `host.rs:1929` | - | fine |
| Send buffers spare | up to 768 MB, **no meter** | `host/send.rs:123` (sized for a big PC) | none | meter line needed |
| Buffers on the card | 3 copies of the same bytes (title, CPU side, card): 1.25 GB x 2 [M] | cap 2,048 MB | `VICTORY_MEMORY_CARD_MB` | by design; card time if lowered |
| Title page pool | given-back pages: 17.5 GB discarded so far, 1.3-2.5 GB resident [M] | `MEM_RESET`: RAM only, commit stays | `VICTORY_POOL_DISCARD=1` (on) | 0-788 MB rewritten within 5 s |
| Shader containers + translations | 285 MB, grows to 19,108 / 2,961 [M] | kept for the run | none | recompiles; leave |
| 1 GiB x 4, 32 MB x 40, 32 MB WC x 99 | 6.2 GB committed, owner unknown [M] | ? | ? | name them first |

## 4. Ranked: memory held for nothing
RAM (commit):
1. **Staging blocks, 1.0-2.5 GB** at bursts (exactly when free RAM drops to 0.6-1.3 GB). No switch reaches it. Small
   change: a rest time per pool (staging 0-250 ms: a staging piece is given back only after its list was reset,
   `pool.rs:13-18`), `pack = true` for `STAGING` in `pool::staging` (`pool.rs:428`), and a "staging blocks by
   occupancy" meter (copy of `texture_blocks`, `pool.rs:532`). Risk: wrong texels if a list is reset before its copy
   ran; start at 250 ms.
2. **Unnamed 6.2 GB** (1 GiB x 4 = 2.4, 32 MB x 40 = 1.25, 32 MB WC x 99 = 2.5). One meter line first (section 1).
3. **Window bytes never asked for, 0.9-1.3 GB**: `VICTORY_WINDOW_KB=1024 VICTORY_WINDOW_TAILS=1` (never run on this
   base). Risk medium (interface vertices); judge by "of the windows alive, N MB lie between ..." close to alive MB.
4. **Holes in texture blocks, 2.0-2.9 GB** (commit and card): `VICTORY_HEAP_PACK=1` gives ~0.4 GB; the rest needs
   live copies moved card-to-card (vram-plan #2, not built).
5. **Idle host lists, 0.2-0.4 GB**: `VICTORY_LIST_POOL_IDLE_S=10`. Risk low.
6. Send buffers up to 0.6 GB (cap 768 -> 128 MB on <= 32 GB, after a meter).
VRAM: (1) holes 2.0-2.9 GB, as above; (2) resting pieces 0.8-1.2 GB at bursts (safety, keep); (3) copies over
unmapped title memory 0.6-0.9 GB (the only switch doubled re-uploads); (4) empty blocks 128 MB. Everything else on
the card is in use: the scene needs more than 15 GB, the rule already drops at 5 s and 20 % comes back.

## 5. One test run (existing switches only), on run-121's settings
`VICTORY_HEAP_PACK=1 VICTORY_LIST_POOL_IDLE_S=10 VICTORY_WINDOW_UNMAP=1 VICTORY_MEMORY_MAP=6`
| Switch | Counter that must move | Expected back |
|---|---|---|
| HEAP_PACK=1 | "copies let go to empty a sparse heap N" > 0; "given back this run" > 21; "free space inside heaps" < 1,700 MB; blocks <= 160 | 0.4-0.7 GB card + commit |
| LIST_POOL_IDLE_S=10 | "host lists idle" < 100, "let go as idle" > 0; "host lists alive" < 1,000 | 0.2-0.4 GB |
| WINDOW_UNMAP=1 | "windows let go on unmap N (M MB)" > 0; windows alive < 2,800 MB mid | 0-1 GB (not measured before) |
| MEMORY_MAP=6 | the walk prints again (11-17 ms stall each 30 s) | reconciliation only |
Watch: "made again ... in the last 5 s" must not rise (HEAP_PACK=1 reorders who goes); frame times as run-121.
Honest target: this set should bring mid-run private from ~32.4 to ~31 GB and raw card from 15.3 to ~14.9 GB (at
budget). It will **not** bring the peak (35.4-36.2 GB private; 16.6-16.9 GB raw) under 32 GB / under budget: that
needs the staging change (1-2.5 GB), the window tails (about 1 GB), naming the 6.2 GB, and card-to-card packing.
Second run, separate because of its risk: add `VICTORY_WINDOW_KB=1024 VICTORY_WINDOW_TAILS=1`.
Not worth testing: `KEEP_FREED=0` and `STAGING_SPARE_BLOCKS=8` (their memory is already gone under pressure),
`VRAM_RAW_PCT`, `UNMAP_EXACT=1`, `TEXTURE_RAM_CAPS=0` (rejected earlier). No picture is traded by any of the above.

## Holes: design (coder F, 2026-11-26; nothing here is built)
Holes are 2.0-2.9 GB of free space inside live 64 MB texture blocks [M]. They count on the card AND in commit, and the
room rule counts net (`resource.rs:2498`: raw minus free block room), so raw stays over budget by exactly the holes.
Why they last: `pool.rs` takes a piece first-fit (lowest block, lowest address) from one pool for every size, from
4 KiB up to a whole 64 MB block (`len > BLOCK` is the only limit once blocks are made ahead). One small live texture
keeps a block alive, and the hole a 20 MB texture leaves is filled by small pieces, so the next 20 MB texture needs a
new block. A new block is made whenever no hole fits, whatever the raw use is (`Pool::grow`).

### A. Moving live copies card-to-card (vram-plan #2)
- **What moves:** the live copies in the sparsest blocks (`texture_blocks()` bucket < 25 %, then 25-50 %), sparsest
  first. Only as many as fit in the holes of blocks over 75 % plus ripe empty blocks. One block's worth of live
  bytes moved frees one 64 MB block. Never a copy that a list recorded but not yet submitted names
  (upload target, alias copy, a root view), and never one made in the last second (`pool::fresh`).
- **How:** a new placed resource with the same description in a ripe destination piece (`Ripe`: n08/n11 lost the
  device on fresh blocks). Then aliasing barrier, source to COPY_SOURCE, `CopyResource(new, old)` (same desc, so a
  whole copy is legal; `CopyTextureRegion` per subresource only if the copy holds part of its levels), and back to the
  old state. It is recorded in a host-own list submitted on the title's direct queue at a submission boundary
  (between two title submissions, where `Batch::flush` already runs), at most ~256 MB per boundary
  (~1 ms of card copy) so no submission comes near a TDR.
- **Views and descriptors:** a placed resource cannot move, so this is a re-create, a copy and a descriptor refresh.
  The host copy's `ID3D12Resource` is swapped and its serial bumped. Every view-heap slot and kept CPU-side view that
  shows it is made again. The same machinery exists for a copy that took the place of one with fewer levels
  (`grown` -> `descriptor::host::levels_changed_after`, `resource.rs:1366/1618`). It is run after the move list, in
  the order of that list's steps. The old resource is kept by the move list's `kept` until that list has run. Its
  piece then rests `REST` 2.5 s as usual (`Reclaim`).
- **Device-loss risks:** (1) a slot rewritten while a submission recorded before the swap still reads the old copy.
  That is harmless as long as the old one stays alive and valid, which `kept` + REST covers. (2) A destination in
  a block made while lists ran: ripe pieces only. (3) A missed holder, such as a root SRV or an upload recorded into
  an open list: the swap then writes into the old copy and the picture is wrong (not a device loss). So a copy held by
  an open list is skipped. (4) Wrong states on the copy. Use the host's own state tracking at the boundary, or skip
  copies not in a known read state.
- **Expected:** blocks packed to ~90 % instead of 70-78 % gives 1.5-2.0 GB back, card and commit, at 11.8 GB of
  blocks [I]. Cost: the copy bandwidth (2 GB once, then the churn) and one descriptor refresh per moved copy.

### B. Cheaper: make the holes reusable (recommended first)
1. **Size classes (`VICTORY_HEAP_CLASSES=1`, not built).** Use three pools of 64 MB blocks: pieces <= 1 MiB, pieces of
   1-8 MiB, and pieces > 8 MiB. A hole is then left by a piece of the same class and taken by the next one of that
   class, and small textures no longer pin big blocks. Ripe/REST/HEAP_PACK apply per pool unchanged. It is ~40 lines
   in `pool::texture` (choose `TEXTURES[class]`). `texture_blocks()` sums the pools, with a class column added.
   Expected: about half the holes, 1.0-1.5 GB card + commit [I]. Measure first: a piece-size histogram in
   `texture_blocks()`.
2. **Best fit instead of first fit.** Take the smallest free range that fits, over the ripe blocks, not the lowest
   address of the first block. `Ranges` is a BTreeMap by start, so add a by-length index. This keeps big holes
   whole for big textures. It stacks with 1.
3. **Raw cap on growth (from the old tree's `no_room`, `victory-dev resource.rs:3449`).** Make a new block only while
   raw use plus 64 MB stays under 95 % of budget. Past that, cut the piece from any block with room
   (`take_fullest`), and only if none fits make one. That is what kept the old tree's raw within budget. Run
   11-mem-tex lost the device making ten heaps in a burst while 2 GB lay free in draining heaps.
What the old tree did (22 x 256 MB heaps, 5.6 GB): `TEXTURE_HEAP` 256 MB; textures over 1/8 of a heap (32 MB) were
committed resources of their own (no holes, but a fresh allocation; fox puts them in made-ahead blocks because of
n08/n11, so do NOT bring the cutoff back); fullest heap first with HEAP_PACK (already ported: `VICTORY_HEAP_PACK=1`,
-370 MB raw in run-107); the raw 95 % `no_room` rule (item 3: cheap to bring over); draining (rejected: lost the device).
Bigger blocks alone do not help: one live piece pins a bigger block.
Order: HEAP_PACK=1 now, then a piece-size histogram, then classes + best fit + raw cap, then A only if raw is
still over budget.
