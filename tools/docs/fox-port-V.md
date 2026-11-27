# Coder V: texture-block holes (2026-11-26), copy `Tools\par\foxV`, file `crates/victory-gpu/src/pool.rs` only

## Finding (run-130-fox-classes, POOL_CLASSES=1 + HEAP_PACK=1) - why slabs were NOT built
- The pool grew 18 -> 187 blocks in 16 s while the world loaded (12:04:00 -> 12:04:16) and then sat at 177-187
  blocks (11.3-12.0 GB) all run; copies alive ~7.6-8.5 GB (the room rule's level). 20 blocks given back in 6 min.
- After the cap engaged, **no piece lacked a hole**: "0 new blocks past the cap for want of room", 1 new block under
  the cap, 103-115 thousand blocks made ahead held back. Every request found a fitting hole.
- No block is sparse: <25 % 0, 25-50 % 15, 50-75 % 64, >75 % 98. Holes swing 1.4 -> 3.6 GB within 5 s (churn: 0.5-1.9
  GB let go per 5 s), so they are refilled all the time.
- Fox already places small textures at 4 KiB (`D3D12_SMALL_RESOURCE_PLACEMENT_ALIGNMENT`, `pool::texture`, falls back
  to 64 KiB when the device refuses): alignment is not the cause.
- So holes = pool size (load high-water mark) - content (room-rule level), spread evenly, ~20 % per block. This is
  excess capacity, not external fragmentation: equal slots would leave it as it is (every freed slot still
  sits in a block that keeps live slots). The lever is fewer blocks: make blocks empty.
- Side bug in F's classes: a piece put in another class's block past the cap relabels the block (`block.class =
  class`): 79 -> 52 medium blocks in one 5 s step. Fixed only under `VICTORY_POOL_DRAIN=1`.

## Built (default off; nothing changes with the switches unset)
- `VICTORY_POOL_DRAIN=1` (`VICTORY_POOL_DRAIN_PCT`, default 90, 50-100): once a second (`pool::room`) the blocks that
  hold something beyond `ceil(bytes in use / (64 MB x pct))` are marked "draining", sparsest first (a draining block
  counts 25 % emptier, so the set stays put). A new piece avoids them while any other block (ripe, then unripe, by the
  usual class/best-fit or first-fit/pack rules) can take it; otherwise it goes into a draining block *before* a new
  block is made (counted). Copies leave as they do today (title turnover; HEAP_PACK=1 already takes the sparsest first);
  an emptied block goes back by the existing spare rule (2 kept under pressure). No move, no extra let-go, no extra
  block, no frame-time work (one sort of ~200 blocks a second).
- `VICTORY_POOL_SIZES=1` (meter): every 30 s, the size histogram of the live pieces (power of two + top 30 exact sizes,
  pieces at 4 KiB placement), the holes by size, sizes asked for in the last 30 s, holes smaller than any size asked,
  blocks by the age of their oldest piece, and live bytes older than 10/30/60/120/300 s (which says whether draining
  can empty blocks or long-lived pieces pin them: then Part 2 is needed).
- Lines (diag build): `memory housekeeping, texture blocks drained (VICTORY_POOL_DRAIN=1, to 90 %): ...` every 5 s,
  `memory housekeeping, texture pieces (VICTORY_POOL_SIZES=1): ...` every 30 s.
- Test: `pool::tests::drained_blocks_take_nothing_while_others_have_room_and_empty` (both placement paths), `size_steps`.

## run-157 (3 min, 03f8e87, DRAIN+SIZES, CARD_AWAKE=0): frame collapse, and V2 (58061a9)
- 5.5-7.7 fps against 10-15 for run-130's settings and 16-25 for runs 151/155/156. The submit thread's extra time is
  in snapshots (21.4 vs 0.6 ms), waiting for flights and the submission lock. Free RAM fell to 0.6-1.5 GB (run-130:
  2.6-4 GB) and the working set was about 2 GB higher. By the code, the pool's own cost is at most ~40 ms/s: up to 8
  passes over ~184 blocks per placement at ~400 placements/s. Marking takes µs/s; the meter takes ms per 30 s.
- V2: one pass per placement for all tiers; the meters copy under the lock and log after it; the drain line prints
  placements, µs under and waiting for the lock, and marking µs.
- Progress was slow because 9 of the 11 emptied blocks were refilled as "empty blocks" (empty placements 186 -> 195)
  before other-class holes. With drain on and past the cap, holes in any ripe block (draining ones included) now come
  first.
- Next test: 3 min, same env. If placement µs are small and fps still collapses, run a baseline with both switches
  unset.

## run-158 / run-159 (58061a9, drain without POOL_CLASSES): speed fine, purpose failed
- run-158: 5,239 placements took 71 ms under the lock per 5 s, 15 ms waiting, marking 45 µs. run-157's collapse came
  from the memory switch set and low RAM.
- run-159 (6 min): blocks 171 -> 194, holes 2.3 -> 3.5-4.6 GB, raw 15.7 -> 18.1 GB with 7.5 GB alive. Worse than
  with drain off.
- Who made the blocks: without POOL_CLASSES there is no raw cap. The made-ahead path keeps 2 empty blocks and makes a
  new one each time one is taken. First-fit takes an empty block whenever it comes earlier than a fitting hole. Drain
  hid 26-55 blocks holding 1.3-2.5 GB of holes, so more pieces went into empty blocks.
- Sizes: holes of 1-64 MB hold ~3.0 of 3.6 GB. 0 MB of holes is smaller than any size asked for: slabs are moot.
  147-157 of ~175 blocks hold a piece 120-300 s old, while only 0.5-2.2 GB of bytes is older than 120 s. A few old
  pieces pin each block, so draining blocks empty at ~3/min (20 in 6 min).
- Verdict: without moves, a pool can at most stop growing (the classes cap); it cannot shrink to budget while eviction
  punches holes everywhere.
  (a) Drain off. If kept: holes in draining blocks before empty or new blocks, and the 95 % cap in every mode.
  (b) Part 2: move the 150-980 MB that pins the draining blocks; ~1 GB copied once frees 1.6-3.5 GB.

## What to read in a run (3 min, then 6), with run-130's settings + `VICTORY_POOL_DRAIN=1 VICTORY_POOL_SIZES=1`
- drained line: "blocks emptied while drained" rising, "empty blocks given back" > 20, blocks falling from ~180 to
  ~140 (content 8 GB / 0.9 / 64 MB), holes < 1 GB, raw <= budget; "pieces put in a block being drained for want of
  another" small next to placements.
- Must not get worse: "made again", "made none for lack of room", frame times. If blocks do not fall and the sizes line
  shows most bytes older than 120-300 s, long-lived pieces pin the blocks: Part 2.

## V3 (a): `VICTORY_POOL_CAP=1` (default off; also on with `VICTORY_POOL_DRAIN=1`)
- New texture blocks, whether made ahead or made for a piece, are made only while the card's raw use with one more
  block stays under `VICTORY_POOL_RAW_PCT` (95) % of the budget. This now holds without classes too.
- Past the cap, a block is made for a piece only if no block can take it (counted, so no texture is left without memory).
- Placement order without classes: a hole in a ripe block that holds something and is not draining, then one that is
  draining, then an empty ripe block, then the oldest unripe one, then a new block. One pass over the blocks.
- With classes and drain: its class's holes in draining blocks come before an empty block.
- Counters are on the drain line: new blocks under / past the cap, and blocks made ahead that the cap held back.
- Target: blocks stop growing and raw stays at or under its starting level (~15.7 GB). It will not shrink below that
  (see run-159).

## V4-V6: tiled A/B and the page pool (22ae58c, `crates/victory-gpu/src/pool/pages.rs`)
- Why `tiles.rs` never engaged on fox: `resource.rs` `host_texture_as` hands any filled texture to `pool::texture`
  before its `reserved` branch. The "reserved" copy came back placed, `Tiled::make` found no tiles, and both were
  dropped. `VICTORY_LEVELS_ALL=1` (V4) plus its refusal meter (V5) stay in as an unused diagnostic.
- `VICTORY_PAGE_POOL=1` (default off; `=verify` counts only):
  - Which copies: texture copies of 64 KiB or more, 2D, one layer, any format or number of levels, that enter
    `pool::texture`.
  - How: each becomes a reserved texture (64KB_UNDEFINED_SWIZZLE) with all tiles, packed levels included, mapped at
    creation on the host queue from 64 MiB heaps of 64 KiB pages: one `UpdateTileMappings` per heap, regions split
    per level. The copy queue of uploads made ahead waits on a fence for the mappings (`pool::pages_ready` in
    `texture/ahead.rs`).
  - Safety carried over from the placed pool:
    - ripe heaps, lowest first; unripe heaps oldest first, counted;
    - heaps made ahead under the raw cap; not-resident / resident / mapped waits;
    - given-back pages rest 2.5 s; empty heaps past the kept spares go;
    - free and resting pages count in `texture_room`;
    - HEAP_PACK's eviction order ranks page copies last.
  - Fallback to the placed pool, by reason: not 2D, array or cube, multisampled, under 64 KiB, rounding over 25 %,
    tiling does not fit, refused as reserved, over 64 MiB, no pages under the cap, mapping budget spent (2000 µs per
    33 ms window, `VICTORY_PAGE_POOL_US`), no heap could be made.
  - Line every 5 s: "texture pages (VICTORY_PAGE_POOL=1): ...".
  - Tests: regions across levels and the packed ones; free runs; a device copy mapped and given back.

## V7: page pool runs 168/169 and the changes
**Speed (A).**
- Causes:
  - The mapping calls ran on the host's own queue, the queue the submit thread sends on, so they contended with its
    sends.
  - The budget was checked before the pool lock, so threads creating copies at the same moment all passed it. One
    window spent 37 ms against 10 ms.
  - The cost per call (78 -> 136 µs) is measured as wall time including that waiting, so it rose with the load.
- Now:
  - A mapping thread with its own direct queue makes the calls. The creating thread only takes pages, queues the job
    and inserts a GPU-side `Wait(fence, n)` into the host queue, so every list sent later (all that can name the
    copy) runs after the mapping.
  - The ahead copy queue waits for the last job queued.
  - The budget is a reservation: queued µs (calls x measured cost per call) may not pass `VICTORY_PAGE_POOL_US`
    (2000), or the copy goes to the placed pool. The host queue's possible GPU wait is therefore bounded by the
    backlog.
  - Every job signals, even if mapping failed (`catch_unwind`), so the host queue can never wait forever.
- One `UpdateTileMappings` maps ONE resource, so batching several textures per heap into one call is impossible.
  Calls are one per heap a copy spans.
- New line part: "texture page mappings (mapping thread) in the last 5 s: calls, pages, µs, µs a call, µs a page,
  cost now, queued µs, failed jobs".

**Capacity (B1).** Pages now come from the fullest ripe heap that holds the whole copy, else ripe heaps fullest
first, then unripe ones. A heap is made only when all free pages together are fewer than asked (as before).

## Page migration (B2): design, NOT built
**Goal.** Empty the sparse page heaps that turnover does not empty (run-169: 125 heaps, 2.1 GB free + 0.8 GB
resting). No descriptor or view changes: the texture object stays and only its tiles' memory moves.

**What moves.**
- The pages of copies in heaps at most 25 % in use, sparsest heap first.
- Only copies that are not fresh (< 1 s), have no upload staged or in flight (ahead state done, `holds` set), and
  whose host state is known (needs a read of the copy's tracked state from the resource record, coder G's file).
- Destinations: pages in the fullest ripe heaps, never in a heap being emptied.

**How** (on the host queue, at a submission boundary, so nothing of the title's runs in between):
1. List 1: barrier the copy to copy source, `CopyTiles` of the tiles held in the sparse heap into a scratch card
   buffer (swizzled to linear), barrier back.
2. `UpdateTileMappings` of those tiles to the new pages.
3. List 2: barrier to copy dest, `CopyTiles` buffer to the tiles (linear to swizzled), barrier back.
4. The copy's record of its runs (in its `Reclaim`) swaps old for new under the pages lock. The old pages rest 2.5 s;
   a heap that empties goes back by the spare rule.

**Budget.** At most 8 copies or 64 MB per boundary: about 1 ms of mapping calls and 0.2 ms of card copy. ~0.5-1 GB
to move then takes some seconds. Moving stops for the second when a boundary takes more than 1 ms.

**Verify mode** (`VICTORY_PAGE_MOVE=verify`, on for the first runs): after list 2, the tiles are copied back out and
the digest is compared with the scratch copy. A mismatch turns moving off for the run at once (counted, logged).
Placement goes on as before.

**Device-loss risks and guards.**
| Risk | Guard |
|---|---|
| another queue touching the copy while it is remapped (the ahead upload queue) | only copies whose ahead upload is done; never fresh ones |
| new pages still read through a dead copy's mapping | they rested 2.5 s like every page |
| old pages read by work sent earlier | mapping updates run in queue order on the host queue; the old pages also rest 2.5 s |
| a long boundary (TDR) | 64 MB / 1 ms cap |
| a failed creation or mapping | nothing is swapped, counted |

**Off.** `VICTORY_PAGE_MOVE` unset = off. The verify mode turns it off by itself on a mismatch.

**Expected.**
- Free pages fall from ~2 GB to near 0 and empty heaps go; resting stays (~0.5-0.8 GB, the safety rest).
- raw - net ≈ resting + the placed pool's own holes. The placed pool still holds ~3.4 GB: small, array, rounding and
  fallback copies. So raw within 0.5 GB of net also needs pages for arrays/cubes (a next step) and a shorter rest
  for pages proven idle.

## Page migration: state when parked (2026-11-26; OFF in all runs, not to be resumed without the experiment below)
**Built** (V9-V11, `pool/pages/migrate.rs`; `VICTORY_PAGE_MIGRATE=verify|1`, default off).
- A thread of its own picks the sparsest page heap (at most 25 % used) and moves the pages of settled copies into
  ripe fuller heaps:
  1. two temporary reserved textures over the old and the new pages;
  2. `CopyTiles` old -> card buffer -> new;
  3. under the submission lock: execute that list and `UpdateTileMappings` of the copy's own tiles to the new pages;
  4. the old pages rest after the fence;
  5. verify compares old and new pages byte for byte.
- Device tests pass: one-level RGBA and BC1 with 10 levels and a packed tail. Moved copies keep their texels.

**Runs.**
| run | build | result |
|---|---|---|
| 178 | V9 (`CopyResource`) | 2 verified, then 1 mismatch (6 tiles), hang 5 s later, device removed at 83 s. Cause: a packed tail partly mapped in the temporary textures. |
| 183 | V10 (`CopyTiles`, no split tails) | 14 moved, 10 verified, 0 mismatches, lock longest 327 µs. Device hung at 85 s. |
| 184 | V10 + `VICTORY_D3D12_DEBUG=1` | No validation message on our commands, only DEVICE_HUNG. Lock held ~5 s once (the calls block once the card hangs). |
| 189 | V11 (new pages only from ripe heaps, 2 ms lock guard) | 42 moved, 372 tiles, 38 verified, 0 mismatches, lock longest 345 µs. Device hung at 102 s, within ~30 s of the first moves. |

**What is ruled out.** Wrong bytes (all verified); unripe destination heaps (V11); long lock holds; invalid API use
(debug layer); partly mapped packed tails (V10).

**Remaining suspects.**
1. Remapping tiles of a live texture that lists already submitted, or sent ahead by the relay (`VICTORY_RELAY=2`),
   still reference. `UpdateTileMappings` is ordered on its queue, but the driver may not drain or flush the GPU's
   translation caches for work in flight on the queue.
2. Driver behaviour on `UpdateTileMappings` of a resource bound in descriptor heaps in use (NVIDIA, 4080 SUPER).
3. The old pages: given to new copies after the rest, possibly while a cached translation still points there.

**The experiment that settles it.** Moves only at a full drain of the host queue: the submit thread waits for every
flight, a move runs, nothing is in flight. Second, cheaper variant: only copies no view slot has shown for N frames.
If the drain variant still hangs, the driver cannot remap live textures and migration is dead. If it does not, the
cause is in-flight use, and the "not shown for N frames" filter is the shippable form.

## Part 2 - moving live copies out of draining blocks: the plan (NOT built; waits for the user's go)
**Goal.** Raw card use within the budget with the same content. The holes are 2.3-4.6 GB; blocks stay because a few
long-lived copies pin each one (run-159: 147-157 of ~175 blocks hold a piece 2-5 min old). Moving those few copies
(~150-980 MB, about 1 GB once) lets 25-55 blocks go: **1.6-3.5 GB back**, card and process commit alike.

**What moves.**
- Only copies in draining blocks holding at most 16 MB (sparsest first).
- Only pooled textures. These are never render targets, depth or shader-written, by construction (they sit in
  `ALLOW_ONLY_NON_RT_DS_TEXTURES` blocks).
- A copy is skipped if it is any of these:
  - made in the last second;
  - receiving an upload (staged or in flight);
  - held by a list that is recorded but not yet sent;
  - in a state the host does not know.
- Destination: a hole in a full, ripe block (never a block made while lists ran: the n08/n11 losses).

**When.** On the submit thread, between two title submissions (`Batch::flush`), on the title's own queue. That is
one small host list per boundary, at most 64 MB per boundary and 256 MB a second (about 0.1-0.3 ms of card time per
64 MB). Moving stops for that second if a boundary takes more than 0.5 ms.

**How.**
1. Make the new copy (same description) in the destination hole.
2. Copy on the card: old -> copy source, new -> copy dest, `CopyResource`, both back to their state.
3. Swap the record under its lock, as the existing level swap (`want_level`) does. The old copy goes to the existing
   `REPLACED` list, so threads that still hold a raw pointer keep a live object.
4. Remake every slot that shows it after the move list has run, with the existing "slots made again" path
   (`levels_changed_after` -> `texture_views_remade`).

**Old copy kept alive.** Three things hold it:
- the move list's keep list;
- the slots still showing it, until they are remade;
- `REPLACED`, until the moment passes.

After that its piece rests 2.5 s like any piece.

**Verify mode** (`VICTORY_POOL_MOVE_VERIFY=1`, on for the first runs):
- Before the old copy is released, both copies' level 0 and last level are read back and their digests compared.
- A second count: copies whose old object is still referenced 10 s after the move (a holder we missed).
- **Any mismatch or missed holder switches moving off for the rest of the run at once.** Later texture-making is
  unaffected (today's behaviour), and the line says why.

**Device-loss risks and guards.**
| Risk | Guard |
|---|---|
| destination in a block made while lists run | ripe blocks only |
| a list recorded before the swap reads the old copy | old copy stays alive (keep + `REPLACED` + rest) |
| an upload lands in the old copy after the swap (picture, not loss) | skip copies with uploads staged or in flight; verify mode |
| wrong resource states for the copy | only copies in a known read state |
| a long submission (timeout reset) | 64 MB per boundary, 0.5 ms stop |
| any creation failure | copy skipped, counted, nothing changed |

**Off.** `VICTORY_POOL_MOVE=0` (the default until a go); the verify mode stops it by itself.

**Expected.**
- Blocks drop from 175-194 to about 140-150, holes under 1 GB, raw about 14-15 GB (budget 14.9).
- The room rule then works on what is really there.
- Cost: about 1 GB copied once, then only what the churn pins again.

## Placement keyed to the title (asked by the user): evaluation, not built
**What the title does (F25).** A texture version is ONE allocation (all its resident levels; F11: a level change is a
new allocation at a new address). Under 64 KiB (and 0x18000) it comes from size slabs; larger from a buddy allocator
over 64 KiB virtual pages, backed by physical pool pages taken FIFO. Its *physical* memory is a set of fungible 64 KiB
pages: any free page serves any texture. That is why the title never fragments.

**Fox today.** One placed resource per title texture (`pool::texture`; levels from `base` on). Run-159 copies:
26k pieces, 7.5-8.6 GB. Small ones are at 4 KiB placement where the device allows (8.5k pieces, only 150-180 MB).
The holes are 1-64 MB ones: 0 MB of holes is smaller than any size asked for.

**(1) Address-mirrored placement (copy at a heap offset derived from the title's GPU-view address): rejected.**
- Host size and layout differ from the title's per format and level (console tiling vs
  `GetResourceAllocationInfo`), so a copy does not fit the title's slot in general.
- A placed heap mirroring the title's address span commits the whole span, including the buddy allocator's unmapped
  virtual space: more memory, not less.
- Kind-0 textures in resource files (~10k copies, ~5.5 GB) do not come from that allocator.
- Placed resources need 64 KiB alignment above the small-texture limit, while title slabs go down to 4 KiB.

**(2) Size slots from the title's live size classes: rejected.** The holes are not too small to use; slots would
change nothing (run-130/159 data).

**(3) Recommended: mirror the title's PHYSICAL model, a page pool (`VICTORY_POOL_PAGES=1`, not built).**
- Every pooled texture of 64 KiB or more becomes a reserved resource whose tiles map onto fungible 64 KiB pages from
  a pool of 64 MB heaps; one texture may span heaps.
- Textures under 64 KiB stay in today's placed pool (small, ~0.2 GB).
- Holes can then only be free pages, and any free page serves any texture, so the pool never grows while one is
  free. Its size follows the live copies plus resting pages, not a load-time high-water mark of unusable gaps.
- Moving later is cheap: copy a page's tiles and remap them. The texture object and every view stay the same, with
  no descriptor work, which removes the hardest part of Part 2.
- The title hooks (`level_freed` / `alloc_returned`, coder K, par\foxG / foxM) then say exactly when a copy's pages
  are free. The room rule lets go only when the title's live set really exceeds the budget.
- **What exists.** `tiles.rs` already has reserved textures, a 64 MB tile-pool, mapping on the queue ahead of the
  upload, and views that follow the levels that have tiles. It is on in these runs (`VICTORY_LEVELS_ON_DEMAND=1`,
  tier 4) but **no texture qualifies on fox**: its status line never printed, so the path is unexercised on this base.
- **Needed:**
  - send every pooled texture of 64 KiB or more there (the whole chain from `base`, all tiles given at the upload
    step);
  - allow tiles across heaps (one `UpdateTileMappings` per heap run);
  - ripe / made-ahead heaps and the 2.5 s rest for given-back tiles (the tile pool has neither today: the same n08/n11
    risk);
  - the raw cap of (a);
  - meters: pages alive / free / resting, heaps, mapping µs.
  - Per texture, fall back to today's placed pool, counted, when tiling or reserving fails, when a texture needs more
    than a heap's tiles, or for arrays, cubes and similar shapes if they misbehave.
- **Expected.** Pool ≈ live + resting (8-9.5 GB instead of 11.3-12.4), raw ≈ 13.5-14.5 GB on a 14.9 GB budget:
  **1.5-3 GB**, without moving anything. Migration of pages is extra.
- **Risks.**
  - Sampling speed of reserved textures on this card (64 KiB undefined-swizzle layout; usually equal, must be A/B'd).
  - `UpdateTileMappings` CPU: old tree measured ~23 µs a call (4,262 calls, 98 ms); at ~400 new copies/s that is
    ~9 ms/s on the submit thread.
  - A level read before its mapping ran: the existing upload-step rule covers it.
  - Taking tiles back while read (run-10's loss): tiles go back only when the copy is dropped, after its flights,
    plus the rest.
- **Size.** Medium: `tiles.rs` and `pool.rs`, plus the creation branch in `resource.rs` (`hosted`). The last is
  coder G's area, so the branch needs coordinating.

**Page pool vs Part 2.**
- The page pool removes the cause, gets the card under budget without copying live data, and has no view or holder
  switching. Its device-loss surface is the tile mapping rules that `tiles.rs` already follows.
- Part 2 is a repair loop that must catch every holder of a moved resource.
- Recommendation:
  1. Run (a) now.
  2. Build the page pool behind its switch with the placed pool as per-texture fallback, A/B'd for speed first.
  3. Keep Part 2 only as page migration on top, if the page pool still leaves raw over budget.
