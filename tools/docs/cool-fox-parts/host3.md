# cool_fox review, HOST part 3: codec_d commit f4b48ad, non-shader hunks

Scope: `git show f4b48ad -- crates/victory-gpu Play-VIctory.cmd README.md docs` in
`C:\GTAVI\Tools\ref-forks\cool_fox` ("Publish the pareto-residency Windows build and portable launcher",
codec_d, 2026-11-25 22:33 -0800). Read-only review: git show / grep / read only. Nothing built or run.

Line numbers: "theirs" = cool_fox HEAD f4b48ad. "ours" = `C:\GTAVI\Tools\victory-dev\crates` as read on
2026-11-26; other agents edit that tree, so our lines may drift by a few (one drifted by 9 during this review).

Text addressed to AI assistants in the scope: none found. (Base commit 9a6c829 carries a
`Co-Authored-By: Claude Opus 5.5` trailer; not an instruction.)

Contents
1. Verdict in short
2. Journal claims checked against code (16:50 - 22:28)
3. Port entries, in the order to take them (H3-1 ... H3-8)
4. Hunks not worth porting, and why
5. Play-VIctory.cmd: every launch setting
6. README claims
7. Game facts
8. Coverage list

---

## 1. Verdict in short

- The commit is mostly repairs of regressions that the author's own base stack created
  (`VICTORY_THIN` worker threads, `VICTORY_SHOW_AFTER` grey stand-ins, `VICTORY_UPLOAD_AHEAD` copy queue,
  `VICTORY_BURY_LAG`, the commit-headroom gates of ab75ac3). victory-dev has none of these five
  (0 hits each), so about two thirds of the +1,200 host lines have nothing to attach to in our tree.
- What does apply to us, by value:
  1. bake uploads must be exact (our `VICTORY_TEXTURE_NO_WAIT` is ON by default and a bake is one-shot):
     H3-1, H3-2. Direct candidate for "a head left black, a hand of noise" and black NPC faces.
  2. bake detection is wider than our four pixel-shader hashes: H3-3.
  3. one oversized indirect record empties the rest of its list: ours has the `REST=1` behaviour
     hard-wired, with no switch and no counter: H3-4. Candidate for far-plant flicker / bare palms.
  4. dynamic descriptor slots: ours does NOT have the 16-submission pinning (no bury lag). Only a small
     tightening is left: H3-5.
  5. strides table with view-kind bits for table views (host half of the facial deformation fix): ours
     solves the same defect another way (`half_view`); the port is optional and needs the translator: H3-6.
- Not acceptable for our goal: `--console series-s`, `VICTORY_AMBIENT=0.8`, `VICTORY_AMBIENT_AIR=1.5`
  (launcher; section 5). `VICTORY_SHOW_AFTER`, `VICTORY_LEVELS_ON_DEMAND`, `VICTORY_CARD_STEPS=2` trade
  "later data" for stability / memory / speed.
- No fps figure is attached to anything in f4b48ad. The only timing figures in the journal belong to
  earlier commits (section 6).

---

## 2. Journal claims checked against code

`docs/bug-journal.md` (19 lines; f4b48ad adds lines 16-19).

| Time | Claim | Found in code (theirs) | True? |
|---|---|---|---|
| 16:50 | `SLOT_TABLE_STRIDES = 252` | `victory-shader/src/emit/shader.rs:31` | yes |
| 16:50 | `heap_strides` "in descriptor/host.rs" | it is `State::heap_strides`, `draw.rs:2125`; bound at `draw.rs:1750-1767` | yes, wrong file named |
| 16:50 | dynamic descriptor slot stride population | `descriptor/host.rs:2765 write_stride`, called from `make_at` 2803, `make_listed` 2853, `make_null` 2878, raw / constant / null helpers 2965, 2975, 3010, 3040, 3098, `texture_slot_gone` 2552 | yes |
| 16:50 | integer branching only on `_d16` loads / stores | translator (other agent); host side has no gate | n/a here |
| 19:15 | `has_upload_reserve` 256 MiB bake / 512 MiB non-bake | `residency.rs:58-63` (was 2 GiB / 4 GiB) | yes |
| 19:15 | `has_growth_reserve` 256 MiB bake / 1024 MiB speculative | `residency.rs:65-70` (new) | yes |
| 19:15 | pressure entry 3 GiB RAM / 4 GiB commit, recovery 5 / 6 | `residency.rs:98-104` (was entry 4 / 6, recovery 6 / 8) | yes |
| 19:15 | 64 MiB staging-block growth gate scoped to speculative bindless, "`REST = 2500 ms`" | gate: `pool.rs:389-395`, `texture.rs:1893-1898`. `BLOCK = 64 << 20` `pool.rs:30`. `REST = 2500` `pool.rs:76` is an unchanged base constant (avatarmanz 9fea743), not part of this change | gate yes; REST is only context |
| 19:15 | `BAKE_GEOMETRY_SHADERS`, `ahead::drain`, `BAKE_PAGE_WRITTEN` | `draw.rs:258`, `texture/ahead.rs:155`, `descriptor/host.rs:764` | yes |
| 19:15 | `BAKE_UPLOAD` / `execute_sync_internal`, `prepare_bake_slot` / `publish_uploaded_slot` | `draw.rs:230`, `host.rs:5135`, `descriptor/host.rs:2401`, `2413` | yes |
| 20:50 | alias flood reverted | no trace in HEAD (`REMAPPED` takes no alias ranges) | consistent |
| 21:30 (2) | `Detached::release_tables()` at GPU completion | `list.rs:209`; called in `host.rs:4926-4928` (`relay_land`) and `5404-5406` (`settle`) | yes |
| 21:30 (2) | `bury_lag()` = 16 submissions | `host.rs:497-503`: 16 only when total RAM <= 32 GiB (`limited_host_ram`), else 64. `land()` `host.rs:3248-3257` keeps the newest `bury_lag()` landed flights unrecycled: that is the pinning | yes |
| 21:30 (2) | `settle_for_fence()` retry | three sites in `draw.rs`: 1926-1929, 2000-2003, 3762-3765 | yes |
| 21:30 (2) | "262k dynamic slot pool" | `DYNAMIC = 1 << 18` `descriptor/host.rs:1483`; default most is `DYNAMIC << 2` when the heap has room (`dynamic_slots`, 1491-1503) | yes for a capped NVIDIA heap |
| 21:30 (2) | `VICTORY_INDIRECT_REST=0` default | `indirect.rs:300-302`: on only when `=1` | yes |
| 21:30 (3) | `critical_commit()` < 1536 MiB or upload refusal | `residency.rs:144-147` | yes |
| 21:30 (3) | 64 MiB/frame prefetch and one spare staging block under mild pressure | `residency.rs:151-159` (16 MiB only when critical), `pool.rs:55-62` (limit 0 critical / 1 pressure) | yes |
| 21:30 (3) | `VICTORY_UPLOAD_AHEAD` stays active under mild pressure | `texture/ahead.rs:108`: gate is `!allow_staging_growth(false)` (>= 1024 MiB commit) instead of `pressured()` | yes |
| 21:30 (3) | pending bake slots never overwritten with `neutral()` | `prepare_bake_slot` only reads the record; `mirror_after` skips while `BAKING` (`descriptor/host.rs:2292`); `shows_other` / `late` are off for a sync bake (`draw.rs:2823-2835`) | yes |
| 21:30 (3) | false `address=0x0` unordered-SRV warning removed | `descriptor/host.rs:4342-4363`: warns only `if *unordered || at != 0` | yes |
| 21:30 (4) | `VICTORY_AMBIENT_AIR=1.5` in the launcher | `Play-VIctory.cmd:18` | yes |

---

## 3. Port entries (take in this order; each is independent unless a prerequisite is named)

### H3-1. Bake uploads read title memory exactly (`exact` staging flag)  -- coder: HOST

Kind: visual / correctness. Addresses: black NPC faces, "a head left black, a hand of noise", any one-shot
bake that ran while its sources were being remapped. Does not touch bounds (a)-(d) outside bake frames.

What theirs does (f4b48ad, `texture.rs`):
- `Staging.exact: AtomicBool` (`texture.rs:94-98`), set when the staging is made while the thread-local
  `BAKING` is set (`:1941`) and again at every `upload_from` under `BAKING` (`:764-767`).
- `upload_from` (`:822-825`): `if !baking && staging.uploaded && !staging.dirty { defer; return }`. Under a
  bake the copy is always recorded into the bake's list, so "what the submission-time check finds changed"
  lands before the bake draw executes instead of one list later.
- `Staging::refresh` consumes the flag once (`let exact = self.exact.swap(false)`, `:1268`) and with it
  bypasses every shortcut that may leave old bytes:
  - `:1270` `alone && !exact && last.hash.is_some()` (one-shot staging already filled),
  - `:1274` write-watch "nothing written since asked",
  - `:1318` guard says unwritten and it is not the texture's hash turn,
  - `:1337` `if no_wait() && !exact` -> a bake WAITS for `title_memory_reading` (the runtime's remap lock)
    instead of reading through the kernel "whole or not at all",
  - `:1357` the 30-frame throttle on partly mapped textures.
- `BAKING` itself (thread-local `Cell<bool>`, `texture.rs:663`) comes from base 8d79927; set around the bake
  pass in `draw.rs:2334/2363` only when `bake_exact()` (`VICTORY_BAKE_EXACT`, default on, `draw.rs:196`).

Ours today:
- `no_wait()` is ON by default (`victory-gpu/src/texture.rs:821-823`; theirs is off by default and the
  launcher turns it on). So a texture whose pages are being remapped at the moment of a bake is "left as it
  is and read at its next refresh, a frame later" (`:1544-1556`) -- the bake has already run by then and its
  result stays for the whole scene.
- same shortcuts as theirs without an escape: `:1164` (uploaded and not dirty), `:1525-1535` (unwritten),
  `:1570` (partial, 30 frames).
- grep: `BAKING` 0 hits, `bake_exact` 0 hits, `exact` flag 0 hits.

Port:
1. `texture.rs` near `:865` (thread_local block): add `pub(crate) static BAKING: Cell<bool>`.
2. `Staging` struct: add `exact: AtomicBool`; init `BAKING.get()` in `staging()` (`:2175`).
3. `upload_from` (`:1118`): set `staging.exact` when `BAKING`; change `:1164` to `if !baking && ...`.
4. `Staging::refresh`: `let exact = self.exact.swap(false, Relaxed)`; add `!exact &&` at `:1533`, `:1570`,
   and `if no_wait() && !exact` at `:1544`. (Ours has no `alone` / `once` path: skip that one.)
5. `draw.rs:2663-2665` (`early_textures`): `BAKING.set(bake && bake_exact())` around `heap_textures`.

Switch: theirs `VICTORY_BAKE_EXACT` (default on, `=0` as before). Ours: same name, default on.
Verify: log line "a draw of the title's decoration bake ..." (ours `draw.rs:2659`) still appears; add a
counter of exact refreshes to that line; picture check = NPC heads and hands in the shots that are black
today. Frame cost shows as "textures" time in bake frames only (the wait for the remap lock: their base
measured 1.3-3.7 s worst case for ALL refreshes waiting, which is why NO_WAIT exists; here only bake sources wait).
Picture: improves. Shader cache: none. Author's claim: no number. Size S (about 40 lines, 2 files).
Risk: low-medium: a bake frame can stall on the remap lock (bounded by `VICTORY_BAKE_TEXTURES_MS`, 1500).
Tests to port: none specific in f4b48ad.

### H3-2. Pass before a bake also takes slots that are due only in the next frames  -- coder: HOST

Kind: visual. Base dependency (avatarmanz 8d79927, 2dfba4d; not codec_d) that H3-1 needs to be complete.

Theirs (`draw.rs`):
- `BAKE_AHEAD: [u64; 2] = [12, 2]` (`:213`): frames ahead whose due slots the bake pass takes; 12 at the
  frame's first bake, 2 at later bakes of the same frame ("a look reads 48 KiB, and a frame bakes up to
  sixteen times").
- `heap_textures` (`:2584-2593`): `texture_slots_due(frame, if baking { 1 << 16 } else { 4096 })`, then the
  slots due up to `frame + BAKE_AHEAD[again]` are appended (dedup by index).
- `BAKE_SERVED` (`:218`, used `:2340-2345`, `:2385`, `:3044`): `texture_news().0 + 1` when a bake pass
  served everything; the next bake draws of the frame skip the pass until a descriptor write, a remap or a
  page write is noted (`HostHeap::texture_news`, `descriptor/host.rs:1805`).
- Reason given (`draw.rs:186-195`): the pass looks at a slot whose memory was written or mapped "a frame or
  two later ... a bake in between took the bytes from before (another texture's), and what it bakes stays:
  coloured specks all over the waitress's face for the whole diner scene (run n01)".

Ours: `heap_textures` (`draw.rs:2789`) takes `texture_slots_due_aged(frame, 4096 or 512, ...)` (`:2855`);
no look-ahead; written slots are due one frame on (`descriptor/host.rs:1546`, `after = 1` with
`VICTORY_UPLOAD_ON_WRITE` default) and remapped / recheck slots later (`RECHECK_FRAMES = [4, 12, 40, 120,
400]`, `draw.rs:972`). A bake in the same frame as the write does not see them. 0 hits for `BAKE_AHEAD`,
`BAKE_SERVED`, `texture_news`.

Port: in `heap_textures` when `BAKING`: cap 1 << 16 and extend `due` with
`texture_slots_due_aged(frame + 12 (first bake of the frame) / 2, ...)`; add `HostHeap::texture_news()`
(`(textures.writes, !REMAPPED.is_empty() || !PAGE_WRITTEN.is_empty())`, ours statics at
`descriptor/host.rs:396` and `:486`) and the `BAKE_SERVED` skip in `early_textures`.
Switch: under `VICTORY_BAKE_EXACT`. Verify: bake log line's `due_at_the_last_pass` grows at bakes; no
coloured specks / wrong-texture patches on baked skin. Picture: improves. Cache: none. Claim: "none in other
runs" (no number). Size S-M (about 60 lines, 2 files). Risk: low (more bytes uploaded in bake frames; byte
cap is already 2 GiB for a bake, `draw.rs:2629`).

### H3-3. Wider bake detection: 3 more pixel shaders, 2 geometry shaders, a shape rule  -- coder: HOST

Kind: visual. Addresses: black / grey NPC heads and clothing whose bake program is not one of our four.

Theirs (`draw.rs:244-258`, `:2264-2278`, f4b48ad):
- `BAKE_PIXEL_SHADERS` grows from 4 to 7: adds `fa455c8a3c32577b`, `d126bcae5d5d8d26`, `fcee612943c008e0`.
- `BAKE_GEOMETRY_SHADERS = ["089d6271be6650ef", "d4111a6e6ea49d91"]`: "vertex 089d6271be6650ef
  fullscreen-triangle ped baker and mesh d4111a6e6ea49d91 decoration projection mesh", so any pixel-shader
  permutation drawn with them counts.
- `is_bake_draw`: any stage hash in either list, OR `self.depth == 0 && self.targets.len() >= 4` and the
  first target is square with width >= 512 ("depth-less 4+ MRT square atlas bakes").

Ours: `BAKE_PIXEL_SHADERS: [&str; 4]` (`draw.rs:436-437`, the same first four hashes), matched on any stage
at `draw.rs:2615-2616`. 0 hits for the three new hashes, the two geometry hashes, `is_bake_draw`.

Port: extend the list, add the geometry list, add `fn is_bake_draw(&self, p)` next to `early_textures`
(`draw.rs:2610`) and call it at `:2615`. Needs `crate::resource::texture_kind(target) -> (w, h, _)` or our
equivalent for the target size.
Switch (ours, new): `VICTORY_BAKE_DETECT=hashes|shape` (default `shape`, `hashes` = the two lists only), so
the shape rule can be A/B'd: it can also match a non-bake pass (any depth-less 4-target square draw), and
every match may spend up to `VICTORY_BAKE_TEXTURES_MS` (1500 ms a frame in all).
Verify: the bake log line's `bakes = n` rises compared with a run on the old list; frame listing of a frame
with a black head shows one of the new hashes. Picture: improves. Cache: none. Claim: no number.
Size S (about 30 lines, 1 file). Risk: low for the hash lists, medium for the shape rule (frame time).

### H3-4. An indirect record that does not fit no longer empties the rest of its list  -- coder: HOST

Kind: visual / correctness. Addresses: flicker of far plants, bare palms, world pieces missing for a frame
(the shortlist already names this suspect: `docs\fork-review-shortlist.md:249`, "one bad indirect record
empties its list's budget -- `VICTORY_INDIRECT_REST=0` is their experiment").

Theirs:
- base 449830f made it a switch with a counter; f4b48ad flips the default (`indirect.rs:296-302`):
  `u32::from(std::env::var("VICTORY_INDIRECT_REST").is_ok_and(|v| v == "1"))`, compiled into the clamp as
  `#define REST` (`:309`).
- clamp shader (`indirect.rs:163-193`): per group of 64 records, in lane order:
  `next = (BUDGET - next < c) ? (REST ? BUDGET : next) : (next + c)`; a record that does not fit runs empty;
  with REST 0 it charges nothing and later records that fit still run. `BUDGET = 1u << 27`
  (vertex instances, or threads: groups x 64, mesh groups x 128), `MOST_DRAW = 1u << 23`, `GROUPS = 1u << 20`.
- diagnostics: budget buffer word 12 = records refused for want of budget, word 16 = largest cost
  (only costs >= 1 << 22) (`:194-202`); the frame line prints them.
- test: `four_records_of_a_group(rest)` (`indirect.rs:2604-2660`): draws of 100, 250, 50, 100 with 300 left;
  REST 1 -> only the first runs; REST 0 -> 100, 50, 100 run.

Ours (`victory-gpu/src/indirect.rs`):
- `:196-208`: `uint next = (BUDGET - before <= cost) ? BUDGET : (before + cost);` then
  `over = before >= BUDGET || BUDGET - before < cost;` -- a record that does not fit saturates the budget:
  exactly REST=1, hard-wired. Every later record of the same host list runs empty.
- 0 hits for `INDIRECT_REST`. Our budget words 12 and 16 are used for other counts (compute records cut,
  dispatches in parts, `:147-156`), so we have no "refused for want of budget" figure at all.
- CPU-issued records (`indirect.rs:1368`, `decide`): a record that does not fit goes to the clamp
  (`Way::Fallback(CLAMPED)`) without charging -- already REST=0-like; the clamp then applies the rule above.

Port (ours `indirect.rs:196-208`):
```hlsl
while (before < BUDGET) {
    if (BUDGET - before < cost) { if (!REST) break; }          // takes nothing
    uint next = (BUDGET - before <= cost) ? BUDGET : (before + cost);
    ...CAS as now...
}
over = before >= BUDGET || BUDGET - before < cost;            // unchanged
```
plus `#define REST` prepended where the source is compiled, `fn rest()` reading the switch, and two new
budget words (pick free offsets, e.g. 20 and 24) for "refused" and "largest cost", printed in the frame line
next to the existing indirect counts. Port their test with our WARP harness.
Switch: theirs `VICTORY_INDIRECT_REST` (now default 0). Ours: same name; default 0 after one A/B run with 1.
Why it is safe: total work of a list is still capped at 2^27 either way, and a record of >= 2^23 vertices an
instance already runs empty and charges nothing (ours `:133-137`).
Verify: new frame-line figure "records refused for budget" > 0 in the frames where plants vanish, then the
same scene with REST 0: later draws present; no device loss. Picture: improves. Cache: none (the clamp is
our own HLSL, not a translated shader). Claim: no number ("so one oversized indirect record does not zero
out subsequent draws"). Size S (about 40 lines + test, 1-2 files). Risk: low-medium (first run should watch
for the hangs REST was made against; base 449830f kept REST 1 as its default).

### H3-5. Dynamic descriptor slots given back at landing, and one settle-and-retry before a table is refused  -- coder: HOST

Kind: visual (dropped draws) / correctness. Addresses: flicker, missing world parts -- but see "Ours" first.

Theirs:
- The bug: `land()` (`host.rs:3248-3257`) recycles only the landed flights older than the newest
  `bury_lag()` ones (`VICTORY_BURY_LAG`, base a0b4909; 16 when RAM <= 32 GiB, else 64; made against page
  faults "in memory nothing of ours named any more"). `Detached._state` (the list's `draw::State`) owns the
  dynamic slots and frees them on drop, so up to 16 / 64 finished submissions kept their tables' slots:
  the 262,144-slot pool ran out, `host.dynamic(n)` returned `None`, the draw was dropped.
- The fix (f4b48ad): `Detached::release_tables()` = `self._state.tables_done()` (`list.rs:207-211`), called
  for every list of a flight in `relay_land` (`host.rs:4926-4928`) and `settle` (`:5404-5406`) right when the
  card has finished it; and at the three `host.dynamic(..)` call sites
  `.or_else(|| { crate::host::settle_for_fence(); host.dynamic(n) })` (`draw.rs:1926`, `2000`, `3762`).

Ours -- checked precisely:
- No bury lag: `bury(finished)` drops everything up to `finished` (`host.rs:263-276`); `land()` takes ALL of
  `line().landed` (`host.rs:4824-4829`); 0 hits for `bury_lag` / `BURY_LAG`.
- `Detached` (`list.rs:226-230`) holds `_state` and `recycle` drops it (`list.rs:319-321`); `State::drop`
  releases the slots (`draw.rs:793-799`).
- But landed lists are not recycled inside `settle` / `relay_land`: they are pushed to `line().landed`
  (`host.rs:7448`, `:7020-7024`) and recycled by the helper thread after a poke (`late_helper`,
  `host.rs:4862-4866`, `:4887`). So slots stay pinned from GPU completion until the helper runs: thread
  wake-up latency, not 16 submissions.
- `HostHeap::dynamic` (`descriptor/host.rs:1265`) already waits up to `VICTORY_DYNAMIC_WAIT_MS` (30) in
  200-microsecond sleeps (`:1310`) but never asks for a settle; once one table has timed out in a frame the
  frame's later ones are refused at once. Pool is a fixed `DYNAMIC = 1 << 18` unless `VICTORY_DYNAMIC_SLOTS`.
- Call sites without retry: `draw.rs:2309`, `2383`, `3966`. Dropped-draw warning `draw.rs:2017`.

So: the cool_fox defect is NOT in our tree. Before porting anything read our own log:
"descriptor tables that found no free slots, per frame: N waited (.. ms in all), M given up (their draws not
drawn)" (`host.rs:10651-10654`) and the once-warning "no free dynamic descriptor slots: a table is not made".
If M is 0 in the flicker scenes, this is not our flicker.

Port (only if M > 0 or the waits cost time):
1. `list.rs:233` impl: add `pub(crate) fn release_tables(&mut self) { self._state.tables_done(); }`
   (`State::tables_done` exists, `draw.rs:809`).
2. `host.rs:6991` and `:7430`: `Late::Run(mut flight)` and, after `card_done`,
   `for d in &mut flight.lists { d.release_tables(); }`.
   Check first: `tables_done` also clears `fresh_tables / ready_tables / seen_tables`; a list "cut at waits"
   keeps its state with its last part (comment `list.rs:188-190`) -- make sure a `Detached` here is never an
   earlier part of a list whose later part is still unsent.
3. In `HostHeap::dynamic`'s wait loop (`descriptor/host.rs:1308-1311`): on the first miss call
   `crate::host::settle_for_fence()` once (outside the `next` lock) before sleeping.
Switch (ours, new): `VICTORY_TABLES_EARLY=0` to keep today's behaviour.
Verify: the frame-line figures above; diagnostics build: "dynamic slots: N tables asked, M refused"
(`copystat.rs:1213`). Test to port: none new in f4b48ad for this (their `dynamic_slots_are_held_until_released`
already exists in ours, `descriptor/host.rs:5150`).
Picture: improves or neutral. Cache: none. Claim: none measured ("exhausting NVIDIA's 262k dynamic slot
pool and dropping draws"; journal 16:00 and 16:04: "The descriptor-slot warning recurred").
Size S (about 20 lines, 3 files). Risk: low-medium (a blocking settle on a recording thread; ours already
does that from `fence.rs:97`).

### H3-6. Strides table carries view kinds for table views (`SLOT_TABLE_STRIDES`)  -- coder: HOST half; needs the TRANSLATOR half

Kind: visual / correctness (facial deformation, "wrapped meshes"); possibly performance on bound (a).

The contract (theirs; the shader side is another agent's part):
- Root SRV `t252` in space 1 (`ROOT_CBV_SPACE`), a raw buffer: the SAME buffer as `SLOT_STRIDES` (`t255`):
  one upload buffer per view heap of `SLOT_STRIDES_COUNT = 1 << 21` dwords (8 MiB), mapped, written by the CPU.
- dword `i` describes HOST heap slot `i` (title slots and the dynamic slots above them):
  bits 0-29 `VIEW_STRIDE_BITS` = bytes per element (structured: the stride; raw: 1; typed: the format's
  element size); bit 31 `VIEW_INTEGER` = typed buffer view of a UINT / SINT format; bit 30 `VIEW_SIGNED` =
  SINT. 0 = none or not a buffer. Console formats counted as integer (`descriptor/host.rs:1171-1175`):
  UINT 3, 7, 12, 17, 25, 30, 36, 42, 50, 57, 62; SINT 4, 8, 14, 18, 32, 38, 43, 52, 59, 64.
- The shader computes the slot (`heap_index`), clamps it to `SLOT_STRIDES_COUNT - 1`, loads the dword and,
  for `_d16` typed loads / stores only, picks integer or half-float handling from bits 31 / 30
  (`victory-shader/src/emit/lower.rs:7512-7560`, `:11899-11912`).
- When filled: at the moment the descriptor is made on the recording thread, before the draw is recorded
  (`write_volatile` of one aligned dword). Title slots: `note_buffer` (`descriptor/host.rs:2748-2749`).
  New in f4b48ad: every path that makes a DYNAMIC slot also writes its word -- `make_at` (`:2803`),
  `make_listed` (cached `ListView.stride_word`, `:2836`, `:2853`), `make_null` (0, `:2878`), raw views
  (1: `:2965`, `:3010`), null / constant views (0: `:2975`, `:3040`, `:3098`), `texture_slot_gone` (0, `:2552`).
  "dynamic slots are exclusively owned until submission finishes" (`:2769-2771`) -- this relies on a slot
  not being reused while a list that reads it is in the air.
- Binding: a shader with root SRV 252 gets the buffer's GPU address WITHOUT the bindless heap sweep
  (`State::heap_strides`, `draw.rs:2125-2140`; the list keeps the buffer, tag `gpu | 2`), where slot 255
  runs `heap_buffers` (buffers + textures passes over the whole heap). The root signature keeps slot 252 a
  root descriptor (`pipeline/host.rs:524-528`).
- Test: `dynamic_slots_populate_and_clear_stride_table` (`descriptor/host.rs:5709-5771`).
- Base it sits on: avatarmanz 9a6c829 ("d16 data of integer views keeps its bits", after upstream VIctory
  4243a96a "the face network keeps its halves in UINT views; its output was noise") added bits 31 / 30 to
  `SLOT_STRIDES` for heap-picked buffers only; f4b48ad extends it to table views.

Ours today (grep):
- `SLOT_TABLE_STRIDES`, `heap_strides`, `VIEW_INTEGER`, `VIEW_SIGNED`, `write_stride`: 0 hits.
  (`HalfUintUav`: 0 hits too -- the port is named `half_view` / `half_views`, 48 hits.)
- Strides dword = stride only (`descriptor/host.rs:884 view_stride`, written at `:2060-2066` in `note_buffer`,
  title slots only). Dynamic slots are never written.
- The same defect is handled statically: the translator names table places through which a shader moves
  16-bit halves (`HostShader::half_views`, `victory-shader/src/emit/mod.rs:289`; `lower.rs:10273-10284`;
  `VICTORY_HALF_VIEWS`, default on) and the host shows the shader an unsigned twin of a 16-bit float / SINT
  buffer view at those places (`descriptor/host.rs:3889 half_view`, `draw.rs:3756-3766`, `:4029`).
- Slot 252 in ours is `SLOT_HEAP_MARKS` (`emit/shader.rs:43`), a root UAV. An SRV `t252` would not collide
  in register terms, but our host code tells "ours" slots by number per kind (`pipeline/host.rs:572-578`),
  so use a free number (251) to keep them apart.

What ours lacks compared with theirs:
1. Heap-picked buffers (`RootType::ResourceHeap`): our half-view rule needs a table place
   (`self.table_place(src)`), so a `_d16` access through a view the program picks by a computed index gets
   no integer handling at all. This is the case upstream fixed first (4243a96a).
2. Table places where not every access moves halves (`halves_only`), or where the same record is read as
   floats by another shader in the same table: ours leaves them float.
3. Ours pays a table made afresh in dynamic slots for every draw with a half place (`draw.rs:3752-3770`):
   recording cost on bound (a) and dynamic-slot use. Theirs reads the kind from the table at run time and
   needs no fresh table.
What theirs lacks compared with ours: format loads of whole dwords read as halves (our
`format_load_read_as_halves`, the head program's reads of the face network's output) -- theirs branches on
`_d16` forms only.

Port (host half; do it only together with the translator entry for `integer_view` / `slot_word`):
- `emit/shader.rs:26`: add `VIEW_STRIDE_BITS`, `VIEW_INTEGER`, `VIEW_SIGNED`, `SLOT_TABLE_STRIDES` (251 in
  ours); export in `emit/mod.rs:124`.
- `descriptor/host.rs:884`: `view_stride_and_kind`, `view_stride_word`; `:2060-2066` -> `write_stride`;
  add `write_stride` calls in `make_at` (`:2130`), `make_null` (`:2139`), `raw_at` (`:2193`), `null_raw_at`
  (`:2219`), `null_raw_uav_at` (`:2258`), `constant_at` (`:2294`), `texture_slot_gone` (`:1684`).
- `draw.rs:2092`: second branch for the new slot calling a new `heap_strides` (copy of the head of
  `heap_buffers`, `:2450`, without the passes), through our `issued.root` filter as slot 255 does.
- `pipeline/host.rs:576`: add the slot to "ours".
- Note for ours: with the kind bits a twin made by `half_view` must write the kind of the TWIN's format.
Switch: theirs none (always on). Ours: `VICTORY_VIEW_KINDS=1` (off by default until A/B), exclusive with
`VICTORY_HALF_VIEWS` per shader. Verify: port their test; faces animate and cloth is not noise with
`VICTORY_HALF_VIEWS=0 VICTORY_VIEW_KINDS=1`. Picture: neutral if ours already covers the cases; improves if
case 1 exists in the title (unknown: count `_d16` accesses with a heap-picked source in the translator).
Cache: YES -- shaders that use the slot change; every such shader is translated anew (theirs kept the cache
key by limiting the branch to `_d16`, "`made=0`" against their `tools/cache-pareto`).
Claim: no number. Size M (host about 120 lines in 4 files + test; translator separately). Risk: medium.

### H3-7. Synchronous bake upload list (`BAKE_UPLOAD`, `execute_sync_internal`, `prepare_bake_slot` / `publish_uploaded_slot`, `flush_for_bake`)  -- coder: HOST; only if `VICTORY_THIN`, `SHOW_AFTER` or `UPLOAD_AHEAD` is ever ported

Kind: visual / correctness. In theirs this is the main body of the 19:15 and 21:30 (3) entries.

What a "bake" is: the title composes its characters' skin, head, clothing and decorations (tattoos, dirt,
projected decals) into textures of its own, once, while a scene loads ("159-1,165 frames before" the first
world frame, runs 156-174). The bake's pixel shader picks its source textures by index from the view heap
and names none in a table, so nothing tells the layer which textures it needs; whatever is not on the card
at that draw is baked as nothing "once, for the run". See Game facts for the programs.

Theirs, end to end (`draw.rs:2289-2416 early_textures`, f4b48ad):
1. `is_bake_draw(p)` (H3-3) and `VICTORY_BAKE_TEXTURES_MS > 0`.
2. Lock `BAKE_UPLOAD` (`:230`): one reusable upload-only `HostList` for all recording threads
   (`BakeList`, `Send` by the mutex).
3. `BAKING.set(bake_exact())`; `flush_for_bake(bake_list, host, Some(&mut sync))` (`:2148-2255`):
   - `texture::ahead::drain()`: waits for every copy-queue upload on its way (at most 2000 ms, 50 ms steps;
     `texture/ahead.rs:155-169`; `finish` now removes a job from `ON_THEIR_WAY` only after its `then`
     callbacks queued their flips, `:341-348`),
   - up to 4 rounds: if `needs_gpu_settle_for_bake()` (pending list flips, `FLIPS` after an unfinished
     submission, `RESHOWN_AFTER`) -> `settle_for_fence()`; then `texture_views_remade()` +
     `texture_slots_flip()`,
   - `take_pending_list_flips()`: flips still sitting in unsubmitted lists (`PendingListFlip`,
     `descriptor/host.rs:432-468`, `:2306-2343`) are promoted: skipped if the slot no longer holds the record
     (`slot_still_at`), restored if the placed texture's memory is unmapped or the staging was deferred,
     otherwise uploaded now (`readable`) and queued in `sync.publish`.
   - counters `RESCUED[4]`, info line "flushed deferred texture stand-ins before decoration bake".
4. If budget is left (`bake_ms - BAKED_MS`) and something is new (`BAKE_SERVED` vs `texture_news`):
   `PASS_BUDGET = (left ms, 2 GiB, bake = true)`; `heap_textures(bake_list, heap, host, Some(&mut sync))`:
   - `texture_slots_bake_written(frame)` (`descriptor/host.rs:1976-1998`): slots under physical-page writes
     kept in `BAKE_PAGE_WRITTEN` (at most 1 << 14 ranges, joined past 256) are made due this frame,
   - per slot: `prepare_bake_slot` returns `(view, key)` WITHOUT touching the host descriptor (no `mirror`,
     no `neutral()`); the upload is recorded into the bake list; `(index, handle, key)` goes to
     `sync.publish`, the host copy to `sync.keep`,
   - the commit gate `admit_texture_pass(required)` moved from the top of the pass to just before a slot is
     mirrored (`draw.rs:2813-2816`), after `texture_slot_gone` reclamation; a deferred staging
     (`take_staging_deferred`) leaves the slot due (`:2950-2954`, `:3813-3818`).
5. `BAKING.set(false)`; if the bake list `has_recorded_work()` (`host.rs:6367`): `close`,
   `execute_sync_internal` (`host.rs:5135`: `self.submit(&[list], &[], false)` without `queue::before_sync`,
   "deadlock-safe when invoked from `VICTORY_THIN=1` worker threads"; waits for its fence), `reset`.
   Else if something is to publish: `settle_for_fence()`.
6. `publish_uploaded_slot(index, handle, key)` for each (`descriptor/host.rs:2413-2441`): under the slot's
   `made` lock, if the record is still the same, `note_buffer`, `make`, `put`, `quick_is(0)`, `clear_twin`,
   `made.set`, `keep`. Then `texture_views_remade()`, `texture_slots_flip()`.
7. Fallback without a host list (tests): the old in-list path with `flush_for_bake(list, host, None)`.

Why they need it: with `VICTORY_THIN=1` lists are translated by worker threads and "concurrent lists can
[otherwise] submit a bake draw ahead of its texture uploads"; with `SHOW_AFTER` / `UPLOAD_AHEAD` a slot
shows a 1x1 grey texture (`neutral()`, `descriptor/host.rs:315`) until its upload has run -- a bake that
samples it bakes grey forever.

Ours: none of the three mechanisms exists (0 hits `show_after`, `fn neutral`, `UPLOAD_AHEAD`,
`list/thin.rs` absent). Our bake pass records its uploads into the SAME list ahead of the bake draw
(`draw.rs:2663-2665`), and `host.mirror(index, handle, true)` shows the real view at once. With H3-1's
"always record the copy under BAKING" the one remaining cross-list case (texture staged by another list of
this frame that is submitted later) is closed too.
Port: none now. If THIN is ported later (shortlist rows 164 / 238), this entry becomes a prerequisite:
L (about 600 lines: `draw.rs` +320, `descriptor/host.rs` +180, `host.rs` +20, `texture.rs` +20,
`ahead.rs` +40) on top of base `show_after` (3bab23c), `texture/ahead.rs` (473 lines), `pool.rs` (749),
`residency.rs` (201).
Tests of theirs: `pending_list_flips_promote_once_and_clean_up`,
`deferred_flip_closure_keeps_pending_entry_alive_until_called_or_dropped` (`descriptor/host.rs:5872-5941`),
`drain_waits_until_then_callbacks_have_run` (`texture/ahead.rs:447-472`).
Claim: "GPU suite: 335 passed, 34 ignored"; no frame figure. Picture: improves (theirs). Risk in theirs:
each bake draw can block up to 2 s in `drain` plus settles, serialized over all recording threads.

### H3-8. Host-memory gates: mild pressure vs critical commit (`residency.rs`)  -- coder: HOST; optional, memory

Kind: memory / stability (Windows commit, not VRAM). Base: codec_d ab75ac3 + e05d463 (other parts of this
review); f4b48ad only retunes it.

All thresholds at HEAD (`residency.rs`):
- `sample_commit`: `GlobalMemoryStatusEx().ullAvailPageFile`, at most once per 32 ms (`:45-56`).
- `RETRY_UPLOAD_AT`: after an `E_OUTOFMEMORY` every gate is closed until that time (`upload_refused`, `:41`).
- `has_upload_reserve`: commit >= 256 MiB for a bake, >= 512 MiB otherwise (were 2 GiB / 4 GiB) (`:58-63`).
- `has_growth_reserve`: >= 256 MiB bake, >= 1024 MiB otherwise, to commit a NEW 64 MiB staging block (`:65-70`).
- `admit_texture_pass(bake)` = not refused and upload reserve (`:74-80`); `allow_staging_growth(bake)` (`:85-91`).
- `limited_host_ram`: total physical <= 32 GiB (`:93-96`).
- `next_pressure`: enter at physical <= 3 GiB or commit <= 4 GiB; leave only when physical >= 5 GiB and
  commit >= 6 GiB (were 4 / 6 and 6 / 8) (`:98-104`); sampled by the once-a-second sweep (`refresh`, `:107`).
- `critical_commit`: refused, or commit < 1536 MiB (`:144-147`).
- `prefetch_limit(bytes, pressure, critical, limited, staging)`: critical -> min(bytes, 16 MiB);
  pressure, or limited RAM with >= 1 GiB live staging -> min(bytes, 64 MiB) (was 16 MiB under pressure,
  32 MiB limited); else unchanged (`:151-159`). Not applied to a bake pass or an asked-for pass
  (`draw.rs:2648-2657`).
- `pool.rs:55-62`: spare empty staging blocks kept: 0 when critical, 1 under pressure, else the usual limit
  (was 0 under any pressure). `pool.rs:389-395`: a new 64 MiB block is refused only for speculative work
  (`PICKING.is_some() && !BAKING`) without growth reserve; "table-bound textures and decoration bakes are
  sampled immediately and would otherwise read uninitialized/reused VRAM".
- `texture.rs:1893-1898`: when the pool gives nothing and the upload is refused or speculative without
  growth reserve -> `note_staging_deferred()` and no staging (the slot stays due) instead of a private buffer.
- `texture/ahead.rs:107-108`: copy-queue uploads capped at 32 MiB in flight on limited RAM (else
  `MOST = 96 << 20`), and taken only when `allow_staging_growth(false)`.
- Tests: `bake_and_lookahead_leave_room_for_live_game_allocations`,
  `either_constraint_enters_pressure_and_both_must_recover`,
  `prefetch_respects_existing_budgets_and_resumes_after_recovery` (`residency.rs:165-200`).

Ours: no `residency.rs`, no `pool.rs` (0 hits for `pressured`, `admit_texture_pass`, `critical_commit`,
`upload_refused`). Staging buffers come from a size-class spare pool (`texture.rs:152-175`,
`VICTORY_STAGING_POOL`); host RAM is only read for logs (`host.rs:139-146`, `copystat.rs:479`,
`framelog.rs:555`). Our machine is the same class as the author's (32 GB RAM), and his process reached
34.42 GiB commit with 1.30 GiB headroom (journal 16:04) -- on a tree with THIN, 64-submission bury lag and
upload-ahead that we do not have.
Port: nothing from f4b48ad by itself. If our logs ever show `E_OUTOFMEMORY` from `CreateCommittedResource` /
upload buffers, port the base (ab75ac3) with THESE thresholds, not the earlier ones: the author's own
finding is that the 2 / 4 GiB reserves "stall[ed] tens of thousands of textures on `neutral()`".
Switch: theirs none (constants). Picture: the gates trade picture under pressure (textures later) -- with
the earlier thresholds badly, with these mildly. Size M-L (200 lines + hooks in 5 files). Risk: medium.
Claim: none for f4b48ad. Bound: (d) freezes only if they come from commit exhaustion (not shown for ours).

---

## 4. Hunks not worth porting, and why

| Hunk (theirs, f4b48ad) | What it does | Why not |
|---|---|---|
| `descriptor/host.rs:432-468`, `2306-2391` `PendingListFlip`, `PENDING_LIST_FLIPS`, guard in `flip`, `needs_gpu_settle_for_bake`, `take_/restore_pending_list_flip`, `slot_still_at` | tracks deferred slot flips of unsubmitted lists so a bake can promote them | exists only for `VICTORY_SHOW_AFTER` (ours: 0 hits). See H3-7 |
| `descriptor/host.rs:2292` `mirror_after` skips while `BAKING` | no grey stand-in while a bake pass runs | `show_after` only |
| `descriptor/host.rs:2461`, `2504`, `2523` `texture_slots_flip` returns `flipped + late_flipped` | so `flush_for_bake` counts late flips | `show_after` only |
| `descriptor/host.rs:764`, `1953-1998` `BAKE_PAGE_WRITTEN`, `texture_slots_bake_written`; `1808` `texture_news` adds it | keeps physical-page write ranges for the next bake when `VICTORY_PAGE_WRITES` bit 2 is off (their default: bit 1 only, `watch.rs:2114-2122`) | ours makes written slots due by default (`VICTORY_UPLOAD_ON_WRITE` on -> bit 2, `watch.rs:1837`; due one frame on, `descriptor/host.rs:1546`). H3-2's look-ahead covers the one-frame gap. Port only if we ever turn bit 2 off |
| `descriptor/host.rs:4342-4363` warning only `if *unordered \|\| at != 0` | removes a false "placed unordered texture view not mirrored" warning for SRVs with address 0 | ours says it once (`once!`, `descriptor/host.rs:3836`): no spam. Note: their text change shows the branch is also reached by non-unordered placed TEXTURE views that are not mirrored (null) -- same in ours |
| `texture/ahead.rs` all hunks (`IN_FLIGHT`, `drain`, `finish` order, claim gate, test) | copy-queue uploads vs bakes | no `texture/ahead.rs` in ours (`VICTORY_UPLOAD_AHEAD` 0 hits; listed in shortlist line 230) |
| `pool.rs:55-62`, `389-395` | spare staging blocks and growth gate | no `pool.rs` in ours; see H3-8 |
| `residency.rs` all hunks | thresholds | see H3-8 (no base in ours) |
| `texture.rs:1893-1898` staging deferral | no private upload buffer when refused | needs `residency.rs`; H3-8 |
| `host.rs:5131-5137` `execute_sync_internal`, `host.rs:6365-6370` `has_recorded_work` | sync submit of the bake list | H3-7 only |
| `draw.rs:2113`, `2511-2515`, `2993`, `3047` `heap_textures` returns `served`, takes `sync` | plumbing for the sync bake | H3-7 only (the `served` return is useful for `BAKE_SERVED`, H3-2) |
| `draw.rs:2813-2816` commit gate moved after `texture_slot_gone` | reclamation still runs when uploads are refused | needs `residency.rs`. Worth remembering for our VRAM "no room" gate: never put a refusal in front of the code that frees slots whose memory went |
| `indirect.rs:2608` test helper | REST true / false both forced | goes with H3-4 |
| `pipeline/host.rs:527` | slot 252 stays a root descriptor | goes with H3-6 |
| `docs/bug-journal.md` +4 lines | notes | read, section 2 |
| `Play-VIctory.cmd`, `README.md` | launcher and its instructions | section 5, 6; Series S and ambient stand-in: not acceptable |

---

## 5. Play-VIctory.cmd (new file, 40 lines): every setting

Each `set` applies only if the variable is not already defined. The run line is
`"%runner%" run "%VICTORY_GAME_DIR%\game_scarlett_master.exe" --console series-s --execute`.

| Setting | What it does (their source) | Class | Ours (grep of `victory-dev\crates`) | Earlier verdict (`fork-review-shortlist.md`) |
|---|---|---|---|---|
| `--console series-s` | Series S profile: lower internal resolution and assets | QUALITY CUT -- not acceptable: trades picture | we run Series X | line 36, 67, 110: rejected |
| `VICTORY_GAME_DIR` (default `..\GTA6.Xbox.v1.0.2.1`) | where the dump is; must hold `game_scarlett_master.exe` | tooling | n/a | - |
| runner: `victory-3d-rtx-pareto-residency.exe`, else `target\release\victory.exe` | which binary | tooling | n/a (never run by this review) | - |
| `VICTORY_AMBIENT_AIR=1.5` | bounds what room air scatters to 1.5x the light around (`ambient.rs:106-112`); "to clear milky indoor ambient haze" | QUALITY CUT (changes the title's fog; part of the stand-in) -- not acceptable | 0 hits | line 225: "faked pass" |
| `VICTORY_AMBIENT=0.8` | 80 % of a screen-space estimate replaces the output of the title's traced indirect light when its rays find nothing (`ambient.rs:27-30`, `:97-103`) | QUALITY CUT (stand-in light) -- not acceptable | 0 hits | line 103, 225, 334: faked pass |
| `VICTORY_CACHE_DIR=%LOCALAPPDATA%\VIctory\cache-pareto` | shader cache folder | tooling | have (`victory-core/src/config.rs:88`) | - |
| `VICTORY_TEXTURE_NO_WAIT=1` | texture refresh does not wait for the runtime's remap lock; reads through the kernel or leaves the texture a frame (`texture.rs:374`) | speed; picture risk (texture a frame late; with a one-shot bake: forever -> their `exact` flag, H3-1) | ON by default in ours (`texture.rs:821-823`) | line 227: "picture faults" in trustno1's set; line 338 |
| `VICTORY_RELAY=2` | relay second stage: a submission waits only for the flights that declared the memory it reads (`host.rs:2484`) | speed | default in ours (`host.rs:2859`) | line 227 / 266: "stretched bodies" reported with RELAY=2 + CARD_STEPS in another fork |
| `VICTORY_CARD_STEPS=2` | constant-buffer fills and the title's buffer copies that read memory flights may still write are done on the card in flight order instead of waiting (`host/card_steps.rs:10-26`); "25-30 ms of a 105 ms frame" on the 7900 XTX | speed; picture risk (skinned data handed on early) | 0 hits | line 247, 266: "do not port" (bodies stretch) |
| `VICTORY_RELAY_COPY=1` | relay fetches card memory over the copy queue, not the drawing queue (`host.rs:2486-2505`); bytes sent later are left out of the merge | speed; correctness risk ("later data") | 0 hits | not listed before; goes with CARD_STEPS |
| `VICTORY_WIDE_BACK=1` | wide copy-back across ranges | speed | default on in ours (`shadow.rs:1588-1589`) | have |
| `VICTORY_CARD_REHASH_RUNS=1` | rehash of card buffers in runs, once | speed | have, off by default (`host.rs:2507-2526`) | have |
| `VICTORY_JOIN_COPIES=1` | a copy joined with the step after it | speed | have, off (`host.rs:5350-5369`) | have |
| `VICTORY_INDIRECT_WINDOW=1` | indirect records clamped a window at a time | speed | default on in ours (`draw.rs:537-590`) | have |
| `VICTORY_INDIRECT_BATCH=2` | clamps batched; a dispatch does not end the batch (`draw.rs:345-373`) | speed | 0 hits | line 247: "Not for us" (same goal as our gather) |
| `VICTORY_MAP_THREADS=1` | one thread maps page groups (`victory-hle/src/modules/xmem.rs:1286-1322`) | speed (turns a threaded path off) | 0 hits | line 177: measured at nothing |
| `VICTORY_CARD_AWAKE=1` | keeps the card's clocks up between submissions | speed | have, off (`host/awake.rs:81`) | line 237 |
| `VICTORY_SHOW_AFTER=1` | a title slot shows a new texture only after its upload has run; grey 1x1 meanwhile (`descriptor/host.rs:284-293`, `:315`) | stability (device loss); QUALITY trade (later data, grey frames) | 0 hits | line 193: listed, not ported |
| `VICTORY_THIN=1` | list calls only written down, translated by worker threads (`list/thin.rs:55-58`) | speed, bound (a) | 0 hits (one comment) | lines 164, 238, 310: listed; needs more dynamic slots -- and, per this commit, the sync bake (H3-7) and early table release (H3-5) |
| `VICTORY_JOIN_PARTS=8` | up to 8 parts joined into one host submission (`host.rs:3095`) | speed | have, off; ours is on/off (`queue.rs:504`) | line 152, 230 |
| `VICTORY_UPLOAD_AHEAD=1` | first uploads of index-picked textures on a copy queue of their own (`texture/ahead.rs:1-13`) | speed / streaming, bound (d) | 0 hits | line 230: listed |
| `VICTORY_LEVELS_ON_DEMAND=1` | host copies take card memory level by level, tiled resources tier 2 (`tiles.rs:46-50`, `:74-81`) | memory (VRAM); QUALITY risk (levels arrive later; "blurred or black" until tiles are had) | have, off (`tiles.rs:77-83`) | not rated before; our VRAM plan decides |
| `VICTORY_CARD_BUFFERS_MIN_KB=64` | title buffers from 64 KiB live on the card | speed | have, unset (`shadow.rs:369-372`) | line 137, 339 |
| `VICTORY_LOG=warn` (unless `RUST_LOG`) | log level | tooling | n/a | - |

Usage: `Play-VIctory.cmd` or `Play-VIctory.cmd pareto-residency`; any other argument prints usage, exit 2.

---

## 6. README (13 new lines) and timing claims

- The new section only says how to run: download the Windows x64 ZIP from "private releases", set
  `VICTORY_GAME_DIR`, run the launcher; "The launcher selects Series S and the RTX-tested residency
  settings"; needs Windows x64, AVX2, D3D12 with shader model 6.6 and mesh shaders; build with
  `cargo build --release --locked -p victory`. No fps claim by codec_d in the README.
- The fps table further down is the base AMD fork's (RX 7900 XTX, 1440p, Series X profile): menus 30 fps;
  prologue indoors 50-75 ms; first mission with the boat 46-53 ms; street chase 100-165 ms; outdoors on foot
  60-90 ms; flying 42-57 ms. Not changed by this commit.
- codec_d's own figures are in the journal and belong to earlier commits (RTX 5090, 32 GiB RAM, launcher
  settings = Series S + the speed set above, opening save, intervals with >= 1,000 draws/frame):
  - 15:38: 81.4 ms/frame at 4,463 draws ("no controlled scene match").
  - 15:46: baseline median 64.2 ms vs candidate 54.8 ms over 15 intervals; draws 2,868 / 2,839; peak commit
    27.15 / 26.12 GiB; adapter 18.67 / 17.45 GiB; "this pair is directional".
  - 15:49: median 57.7 ms over 23 intervals, slowest 566 ms, commit 28.74 GiB, adapter 19.12 GiB.
  - 16:04: median 54.3 ms over 120 intervals, 39 at or below 50 ms, commit 34.42 GiB, lowest headroom 1.30 GiB.
  - 20:50 (rejected experiment): median 70.5 ms in a diagnostics capture.
  - f4b48ad itself (16:50-22:28): tests only -- GPU suite 335 passed / 34 ignored; shader lib 281 passed;
    targeted GPU suite 20 passed; `translate --validate --pipelines` on 154 shaders: 43/43 graphics, 96/111
    compute. "Sustained 50 ms/frame and full material coverage remain open" (15:49).
- For scale: a 5090 at Series S with a faked ambient pass reaches 54 ms median. Nothing here is a route to
  33 ms at 1440p Series X.

---

## 7. Game facts (collected, with source)

Bakes
- The title bakes characters' skin and decorations into its own textures while it loads, 159-1,165 frames
  before the first world frame (runs 156-174), with a pixel shader that picks its sources by heap index and
  names no texture in a table; what is missing then is baked as nothing "once, for the run". On the console
  "a texture is there when its bytes are". (theirs `draw.rs:163-175`; same text in ours `draw.rs:409-421`)
- Bake pixel shaders `PS_BakeDecorations_*`, first 16 hex digits of the hash: `c0a7fe77b1862e10`,
  `0d1575a396dfe513`, `b7cfd8dda91ebb6b`, `280967ad158cee0b` (known to us) + `fa455c8a3c32577b`,
  `d126bcae5d5d8d26`, `fcee612943c008e0` (new). They draw into a 256x256 corner of 2048x2048 targets.
  (theirs `draw.rs:239-252`)
- Geometry shared by every bake permutation: vertex shader `089d6271be6650ef` = fullscreen-triangle "ped
  texture compositor" (ped baker); mesh shader `d4111a6e6ea49d91` = decoration projection mesh.
  (theirs `draw.rs:254-258`)
- Shape of a ped / head / clothing atlas bake: no depth target, 4 or more render targets, first target
  square and >= 512 wide. (theirs `draw.rs:2257-2277`; a heuristic of the author, not proven complete)
- A frame bakes up to sixteen times. (theirs `draw.rs:209-212`)
- A bake that runs between a memory write / map and the layer's next look takes "the bytes from before
  (another texture's)": coloured specks on the waitress's face for the whole diner scene (run n01).
  (theirs `draw.rs:186-195`)
- Experiment 20:50: making aliased view maps/unmaps count as remaps and widening the bake look-ahead caused
  2-4 GiB bake passes on frames 1368-1369 of the opening save and a "face-mask/shorts regression".
  (journal line 18) -- i.e. face masks and shorts are baked items.

Views and formats
- The face network keeps its 16-bit halves in UINT typed buffer views; read as floats its output is noise
  (upstream VIctory 4243a96a, via 9a6c829; theirs `lower.rs:11903-11906`). Integer handling must be limited
  to `_d16` typed loads / stores: applying it to other typed loads "caused shader regressions" (journal 16:50).
- The console's buffer descriptor holds the element stride and the hardware multiplies the index by it; the
  program's code does not say it. (theirs `emit/shader.rs:16-25`)
- A placed (title-memory) non-buffer view with address 0 reaches the view maker as a plain SRV: it is an
  empty slot, not an unordered texture. (theirs `descriptor/host.rs:4342`)

Programs
- `alphaResolve:CS_AlphTransmissionPrefilterMip_610a2fad` (hash `2da4beb386509b77`) fills the transmission
  buffer; if it is dropped, `ptfx_PS_Transmission_aba1527f` (dual-source blend) and transmissive eye / alpha
  shaders output black squares / corrupted blending. (journal line 19 (1); translator part covers the fix)
  Bears on our "glowing eyes" / black squares.
- Skinning: eleven dispatches a frame in two early submissions write through one view of the title's whole
  174 MB heap at offset 5920 of the bindless table, at places read from job records (`17af15c4015cb4b6`,
  `55679a0547649d0b`, `89d08e561897d4e5`, `fdd56d97a806bcca` + three more); the title's ring of constants
  lies in the same heap. (theirs `host/card_steps.rs:16-22`, base)

Indirect records
- Slots of an argument buffer the title has not filled hold all ones; a record of >= 2^23 vertices an
  instance is never a real draw (one list had 90 draws of 2^24 vertices). (theirs `indirect.rs:107-113`,
  `:131-136`; ours `indirect.rs:123-137`)
- Which record is the oversized one "may differ from frame to frame" (text removed by f4b48ad at
  `indirect.rs:296`), hence flicker rather than a steady hole.

Tables
- A frame of the opening cutscene asks for 12,500 descriptor tables (ours `draw.rs:804-806`); with THIN on
  NVIDIA 12,000-16,600 a frame (shortlist line 238).

Memory (author's machine, for scale)
- Windows returned `E_OUTOFMEMORY` (0x8007000E) for 64 MiB pool requests and uploads as small as 24,080
  bytes; process commit 34.42 GiB on 32 GiB RAM (journal 15:53, 16:04).

---

## 8. Coverage

Commit f4b48ad, non-shader paths: 12 of 12 files read in full diff.

| File | +/- | Read | Where it is covered |
|---|---|---|---|
| `Play-VIctory.cmd` | +40 | all | section 5 |
| `README.md` | +13 | all | section 6 |
| `docs/bug-journal.md` | +4 (whole file, 19 lines, read) | all | section 2 |
| `crates/victory-gpu/src/descriptor/host.rs` | +489/-... (699 diff lines) | all | H3-6, H3-7, section 4 |
| `crates/victory-gpu/src/draw.rs` | +469/-... (669 diff lines) | all | H3-1..H3-3, H3-5..H3-7, section 4 |
| `crates/victory-gpu/src/host.rs` | +25 | all | H3-5, H3-7 |
| `crates/victory-gpu/src/indirect.rs` | +10 | all | H3-4 |
| `crates/victory-gpu/src/list.rs` | +6 | all | H3-5 |
| `crates/victory-gpu/src/pipeline/host.rs` | +1 | all | H3-6 |
| `crates/victory-gpu/src/pool.rs` | +16 | all | H3-8, section 4 |
| `crates/victory-gpu/src/residency.rs` | +116 | all | H3-8 |
| `crates/victory-gpu/src/texture.rs` | +34 | all | H3-1, H3-8 |
| `crates/victory-gpu/src/texture/ahead.rs` | +70 | all | H3-7, section 4 |

Also read for context (not in scope, not exhaustively): theirs `host.rs:455-520`, `3225-3257`
(bury lag, land), `indirect.rs:56-215`, `draw.rs:150-260`, `2556-2655`, `pool.rs:40-80`, `286-312`,
`380-400`, `residency.rs:93-135`, `descriptor/host.rs:284-330`, `emit/shader.rs:10-50`,
`emit/lower.rs:7505-7570`, `11895-11915`, heads of `card_steps.rs`, `ambient.rs`, `tiles.rs`,
`texture/ahead.rs`; commit messages of 9a6c829, 8d79927, 2dfba4d; ours: the functions cited above.

Not read: the `crates/victory-shader` hunks of f4b48ad (another agent's scope; only the constants hunk in
`emit/shader.rs` and the `TypedUav` test-host hunk in `emit/host.rs` were looked at). Nothing else in scope
was left unread.

Not done (by the rules): no build, no test run, no binary or archive opened. Every "0 hits" above is a
grep of `C:\GTAVI\Tools\victory-dev\crates` (`*.rs`).

Open points a coder should check before relying on this file
- H3-5 step 2: whether a `Detached` in a flight can be an earlier part of a list cut at a wait (then its
  tables must stay until the last part).
- H3-4: free word offsets in our budget buffer for the two new counters.
- H3-3: whether our frame listings print bake hashes the same way for mesh / vertex stages as for pixel
  stages (the list is matched against `s.shader.hash` of any stage, same as theirs).
- H3-6: how many `_d16` accesses in the title use a heap-picked source (decides whether the port has any
  picture value for us).
