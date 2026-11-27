# Heavy scene (run-120, played by hand): what the frame is made of and what to optimise (2026-11-26)
Read-only. Sources: stdout.txt of `run-120-fox-best118-5min` and `run-118-fox-fill-delta2` (same build, same
switches), captions.txt, code in `victory-fox\crates\victory-gpu\src`. Frame-weighted. **[P]** log line or code,
**[I]** inferred. "Heavy" = t >= 180 s, >= 5,500 draws: 14 windows, 927 frames, mean 7,510 draws. "Route" = run-118
after loading, 2,500-4,000 draws: 30 windows, 2,721 frames, mean 3,370 draws.

## 1. Verdict
- [P] **The heavy scene is bound by the card, not by per-draw CPU work.** Card's own stamps: 60.3 ms of work per
  68.9 ms frame in 18.3 submissions, idle 7.6 ms (4.4 behind us); "the card about 90-96 % busy"; the longest single
  submission 27-28 ms (route: 12.5). gpu.csv: 100 % in every sample.
- [P] Draw count explains little of it. Inside the heavy part, 6,500 -> 9,190 draws (+2,690) costs +4.2 ms of frame
  (1.6 us a draw) and +4.0 ms of card (1.5 us a draw). Route -> heavy is +24.2 ms of card for +4,140 draws: about
  6 ms of that is draws, **~18 ms is what this scene's passes cost on the card** (content, not count).
- [P] Second bound, hidden behind the card: the single submit thread. Preparing 36.1 ms; its other 19.4 ms are one
  read wait a frame (17.1 card still running the flights, 0.6 landing). With a free card: ~37-40 ms, still over 33.
- [P] Not bounds: render thread 2.2 ms (present 0.2, submit calls 1.8, paced 0.0); title threads waited 0.0 ms for
  room in the line; pipelines 0.2 ms waited; remapping 6.4 ms; texture refresh 6.3 ms.
- So CPU switches (section 4) move their counters but not the frame until card time falls below ~40 ms.

## 2. The frame per thread (ms a frame; heavy vs route)
| Piece | Route 3.4k | Heavy 6.5k | Heavy 9.2k | us per draw (6.5k->9.2k / route->heavy) | Scales with |
|---|---|---|---|---|---|
| Frame | 46.7 | 67.3 | 71.5 | 1.6 / 5.4 | card |
| **Card: all lists of our submissions** | 36.1 | 58.8 | 62.8 | 1.5 / 5.8 | scene content, then draws |
| Card idle (of it behind us) | 9.8 (6.7) | 9.0 (4.4) | 5.2 (4.4) | - | - |
| **Submit thread: preparing** | 28.3 | 34.2 | 39.3 | 1.9 / 1.9 | draws (flat part ~22) |
| - refreshes of copies (33-35 flushes of ~10,000; 87 % known to copy nothing; 94 % wide) | 11.3 | 12.6 | 16.0 | 1.0 / 0.6 | bytes: copies 84 -> 104 -> 128 MB |
| - copies in the card's memory (55-64 MB sent up in 27-45k ranges) | 6.7 | 7.7 | 9.4 | 0.5 / 0.4 | ranges: 2.1 a draw |
| - write-watch asking (250-265 kernel askings, 380-420 watches, 10 GB checked) | 3.4 | 3.9 | 4.5 | 0.2 / 0.15 | nearly flat |
| - copies and fills | 2.5 | 3.2 | 4.0 | 0.25 / 0.2 | title copies: 2.3 a draw |
| - title bytes under flights' writes (28-30k changed runs) | 1.9 | 2.3 | 2.5 | 0.1 | runs: 2 a draw |
| - snapshots / sending / planning card steps / heaps' records + wait | 0.3/1.2/0.5/1.8 | 0.5/1.0/1.0/1.4 | 0.4/1.0/0.9/1.6 | ~0 | flat |
| Submit thread: waiting for the card (1 read wait a frame, "title memory read") | 8.7 | 18.3 | 21.3 | - | card |
| Copy-back thread: landing (52-63 MB, 1.5-1.9k pieces; merge 9.5-13 wall, 34-48 on 4 threads) | 10.8 | 16.3 | 20.7 | 1.6 / 1.7 | MB fetched; it waits 42-45 for the card |
| Recording, all title threads (52-59k list calls, 2.8 us each) | 112 | 148 | 164 | 5 / 9 thread-us | 6.2 calls a draw |
| Heap sync (aside; 0.54-0.63 M slots looked at; 0.8 waited for) | 8.3 | 9.2 | 11.0 | 0.6 / 0.35 | flat scan |
| Heap pass (aside): slots / spans / textures | 5.5/1.9/11.2 | 4.5/2.6/8.4 | 4.6/2.5/12.4 | 0 | flat |
| Process CPU, all threads (user + kernel) | 316 | 458 | 530 | 27 / 41 | recording mostly |
Flat with draws [P]: card hash passes 311 -> 319-345, compare passes 400, pieces compared 17-20k, guard faults
380-390, guards lock waits 23 ms (all threads), submissions. Counts per draw [P]: title copies 2.3, refreshes asked
2.7, dynamic tables 1.95, raw twins of buffer views 1.76 (17-19k made a frame), ranges sent up 2.1.
Rank at 7,000 draws (each on its own thread): card lists ~59 (10 per-draw); recording ~150 thread-ms; copy-back
landing 17; submit refreshes 13; card-memory copies 8; watch 4; copies and fills 3.3; bytes under flights 2.3.

## 3. What the card's 60 ms are: NOT measured in this run
`VICTORY_CARD_METER=1` gives fox only whole-submission stamps (`host/spans.rs:69`); kinds and pipelines read 0.
From earlier runs (card-time-analysis.md, heavy-passes.md), [I] for this scene:
- 20-27 passes of >= 200 us (shadow masks `5cf00dc3` 1.3-9.4 ms by light count, ray requests `cb6adcc6` 1.7-6.4,
  probe chain, fog) plus a tail of small bindings at 11-15 us; more lights and particles grow exactly these.
  Mesh group requests here: 24-38k a frame against 2-25k on the route [P].
- Our own lists are in the same stamps: 319-345 hash + 400 compare passes over 17.4k pieces of 3,408 card copies
  (1,248 MB) every frame [P]; the old tree metered such lists at 10-13 ms; fox has no meter for them.
- Placement [P]: "card 17,135 MB in use of a budget of 14,735 MB, 7,411 MB of the PC's memory shared with it";
  free space inside texture heaps 3.9-4.2 GB (71 of 184 blocks under half full); 640 buffers refused a card copy.
- 755-830 global waits a frame for the title's barrier groups, 180-211 bounded indirect commands [P] (nearly flat).

## 4. Top costs: where, why, what to change
| # | Cost | Where (victory-gpu/src) | Why it grows | Change | Est. at 7,000 draws | Existing switch |
|---|---|---|---|---|---|---|
| 1 | Card lists 59-63 ms | title shaders (victory-shader `emit/`), `host.rs:4416` `card_before` (our hash/compare lists), `pool.rs`/`resource.rs` (placement) | scene content; ours flat | first split it (run A below); then: hoisted/deduped loads; compare only pieces a submission declared written; pack heaps so raw use fits the budget | unknown until split; hoist 0.5-2 [I]; ours up to 10 [I]; placement 5-20 [I] | `VICTORY_HOIST_LOADS=1` (`victory-shader/src/emit/mod.rs:235`, new cache key), `VICTORY_HEAP_PACK=1`, `VICTORY_GPU_TIMING=1` (meter, `host.rs:1819`) |
| 2 | Refreshes 12.6-16 ms (submit) | `host.rs:8480` `Batch::flush` / `flush_all`, `host.rs:8216` `update`, `host.rs:8292` `refresh_front` | one refresh step per buffer a draw reads; cost is the 104-128 MB the title really dirties (10.1 GB checked, 10.0 unchanged) | drop refreshes of retired arena chunks before asking; the arena `0x1dc50000+0xae00000` alone sends 23 MB in 644 pieces | 1-3 submit ms | `VICTORY_CHUNK_FINAL=1` (`chunks.rs:106`); `REFRESH_ONCE` is on and skips only 350-580 of ~29,000 |
| 3 | Card-memory copies 7.7-9.4 ms (submit) | `host.rs:5830-5852`, `shadow.rs:4739` `send_up`, `host/send.rs` | 27-45k ranges a frame, 2.1 a draw (3.4-3.9 ms just taking them out of the CPU sides) | join ranges per copy into runs before the send; do the gather on the wide workers | 2-4 submit ms | none (`SEND_OWN` on by default) |
| 4 | Write-watch + guards: 3.9-4.5 ms asking, 4.1-4.5 changing protections, 388 guard faults, 23 ms lock waits (all threads), 7 ms in recording | `watch.rs:585` `ask_once`, `watch.rs:1402` `rest` | blocks made read-only, written, opened again several times a frame | let a taken block rest 3 ms | 1-2 submit ms, 3-5 recording thread-ms | `VICTORY_GUARD_REST=3000` |
| 5 | Bytes under flights' writes 2.3-2.5 ms | `shadow.rs:3753` `merge_under_writes`, `shadow.rs:2194` | 28-30k runs of a few bytes, each its own copy | copy while comparing, 32-byte blocks | 1-1.5 submit ms | `VICTORY_UNDER_FUSED=1` |
| 6 | Recording 148-164 thread-ms | `draw.rs:1656` `State::bind`, `draw.rs:1616/1634` `same_root`/`same_words`, `descriptor/host.rs:1445` raw twins | root constants, root views, tables given again for every draw | leave out unchanged root arguments; raw twin only for views a raw-index shader can pick | 15-30 thread-ms, <= 1 ms card | `VICTORY_BIND_FILTER=1` (`draw.rs:416`); fox's `VICTORY_THIN=1` (`list/thin.rs:58`: calls only written down) is the larger lever, untested here |
| 7 | 5-s and 30-s hitches (longest frame 75-176 ms in every 5-s window) | `host.rs:9028` `meter_lines`, `copystat.rs:536` | ~40 meter lines on the presenting thread; the memory-map walk takes 0.8-1.0 s over 290k regions and holds the map | lines on a thread; no walk | ~30 ms once per 5 s; 11-17 ms stalls per 30 s | `VICTORY_METER_THREAD=1`, `VICTORY_MEMORY_MAP=0` |
`VICTORY_PREPARE_WIDE=1` (`host.rs:7860`): 94 % of refreshes are wide already: <= 0.5 ms ("on several threads" up).
## 5. Heavy-scene test: what to switch on, and what each counter must show
- Run A (timing only, slower): `VICTORY_GPU_TIMING=1` -> copystat "the card's time by what ran" per pipeline in THIS
  scene. Decides item 1. Nothing else changed.
- Run B (CPU switches, default off): `VICTORY_CHUNK_FINAL=1` ("chunk final ... refreshes found final and dropped N
  (M MB)"; watches 380-400 and "10 GB checked" fall), `VICTORY_UNDER_FUSED=1` ("pages fused" > 0; under-flights
  2.3 -> ~1 ms), `VICTORY_GUARD_REST=3000` (guard faults 388 -> < 200; "changing protections" 4.1-4.5 ms down),
  `VICTORY_BIND_FILTER=1` ("bind filter: root arguments given N, left out M"; under 2.8 us a list call),
  `VICTORY_METER_THREAD=1` + `VICTORY_MEMORY_MAP=0` (no "memory map" line; longest frame per 5 s near the median).
  Expected gains are estimates: preparing 36 -> ~30 ms, frame unchanged (card-bound): judge by the submit columns.
- Run C (cold shader cache): `VICTORY_HOIST_LOADS=1` -> "all other lists" at equal draws. (`HEAP_PACK=1`: run-107 +370 MB only.)

## 6. Scene start (t = 100-150 s)
[P] No pipeline work (0 made, <= 1.2 ms waited). Long frames: 31 at 100-120 s (mean 258 ms), 15 at 120-130 s (mean
345 ms). In them remapping of title memory takes 216-312 ms a frame with 24-52k page faults and 0.7-1.7 s of
processor time; the card idles 161-201 ms, 128-151 of it "while the title recorded" (title threads blocked in
mapping); preparing is only 39-91 ms. Uploads by cause per 5 s: 2.7-3.3 GB in 7,400-9,800 uploads at 95-110 s (bake
1.2-1.3 GB, first 0.3-1.5 GB); at 125-135 s 1.9-3.1 GB with no bake: first 0.7-1.2 GB, partial growth 1.0-1.3 GB,
made again after let go 83-251 MB, while the room rule lets go 2.3 GB per 5 s. First usual frames of the scene
(5,049-5,705 draws): 79-86 ms with remapping 34-70 ms, 142-173 uploads, card idle 53-74 ms. The burst is mapping
plus first uploads under memory pressure, not shaders.

## 7. Focus
[P] Unfocused = two stretches (213-215 s, 234-244 s; 8 samples). Frame-log windows in the second (6,835-6,858
draws): 66 ms, card lists 58.1-58.3, preparing 35.0-35.5, present call 0.2-0.3, paced 0.0, render thread 2 ms.
Focused neighbours (6,763-7,690 draws): 67-69 ms, card lists 60.1-60.5, preparing 34.9-35.6, present 0.1-0.3, paced
0.0, render thread 2-3 ms. Present, pacing and render thread do not differ; the 1-3 ms are in the card column. The
captions' 71.9 vs 65.6 ms is mostly draw count (the focused set holds the 8-10k-draw stretch) and a still camera
while unfocused (2 uploads, 6-8 remaps a frame against 5-54 and 20-108). Two windows: too small to call.

## 8. Ranked: what to optimise for demanding scenes
| # | What | Expected at 7,000 draws | Size | Risk | Where |
|---|---|---|---|---|---|
| 1 | Split the card's 60 ms in this scene (GPU_TIMING run; a kinds meter for our hash/compare lists in `host/spans.rs`) | decides 2-4 | S | none | `host.rs:1819`, `host/spans.rs:69` |
| 2 | Our compare/hash passes: only pieces declared written by the submission; hash runs joined | up to 10 card ms [I] | M | stale shader output read by the title if a written piece is missed | `host.rs:4416`, `host.rs:2222` |
| 3 | Card memory under budget (17.1 of 14.7 GB): pack sparse blocks, then card copies for the 640 refused buffers | 5-20 card ms [I] | L | device loss on over-commit; late textures | `pool.rs`, `shadow.rs:225/375` |
| 4 | Translator: hoisted loads (built, off), then mesh output counts (heavy-passes.md plan 1-2) | 1.5-6 card ms [I] | S (switch) / M | stale light data in a tile; missing geometry | `victory-shader/src/emit/mod.rs:235` |
| 5 | Submit thread: CHUNK_FINAL + ranges joined before the send + UNDER_FUSED + GUARD_REST | 5-9 of the 36 ms preparing | S (switches) / M (send) | missed title write = stale constants or vertices | `host.rs:8480`, `shadow.rs:4739`, `shadow.rs:3753`, `watch.rs:1402` |
| 6 | Recording: BIND_FILTER, later THIN | 15-30 thread-ms; frame only once 1-5 are done | S / L | wrong constants on single draws | `draw.rs:416`, `list/thin.rs:58` |
| 7 | Meter hitches: METER_THREAD=1, MEMORY_MAP=0 | 30 ms spikes each 5 s, 0.9 s walk each 30 s | S | none | `host.rs:9028`, `copystat.rs:536` |

Picture, not speed [P]: "dropped 461-518 draws a frame without a host pipeline (0 being made) and 607-631 indirect"
here against 44-164 and 235-321 on the route. Inferred throughout: the split of card time and every expected gain.
