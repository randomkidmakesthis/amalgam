# Memory-sync optimisation plan (victory NVIDIA fork)

Source: the layer's sampling profile, census and frame log from run `nv8-3-census` on this PC, read against the code.
Paths are under `victory-nvidia-8gb\crates\victory-gpu\src\`. Render resolution stays 2560x1440.

## What bounds the frame

- Heavy view: card busy ~96 of 114 ms; ~14 ms idle "behind us". Submit thread at work 110 of 114 ms — the critical path.
- Lighter view: card idle 70–100 ms, so every submit-side millisecond counts in full.
- The game's render thread's 83 ms in `queue::lock` is a wait for the submit thread.
- Caveat: two driver background threads were still compiling (138 ms CPU/frame); GPU time may fall further once settled.

## Findings

| Cost | Per frame | Cause |
|---|---|---|
| "Settle" wait: drain every list in flight | 29–43 ms waiting | a 64 KiB page rewritten in more than 512 runs in one submission (`VICTORY_MERGE_RUNS` cap), ~1.2 times a frame |
| Write-watch lock (`JOURNAL`) | 11.5 ms waiting | ~2,700 separate askings a frame; 99.2 % of bytes asked about are unchanged |
| Descriptor `mirror` lock (`HostHeap.made`) | 15 ms submit; 28.7 ms summed on recording threads | lock taken per record even when the slot is already made |
| Buffer table lock (`BUFFERS`) | 15.3 ms summed on recording threads | plain mutex around a read-mostly lookup |
| Small copies via `ReadProcessMemory` | 6.8 ms | 9,166 copies under 256 bytes a frame |
| Texture-table sweep | ~30 ms hitch once a second | whole table walked in one go |
| The 182 MB heap at 0xb10000 | 22.5 MB re-sent in ~295 blocks; hashed whole twice | game's main GPU data heap, tracked only per 64 KiB block |

## Ranked changes

| # | Change | Switch | Status |
|---|---|---|---|
| R1 | No settle on the merge-run cap: first A/B `VICTORY_MERGE_RUNS=65536` + `VICTORY_CARD_REHASH_RUNS=1`; if the send list gets too long, a masked merge dispatch per page | existing / `VICTORY_MERGE_MASK` | to measure |
| R2 | One write-watch asking per flush: first A/B `VICTORY_REFRESH_ONCE=1`; then `watch::writes_many` | existing / `VICTORY_ASK_BATCH` | to measure |
| R3 | `mirror` without the lock for made slots; `BUFFERS` as RwLock | `VICTORY_MIRROR_FAST` | being implemented in `Tools\victory-dev` |
| R4 | Fills under 256 bytes copied in place | `VICTORY_COPY_PLAIN_FILLS` | planned |
| R5 | `sync_heaps` on a worker thread | `VICTORY_HEAP_SYNC_WIDE` | planned |
| R6 | Texture sweep in slices | `VICTORY_SWEEP_SLICES` | planned |
| R7 | Joined hash passes | `VICTORY_CARD_REHASH_RUNS=1` (existing) | measured with R1 |
| R8 | Keep texture copies longer while VRAM has room | `VICTORY_TEXTURE_IDLE_S` | planned |

Levels on demand (`VICTORY_LEVELS_ON_DEMAND`) saves VRAM, not frame time, on this card; blocked from default by: one
real-card run so far, tiles never returned while a copy lives, needs tiled-resources tier 2, picture cost unmeasured.

## Open questions and the run that settles each

- Does the over-cap page lie in the 0xb10000 heap, and how many runs has it? (log at the cap, or just run R1)
- Are the heap's re-sent 64 KiB blocks densely or sparsely changed? (count differing 4 KiB sub-blocks)
- Does GPU time fall once driver background compiles finish? (15-minute run with the card meter)
- Are the once-a-second long frames the sweep? (sweep time in the frame log)
