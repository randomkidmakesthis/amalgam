# Neptune gap audit (2026-11-26)

Neptune = `ref-forks\neptune-vi` at `origin/main` 7af0fa5 (exported to scratch, local clone untouched); ours =
`victory-dev` (local/scarlett, uncommitted tree). Method: (a) every `"VICTORY_*"` literal in `crates/` + `apps/`
(Neptune 265, ours 319, common 194; first read line of each common switch compared); (b) `diff -rq` of all crates
except victory-shader, each differing file read for Neptune-only code (four parallel readers + own reading of
draw.rs/indirect.rs); (c) Neptune README / NEPTUNE-README.txt / launchers (`tools\play.ps1`, `play-zm.ps1`,
`start-neptune.ps1`). Budget rows refer to `frame-budget.md` section 2 / 8.

**Headline.** 71 switches only in Neptune (≈30 perf-relevant, rest features/diagnostics/Series S); 21 common switches
whose read line differs (8 real default differences); 13 unconditional Neptune-only changes. Neptune's own number:
open world 6,500 draws in 90 ms with **the card 35 % busy** (CPU-bound); ours is card-bound (121 of 141 ms). So the
biggest gaps are the ones that take work off the card:
1. **`VICTORY_CARD_WINDOWS_MB` default 4096 on a ≥14 GB card (ours 0)** — Neptune: "draws stop reading their vertices
   over the bus". Title windows are 3.5 of 5.4 GB of shared memory; every small binding that reads vertices/indices/
   constants from the title's allocator reads them over PCIe. Attacks the 74 ms of ~3,300 small bindings at a flat
   25 us (card row 1). Unmeasured on ours; VRAM risk (we sit at 14.5–15.4 of 16 GB; guarded by the "a fifth free" rule).
2. **Indirect clamps: `VICTORY_INDIRECT_RAW=3` + `VICTORY_CLAMP_AHEAD`** — "3,200 records bounded in place, each a
   dispatch of ours between two waits ... the card was busy all frame at a third of its power". Card row 1 (30–50 ms).
3. **Submit-thread locks/askings: lock-free journal page table + guards RwLock + `ASK_QUIET`** — "a third, then two
   fifths of submit time"; "3,000 askings over 11 GB, a third of submit time". CPU rows copy-up/write-watch 19 ms, guard lock 6.

## A. Switches only in Neptune (performance-relevant)
| Switch (code default) | What | Author's number | Our row | Ours equivalent |
|---|---|---|---|---|
| CLAMP_AHEAD (on; inert while INDIRECT_WINDOW on) | single records clamped in list prelude | 3,200–5,000 clamps/frame | card: clamps 30–50 | INDIRECT_GATHER (needs windows) |
| INDIRECT_RAW (off; 1 mesh raw, 2 all raw, 3 CPU-checked draw/indexed) | skip clamp | "third of its power"; 2 hung zm0kie's GPU | card: clamps | none |
| UNDER_WRITES_WORDS / _RUNS (512) / _MASKS / _MASKS_LEAST (4) | small changes under shader writes sent without a settle | 26–33k runs/frame | waiting 74 / settles | MERGE_RUNS, MERGE_MASK (partly) |
| ASK_QUIET (0) / _GAP_MB (32), ASK_WIDE_MB (0) | quiet 1 MiB blocks asked once a frame | 1/3 of submit; 36 ms | copy-up+watch 19 | none (ASK_ONCE only) |
| COPY_CHANGED (0) | only differing 256 B spans go up | 1,500 pieces/frame | copy-up 19 / card | REFRESH_COMPARE (equivalent) |
| QUIET_BACK (0), _TOGETHER (1), _WATCHED (0) | copy-back writes not seen as title writes; resets batched | 1/8 submit; 650 resets x 50 us | copy-back 40, watch | BACK_OWN_VIEW (partly) |
| LOOKUP_CACHE (on) | per-thread lookup cache (8,192) | 1/10 of recording threads | recording (not critical) | none |
| RELAY_OWN_QUEUE (0), RELAY_KEPT_PIECES (2048) | copy-back fetches on own queue | – | copy-back 40 | QUEUE2 (other purpose) |
| WIDE_THREADS (pool size) + unconditional pool split | own pool for copy-back merges | submit waited 1/8 | merge 8.5 | one pool |
| LIST_POOL_MOST / _MB (128 / 256 MiB) | idle lists kept | 1/10 of submit | small | ours fixed 512 / 256 MiB (better) |
| TEXTURE_HASH_EVERY (8) | texture hash interval | 5 ms first room | textures 4.5 | const 8 (same default) |
| CARD_WINDOW_UPLOAD (0) (+windowup.rs) | windows on card without CPU twin | – | memory | none |
| GUARD_HOT (0), GUARD_OPEN_AHEAD (1) | leave hot blocks unguarded / open neighbours | 1/10 submit; 1/10 rec. | guard lock 6 | none |
| WATCH_4K (0), DS_NOTIFY (0), WINDOW_REBAR (0) | 4 KiB tracking, DS journal, ReBAR windows | – | copy-up | none |
| CARD_COPIES (on) | title buffer copy on card | 60 ms settle | waiting | GPU_BUFFER_COPY (on) |
| FAST_COPY (on), TITLE_COPY_PLAIN (on) | own-process copies, title-to-title too | 45–48 ms submit | stale/small copies 12 | COPY_PLAIN (in our run; no title-to-title, 256 B floor) |
| PATCH_WAITS (on), PATCH_CLZERO (on) — loader | MONITORX/MWAITX patched to NOP/PAUSE; clzero loop to a jump | 1/5 main thread; 2,600 traps/frame | game threads (not critical) | none: traps every time |
| CONSTANTS_WAIT (0 = no wait) | constants deferred | – | waiting | fill-on-card (on) |
| STAGE_POOL_MB (256), SLOT_EVICT, VRAM_EVICT, PLACED_LOOSE_KEY | pools / VRAM | 123k buffers | textures 4.5 | own pools |

Not relevant / decided against: AMBIENT*, DISCORD*, OVERLAY, FSR_DLL, RENDER_S, NO_COPY_BACK, CAPTURE_*, DUMP_AFTER,
FRAMETIME_DUMP, DRAW_CENSUS, NULL_MAGENTA, HEAP_COUNT, MOUSE, OCEAN_READBACK/TEXTURE_BACK* (correctness), ACTIVATE_RAYS/
RT_RESOLVE (compile time), DROP_DXIL (memory), DESC_DATA/WAVE_OWN_LDS_GROUPS (shader translator: cache), VRAM_IDLE_BY_ROOM
(ours has own room rule), SIGNAL_NOW, PACE (same as ours), TEXTURE_GATE_S, BUILD, TEST_*.

## B. Unconditional Neptune-only changes
| Change (Neptune fn) | Number | Our row | Ours |
|---|---|---|---|
| `window_card_most` default 4096 MB on ≥14 GB | windows 3.5 of 5.4 GB shared | card: small bindings | default 0 |
| watch.rs journal: page→asking table of atomics, `JOURNAL` RwLock, `written_set` lock-free | 1/3 → 2/5 of submit | watch 19 | Mutex + BTreeMap (ours asks outside lock) |
| watch.rs guards: RwLock + lock-free `CLEAN`/`ABSENT` | 1/6 and 1/8 of submit | guard lock 6 | Mutex each time |
| shadow `workers`/`wide_for_back`: two pools | submit waited 1/8 | merge 8.5 | one pool |
| shadow `card_changed_runs`: one `dirty` lock per page | hundreds of runs/page | copy-up | per run |
| host `RETIRED` sharded 16 ways, sorted outside lock | 1/3 of lock waits | sweep 6 | one Mutex |
| host coarse clock `moment`/`tick` | 7 % of recording | recording | per call |
| descriptor/host `made_too` lock-free slot check; `kept` per-slot locks | 2/5 of recording | mirror 9 | MIRROR_FAST (in our run); kept one lock |
| shadow `LOOSE` RwLock, shared `BUFFERS` read | 1/10 recording | recording | MIRROR_FAST only |
| texture `staged()` counts on-demand textures as whole when held levels mapped | ~200 re-uploads/s | textures 4.5 | not whole |
| loader `SEEN_ALL` skip of the emulation lock | – | game threads | lock each trap |
| draw `early_budgets` 40 / 10,000 ms (ours 12 / 1,500) | 9,371 → 504 textures due | loading | old values |
| `targets_room_kept` 1,536 MB for Series S | – | not relevant (Series X) | – |

Where ours is ahead: ask outside the journal lock (Neptune holds `JOURNAL.write()` during GetWriteWatch), CARD_QUEUE
(Neptune walks every card copy), detiling (`detile_wide`), list pool 512, heap marks, UNMAP_EXACT, MAPPING_RANGES.

## C. Free settings to test (exist in both, different value)
| Switch | Ours (code / standard run) | Neptune launcher | Note |
|---|---|---|---|
| CARD_WINDOWS_MB | 0 / unset | 4096 (code default on 16 GB) | #1 above; watch VRAM line |
| LATE_WAIT_LISTS | 256 / unset | 1024 | ~230 lists a frame: settles when 256 in the air |
| CARD_BUFFERS_MB with CARD_BUFFERS_MIN_KB=64 | 1024 / unset | 1536 (3072 in start-neptune) | run-20 harm was the 1,024 budget filling; never MIN_KB without MB ≥1536 |
| WINDOW_TAILS | 0 / unset | 1 | |
| WINDOW_KB | default / unset | 1024 | |
| TEXTURE_STAGE_MB | 192 / unset | 512 | texture refresh |
| D3D12_BACKGROUND | on / unset | 0 | driver re-compiles in background (run 54: 1,100 → 900 ms) |
| ASYNC_PRESENT | off / unset (ASYNC_SUBMIT=1 set) | code default on under ASYNC_SUBMIT | present a third of the frame there |
| PIPELINE_WAIT_MS | 500 / 20 | 2500 | picture vs hitches; keep 20 for timing |
| EARLY/BAKE_TEXTURES_MS | 12 / 1,500 | 40 / 10,000 | loading only |
| VRAM_IDLE_MS | default | 15000,2500,1000 | black-then-back |
| INDIRECT_WINDOW | 1 in our run | unset = off in Neptune | Neptune's CLAMP_AHEAD only acts with windows off |

## D. Port order (by our budget)
1. INDIRECT_RAW 1/2/3 + CLAMP_AHEAD (card). 2. CARD_WINDOWS_MB Neptune default (card). 3. UNDER_WRITES_* (settles).
4. ASK_QUIET (off), QUIET_BACK, LOOKUP_CACHE, WIDE_THREADS/pool split, RELAY_OWN_QUEUE, LIST_POOL, TEXTURE_HASH_EVERY.
5. Loader PATCH_WAITS / PATCH_CLZERO (cheap, self-contained). COPY_CHANGED: ours has REFRESH_COMPARE (skip).

## E. Port results (2026-11-26, uncommitted)
| Item | State | Default | Log to grep |
|---|---|---|---|
| INDIRECT_RAW=1/2/3 (draw.rs, `record_sane`, `host::shader_written`) | ported | off | `(Neptune) indirect calls per frame:` |
| CLAMP_AHEAD (on our gather prelude, `indirect::gather_ahead`) | ported | on, acts only with windows off | same line, "clamped ahead in the prelude" |
| CARD_WINDOWS_MB 4096 on ≥14 GB | ported | Neptune's rule | `(Neptune) windows on the card: at most` |
| UNDER_WRITES_WORDS, `card_changed_runs` (unconditional), _RUNS / _MASKS_LEAST as fallbacks of MERGE_RUNS / MERGE_MASK_RUNS | ported | WORDS off | existing merge line |
| ASK_QUIET (+_GAP_MB) | ported | off (zm0kie: darkening) | `(Neptune) title memory asked about per frame` |
| WIDE_THREADS + copy-back pool | ported | ours (cores/4, 2–6) | `(Neptune) kept threads:` |
| Guards RwLock + shared "all clean" answer | ported (partial of Neptune's lock-free tables) | unconditional | `(Neptune) view memory askings answered clean` |
| On-demand `staged()` whole + `staged_memory` held bytes | ported | unconditional | `heap textures` partly counts fall |
| LIST_POOL_MOST/_MB, TEXTURE_HASH_EVERY | ported | 512 / 256 MiB, 8 | – |
| PATCH_WAITS, PATCH_CLZERO, SEEN_ALL (loader) | ported | on | `replaced where it stands` |
| COPY_CHANGED / QUIET_BACK | skipped: REFRESH_COMPARE / BACK_OWN_VIEW equivalent | | |
| LOOKUP_CACHE, journal atomics, RETIRED shards, coarse clock | skipped: recording threads (not our limit) or too entangled | | |
| RELAY_OWN_QUEUE | skipped: needs CARD_COMPARE snap slots, clashes with our inflight fetch / QUEUE2 | | |
