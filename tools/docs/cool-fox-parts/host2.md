# cool_fox review — HOST part 2: non-shader hunks of 8cc31ab, 18bdf79, ab75ac3

Read-only review (git show / grep / read). Nothing built or run. No text addressed to AI assistants was found in the
hunks or in `docs/bug-journal.md`.

Reviewed clone: `C:\GTAVI\Tools\ref-forks\cool_fox` @ f4b48ad. "Theirs" line numbers are at HEAD (f4b48ad) unless a
commit is named. "Ours" = `C:\GTAVI\Tools\victory-dev\crates`.

## 0. Three things to know before reading the entries

1. **The numbers in the task (16/32 MiB, 4 GiB/2 GiB) are the values of 18bdf79/ab75ac3. The author changed them six
   hours later in f4b48ad** (another agent's commit, but a coder must port the final values):

   | constant | 18bdf79 / ab75ac3 | HEAD f4b48ad | where (HEAD) |
   |---|---|---|---|
   | per-frame staging cap, memory pressure | 16 MiB | 64 MiB | `residency.rs:151-159` |
   | per-frame staging cap, 32 GB host with >= 1 GiB live staging | 32 MiB | 64 MiB | same |
   | per-frame staging cap, "critical" (commit < 1.5 GiB or an out-of-memory return in the last 500 ms) | – | 16 MiB | `residency.rs:144-147` |
   | commit needed for the texture pass (ordinary / bake) | 4 GiB / 2 GiB | **512 MiB / 256 MiB** | `residency.rs:58-63` |
   | commit needed to make a NEW 64 MiB staging block (ordinary / bake) | – | 1,024 MiB / 256 MiB | `residency.rs:65-70` |
   | staging blocks kept under pressure | 0 | 1 (0 only when critical) | `pool.rs:54-62` |

   Their reason, in the HEAD comment (`residency.rs:59-61`): the 4 GiB / 2 GiB floor starved "bindless world textures
   or one-shot ped decoration bakes when steady-state commit headroom sits in the 1-4 GiB range on 32 GiB hosts". That
   is our RAM size. **Porting the ab75ac3 values as they are would cost us textures.**

2. **Our tree does not have their allocator.** Their `pool.rs` (9fea743 + 3b73cb0 + 7d3e179, avatarmanz base; 640 lines),
   `residency.rs` (e05d463; 190 lines), `texture/ahead.rs`, `host/card_steps.rs`, `host/place.rs` are 0 hits in ours.
   Ours has its own equivalents, written separately:
   - staging: 64 MiB upload blocks `texture.rs:187-337` (`upload_memory`), spare buffers by size `texture.rs:117-175`
     (`SPARE_MOST` `:121`, `spare_most()` 2,048 MB `:129`), idle trim `texture.rs:391-442` (`trim_pools`);
   - textures: 256 MiB heaps `resource.rs:3273` (`TEXTURE_HEAP`), cut at `resource.rs:3740-3890`, idle trim
     `resource.rs:3608` (`trim_texture_heaps`), card-room test `resource.rs:1530` (`room`);
   - scratch chunks `host.rs:1786-1935` (`SPARE_MOST` 128 MiB, `trim_scratch` `:1862`);
   - once-a-second sweep `host.rs:222-244`.
   So every entry below is "the idea, applied to our functions", not a file copy.

3. **Every mechanism in 18bdf79/ab75ac3 is about the PC's memory (RAM / Windows commit), not the card's.** The author
   has a 32 GB card and was running out of commit (peak process commit 27-34 GiB on a 32 GiB PC, journal 15:46, 16:04).
   Our known limit is the card (14.4 / 15.1 GB). None of these items frees card memory, and none shortens a streaming
   freeze in the normal case. They prevent aborts, and two of them do so by uploading textures later (pop-in).
   Whether we are near the commit limit at all is not known: measure first (`framelog.rs:555-560` and
   `copystat.rs:479-485` already read `ullAvailPageFile`; look at the lowest value of a run before porting H2-4/H2-5).

The author's only timing claim for this group (journal 15:46-15:48, RTX 5090, Series X path "normal Pareto rendering
settings", opening save, 15 title-bar intervals of >= 1,000 draws): "Baseline median 64.2 ms/frame, candidate 54.8 …
Peak process commit was 27.15 and 26.12 GiB … so this pair is directional." It covers all of 18bdf79 at once on a PC
that was paging. After ab75ac3 (16:04-16:07): 135 s, median 54.3 ms over 120 intervals, peak commit 34.42 GiB, lowest
commit headroom 1.30 GiB, "Zero E_OUTOFMEMORY returns and zero Rust allocation aborts". No per-mechanism numbers.

---

## 1. The mechanisms, end to end (theirs)

### M1 Staging admission (18bdf79; `draw.rs:2524-2660`, `residency.rs:151`)
The pass that uploads textures shaders pick by index (`heap_textures`) has a byte budget per frame:
`staging_budget_per_frame(card budget)` `draw.rs:4865` = 96 MiB (card <= 6 GiB) / 192 / 256 (> 12 GiB) / 384 (>= 20 GiB);
8cc31ab caps it at 128 MiB on a host with <= 32 GiB RAM (`limited_host_ram`, `residency.rs:93`: `ullTotalPhys <= 32 GiB`,
asked once). Catch-up (`on_time >= 1024` slots due) triples bytes and time (`draw.rs:2636`); an early pass or a bake
has a budget of its own on top (`PASS_BUDGET`, bake: 2 GiB). 8cc31ab applied the pressure cap to the base figure,
so catch-up tripled it afterwards and the early pass added its own: 18bdf79 moved the cap to the end
(`draw.rs:2648-2658`): `bytes_at_most = prefetch_limit(bytes_at_most, pressured, [critical,] limited, live_staging)`
unless the pass is a bake (`PASS_BUDGET` gained a third field `bake`, `draw.rs:160`) or `VICTORY_TEXTURE_STAGE_MB` is
set. "Pressured" (`residency.rs:98-104`, refreshed once a second from the sweep `host.rs:443`): enters at free physical
<= 3 GiB or available commit <= 4 GiB, leaves only when physical >= 5 GiB and commit >= 6 GiB. The cap is soft: the
loop tests `SPENT >= bytes_at_most` before each slot, so one texture may cross it; unserved slots stay due.

### M2 Pool growth back-off (ab75ac3; `pool.rs:201, 248-286`)
`Pool::take` finds a free range in an existing 64 MiB block; if none, `make()` creates a block (`CreateHeap` for
textures `pool.rs:533`, a 64 MiB upload buffer for staging `pool.rs:396`). New `Pool::grow`: if `now < retry_growth_at`
return `None` without calling the driver; if `make()` fails set `retry_growth_at = now + 500` (ms). Free ranges of
existing blocks stay usable. Both call sites (the needed block and the block made ahead) go through it. Their
reproducer (`pool.rs` test `a_failed_block_is_not_retried_for_each_small_upload`): eight uploads of 24,080-704,008
bytes made eight failing 64 MiB creations in a burst; now one.

### M3 Commit-headroom gate (ab75ac3; `residency.rs:45-91`, `draw.rs:2583, 2813`)
`admit_texture_pass(bake)`: false while an out-of-memory return is younger than 500 ms; else reads
`GlobalMemoryStatusEx().ullAvailPageFile` at most once per 32 ms (`NEXT_COMMIT_SAMPLE`, compare-exchange so one thread
samples) — "The texture sweep may run for seconds inside a single submitted frame" — and admits if commit >= floor
(table in section 0). Called once before the loop and again per slot. Tables' textures (bound through a descriptor
table, uploaded by the draw) bypass it. HEAD adds `allow_staging_growth` (`residency.rs:85`): a speculative upload
(`PICKING` set and not `BAKING`) may reuse free staging ranges but not make a new block below 1 GiB commit
(`pool.rs:389-395`, `texture.rs:1893-1898`).

### M4 E_OUTOFMEMORY handling (ab75ac3; `residency.rs:23-43`)
`allocation_error(&e)` is called at the three creation sites — upload buffer `host.rs:8621`, texture block
`pool.rs:535`, host texture `resource.rs:2536`. If the code is `E_OUTOFMEMORY` it sets `RETRY_UPLOAD_AT = now + 500 ms`.
`upload_refused()` is then true for 500 ms. The texture pass tests it at four points of a slot's work
(`draw.rs:2849, 2862, 2931, 2950`) and on a hit does `served = false; break` **without marking the slot done**: "A
failed allocation leaves this slot due. Publishing it or marking it served would lose the only retry for some
materials." HEAD adds a thread-local `STAGING_DEFERRED` (`residency.rs:29-39`) so a staging that was only turned down
(not failed) also leaves its slot due.

### M5 Bindless sweep per heap + stride-buffer identity (18bdf79; `descriptor/host.rs:115-116`, `draw.rs:637-650, 889, 2076-2100`)
Before: the buffer-view pass kept its per-slot ranges in a process-wide map keyed by the `HostHeap`'s address, its
"once a frame" in one static `FRAME`, the per-list "already done" under one constant key, and the unwatched-copy
throttle in a second global map. A heap freed and another made at the same address inherited the dead heap's ranges;
two view heaps in one frame shared one "once a frame"; the deferred (aside) pass looked the heap up by CPU address
at submission and could find a different heap. Now:
- `HostHeap.buffer_sweep: Mutex<BufferSweep>` (passes, ranges, unwatched) and `HostHeap.buffer_frame: AtomicU64` live
  and die with the heap;
- the list key is `first_use(gpu as usize | 1)` — the stride buffer's GPU address, odd so it cannot equal a title
  object pointer: once per list **per stride buffer**;
- the deferred pass runs only if `heap.host().filter(|h| h.strides_gpu() == Some(gpu))` — same stride buffer as the
  one the list recorded and keeps alive (`list.keep(buffer)`, already there before);
- unwatched throttle: skipped entirely at the default `VICTORY_UNWATCHED_FRAMES=1`; with > 1 a copy is skipped only if
  the last refresh covered the interval now asked (`low <= from && to <= high`); the map is rebuilt each pass, so it
  no longer grows.

### M6 Trimming free blocks at the tick (18bdf79; `pool.rs:44-64, 307-331`)
`pool::room` (once a second) used to store only the number of empty blocks to keep (`SPARE` 2, or `SPARE_KEPT` 64 =
4 GiB while the card is under 85 % and `VICTORY_KEEP_FREED` is on); blocks were dropped only when an allocation next
ran. Now `reclaim(now, limit, sweep=true)` runs at the tick on both pools (with `try_lock`: a busy allocator is left
alone): pieces whose rest (2,500 ms) is over return to their block, and whole-free blocks past the limit are dropped.
Under pressure the texture pool keeps 2 instead of 64 and the staging pool keeps 0 (HEAD: 1; 0 when critical). The
sweep also frees spare scratch chunks under pressure (`host.rs:444`, e05d463). 8cc31ab: on a <= 32 GiB host the
five-minute keep of idle staging/windows (`ram_idle_for` `host.rs:382`) and the 1 GiB scratch keep (`host.rs:1935`) are
off, and `texture::let_go_idle` takes `residency::headroom()` = min(free physical, available commit) instead of free
physical alone (`texture.rs:527`).

### M7 Movie-plane uploads (8cc31ab; `texture.rs:800-818, 884-988, 1241`, `host.rs:2240, 2255, 2320, 2999, 7283-7296, 7416`)
`Staging::movie_plane()`: 2D, linear (`swizzle == 0`), depth 1, one mip, one sample, one footprint at offset 0, format
`R8_UNORM`, not on-demand. For such a texture `upload_from` no longer uses the shared staging buffer and its
hash/dirty check. It takes a scratch chunk owned by this list (`HostList::plane_upload`), records the
`CopyTextureRegion` from that chunk (`record_upload`, the old copy loop made a function with `source`/`base`
parameters), and pushes `Step::Plane((at, len), fill)`. At submission the step copies title memory into the chunk
(`fill_plane`: straight `read_into` when pitch and offset match, else through `layout.detile`; zeros on error). The
chunk goes back to the pool when the list is reset (`Deferred::drop`). `Step::Plane` is treated as a read of title
memory `[at, at+len)` in the four places that order submissions (`host.rs:2999` `blocked_by`, `card_steps.rs:204, 444`,
`place.rs:122`). Effect: two lists in flight each carry their own picture of the plane (the decoder reuses its
buffers), and no hashing of the plane at each check.

### M8 XMA audio (8cc31ab; `victory-hle/src/modules/audio.rs:519-530, 560-573`)
(a) `pump`: up to 8 `decode` calls until a full block (`SAMPLES_PER_BLOCK`) is there; a block is written only if full,
or if the voice has ended (the padded tail). Before, one decode that produced nothing (skipped frame, switch of input
buffer) still wrote a block padded with zeros. (b) `decode`: an ended voice starts again when the title publishes a
valid input (`context.field(0, 20, 2) != 0`): `ended = false`, and if the state field says `INPUT_INVALID` it is
cleared with `set_field(2, 26, 6, 0)` — six bits, so the "ended" bit 31 set at `:587` goes too.

### M9 `pipeline/make.rs` (8cc31ab) — no Wave32 code here
The +94 lines hold three things, none of them wave handling: `left_out` (skips `CS_RaytracingResolve` and
`CS_ScreenProbes_ActivateRays` pipelines), `scene_input` (six shader hashes + the four bake pixel shaders are made at
creation, queue class `REQUESTED`, and count as "screen" pipelines), and `worker_count_for` (3/4 of the logical
processors, 2 to 8, on a <= 32 GiB host; else 2/3, 2 to 16). The host side of the Wave32 contract is older
(`host.rs:930-938`, fxceless 578d459): the host reads `D3D12_OPTIONS1.WaveLaneCountMax` and calls
`victory_shader::emit::set_host_max_wave`; the translator does the rest. **Ours has exactly that**
(`victory-gpu/src/host.rs:847-848`). Nothing for the host coder to do for Wave32.

---

## 2. Port-ready entries (take in this order)

### H2-1 Heap-scoped buffer sweep and stride-buffer identity — HOST
- Theirs: 18bdf79; `descriptor/host.rs:115-116, 1594-1595`; `draw.rs:637-650` (`BufferSweep`), `:889-890`, `:2076`, `:2089`, `:2100`.
- Ours has the old shape in all four places:
  - `draw.rs:1190-1196` `static RANGES` keyed `ptr::from_ref(host) as usize`; also `HEAP_SPANS` `:1299-1303`,
    `HEAP_UNORDERED` `:1335-1339`, `heap_pass::slots(ptr, …)` `:1204`, `aside` key `:2502`. None is cleared when a heap
    dies, so each entry pins `Arc<Shadow>` (host copies of title buffers) until the address is reused;
  - `draw.rs:2460-2461` `const ONCE = usize::MAX - 0x5e; list.first_use(ONCE)` — one key per list whatever the heap;
  - `draw.rs:2477-2479` `static FRAME` — one "once a frame" for all heaps. In ours `heap_textures` is called behind
    this same gate (`draw.rs:2520`), so with two view heaps in a frame the second gets neither its buffer pass nor its
    texture pass that frame;
  - `draw.rs:2509-2513` the aside closure looks the heap up by address with no identity check.
- Change: add `buffer_sweep: Mutex<BufferSweep>` and `buffer_frame: AtomicU64` to `HostHeap`
  (`descriptor/host.rs:16`, constructor `:1144`); `BufferSweep { passes, ranges, whole }` replaces `RANGES` and `WHOLE`
  (`draw.rs:1350`); key the list with `gpu as usize | 1`; swap `host.buffer_frame` instead of the static; add
  `.filter(|h| h.strides_gpu() == Some(gpu))` in the closure. Keep `HEAP_SPANS`/`HEAP_UNORDERED` as they are or move
  them into the same struct (they are read by `heap_buffer_writes` `:2560` and the every-list path `:2482`).
  Our `Ranges` is a `HashMap<u32, (Arc<Shadow>, u64, u64, bool)>` (theirs a `HeapRanges` vector): keep ours.
- Switch: theirs none. Ours: none needed for the struct move (same behaviour with one heap); if wanted,
  `VICTORY_HEAP_SWEEP_OWNED=0` to keep the statics.
- Verify: first count view heaps — log once in `HostHeap` construction (`descriptor/host.rs:1144`) how many non-sampler
  heaps were made in a run. One heap for the whole run: this is a no-op kept for safety. More than one: the line
  "buffer views of the heap made ready…" (`draw.rs:1393`) must appear per heap, and the memory line's host-copy bytes
  must fall after a heap is released.
- Picture: neutral or better (never worse). Shader cache: none.
- Claim: no separate number (part of the "directional" 64.2 → 54.8 ms pair).
- Size S (about 60 lines, 2 files). Risk low. Addresses: correctness (stale views after a heap is remade — a
  device-loss class), possibly memory (pinned host copies), possibly pop-in if the title uses two view heaps.
- **Freezes / pop-in: no effect with one heap.** Does not delay textures.

### H2-2 A slot whose upload could not be made stays due — HOST
- Theirs: ab75ac3 `draw.rs:2849, 2862, 2931, 2950` + `residency.rs:23-43`; f4b48ad `STAGING_DEFERRED`.
- Ours: `draw.rs:3119-3121` `readable(list, …)` records the upload; if `texture::staging()` (`texture.rs:2175`) returns
  `None` because `upload_memory` failed (`texture.rs:2242`), nothing is recorded, yet `draw.rs:3133`
  `staged(resource).unwrap_or((0, true))` → `:3164-3180` `texture_slot_done(index, write, None)`. **The slot is marked
  served with no upload: that texture stays empty until the title writes its descriptor or memory again.** Same at a
  failed host texture (`resource.rs:4047-4048` "no host texture").
- Change: (1) a small module (ours: `memory.rs` is taken by something else — put it in `host.rs` next to
  `available_physical_ram` `:140`): `static RETRY_UPLOAD_AT: AtomicU64`; `allocation_error(&windows::core::Error)`
  sets `now_ms() + 500` on `E_OUTOFMEMORY`; `upload_refused()`. (2) call `allocation_error` at `host.rs:10499`
  ("no upload buffer"), `resource.rs:3834` ("no heap for plain textures"), `resource.rs:4048` ("no host texture"),
  `tiles.rs:176`. (3) in `heap_textures`, after `readable` (`draw.rs:3121`): if `!tabled &&
  crate::texture::staged(resource).is_none()` or `upload_refused()` → `host.texture_slot_later(index, write, frame,
  most)` (`descriptor/host.rs:1438`) and `break`/`continue` instead of falling through to `texture_slot_done`.
  Note `staged()` is also `None` for reasons other than a failed allocation (layout not staged): test with a
  thread-local "staging failed" flag set in `texture.rs:2242` (their `note_staging_deferred`), not with `staged()` alone.
- Switch: propose `VICTORY_UPLOAD_RETRY=0` (on by default: it only changes what happens after a failure).
- Verify: `grep -c "no upload buffer\|no heap for plain textures\|no host texture"` in a run log. Zero today → no
  visible change (then it is insurance only). Add a counter next to `HEAP_TEXTURES_FULL` (`draw.rs:2831`).
- Picture: improves (a texture that failed once comes back) — only in runs where an allocation fails.
- Claim (journal 15:53-16:07, RTX 5090): before, "E_OUTOFMEMORY (0x8007000E) for a series of 64 MiB pool requests and
  individual uploads as small as 24,080 bytes. Rust subsequently aborted on a 786,432-byte allocation"; after, zero.
- Size S (40-60 lines, 4 files). Risk low. Addresses: texture pop-in that never heals / out-of-memory aborts.
- **Freezes: none. Pop-in: removes a permanent case, adds no delay in the normal case.**

### H2-3 Growth back-off after a refused block (500 ms) — HOST
- Theirs: ab75ac3 `pool.rs:201, 206, 248-262, 278-287`; test `pool.rs` `a_failed_block_is_not_retried_for_each_small_upload`.
- Ours, same defect in two places:
  - staging blocks `texture.rs:288` `let (buffer, data) = host.upload_buffer(BLOCK)?;` — on failure returns `None`
    for this texture (it does not even fall back to `own()`), and the next texture asks the driver for 64 MiB again;
  - texture heaps `resource.rs:3834-3837` `CreateHeap` of 256 MiB fails → `return None` → next texture tries again.
- Change: a `static RETRY_GROWTH_AT: AtomicU64` per pool. Before `host.upload_buffer(BLOCK)`: if `now_ms() <
  RETRY_GROWTH_AT` → `drop(blocks); return own();`; on failure store `now_ms() + 500` and `return own()` (a small
  buffer of the texture's own size usually still fits). Same before `CreateHeap` at `resource.rs:3834`: within the
  back-off behave as `NoRoom::Existing` / return `None` (the caller makes a committed texture).
- Switch: propose `VICTORY_GROWTH_BACKOFF_MS` (500; 0 = as before).
- Verify: port their test against `upload_memory` with a failing `Host` is not possible without a device; unit-test
  the gate function alone (eight sizes 704008, 179720, 179720, 24080, 48648, 524288, 89616, 351760 → one creation
  attempt). In a log: at most two "no upload buffer" lines of size 67108864 per second.
- Picture: neutral (free ranges and own buffers still serve). Shader cache: none.
- Claim: reproducer "made eight block creation attempts in one burst", now one. No frame time.
- Size S (25 lines, 2 files). Risk low. Prerequisite: none (H2-2 shares the 500 ms clock but is independent).
- **Freezes: shortens a stall only while allocations are being refused (each refused 64/256 MiB creation is a driver
  call). No effect otherwise. Does not delay textures.**

### H2-4 Commit-aware idle trim (cheap half of M6) — HOST
- Theirs: 8cc31ab `texture.rs:527` (`ram_idle_ms(residency::headroom())`), `host.rs:374-379`; 18bdf79 `pool.rs:44-64`.
- Ours: `ram_idle_ms(available_physical_ram())` at `texture.rs:980`, `host.rs:1865`, `pipeline/library.rs:380`,
  `shadow.rs:2928`. `available_physical_ram` (`host.rs:140-148`) returns `ullAvailPhys` only. Windows refuses
  allocations on **commit**, not on free physical memory: a PC with 8 GiB free physical and 300 MiB commit left (their
  15:41 sample: "277 MiB of commit headroom and 7,648 MiB of free physical memory") still gets the 10 s idle time.
- Change: in `available_physical_ram` return `ullAvailPhys.min(ullAvailPageFile)` (or add `ram_headroom()` and use it
  at the four call sites). The thresholds of `ram_idle_ms` (`host.rs:176-191`: < 2 GiB → 2 s, < 4 GiB → 5 s, else 10 s)
  stay. Optional second step, their "release all spare staging under pressure": in `trim_pools` (`texture.rs:391`)
  when headroom < 2 GiB pass `idle_ms = 0, idle_frames = 4` for the spare buffers and empty blocks.
- Not needed from theirs: `reclaim(…, sweep)` itself — ours already trims empty staging blocks, spare buffers, scratch
  chunks and empty texture heaps by idle time at the once-a-second sweep (`host.rs:236-243`); and ours has no
  five-minute / 64-block keep to switch off (0 hits `VICTORY_KEEP_FREED`, `SPARE_KEPT`).
- Switch: propose `VICTORY_IDLE_BY_COMMIT=0` (on).
- Verify: trail lines "sweep: N spare staging buffers and M empty staging blocks let go" (`texture.rs:433`) appear
  earlier when commit is low; their test `budget_tick_reclaims_idle_blocks_without_reusing_live_or_resting_pieces`
  is for their `Pool` type and does not port.
- Picture: neutral. Size S (5-15 lines). Risk low; a pool trimmed sooner is re-made at the next burst (0.25-0.5 ms per
  buffer, `texture.rs:143-150`), so do not shorten the times beyond this.
- **Memory only (PC RAM). No effect on the card's 14.4 GB, on freezes or on pop-in. Does not delay textures.**

### H2-5 Commit floor for the index-picked texture pass (M3) — HOST — delays textures
- Theirs: ab75ac3 `residency.rs:45-80` (HEAD), `draw.rs:2583, 2813`; test
  `bake_and_lookahead_leave_room_for_live_game_allocations` (values changed at HEAD).
- Ours: `heap_textures` loop head `draw.rs:2974` (`SPENT >= bytes_at_most || began.elapsed() > span`); bake flag is
  known at `draw.rs:2615-2616, 2649` but `PASS_BUDGET` (`draw.rs:406`) is `(u64, u64)` — add the third field as theirs
  (`:2663` sets it, `:2691` catch-up, `:2950` reads it).
- Change: `admit_texture_pass(bake)`: `GlobalMemoryStatusEx` at most once per 32 ms, cached in an atomic; false if
  commit < 512 MiB (bake: 256 MiB) or `upload_refused()`. Test it in the loop condition at `:2974` (slots stay due:
  the loop just breaks). **Use the HEAD floors, not 4 GiB / 2 GiB.**
- Switch: propose `VICTORY_COMMIT_FLOOR_MB` (default **0 = off** until a run shows commit near the floor; their
  default is on).
- Verify: a counter "texture pass held for commit" in the frame line; free commit in `framelog` (`framelog.rs:555`).
- Picture: **while it is active, textures picked by index are uploaded later = pop-in. It trades picture for not
  aborting.** Acceptable only as an abort guard at a floor we do not normally reach; as a default with the 4 GiB
  figure: **not acceptable: trades picture** (the author found the same and lowered it).
- Size S (40 lines, 2 files). Risk: medium if the floor is set high. Prerequisite: H2-2 (shared clock).
- **Prevents out-of-memory aborts only. Does not reduce freezes. Delays textures when active.**

### H2-6 Per-frame staging cap under pressure (M1) — HOST — delays textures
- Theirs: 18bdf79 `residency.rs:151-159`, `draw.rs:2648-2658`; test
  `prefetch_respects_existing_budgets_and_resumes_after_recovery`.
- Ours: budget `draw.rs:2807-2810` (192 MiB fixed, `VICTORY_TEXTURE_STAGE_MB`), catch-up x3 `:2918-2919`, own budgets
  `:2950-2955` (early 64 MiB `:2631`, bake 2 GiB `:2629`, catch-up turn 48 MiB `:2691`). The cap would go right after
  `:2955`, skipped for a bake and when `VICTORY_TEXTURE_STAGE_MB` is set. Needs `live_staging_bytes()` = our
  `STAGED[1]` (`texture.rs:381`) and a pressure flag (their hysteresis 3/4 GiB in, 5/6 GiB out, `residency.rs:98-104`).
- On our PC `limited_host_ram()` is true (32 GB reports under 32 GiB), so their middle clause "limited and >= 1 GiB of
  live staging → 32 MiB (HEAD 64 MiB) a frame" would apply at every world load: our run dev-3-rawoff had 1,192 MB of
  staging alive at the world's first frame (`texture.rs:192-194`). That is 1/9 (HEAD: 1/3) of our catch-up budget at
  exactly the moment 22,000 slots are due (`draw.rs:2912`).
- Switch: propose `VICTORY_STAGE_PRESSURE_MB` (default **0 = off**).
- Picture: **not acceptable as a default: trades picture** (longer black/low-detail stretch after a load). Port only
  the "critical" clause (commit < 1.5 GiB or a failure in the last 500 ms → 16 MiB), and only if H2-5's measurement
  shows we get there.
- Claim: none of its own.
- Size S (30 lines). Risk: picture. Prerequisites: H2-2, H2-5.
- **Prevents aborts only. Shorter texture pass while active (the pass is on a recording thread, so frames get
  shorter), but that is the pop-in trade, not a fix for freezes.**

### H2-7 XMA: whole blocks only, and an ended voice starts again — HOST (victory-hle)
- Theirs: 8cc31ab `victory-hle/src/modules/audio.rs:519-530, 560-573`.
- Ours: `victory-hle/src/modules/audio.rs:519-523` (one `decode`, then a block is written with `unwrap_or(0)` padding
  at `:530`) and `:545-548` (`if voice.ended { return false; }`; `ended` is set at `:560` and reset nowhere — only at
  voice creation `:429`). So in ours a voice whose two inputs ran dry once is silent for the rest of its life even if
  the title refills it, and every decode step that yields no samples writes a block of partly zeros.
- Change: copy both hunks as they are (21 lines). Keep `set_field(2, 26, 6, 0)` with **6** bits (it clears the state
  and the ended bit together; the line that sets them is ours `:562-563`).
- Base hunks ours also lacks, same functions, take them together: AnoHunted 64c3178 (`let write =
  context.field(0, 27, 5) % size;` ours `:523`) and 7e4bb28 (`if write == context.field(9, 0, 5) % size` ours `:536`;
  "a stream that loops would keep this voice (and its lock) filling forever").
- Switch: theirs none. Ours: `VICTORY_XMA_RESTART=0` if an A/B is wanted.
- Verify: their test at `audio.rs:1255-1264` (26 blocks for six frames + tail; `INPUT_INVALID` after) still passes;
  add one that refills an input after the end and expects more blocks. In a run: voices' `produced` in the audio trace.
- Picture: none (sound: fewer gaps, voices that come back). Size S (25 lines + 2 one-liners). Risk low — the 8-step
  bound is there so a broken looping voice cannot hold its lock. Addresses none of (a)-(d); correctness.
- **No effect on freezes or textures.** (A voice lock held forever, 7e4bb28, could stall the audio thread, not the frame.)

### H2-8 Movie planes from a per-list upload buffer — HOST
- Theirs: 8cc31ab; all of M7. Dependencies ours has: `take_chunk`/`give_chunks` (`host.rs:1895, 1921`), `read_into`
  (`host.rs:10304`), `copy_mapped` (`:10157`), `with_read` (`texture.rs:874`), `Step` (`host.rs:2603`), `Deferred`
  (`:2628`), `defer_texture` (`:9594`). Differs: `watch::title_memory_reading` takes `(at, len)` in ours
  (`watch.rs:2133`). Ours has no `card_steps.rs` / `place.rs`, so `Step::Plane` needs arms only in `host.rs`:
  `blocked_by` `:3672` (as `Step::Texture`: a read of `[at, at+len)`), the walk at `:9610`, the runner `:9670-9706`.
- Ours today: `upload_from` `texture.rs:1118-1215` uses the shared staging buffer for every texture; a linear one-level
  texture is hashed at every check (`texture.rs:1528-1533`, `VICTORY_TEXTURE_UNSEEN`, myokex c3723f3).
- Change: add `Staging::movie_plane()`, the early branch in `upload_from` before the "uploaded and not dirty" return
  (`texture.rs:1163`), `HostList::plane_upload`, `Step::Plane`, `Deferred.plane_uploads` + drop. Their refactor of the
  copy loop into `record_upload(list, resource, texture, staging, source, base)` is needed (ours: `:1177-1215`; ours
  uses `staging.view_first + i`, keep that).
- Switch: theirs none. Ours: `VICTORY_MOVIE_PLANES=0`.
- Verify: intro movie and in-world screens show no old or torn frame; `PREPARING[2]/[3]` (copy time/bytes) get the
  plane's bytes; hash time for planes disappears from the texture check.
- Suspicious: `movie_plane()` is true for **every** linear single-mip R8 2D title texture, not only a decoder's
  planes (glyph or mask pages would qualify). Such a texture is then re-read and re-uploaded by every list that
  samples it, every time, with no "unchanged" skip. Only the luminance-format test stands between a font page and
  this path; chroma (R8G8) planes are not covered at all. Log which textures take the path before trusting it.
- Picture: better for movies; neutral elsewhere. Size M (about 170 lines, 2 files). Risk medium (new step kind in the
  submission ordering). Addresses none of (a)-(d) in the world; relevant to the "rectangular gaps in credits art" /
  intro only if those are movie planes (not shown).
- **No effect on streaming freezes or world pop-in.**

### H2-9 First-scene and bake pipelines made at creation — HOST
- Theirs: 8cc31ab `pipeline/make.rs:89-102, 117-120, 252-260`.
- Ours: `pipeline/make.rs:262-267` `early()` starts a pipeline at creation only if an earlier run drew with it
  (`library::wanted_early`), class `EARLY`. `BAKE_PIXEL_SHADERS` is private at `draw.rs:436` (same four hashes).
- Change: `scene_input(&shaders)` (six hashes below or a bake pixel shader) → `early()` starts it with `REQUESTED`
  (`make.rs:369`) even on a first launch. The other half (`screen = … || scene_input`, mesh-shader exclusion) depends
  on base code ours lacks: `screen_pass` / `screens_first` / `SCREEN_WORKERS` (a989668, shortlist row 10, 0 hits
  ours) — skip that half.
- Switch: propose `VICTORY_SCENE_INPUT_EARLY=0` (on).
- Verify: on a cold shader cache the first frames of the opening scene have no "pipeline not ready" drops for these
  six programs; `library::EARLY` counter rises by their number.
- Picture: better on a cold cache only (first launch). Warm cache: no change — `wanted_early` already covers it.
  Size S (20 lines). Risk low. **No effect on freezes/pop-in in our warm runs.**

### H2-10 Unwatched-copy throttle map no longer global or growing — HOST
- Folded into H2-1 if `WHOLE` (`draw.rs:1350-1373`) moves into the per-heap struct. At the default
  `VICTORY_UNWATCHED_FRAMES=1` theirs skips the map (`if every > 1 && shadow.unwatched()`): ours takes a global mutex
  and inserts per unwatched copy per pass, and the map keeps one entry per copy id ever seen. Size S (10 lines).
  Saves microseconds; **no effect on freezes or pop-in**.

---

## 3. Hunks not worth porting

| Hunk (theirs) | Why not |
|---|---|
| `pipeline/make.rs:53-87, 224, 242, 253` `left_out`: `CS_RaytracingResolve` (~1,000 programs) and `CS_ScreenProbes_ActivateRays` never made unless `VICTORY_BVH_BUILDER=1` / `VICTORY_RT_RESOLVE=1` / `VICTORY_ACTIVATE_RAYS=1` | Already listed, see `fork-review-shortlist.md` line 108 (Neptune row 6). It drops passes. With the structure builder off they shade nothing, but our open defect "unlit areas" is to be fixed by building the structures, and theirs makes the skip the **default** (Neptune made both by default). As a default: **not acceptable: trades picture**. Ours has `VICTORY_BVH_HOLD` (`pipeline.rs:333`) for the same dispatches, off by default. |
| `pipeline/make.rs:474-483, 488-497` workers 3/4 of processors, 2 to 8, on <= 32 GiB | Ours is already `(n / 2).clamp(2, 8)` = 8 on this PC (`pipeline/make.rs:451`); theirs gives 8 too. 0 hits `VICTORY_PIPELINE_WORKERS` in ours; not needed. |
| `draw.rs:2528-2532` base budget capped at 128 MiB on <= 32 GiB | Ours is a fixed 192 MiB (`draw.rs:2809`). Lowering it delays textures: **not acceptable: trades picture**. |
| `host.rs:379` no five-minute keep, `host.rs:1935` no 1 GiB scratch keep on <= 32 GiB | Ours never had the keeps (0 hits `keep_freed`, `SPARE_KEPT`): idle times are 2-10 s (`host.rs:176`), scratch cap 128 MiB (`host.rs:1786`). Already the conservative behaviour. |
| `host.rs:500` `VICTORY_BURY_LAG` 16 instead of 64 on <= 32 GiB (seen in HEAD, e05d463) | Outside these three commits; 0 hits `BURY_LAG` in ours. |
| `texture/ahead.rs:107-108` uploads-ahead cap 32 MiB and refusal under pressure | Ours has no upload-ahead (0 hits; shortlist line 322: "measured at nothing"). |
| `pool.rs:44-64` `room()` reclaim with `try_lock`, `pool.rs:311` `reclaim` | Their `Pool` type; ours trims by idle time already (H2-4 takes the one useful part). |
| `draw.rs:224` `BAKE_PIXEL_SHADERS` made `pub(crate)` | Only for H2-9. |
| `host/card_steps.rs:204, 444`, `host/place.rs:122` `Step::Plane` arms | Files do not exist in ours ("card steps": do not port, shortlist line 266). |
| `draw.rs` (8cc31ab) `prefetch_limit` at the base budget | Superseded by 18bdf79 within 20 minutes. |
| `docs/bug-journal.md` | Notes only (quoted above). |

---

## 4. Answer per item: freezes, pop-in, or aborts only

| Item | Fewer streaming freezes? | Less pop-in / textures ready sooner? | Delays textures? | What it really does |
|---|---|---|---|---|
| Staging admission 16/32 (HEAD 16/64) MiB a frame | no | no — more | **yes** | less RAM held in staging while the PC is short of commit |
| Pool growth back-off 500 ms | only during a burst of refused allocations | no | no | stops repeated failing driver calls |
| Commit floor 4/2 GiB (HEAD 512/256 MiB), sampled every 32 ms | no | no — more while active | **yes** | leaves commit for the game's own allocations |
| E_OUTOFMEMORY → 500 ms pause, slot stays due | no | yes, for textures whose upload failed (ours loses them for good) | 500 ms for those | retry instead of loss |
| Bindless sweep per heap, stride identity | no | only if the title has more than one view heap | no | correctness after a heap is remade |
| Trim of free blocks at the tick | no (slightly more re-allocation) | no | no | returns RAM / card memory sooner under pressure |
| Movie-plane upload buffers | no | no (movies only) | no | no torn/old movie frames, no hashing of planes |
| XMA fixes | no | – | – | sound |
| make.rs scene inputs | no | first launch only | no | pipelines ready on a cold cache |
| make.rs left-out RT programs | shorter first load | – | drops passes | not acceptable as default |

Nothing in these three commits addresses bound (d) as we have it (card memory full, textures not ready when bound).
The freezes on their PC were the PC running out of commit; ours are streaming bursts against a full card.

---

## 5. Game facts found in these hunks

- Six shader hashes the author calls "confirmed first-scene producers" (cutscene lighting, tiling, bakes), by the
  first 16 hex digits of the program hash: `970c4fbb3b017c0e`, `068c90badc86f1fd`, `e4a7e088c7780ce8`,
  `ebe4e53f031f5249`, `2cc4c197ee29e291`, `369cbe906ea72764` (`pipeline/make.rs:93-98`; no comment says which is which;
  0 hits for any of them in ours).
- The four decoration-bake pixel shaders `c0a7fe77b1862e10`, `0d1575a396dfe513`, `b7cfd8dda91ebb6b`,
  `280967ad158cee0b` (`PS_BakeDecorations_*`), drawing "into a 256x256 corner of 2048x2048 targets" and naming no
  texture in a table (`draw.rs:220-225`; ours has the same list at `draw.rs:436`).
- "Without BVH, ~1,000 CS_RaytracingResolve shaders shade no hits while clogging the cold compiler queue"
  (`pipeline/make.rs:66-67`); `CS_ScreenProbes_ActivateRays` is the ray activation program (`:81`).
- The video decoder "cycles CPU-written linear plane buffers": R8, one level, linear, the same memory reused for
  successive frames (`texture.rs:800-802`).
- XMA context fields as used: word 0 bits 20-21 = input buffers valid, bits 22-26 = ring size in blocks, bits 27-31 =
  write block; word 1 bit 31 = decoder may write; word 2 bits 0-25 = bit offset in the input, bits 26-30 = state
  (4 = `INPUT_INVALID`), bit 31 = ended; word 4 bit 31 = current input; word 9 bits 0-4 = the title's read block.
  "The title can refill either input while the context stays enabled" (`audio.rs:562-564`).
- Sizes of streamed texture uploads seen in the author's failing run: 704,008; 179,720; 24,080; 48,648; 524,288;
  89,616; 351,760 bytes (`pool.rs` test, from the pasted log).
- Process figures on the opening save, Series X path, RTX 5090: peak commit 26-34 GiB, whole-adapter use 17.5-19.1 GiB,
  live texture staging 2.9-4.2 GiB at the moment of pressure (journal 15:41-16:07). Our card has 16 GB: their
  configuration does not fit it.
- "The renderer logged a dropped draw due to exhausted dynamic descriptor slots" in both their confirmation runs
  (journal 16:00, 16:04) — an open defect of theirs, not fixed in these commits.

---

## 6. Coverage

Commits: 3 of 3. Every non-`victory-shader` hunk read in full from `git show <c> -- crates/victory-gpu
crates/victory-hle docs`:

| Commit | Files (hunks) read |
|---|---|
| 8cc31ab | `draw.rs` (2), `host.rs` (6), `host/card_steps.rs` (2), `host/place.rs` (1), `pipeline/make.rs` (8), `texture.rs` (8), `texture/ahead.rs` (1), `victory-hle/src/modules/audio.rs` (2) |
| 18bdf79 | `descriptor/host.rs` (2), `draw.rs` (9), `pool.rs` (4), `residency.rs` (2), `docs/bug-journal.md` (new, 11 lines) |
| ab75ac3 | `draw.rs` (6), `host.rs` (1), `pool.rs` (6), `residency.rs` (2), `resource.rs` (1), `docs/bug-journal.md` (+4 lines) |

Also read at HEAD for final values and line numbers: `residency.rs` whole, `pool.rs:1-80, 248-345, 380-400`,
`draw.rs:2060-2105, 2505-2665, 2806-2960, 4865-4885`, `host.rs:355-450, 495-503, 922-945, 1925-1940`,
`texture/ahead.rs:20-40, 96-118`, `audio.rs:510-610`.

Not read: the `victory-shader` hunks of 8cc31ab (other agent); the f4b48ad diff itself beyond the functions named
above (other agent). Not verified: whether any of our runs logs a refused allocation or low commit (no run log was
found under `victory-data`/`victory-run` by name; the two `run-9x.out` files there are 0.5 KB), and how many view
heaps the title makes.
