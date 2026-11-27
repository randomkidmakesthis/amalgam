# Frame budget: where a 110–160 ms frame goes (2026-11-26)

Runs: 19-room, 21-forkports-default (same build as 20, five switches off), 22-profile (profile + census + per-pipeline
card meter), 20-forkports. All numbers are per frame, means over frame-log windows weighted by frames. "Heavy" = windows
over 3,000 draws, "light" = 300–1,000 draws. Tags: **[P]** read from a log line or profile, **[I]** inferred.
Run-22 is ~14 % slower than run-21 in heavy windows (160 vs 141 ms; kernel 28 vs 10 ms; game lists 114 vs 98 ms): the
profiler and 4,138 timestamp pairs a frame cost that. Budgets below use run-21/19; run-22 gives shares only.

## 1. What bounds the frame
- The title's render thread runs 4–8 ms and then stands in the present call (122 of 132 ms) for the submit thread [P].
  Recording threads are 24 % on a processor, 74 % in the title's own waits [P profile]. The game is not the limit.
- Frame = the submit thread's drain: at work 139 of 141 ms; 74 ms of that it stands waiting for the card [P].
- Card: busy 121 of 141 ms (86 %), idle 20 ms of which 17 "behind us"; nvidia-smi 80–99 % in heavy stretches [P].
- Fit over 131 windows: `frame = 34 + 1.08 x game-list GPU ms` (R² 0.83); `waiting = 0.80 x game-list ms − 4` (R² 0.90).
- **Heavy: the card is the limit** (floor ~121 ms if the CPU side cost nothing); the submit thread's own 65 ms is the
  second floor. **Light: the submit thread is the limit** (card 65 % busy, 15 ms idle behind us).

## 2. Budget, heavy scene (run-21: 18 windows, 568 frames, 3,662 draws, 141.0 ms; run-19 gives 140.5)
| Submit-thread ledger (critical path) | ms | who | basis |
|---|---|---|---|
| Waiting for the card (`relay_wait`) | 74.3 | see card ledger | [P] frame log |
| Copy-up of title memory + write-watch askings (`Batch::flush` > `shadow::wide`, `watch::writes`) | 19 | layer | [P] profile share x3.0 |
| Stale-copy refresh, small copies (`Shadow::update`, `copy_mapped`, ReadProcessMemory) | 12 | layer | same |
| Descriptor heap mirror (`sync_heaps`, `mirror_copied_page`) | 9 | layer | same |
| Sweep / `let_go_idle` 6; guard-table lock (`watch::guards`, nine tenths lock wait) 6 | 12 | layer | same |
| Texture refresh on this thread | 4.5 | layer | same |
| Merge under shader writes, ask-ahead, snapshots | 8.5 | layer | same |
| Not at work | 1.8 | – | [P] |
| **Total** | **141.0** | | unexplained inside the 65 ms CPU part: ±15 ms (profile is a whole-run mean, scaled) |

| Card ledger (same 141.0 ms) | ms | who | basis |
|---|---|---|---|
| Title's lists: ~3,300 small bindings at 18–28 us each | ~74 | layer [I] / game | run-22 pipeline table, 76 % of list time |
| Title's lists: ~13 big full-screen passes (5cf00dc3 6.7, cb6adcc6 5.2, 14264103 2.9, 2f03a2df 1.5, ...) | ~24 | game | run-22 pipeline table, 24 % |
| Our compare lists (26 a frame) 10.2; our send-and-hash lists (13 a frame) 4.1 | 14.3 | layer | [P] |
| Idle behind the submit thread | 17.2 | layer | [P] |
| Idle, title had not submitted | 3.1 | game | [P] |
| **Unexplained busy** (present/upscale copy, texture uploads, gaps between lists) | **8.4** | ? | 141 − sum |

Beside the path (overlaps the card's work; a 40 ms floor of its own): copy-back thread 39.5 ms for 75 MB (`12 + 0.37 ms/MB`);
census: 110.6 MB fetched, 74.5 MB of it from write-combining memory [P]. Recording, all threads: 189 ms summed, mostly
waits; whole-run mean on a processor: ours ~19 ms (`State::bind` 11.3, `execute_indirect` 7.2), the title's own 13 [P].

## 3. Budget, light scene (run-19: 4 windows, 195 frames, 565 draws, 54.4 ms)
Submit thread: preparing 30.7 (copy-up 12.3 ms / 57 MB, textures, heaps; layer), waiting for the card 11.4, other work 7.5
(landing, sweep; layer), not at work 4.8. Card: title's lists 28.5 in 147, compare + send-and-hash 4.2 (layer), idle 18.8
(15.0 behind us), unexplained 2.9.

## 4. Per-draw against fixed cost (131 windows over 300 draws, runs 19/21/22)
| Quantity | Intercept ms | ms per 1,000 draws | R² |
|---|---|---|---|
| Frame | 50.0 | 25.3 | 0.73 |
| Preparing (CPU) | 33.4 | 4.4 | 0.25 |
| Waiting for the card | 7.9 | 18.6 | 0.78 |
| Title's lists on the card | 18.6 | 21.8 | 0.76 |

The CPU side is mostly a fixed 33 ms (even an empty scene misses 33 ms); the per-draw cost is on the card: 22 us a
draw. Run-22 tables: 48.8/68.2/86.9/109.4 ms for 1,781/2,813/3,458/4,138 bindings = 24–27 us a binding, flat.

## 5. Is the card time real shading?
- [P] Only ~24 % of list time is in big passes; ~76 % is thousands of bindings at a near-constant ~25 us, whatever
  the pipeline. Constant cost per binding is the signature of overhead, not of pixels.
- [P] Per frame the layer records 2,719 indirect commands "bounded (a dispatch of ours between two waits for every
  earlier shader write)" plus 852 such waits for the title's barrier groups: ~6,300 full waits and 2,719
  graphics-to-compute switches. `indirect.rs:1035` cites another machine: "~73 % of its time in them".
- [I] Run-20 (gather on: 1,486 bounded instead of 2,719): list time outside one broken list (section 7) was 28 ms in
  its light windows against 45–60 in run-19's, and 57–71 against 102–113 heavy: the clamps look like 30–50 ms.
- Not measured: loops at the 2^20 cap, garbage from the absent RT builder. Candidate: compute 5cf00dc3, 8.8 ms a binding.
- Names: no hash→name table exists (`victory-data\reports\translate-list.txt` holds 828 problem lines only). The
  translate tool with `--list` prints one line per program with `#hash`; `VICTORY_GPU_TIMING=1` names commands.

## 6. Hitches (run-21, frames in scenes over 300 draws)
144 long frames of ~1,136 (13 %) take 45 s of ~165 s. 17 are over 500 ms (mean 1,033 ms, worst 2,698): **remapping**,
839 ms in ~2,000 mapping calls in one frame (`title+0xf2c470` > `copy_page_mappings` > Map/UnmapViewOfFile), card idle
913 ms. 33 of 250–500 ms: remapping 156 + copy-back 73. 94 under 250 ms: remapping 78, texture refresh 41 (347 uploads).
No pipeline compiles (0 made, 0 ms waited). The once-a-second sweep is not what makes them [P].

## 7. Run-20 puzzle
| First 47 windows, >300 draws | run-19 | run-21 (switches off) | run-20 (five on) |
|---|---|---|---|
| Frame / draws | 112.5 / 2,368 | 121.0 / 2,800 | 133.4 / 2,791 |
| Heavy: frame / draws | 140.5 / 3,511 | 141.0 / 3,662 | 164.2 / 3,762 |
| Heavy: title lists / preparing / waiting | 93 / 53 / 70 | 98 / 50 / 74 | 115 / 65 / 77 |
| MB sent up / card copies | 136 / 98 | 140 / 98 (945 MB) | 50 / 3,333 (1,024 MB) |
- Always-on parts of the port: neutral. Run-21 equals run-19 in heavy windows at 150 more draws (worth +4 ms by the fit).
- The slowdown is one compute list kind (3cf39f44 + 4518151a/48426bae, 9–12 pipelines, ~0.9 a frame): 0.8–1.1 ms a
  frame in runs 19/21/22, **42–72 ms in run-20** [P card meter]. 3cf39f44 "stores through a view it picks by index"
  into the 24 MB buffer at 0x1010120000 [P].
- Cause: **`VICTORY_CARD_BUFFERS_MIN_KB=64`** [P+I]. 3,333 small buffers (down to 4 bytes, 64 KiB each) fill the
  1,024 MB `VICTORY_CARD_BUFFERS_MB` budget exactly; the 24 MB buffer has a card copy in run-21 and none in run-20; the
  CPU then looks for shader writes in 108.8 MB a frame (105.2 MB in copies ≥1 MiB) against 6.0 MB, compare passes 329
  against 73, recording 251 against 142 ms. Why that costs the card 50–90 ms in that list is not shown by the logs.
- The other four: `ALIAS_ONCE` 0.0 copies made; `REFRESH_COMPARE` 0.1–8 MB; `GUARD_HOLES_FRAME` CPU only;
  `INDIRECT_GATHER` only removes dispatches of ours (and looks like a gain, section 5).

## 8. Ranked causes (heavy frame)
| # | Cause | ms | Evidence | Who | Remedy (size) | Expected gain |
|---|---|---|---|---|---|---|
| 1 | Clamp dispatch + 2 full waits per indirect command, waits per barrier group | 30–50 card [I] | copystat "2719 bounded, 852 groups"; flat 25 us/binding (run-22); run-20 remainder | layer | `VICTORY_INDIRECT_GATHER=1` alone; `VICTORY_CLAMP_WAITS=0`; then one clamp pass per list (M) | 25–40 ms |
| 2 | Big full-screen passes at 2560x1440 through translated shaders | ~24 card | run-22 pipeline table | game | render-size globals 0x1499fecf8 (L); `VICTORY_SHADER_LOOP_LIMIT` A/B (S) | 10–15 ms |
| 3 | Copy-up + write-watch on one thread (140 MB sent, 10.3 GB asked, 99 % unchanged) | 19 CPU + part of 17 idle | profile `Batch::flush`, `watch::writes`; copystat | layer | one asking per flush / batch (plan R2), finer tracking of the 182 MB heap (M) | 10–15 ms |
| 4 | Our compare + send-and-hash lists | 14 card | frame log | layer | compare only declared ranges; hash on change (M) | 5–10 ms |
| 5 | Copy-back 75–110 MB, from write-combining memory | 40 (parallel floor) | frame log, census | layer | fetch only differing bytes; never read WC memory back (M) | removes a 40 ms floor |
| 6 | Stale refresh, descriptor mirror, sweep, guard lock | 12 + 9 + 6 + 6 CPU | profile | layer | plan R3–R6; guards as RwLock (S–M) | 15–20 ms CPU |
| 7 | Remapping storms | hitches: 15 % of wall time | long-frame lines | layer + game | mapping batches off the frame path, coalesce alias runs (M–L) | removes 0.5–2.7 s stalls |

Layer against game [I]: of 141 ms the game's own is ~24 ms of big passes, 3 ms idle and at most 10–15 ms of the small
draws; ~100 ms is the layer. 33 ms needs the card under ~30 ms and the submit thread's fixed 33 ms halved.

## 9. Proven, inferred, and the three runs that settle it
Proven: sections 1, 4, 6, rows tagged [P], the run-20 list and budget numbers. Inferred: the clamps' share of card time,
the game/layer split of small bindings, the profile scaling (x3.0). Open: VRAM is 14.5–15.4 GB of 16 (6.2 GB shared in use).
1. `VICTORY_INDIRECT_GATHER=1` alone, then `VICTORY_BARRIER_TEST=both` (timing only, picture may break): the fall in
   "all other lists" is the clamps' and waits' cost.
2. `--list` names for the top hashes + `VICTORY_SHADER_LOOP_LIMIT=64` A/B (+ `VICTORY_SKIP_PIPELINES=5cf00dc39042edaf,
   cb6adcc6da2613ff,14264103296aa43a`): are the big passes real work, capped loops, or RT garbage.
3. `VICTORY_CARD_BUFFERS_MIN_KB=64` with `VICTORY_CARD_BUFFERS_MB=2048`: confirms the budget explanation of run-20 and
   shows whether card copies for small buffers (50 MB sent up instead of 140) are a gain.
