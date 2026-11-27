# Fox port, coder C: victory-gpu (not frame.rs / shader_cache.rs) + victory-pipeline

Copy: `Tools\par\foxC` (branch local/scarlett-fox @f4b48ad + my commits). Old tree: `victory-dev` (read only).

## 0. Findings before porting (read these first)

1. **run-100 was built WITHOUT the victory-gpu `diagnostics` feature** (or with a build whose gpu crate lacked it):
   every `#[cfg(feature = "diagnostics")]` line of fox is absent from its log ("what shaders wrote put into title
   memory", every `victory_gpu::copystat` line incl. the "memory:" line, 0 of 2,618 in run-98), while its
   non-diagnostic lines ("the queue:", the fps line) are there. So 64 vs 88 ms compares a non-diagnostic fox with a
   diagnostic ours, and the copystat lines our scripts read can only come back with `--features diagnostics`.
   run-100 also overlapped a build (757 CPU-s, build-activity.txt).
2. **run-100's env** had `VICTORY_CARD_STEPS=2` (ours: rejected, stretched bodies), `VICTORY_SHOW_AFTER=1` (shows
   a texture only after its upload's submission finished = later data), `VICTORY_UPLOAD_AHEAD=1`, `JOIN_PARTS=8`.
   SHOW_AFTER alone delays every index-picked texture by >= 1-2 frames: part of "textures later" is the env, not code.
3. **Why fox loads textures later (code):** `residency.rs` host-RAM pressure. In run-100 "host memory pressure:
   limiting texture prefetch" was ON from 07:46:01 to 07:50:05 and from 07:50:22 to the end (most of the run; entered
   at 2.9 GB free physical, not commit). Under pressure `prefetch_limit` caps the index-picked texture pass at
   64 MiB a frame (16 MiB when commit < 1.5 GiB), and `limited_host_ram()` (our 32 GB PC) caps the base at 128 MiB
   (ours: 192 MiB, x3 in catch-up). Second suspect: `VICTORY_MAPPED_BY_VIEWS` (`inplace::shown_bytes`, the
   runtime's view record instead of VirtualQuery): fox counted 52,004 uploads of "not all mapped" textures and 4,681
   "uploaded in part" at a time against our 72 and 1 for the same game: if the view record under-counts, those
   textures are uploaded partly and re-looked at with back-off.

## 1. Inventory (status: present / lacking / conflicts / rejected; "->" = what I do)

| Switch / mechanism (ours) | Fox | Status | Plan |
|---|---|---|---|
| JOIN_COPIES | same switch (host.rs) | present | - |
| REFRESH_ONCE | none | lacking | c (speed, OFF) |
| CARD_REHASH_RUNS | same | present | - |
| BACK_OWN_VIEW | `LAND_BESIDE` (land.rs) | present (other name) | - |
| WIDE_MERGE | same name | present | - |
| LEVELS_ON_DEMAND | same (tiles.rs), off by default, switches HELD_LEVELS off | conflicts | keep fox (HELD_LEVELS) ; env decides |
| MIRROR_FAST | `QUICK_MIRROR` | present (other name) | - |
| CARD_METER (+_KINDS) | none (`CARD_SPANS` own stamps) | lacking | d (meter, OFF) |
| PIPELINE_WAIT_MS | same | present | - |
| ASYNC_SUBMIT | same name, `queue/behind.rs` line | present (fox mechanism) | - |
| QUEUE2 (fetch on copy queue) | `RELAY_COPY` | present (other name) | - |
| INDIRECT_WINDOW | same | present | - |
| TEXTURE_NO_WAIT | same | present | - |
| COPY_PLAIN | `SAFE_COPY` | present (other name) | - |
| WIDE_TEXTURES | same | present | - |
| INDIRECT_GATHER | `INDIRECT_BATCH` (indirect/batch.rs) | conflicts | keep fox |
| LATE_WAIT / LATE_WAIT_LISTS | same | present | - |
| JOIN_PARTS | same (+ REFRESH_FRONT) | present | - |
| GATHER_LATE | none | lacking, overlaps INDIRECT_BATCH | c, OFF / probably not |
| FILL_AHEAD_PER_PIECE (+FILL_PREDICT_CARD) | none (fox `FILL_TITLE` off, CARD_STEPS) | lacking | c, OFF until A/B (ours measured 87->76 ms; PREDICT_CARD=1 not adopted) |
| TEXTURE_RELEASE=game, UNMAP_EXACT (gpu side) | none: copies go only by the room rule (`vram_keep`); slots nulled after `TEXTURE_AWAY_MS` | lacking | b (ON) |
| VRAM_EARLY (+HIGH/LOW_PCT, MB_PER_FRAME) | `TEXTURE_KEEP` / `vram_keep` (NET 88/95/85 %) | conflicts | keep fox; raised marks already REJECTED |
| WINDOW_UNMAP | none (windows go by idle time only) | lacking | b (ON) |
| UPLOAD_ON_WRITE | `texture_slots_remapped` (runtime tells mapped/unmapped/written) | present (other mechanism) | check only |
| HEAP_PACK | `pool.rs` 64 MiB blocks, 4 KiB small placement | conflicts | keep fox |
| WINDOWS_YIELD | none | lacking | b/c (RAM/VRAM), check |
| HEAP_LOCKS (striped kept) | none | lacking | c (OFF until A/B) |
| LOOKUP_CACHE | `QUICK_FIND` | present (other name) | - |
| HEAP_LEAN | none | lacking | c |
| GUARD_REST | same | present | - |
| BIND_FILTER | `BIND_CACHE` (targets, pipelines only) | conflicts (ours wider) | c, OFF |
| CHEAP_DRAWS | partly (`QUICK_FIND`) | lacking | c, OFF |
| DYNAMIC_SLOTS | same | present | - |
| STALE_KEEP_MS (+MAX, EXPIRE_NULL) | `TEXTURE_AWAY_MS` 3 s + `KEEP_SHOWN` | conflicts | keep fox |
| TEXTURE_CATCH_UP_MS (+BUDGET_MS_PER_S, MAX_AGE_MS) | `TEXTURE_CATCHUP` (x3 budget on backlog) | conflicts | b: only if texture arrival still behind |
| TARGET_VARIANTS (V1) | none | lacking | b (ON, picture) |
| TABLE_STAGES (V1) | none | lacking | b/c check |
| LIBRARY_SAVE_S | none (workers every 20 s + exit) | lacking | b (ON) |
| HEAP_SYNC_ASIDE | same | present | - |
| HEAP_PASS_ASIDE | same (host/aside.rs) | present | - |
| VIEW_THREADS | same (land.rs pools) | present | - |
| PREPARE_WIDE | none | lacking | c, OFF |
| RANGED | none | lacking | c, OFF |
| STAGE_POOL_MB | `pool.rs` staging blocks | conflicts | keep fox |
| STEP_CLOCK_LEAN | none | lacking | c (no-risk: ON) |
| RELEASE_AT_LANDING | f4b48ad H3-5 (dynamic slots given back at landing) | present | - |
| BAKE_EXACT, BAKE_DETECT | same (f4b48ad; ours were ports of it) | present | - |
| INDIRECT_REST | same | present | - |
| UPLOAD_RETRY | `residency::upload_refused` + `STAGING_DEFERRED` | present | - |
| V1 visual host fixes | KEEP_SHOWN/KEEP_VIEWS/CARD_ACCOUNTING there; STALE_*, TARGET_VARIANTS, TABLE_STAGES, CATCH_UP not | partly | b per row above |
| FRAME_LOG (framelog.rs: "frame times", "usual frame", "long frame") | none | lacking | a (tooling ON) |
| "memory housekeeping" line | none | lacking | a |
| "memory of textures" (copystat) | fox copystat has other lines | lacking | a |
| "the queue:" line | same wording | present | - |
| copystat lines | fox has its own copystat (cfg diagnostics) | partly | a |
| title timeline (timeline.rs) | fox timeline = Chrome trace, other module | conflicts | a: R only, if cheap |
| HANG_WATCH_S (hangwatch.rs) | none | lacking | a/b (tooling ON) |
| TEXTURE_DETAIL, BACK_CONSUMERS, silent-defects line, prepare line, sweep parts | none | lacking | d (OFF) |
| CARD_FILLS_BATCH, GATHER_DISPATCH | - | rejected (P2: no gain, device loss) | not ported |
| CARD_BUFFERS_MB=2048, VRAM 93/90 marks | - | rejected | not ported |
| BACK_SKIP_UNREAD, ARENA_BY_CHANGED, INDIRECT_RAW=3 with JOIN_PARTS | - | rejected (F1, F8, F7) | not ported |
| MAP_FAST, MAP_DEFER, POOL_DISCARD, DSTORAGE_LINEAR_VIEW | hle (coder A); gpu halves in mapfast.rs/framestream.rs | A's area | gpu half on request |

## 2. Port log (commits in par/foxC, branch local/scarlett-fox)

Rule since the user's note: cool_fox as it ran is the floor; a port changes fox's default behaviour only if it is
a picture/correctness fix with no measurable frame cost. Everything else is a switch, default = fox's.

| Switch / item | Commit | Default | Changes fox default behaviour | Log line / what to look for |
|---|---|---|---|---|
| frame-log feeds (preparing, waiting for the card, copy-back, card idle, copies, submit thread at work, present, submit calls, uploads, remapping, pipeline waits) into A's framelog | 92170fb | on (atomic adds; 2 GetThreadTimes per submit-thread wake) | no (meter) | "usual frame" / "long frame" columns no longer 0 |
| card column "all other lists" from fox's own stamps (`host/spans.rs`) | last | off; on with `VICTORY_CARD_SPANS=1` or `VICTORY_CARD_METER=1` | no | "card: ... all other lists X ms" (= every list of the submission, ours included; read ~512 submissions late) |
| "memory housekeeping: card N MB in use (N MB counting free heap space) of N MB ..." | 92170fb, b2 | on (`VICTORY_HOUSEKEEPING_LOG=0` off), every 5 s, one read lock | no (meter) | in use = fox's NET (vram_keep), high/low = 88/85 %, "out-of-memory fallback ran 0 times" (none in fox) |
| "memory of textures", copystat lines | fox has them | need `--features diagnostics` | - | same wording as ours |
| `VICTORY_TEXTURE_RAM_CAPS` | 58efb2e, 114068c | 1 (fox: 64/128 MiB soft caps + 16 MiB critical) | no | "textures picked by index, since the start: passes cut by a host-RAM cap N critical, M soft"; A/B =0 |
| `VICTORY_MAPPED_CHECK` | 58efb2e | 0 | no | same line: "partial answers ... checked with the host N, the host found more M"; A/B =1 |
| `VICTORY_UNMAP_EXACT` (texture copies go at the sweep after a real unmap) | 58efb2e | 0 | no | housekeeping "let go on a real unmap N (M MB), held at a sweep, mapped again"; A/B =1 |
| `VICTORY_WINDOW_UNMAP` | 58efb2e | 0 | no | housekeeping "windows let go on unmap N (M MB)"; A/B =1 |
| `VICTORY_LIST_POOL_IDLE_S` (idle host lists + allocators let go) | b2, 114068c | 0 (fox keeps 512) | no | housekeeping "host lists idle N, let go as idle M"; A/B =10 (RAM: fox's "lists' allocators 9 GB") |
| `VICTORY_LIBRARY_SAVE_S` (periodic library save on a thread) | b2, 114068c | 0 | no | "host pipeline library: written ..."; A/B =10 |
| `VICTORY_TARGET_VARIANTS` (pipeline made for the RT formats bound at the draw, fxceless a35c557) | b2 | on | yes, picture fix: one 9-word compare per draw | "N pipelines made again for the render target formats bound" |
| alias copy after an upload in the same list (fxceless 19a45d1) | b2 | on (no switch, as ours) | yes, picture fix: only adds a copy where the defect is | "N copies of a card-written texture into its aliased view after ..." |
| `show_from` keeps ResourceMinLODClamp (fxceless 41f9857) | b2 | on | yes, picture fix, free | "N views moved to their first level with memory kept the title's minimum level" |
| `VICTORY_HALF_VIEWS` host half (coder B) | 9b2debf | on (translator switch) | yes, face fix: fresh tables only for the few shaders with half places | "a shader moves 16-bit halves ..." once per shader; "N table slots made with the unsigned twin" |

| A's hooks: `hlehooks::on_title_memory_unmapped` (the only real-unmap path), `hangwatch::set_state` + sites (Execute, Present, Signal, Wait, settle_now), `chunks::refreshed` in `shadow::copy_all` / `copy_stale` | 2d7727c | as A's switches | no | hang watch "layer state: the submitting thread ..." |
| `VICTORY_STAGING_SPARE_BLOCKS` | 71c0f88 | unset = fox (64 empty staging blocks below 85 % of the card) | no | A/B =8 |

### C2 (2026-11-26, continuing C; all default OFF)

| Switch / item | Commit | Default | Log line |
|---|---|---|---|
| `VICTORY_HEAP_PACK` (=1: new piece into the fullest 64 MB block it fits + room rule takes its candidates sparsest block first, LRU within 1/32 steps; =place: placement only) | 58a8232 | off | housekeeping "...; texture blocks N alive (E empty, G given back this run); heaps by occupancy: <25 % a, 25–50 % b, 50–75 % c, >75 % d; free space inside heaps F MB; copies let go to empty a sparse heap N (M MB) (VICTORY_HEAP_PACK=x)" |
| `VICTORY_VRAM_RAW_PCT` (80-99: under fox's 88 % net mark, while RAW use is past this share, the room rule takes copies idle 5 s, <=128 MB a second, down to pct-3; use with HEAP_PACK=1) | 58a8232 | unset | "room rule let go N (M MB) in S sweeps, raw guard ran R s (VICTORY_VRAM_RAW_PCT=..)" |
| frame log "remapping: ... (X waiting for readers, Y in our notes of it)" (gpu hooks inside the mapping lock timed; framelog.rs touched, A finished) | 3d08510 | with frame log | Y vs the total: our notes vs system calls/paging |
| `VICTORY_TEXTURE_DETAIL` (F11 meter, texdetail.rs; draw::readable takes the descriptor record) | d1fbed9 | 0 | "texture detail: ..." every 5 s; =16 as in the old tree |

Findings (C2): run-103 raw card 16.3 GB vs net 13.0 GB on a 14.9 GB budget (3.3 GB holes in partly used blocks;
the room rule counts net, so raw stays over budget and 6.2 GB of PC memory is shared with the card). run-104
UNMAP_EXACT=1: 19.5 GB let go on real unmaps ON TOP of an unchanged 27 GB room-rule release (46.6 total); made again
5.8 -> 12.1 GB ("mapped again" 320k: the game re-maps what it unmaps) -> keep UNMAP_EXACT=0. Long frames (run-103,
after 75 s): 225 frames / 48.2 s, remapping dominant in 165 (34.4 s), 153 us per call vs 78 in the old tree, with free
RAM 1.7 GB vs 4.1 GB in long frames. Usual frame 52 ms bound by the submit thread's preparing (36 ms: refreshes 15,
card copies 9.5). Group 3 not ported, each already in fox: REFRESH_ONCE (REFRESH_FRONT/ahead), RANGED (light refresh
`ranged`), PREPARE_WIDE (wide fills/refreshes/textures), STEP_CLOCK_LEAN (run_deferred continues without a clock);
FILL_AHEAD_PER_PIECE saves at most the 2.8 ms of flight waits; HEAP_LOCKS/HEAP_LEAN/GATHER_LATE target recording, not
the bound. WINDOWS_YIELD n/a (fox windows have no card side, `VICTORY_CARD_WINDOWS_MB`=0). Not ported in group 4:
READBACK_SET sink (it changes copy-back), FAULT_METER (34 sites), TITLE_TIMELINE, CARD_METER_KINDS, silent-defects line (I found none in victory-dev).
A/B set: HEAP_PACK=1 VRAM_RAW_PCT=95 STAGING_SPARE_BLOCKS=8 (UNMAP_EXACT=0).
run-107 (that set + A's mapping group): packing ran (168 blocks, 137 > 75 %, free inside 1.7 GB, 15 given back,
4.1 GB let go from sparse blocks) but raw fell only 370 MB vs run-106: sparse blocks hold copies in use. The raw guard
ran 189 s and its room was refilled at once (let go 43 GB vs 27): VRAM_RAW_PCT REJECTED, keep unset. Next real step
would be moving live copies card-to-card (vram-plan #2; large, device-loss risk), not built. Frame time 103/106/107 =
53.9/60.5/60.7 ms at 2.5k+ draws, all in "waiting for the card" (1.9/10.0/15.2) with card idle 28 -> 15: the card is busier
under A's mapping group; page faults/frame 3.5k -> 4.6k; candidate POOL_DISCARD or MAP_DEFER (one-off A/B needed).

C2 second round (user: port every mechanism that improved the old tree; all default OFF):

| Switch | Commit | What / log line |
|---|---|---|
| `VICTORY_CHUNK_FINAL=1` | e5ca023 | F21: refreshes of ranges in retired arena chunks already taken by the copy are dropped before write-watch asking/compare; undone by chunks named open, our title copies there, unknown title writes. "chunk final ... refreshes found final and dropped N (M MB)". hle chunkmeter.rs installs the hand-over hook for it |
| `VICTORY_CHUNK_COUNT=N` gpu side | e5ca023 | dynamic slots = N x 8,192 (capped heap: at most half), as the old tree; "dynamic descriptor slots scaled with VICTORY_CHUNK_COUNT" |
| `VICTORY_REFRESH_ONCE=1` / `=verify` | f7b9d81, 6af62d7 | own record `Batch::once`: a title copy trims it (fox clears its `ahead` at every one of 4,700 title copies a frame); flush results join it. copystat "N skipped M MB"; verify: WARN "...would have left out took bytes" + totals (must be 0) |
| `VICTORY_PREPARE_WIDE=1` | a9a20f0 | fills wide from 32 (fox 96), a flush's refreshes wide from 2 copies (fox 4) |
| `VICTORY_BIND_FILTER=1` | 3ec0e22 | root constants / root views / tables given again unchanged left out; record cleared when fox sets the root signature and in replay; "bind filter ... given N, left out M" |

Checked present/active in run-103 (not ported): fox wide refreshes 6,793 of 7,593 a frame; fills/copies waiting for
flights 0.0 a frame (CARD_STEPS=2): FILL_AHEAD_PER_PIECE has nothing to save; HEAP_LOCKS (fox has a lock per slot),
HEAP_SYNC_ASIDE / HEAP_PASS_ASIDE / VIEW_THREADS (on by default), RANGED (light refresh), STEP_CLOCK_LEAN, CHEAP_DRAWS'
coarse clock (fox `coarse_ms`), LOOKUP_CACHE (QUICK_FIND), MIRROR_FAST (QUICK_MIRROR), GUARD_REST (fox, off: A/B =3000).
GATHER_LATE / INDIRECT_GATHER conflict with fox INDIRECT_BATCH=2. SETTLED idea = fox's view guards (7.2 of 8.3 ms
watches are guard askings; A/B GUARD_REST=3000). Still to port: HEAP_LEAN (sync runs aside; <= 2 ms of waits),
CHEAP_DRAWS' per-recording lookup cache (`Looked`). Fox's texture sweep already goes 8,192 textures per look; no
sweep meter was in run-103's lines.
Mapping A/B (runs 106-111): remap per call 142 us floor, 68 all six, 143 without POOL_DISCARD, 190 without
DISCARD+DEFER: POOL_DISCARD halves it; frame-time deltas within the floor's own spread.
Tests: victory-gpu lib 395 pass; fails: 2 indirect WARP tests (pre-existing) and hlehooks unmapped test (order-dependent:
the host registers its hook first in a full run).

RAM (run-101, 37.3 GB private vs ours 31.9): the gap is "write-combined (buffers shared with the card)" 21.5 vs
14.4 GB = committed mirrors of card heaps: fox's 239 x 64 MB pool blocks ("11776 MB of textures, 2560 MB of their
staging"). Causes (fox defaults): `vram_keep` keeps copies 10 min under 88 % (host textures 11.2 vs 8.4 GB alive), and
`pool::room` keeps 64 empty texture blocks and 64 empty staging blocks while the card is under 85 % and RAM is not
pressured (`KEEP_FREED`). Host lists alive 1,193 vs 939. A/B set: UNMAP_EXACT=1, WINDOW_UNMAP=1,
STAGING_SPARE_BLOCKS=8, LIST_POOL_IDLE_S=10; stronger: fox's KEEP_FREED=0. (POOL_DISCARD is A's, hle.)

Tests: victory-gpu 393 pass after the show_from test fix; indirect::tests::host_list_budget_starts_empty_at_each_submission
and warp_batch_charges_every_execution fail on pristine fox f4b48ad too (WARP; pre-existing, not ours).

Not ported (and why): every `present`/`conflicts` row of section 1 (fox's own mechanism kept); STALE_KEEP_*
(fox KEEP_SHOWN/TEXTURE_AWAY_MS), TEXTURE_CATCH_UP_MS / BUDGET_MS_PER_S (fox TEXTURE_CATCHUP), TABLE_STAGES (off at
its author's too; fox has TablePlan); rejected rows. Speed mechanisms of group c (FILL_AHEAD_PER_PIECE, HEAP_LOCKS,
HEAP_LEAN, CHEAP_DRAWS, PREPARE_WIDE, RANGED, STEP_CLOCK_LEAN, REFRESH_ONCE, GATHER_LATE): not ported in this pass:
each overlaps a fox path (behind.rs line, land.rs pools, QUICK_FIND, BIND_CACHE, INDIRECT_BATCH) and could only be
default OFF; ported on request after the A/B of the base. Meters TEXTURE_DETAIL, BACK_CONSUMERS, silent-defects
line, prepare line, title timeline: not ported (fox has no equivalent hooks; tooling only).

Needs a game run to judge: RAM_CAPS=0 (texture arrival vs frame time), MAPPED_CHECK=1 ("host found more" > 0 means
fox's view record under-counts), UNMAP_EXACT/WINDOW_UNMAP=1 (VRAM, "made again"), LIST_POOL_IDLE_S=10 (commit),
the three picture fixes and half views (faces). Fair A/B base: fox without CARD_STEPS=2 / SHOW_AFTER=1 vs with.
