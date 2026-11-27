# VRAM: exact accounting and plan (2026-11-26, read-only analysis)

Sources: `victory-data/bench/run-90-combined/plain.txt` (run-87, run-89 for totals), `victory-gpu/src/{resource.rs,
tiles.rs, shadow.rs, descriptor/host.rs}`. run-90's `gpu.csv` is empty (8 bytes); nvidia-smi figures are run-89's.
"measured" = read from a log line; "code" = read in the source; "inferred" = arithmetic on the two.

## 1. Accounting of three mid-run moments (run-90, MB)

| Category (log line it comes from) | 06:52:33 | 06:53:48 | 06:55:40 | menu 06:50:35 |
|---|---|---|---|---|
| Texture heaps, 23 x 256 MB (`memory map, texture heaps`) | 5,888 | 5,888 | 5,888 | 768 |
| - live copies the title fills (`memory of textures`; 21,576 of 21,748 placed) | 5,138 | 5,084 | 5,337 | 533 |
| - free space inside heaps (`housekeeping`) | 587 | 592 | 379 | 234 |
| - placed ranges retired, waiting for their last holder (inferred, ~805 textures) | ~160 | ~210 | ~170 | 0 |
| Level tiles pool heaps (`levels on demand`; given to levels 273 / 392 / 114) | 320 | 512 | 448 | 0 |
| Render targets, depth, host-written copies (`memory of textures`; 652 copies) | 2,662 | 3,100 | 3,426 | 471 |
| Card windows (`windows on the card`) | 2,116 | 1,944 | 1,756 | 8 |
| Title-buffer card copies (card copies 3,061 / 2,889 / 2,697 less windows; cap 1,024) | 945 | 945 | 941 | 174 |
| Sum of what the layer counts | 11,931 | 12,389 | 12,459 | 1,421 |
| System: card in use (`memory as the system counts it`) | 13,621 | 14,208 | 14,315 | 1,822 |
| **Not attributed by any counter** | **1,690** | **1,819** | **1,856** | **401** |
| Budget (system) | 15,127 | 14,938 | 15,050 | 15,277 |
| Housekeeping "in use" = system less heap free ("net"); "counting free heap space" = raw | 13,066 / 13,654 | 13,559 / 14,152 | 13,997 / 14,376 | 1,587 / 1,822 |

- nvidia-smi (run-89): 14,013-14,484 MB mid-run, peak 15,333; process figure peaks 14,344 (run-90) / 14,529 (run-89) at
  the 5 s lines; in the menu both agree within 100 MB, so other processes hold ~0 with the display off.
- The unattributed 1.3-1.9 GB holds, with no counter each: the ~170 title-filled copies committed singly, depth twins
  (196-257 MB, `memory, more`; not known whether inside the 2,662), swap chain + back-buffer copy, upscaler, descriptor
  heaps, query heaps, pipelines, the compare pass's buffers, objects in `host::retire` not yet freed (up to 128 MB a
  frame from the early rule, 256 MB+ bursts from the fallback), driver overhead. Menu baseline 401 MB; the world adds
  0.9-1.45 GB. **Missing counter: a census of every default-heap allocation the host makes, by kind.** No size-class
  split of texture copies exists either (only `VICTORY_CARD_ACCOUNTING=1`, not set in these runs).
- NOT on the card (PC memory shared with it, 6.6-7.2 GB in use): staging blocks 768 MB (673 free), list scratch 132 MB,
  PC sides of windows 3.8-4.3 GB and of buffer card copies 945 MB, read-back buffers ~0. "Plain copies"
  (`VICTORY_COPY_PLAIN`) and own-view stores (`VICTORY_BACK_OWN_VIEW`) are CPU copies into title memory: 0 MB of card.
- "Before" twins of shader-written card copies: 0 MB ("their bytes from before 0 MB").

## 2. Duplication on the card

| Kind | MB | Basis | Exact meter |
|---|---|---|---|
| Window padding: 4 MiB grid; only 2,232 of 3,876 MB (58 %) lies between lowest and highest address asked | ~0.75-0.9 GB of the 1.8-2.1 GB on the card | measured (`memory let go`), card share inferred | per window: card bytes outside the asked span (split with / without a length; 588 of 910 have none) |
| Same title memory in more than one window (3,812 MB of windows over 3,516 MB) | ~100-300 total, ~55 % on the card | measured (`memory, windows`) | exists |
| Free space in texture heaps that never shrink (23 heaps all run, "let go as empty 0") | mean 962-1,055, max 2,397, at peak demand 380-590 | measured | exists |
| Copies of textures whose title memory is no longer mapped (old level after a level change) | mean 160-172, max 559-732 | measured estimate (`memory, more`) | exists |
| - of those held only by a stale-kept descriptor slot (`VICTORY_STALE_KEEP_MS` 10 s, cap 120 s) | mean 14-19, max 217-325 | measured | exists |
| Levels of a full-chain copy the title does not hold (textures outside levels on demand) | unknown | code: `hosted` makes every level | `VICTORY_CARD_ACCOUNTING=1` ("bytes of the levels not mapped") |
| Depth twins | 196-257 | measured | exists |
| Title-filled copies the host also copied into | 57-113 | measured (`sweep never lets go`) | exists |
| Render targets aliased at one address with other descriptions | unknown | ledger note | missing: bytes of targets sharing a `TARGETS_AT` range |
| Texture inside a card window; buffer range in a window AND a card copy | 0 by construction | windows are "outside buffers", made only for buffer views | - |

## 3. Lifetime

| Question | Answer |
|---|---|
| Game unmaps a level's pages | noted exactly (`note_unmapped_exact`); at the next sweep (~0.5 s: 701 sweeps in 350 s) the copy goes if no page is mapped (`mapped_now`) AND nothing holds it (no list in flight, no slot); else re-noted every sweep. Then `host::retire` until flights land; the heap range returns via `sweep_leaving` |
| `VICTORY_MAP_DEFER` | delays the unmap by queue latency (ms); not material |
| Stale-keep | a slot showing the old texture holds a reference, so the copy is not "free": it stays until the slot is rewritten or 120 s. Cost above: mean ~15 MB, max 325 MB. 74-78 slots hit the cap per run |
| New level mapped | new address = new record (`texture_at`) = new host copy; texture detail meter: "re-addressed 3010 (new copy 3010, reused 0)". Never the old slot: the new copy needs fresh room first, peak = old + new, and it is REFUSED when the queue is at its line although the old copy frees a second later |
| Room rules | early: on while RAW > 88 %, off under 82 % (raw includes heap free space, which never shrinks, so it stays on: "on in N sweeps, now on"); lets go copies idle 3 s / 60 frames while NET > 82 %, 128 MB a frame. Fallback: idle floor only 1 s / 4 frames, at least 256 MB, sorted sparsest heap first (`pack_sort`), LRU second |

## 4. The churn (run-90; run-87 / run-89 alike)

| Quantity | run-90 | run-87 | run-89 |
|---|---|---|---|
| Copies let go, MB (copies) | 34,479 (134,872) | 35,820 | 41,275 |
| - on a real unmap by the game | 9,756 (28 %) | 10,174 | 11,741 |
| - early room rule | 15,137 (44 %) | 15,372 | 16,348 |
| - "out-of-memory" fallback (times) | 9,616 (41) | 10,279 (42) | 12,289 (50) |
| Made again, MB (copies) | 7,530 (24,504) | 8,215 | 9,556 |
| Host copies made in all | 154,201 | 162,630 | 184,750 |
| Passes that made none for lack of room | 11,741 | 1,804 | 12,994 |
| DirectStorage bytes written into mapped pages (the game's streamer, all asset kinds) | 39,800 | 43,279 | 48,904 |
| Title memory really unmapped (all kinds) | 43,404 | 46,187 | 51,826 |

- The game cycles ~40-49 GB through its streamer per run (F11: every level change is a whole new copy), while holding
  only 2.8-3.3 GB of its 6,236 MB budget (peak 4.9). Most copy-making (~32 GB, ~130,000 distinct records) is the game's
  own turnover. The layer adds 7.5-9.6 GB of re-makes (+23 %).
- 72 % of released bytes (24.7 GB) were let go by the LAYER's room rules from textures the game still held (mapped,
  not in a slot for 1-3 s). 30 % of those bytes were made again. Victims are never slot-held, so the waste shows as a
  texture that pops in again after the camera turns back, and as refusals while the band is full.
- "Out of card memory" is not a failed allocation: all 43 lines say "an allocation of 0 MB was turned down" with
  1.5-1.8 GB of budget free. It is `resource::room()` answering no (net + reserves >= 90 %) while net > the 88 % mark:
  `note_refused(0)` -> the next sweep lets go >= 256 MB. Queue line (90 %) above high mark (88 %) makes it fire by
  construction; the low mark (82 % = 12.3 GB) means 2.7 GB of the budget is never steadily used.
- Caveat: "made again" counts any record with `gone` set, also one released on a REAL unmap whose address + description
  the game reused for a new texture. 7.5 GB is an upper bound of pure waste. **Missing counter: made again split by
  the reason of the last release (unmap / early / fallback), seconds since it, and whether the memory stayed mapped.**
- Texture detail (1 frame in 16): 3.16 % of bound full-detail textures "not uploaded"; queue due max 8,440 (22,709-24,724
  slots).

## 5. Plan, ranked by MB per unit of risk

| # | Change | MB | Where | Why exact | Risk on screen | Proof meter |
|---|---|---|---|---|---|---|
| 0 | Meters: default-heap census by kind; made-again by cause and age; mapped-now texture demand; set `VICTORY_CARD_ACCOUNTING=1` in run-auto | explains 1.3-1.9 GB | `host.rs` allocation sites, `resource.rs` `hosted` (line 1494), `copystat.rs` | meters only | none | the lines themselves |
| 1 | One coherent room rule: early band on NET, marks 88/82 -> ~95/92; queue line below the high mark so a refusal no longer triggers a 256 MB purge; fallback uses the early idle floor (3 s / 60 frames) and pure LRU (heap sparsity only as tie-break for copies idle > 10 s) | +1.2-1.9 GB steady for textures; fallback -> ~0; made again down by most of 7.5 GB | `resource.rs` `marks_pct`, `room`, `fallback_need`, `let_go_idle`, `LRU_FLOOR_MS`, `pack_sort` | releases fewer copies; makes none differently | raw use past the budget: Windows moves card memory to the PC's = frame time collapse / device loss (runs 18-24). Guard: keep `targets_room_kept` (raise 3,072 -> 3,584: targets reach 3,426) and stop at raw 96 % | fallback count, made again, "made none for lack of room", due count, raw peak <= 14.4 GB |
| 2 | Give back texture-heap slack: move the live copies of the sparsest heaps card-to-card (`CopyTextureRegion`, same format and bytes) into fuller heaps when idle, then release the heap (today `VICTORY_HEAP_DRAIN` only evicts and re-uploads) | 0.4-0.6 at peak, 1.0-2.4 otherwise, returned to targets / windows / headroom | `resource.rs` `TextureHeap`, `trim_texture_heaps`, `placed_texture`; views remade as `texture_views_remade` | same bytes, GPU copy; old copy retired after flights | a slot not remade shows a freed texture: black or wrong texture | free space inside heaps, heaps alive < 23, device stays |
| 3 | Card side of a window only for the span asked (64 KiB rounded), first for windows asked WITH a length; or grid `VICTORY_WINDOW_KB=1024` for those | 0.3-0.4 safe part, up to 0.9 | `shadow.rs` `window`, `window_card_most`, window creation | bytes outside the span were never read by a draw | reads past the span (no-length root views): spiky or missing geometry | "windows on the card", new "card bytes outside asked span" |
| 4 | Release at the unmap, not at the sweep: run the exact-unmap check every frame for noted ranges; let a new level use the range its old copy frees when both are placed and the old is free | 100-170 mean, up to 0.7; removes refusals of new levels | `resource.rs` `let_go_idle` (the `under` loop), `texture_at` | the old copy's source is unmapped: it can show nothing valid | none if the "free" test stays; stale-kept slots unchanged | "not mapped any more (estimate)" -> ~0 |
| 5 | Levels on demand wider: floor 1 MiB -> 256 KiB, array / cube textures; tiles get room before plain copies (283-849 "held level without memory" a run are refusals = black or blurred levels) | unknown until #0 (run 189 note: ~1.9 MB a copy on ~1,600 copies) | `resource.rs` `hosted` (`shape_fits`, `partly`), `tiles.rs` `floor`, `Tiled::make` | same levels shown; unheld levels were never readable | black level if a view reaches one without tiles | `CARD_ACCOUNTING` unmapped-level bytes; "not allocated" MB |
| 6 | Render targets: nothing to release (census: 0 MB idle 30 s, 0 MB unmapped). Only a meter for aliased targets | 0 now | `resource.rs` `targets_census`, `TARGETS_AT` | - | - | new meter |
| 7 | `VICTORY_LEVELS_GIVE_BACK` | < 50 expected: by F11 a level change is a new record, so a live copy rarely loses levels | `tiles.rs` `take_back_locked` | - | device loss (run-10) | "level tiles taken back" |
| - | Keep: `VICTORY_CARD_BUFFERS_MB` 1,024 (2,048 rejected), stale-keep, no compression, no lower detail. Staging blocks (673 of 768 MB free) are PC memory: trimming helps RAM only | | | | | |

Give-back, what went wrong and what a safe version needs (code comment at `tiles.rs` 252-258, cause "not found"): a
card copy INTO the texture recorded while a level had tiles and submitted after an upload took them back lands in
tiles since given to another texture, or in none. Safe version: take back only in the submit step, only when no list
recorded-but-unsent or in flight names the texture as a destination (a per-texture count), never with `card` or
`claimed` set (already so), and return tiles to the pool only after the fence of the unmapping submission.

Excluded from levels on demand today (`resource.rs` 1421-1440): not 2D, array or cube (`depth_or_array_size != 1`),
one level, multisampled, host copy that drops levels, XOR-swizzled layouts, under 1 MiB, and textures all committed
with no later texture in their span and first viewed level 0. Block-compressed formats are NOT excluded.

## 6. Target split of a ~15.0 GB budget (opening route)

| Share | Today (range) | Target | Note |
|---|---|---|---|
| Texture copies alive, tiles included | 3.4-5.7 GB | up to 6.3 GB | the game's streaming budget (6,236 MB) bounds what it can hold |
| Free space in texture heaps + tile pool slack | 0.4-2.4 + 0.05-0.35 | <= 0.3 | #2 |
| Render targets, depth, host-written | 2.4-3.4 | 3.5 reserved | no room test today; reserve via `VICTORY_CARD_ROOM_FOR_TARGETS_MB` |
| Card windows | 1.76-2.5 | 1.3-1.5 | #3; cap 4,096 is never reached (held back 1,278-1,693 times) |
| Title-buffer card copies | 0.94 | 1.0 | unchanged |
| Unattributed (swap chain, upscaler, twins, retired, driver) | 1.3-1.9 | <= 1.5 | #0 first |
| Headroom under the budget | -0.5 to +1.5 (raw peak 15.2-15.5) | 0.6 | raw <= 14.4 GB |
| Sum | | 14.4-14.7 | |
