# Loops in frame drops: is the game (or the layer) doing the same work over and over?

Read-only analysis, 2026-11-26. Runs: 113-fox-floor3 (360 s, main source), 109-fox-floor2, 112-fox-best-5min
(MAP_DEFER, POOL_DISCARD, REMAP_SAME on), 104/105 (UNMAP_EXACT=1), 115-fox-combined (chunk meters, died at 75 s).
run-118-fox-meters did not exist when this was written (see the last section).
Short answer: no game-side retry loop was found in the dropped frames. The game does bulk streaming work (page
mappings, one 64 KiB page a call); the LAYER turns it into stalls and repeats its own work on top: it uploads the
same textures 2-4 times, and it holds every reader of title memory during each mapping call.
## Size of the problem (run-113, from the "frame times" lines)
5,263 frames in 360 s; 233 frames over 150 ms = 80.8 s (22 % of the run); 26 frames over 500 ms = 28.1 s
(one is the 6.5 s load frame); 20 of 70 five-second periods hold a frame over 500 ms.
Logged long frames after 75 s (at most 9 lines per 5 s, so an undercount): 283 frames, 66.6 s, split by kind:

| Kind | run-113 | run-109 | run-112 | Per frame (113) |
|---|---|---|---|---|
| A remapping dominant, under 1,000 calls | 141 fr, 29.2 s | 173 fr, 33.5 s | 44 fr, 6.2 s | 440 calls at 387 us each |
| B remapping dominant, 1,000 calls or more | 62 fr, 19.3 s | 60 fr, 16.6 s | 26 fr, 16.6 s | 2,073 calls at 121 us each |
| C upload burst (over 200 uploads) | 44 fr, 12.7 s | 32 fr, 10.0 s | 10 fr, 5.3 s | 1,003 uploads, 79,730 page faults |
| D other (heavy scene, ~3,900 draws) | 36 fr, 5.5 s | 37 fr, 4.4 s | 17 fr, 2.6 s | preparing 52 ms, card wait 31 ms |

Usual in-game frame for comparison: 60-700 mapping calls taking 10-60 ms in all (about 100 us each), 20-120 uploads,
2,000-12,000 page faults. Long frames over 4,000 draws: 70 of 283 (they are not a kind of their own).
## 1. Page mapping (kinds A + B: 48.5 s of 66.6 s) - game work, layer cost; not a same-page loop per call
- Same mapping asked twice: NO. REMAP_SAME counter, runs 105 and 112, every line: "N pages mapped or unmapped
  (0 of them there already, 0 of nothing unmapped)". The switch has nothing to skip.
- Same addresses unmapped and mapped again later: YES, as streaming churn. run-104: 43,082 MB really unmapped in
  689,326 calls in 6 min (62 KiB a call = one page a call) against 7.7 GB of texture copies alive, about 5 turnovers;
  "let go on a real unmap 32,740, mapped again 320,442": 9 of 10 copies under unmapped memory had pages mapped again
  by the next sweep. Each page is mapped twice by design (pool view, then the GPU's tiled heap; xmem.rs:1054-1062).
- Physical pages are NOT reused fast: POOL_DISCARD (run-112) discarded 6,782 MB, "rewritten within 5 s: 0 MB".
- Kind A is not more calls but slow calls (387 us against ~100; single frames: 227 calls 515 ms, 387 calls 813 ms,
  589 calls 1,041 ms). "waiting for readers" and "our notes" are 0-15 ms, so the time is inside the system calls,
  with free RAM 0.5-3 GB (33-35 GB private on a 32 GB PC). Not a loop: memory pressure.
- Why the frame stops: each call holds the MAPPING write lock (victory-gpu watch.rs:2395), and no in-place read of
  title memory (uploads, refreshes, the submit thread) runs meanwhile.
- MAP_DEFER (run-112) trades many stalls for fewer, longer ones: 20,459 of 53,603 deferred operations were
  "drained by a waiting reader", 1,314 waits = 26.8 s, the longest 2,065 and 1,840 ms. Frames over 500 ms:
  13 frames 16.4 s (remapping 13.4 s) against run-113's 17 frames 13.3 s (remapping 8.9 s).
- Test / fix: (a) new meter in xmem.rs `unmap` (2018) / `map` (1885): per-page stamp of the last unmapping, then
  "pages mapped again at the same address within 1 / 5 / 30 s (same physical page / another)" and a histogram of
  us per call split into lock wait (`Costed.waited`, xmem.rs:1024, today only in the stream log) and system call;
  (b) mapdefer.rs: a waiting reader should drain only the pages it needs, not the queue; (c) less commit.
## 2. Texture uploads (kind C: 12.7 s, and part of B) - the LAYER uploads the same textures again and again
run-113 totals: 293,585 uploads = 114,189 MB in 6 min (run-112: 141,222 = 50,283 MB), while 146,101 host copies
were made (42.2 GB let go + 7.6 GB alive = about 50 GB). So about 64 GB (56 %) went into copies that already existed.

| Repeat | Count (run-113) | What happens |
|---|---|---|
| Partly mapped texture uploaded again when more pages map | 125,312 uploads (43 % of all); 787,039 looks "found no more mapped"; 3,637 standing "in part" | draw.rs:2867-2876: the whole mapped part goes up again each time `mapped` grows; the game maps one page a call |
| "changed after its upload with nothing to tell of it" | 39,001, not 239: the warning prints the first 200, then every 1,000th (`changed=39001`) | title writes through another view of the same pages (blocks_shown_elsewhere 2-127); looked at again the next frame (draw.rs:204, 804) |
| Copy let go by the room rule, then made again | 21,468 copies, 9,303 MB of 43,028 MB let go (22 %) | card 16.5 GB in use of a 14.8 GB budget; load -> let go -> load |

- Top repeaters among the 239 printed warnings: 189 distinct addresses; 0xb0db38000 (176 KiB) 7 times, eight
  addresses 0xb3c6xxxxx (172-684 KiB) 3-4 times each; sizes 44-700 KiB; 88 MB in all. No texture is re-uploaded
  hundreds of times in what is printed; the other 38,800 are not listed (meter needed, below).
- Burst frames: 1,000-3,400 uploads against 20-120; 150,000-360,000 page faults against 4,000. At 332.7 s:
  3,415 uploads, 340,494 faults x 4 KiB = 1.33 GB = the bytes uploaded: one soft fault per 4 KiB page of a freshly
  mapped view. The card sat idle 648 ms "while the title recorded".
- Periods with 0 MB let go by the room rule: 0-1 long frames; with 500-2,600 MB: 7-18 (correlation only).
- Test / fix: new switch at draw.rs:2867 (upload only the newly mapped pages, or wait until `mapped` held still for
  N frames); new per-frame counters "uploads and MB by cause: first / partly again / changed again / made again"
  into the frame log; print the top 20 addresses by re-upload count at exit. Existing: VICTORY_HEAP_RECHECK=0
  (no change looks), VICTORY_UNMAP_EXACT stays 0 (it doubled "made again", 5.8 -> 12.1 GB).

## 3. Refresh / compare loops - real repeats, but steady (every frame), they do not explode in long frames
- "28,133 refreshes 22,322 MB a frame" (run-112) is NOT 22 GB compared: it adds the full length of each range per
  asking. Real work per frame: 413 write-watch askings, 10.7 ms, 8,761 MB checked by their bits (8,680 unchanged),
  15.7 MB hashed. Most repeated: 0x2f90000 + 174 MB 12.0 x/frame (2,187 MB, 2.4 ms); 0xd6b610000 + 90 MB 3.9 x;
  0xfb2600000 + 50 MB 4.2 x.
- Submit thread: 10,372 refresh steps in 35 flushes a frame, 9,215 "known to copy nothing", 10.8-11.5 ms a frame.
  With VICTORY_REFRESH_ONCE working (runs 113/114: 20,211 refreshes, 20,050 skipped) the time is the same
  (11.6-13.0 ms in 21 flushes): the repeats are cheap, the cost is in the few that copy.
- Arena (run-115): "our refreshes from the segment 275-361 MB in 12-16 refreshes" a frame against 12 MB a frame
  really final: the 27 MB segment is taken once per submission. Tests: VICTORY_CHUNK_FINAL=1 (chunks.rs:93).
- Descriptor heap: 677,074 slots looked at a frame, 9.3 ms; 3,933,675 descriptor copies "changing nothing" (112).
- "asking about title memory": 229-256 kernel askings over 1.4-1.65 GB, 3 ms; but 21.8-29.8 ms a frame "waiting
  for the guards" (all threads). Title copies: 3,600-11,500 a frame of about 600 bytes (2-7 MB), 0.7-2.2 ms.
- In long frames "preparing" is 30-100 ms of 400-1,200 ms: this group is the usual frame's cost, not the drops'.
  (One exception seen: 237.1 s, 1,185 ms, preparing 699 ms with copies 636 ms and 1,855 uploads.)

## 4. Game-side retry loops - none found that costs time
- Pipeline "creation method that is not implemented": two bursts at load only (slot 0x218 to 2,048+ calls, slot
  0x1c8 to 16,384+ calls, all within 09:38:50-51, nothing after). Not asked again every frame.
- Draws the title issues every frame and the layer drops: 326 a frame without a host pipeline (151 "being made",
  108 "a stage of the ray-tracing structure builder, which is off") and 494 indirect, of 3,054 draws. Picture
  loss; no time lost. "views refused" 0; "dispatch without a host pipeline" warned once.
- F19 Sleep(1) for want of a chunk: 0.00 a frame in run-115 (216 chunks with CHUNK_COUNT=64). The base with the
  game's own 32 chunks is not measured yet.
- Sleep/yield storms, identical DirectStorage requests, fence polls: no meter in runs 103-115. The frame log's
  "frame thread streaming", "disk", "recording" and "CPU by thread group" columns read 0 in every fox line: not fed.
## What to read in run-118-fox-meters (CHUNK_METER, STREAM_METER, TEXTURE_DETAIL=16)
- "F19 chunk pool per frame": Sleep(1) at 0x140004b01 / 0x140031ce3 and "quota waits ... of 50 us or more" above 0
  in the 5-s periods that hold long frames = the game looping on us (it waits for a chunk our fence frees).
- "F20": serial ms on the render thread rising in long frames = drains, not loops. "F21": "chunk reused before
  read" above 0 = lost work; "refreshes from the segment" MB against "MB final".
- Streamer lines: group drops per second (F12: thousands a second = the streaming thread spinning on empty groups
  while its lists are non-empty) and not-complete polls, against the remapping calls of the same second.
- "texture detail": "partial", "stale", "re-addressed N (new copy N, reused 0)": run-115 showed 22,979 and 4,999
  re-addressed in 5 s, every one a new copy, none reused: the same texture at a new address is uploaded afresh.
