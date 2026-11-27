# Performance research in other forks — 2026-11-26

Read from code and commits (git only; nothing built, run, extracted or downloaded). "Ours" = `C:\GTAVI\Tools\victory-fox`
@e0149ec (read-only). Costs (a)-(f) as in the task: (a) submit thread preparing, (b) page-mapping calls, (c) texture
uploads/VRAM, (d) card time, (e) per-draw CPU, (f) pipeline hitches.

## 1. Repositories examined

All push URLs are `NO_PUSH_local_only`. Bundles in `C:\GTAVI\Tools\patches\`.

| Clone (under `ref-forks\`) | What | Base / ahead | Last push | Safe? |
|---|---|---|---|---|
| `another-victory-fork` remote `upstream` | avatarmanz/VIctory-7900XTX `main` @0dec5dc | **153** past our base 0a43566 (was 146) | 11-26 10:17 | clean: only `.rs`, `.md`, `ci.yml`, `play.ps1`; Cargo.lock adds no new crate (tempfile, tracing, victory-core already there) |
| `marvisak` (new) | Marvisak/VIctory, the original upstream (12.9k stars, 2,113 forks) + **2,523 pull-request heads** as `pr/<n>` | split from our line at ffe310e (11-24); main 479 ahead | 11-25 23:27 | clean; lock = ours + miniz_oxide/adler2 (crates.io). Other architecture (title memory through a page table, ~1 fps): ideas only |
| `network` (new, one repo, 38 remotes named by owner) | forks of avatarmanz, fxceless, soldsawn, Marvisak listed below | see below | 11-26 | no submodules, no binaries beyond the baseline `bcpack-tables.bin`; see flags |

Forks in `network` with own work (unique commits not in avatarmanz/Marvisak/our clones; card they test on):
Rubenpannen/VIctory-RTX5070 (23; RTX 5070, 32 GB), mgrein (54, branches `nv/*`; RTX 4070), LloydNicholson/VIctory-Linux (34;
RX 9060 XT, Proton, current with avatarmanz), david-ojah (13), MImi8superr (7; RTX 4070), jasonc123456 (17; RTX 5070),
ASsDevpile (25; RTX 5060 Ti), Coldtrain (80; RX 6800, "Relay Next"), AwkGrep256 (16, frame generation), xvoiding (12),
tadymila (17), SkillIssueXO (24; 5070 Ti), DexNom (19), Belobug (333), FQY55kai (71), Biskwit (93), ad3lre (29),
PastiiJay/klbf-lab (422), JMAN730/project-flamingo (883, 102 branches), Fayberr/Victory-United (1,130, merges of Marvisak
PRs), triippywhale (320), dkdknight (201; RTX 5080), Xeltax/VIcky (296; RTX 4080), Steenax6 (195), NitricAcidDev (160),
and 12 with <10 commits (launchers, READMEs). The fxceless/soldsawn-line forks run at 6-12 fps where we run ~20: their
speed work is mostly behind ours (lookup cache, bind cache, guards' hole memory, hot blocks = already in victory-fox as
`QUICK_FIND`, `BIND_CACHE`, `known_holes`/`held` `watch.rs:1996`, `note_hot` `watch.rs:486`).

Flags (nothing executed): canalmaloqueru-design e7ca0749 adds a C++ `winmm.dll` proxy shim + `shim_cpp/build.bat`
("FPS Governor"): do not use. ASsDevpile af966f8f loads an NGX library through a proxy DLL (DLSS): do not use. DexNom
lock adds `hidapi`, `pkg-config` (crates.io). Several forks carry `.cmd` launchers. `CLAUDE.md`/`AGENTS.md` in
Rubenpannen, NitricAcidDev, JMAN730 are ordinary contributor notes (policy check, hooks); read as data, nothing followed.
No network/registry/credential code seen in the commits read. GitHub API: 10 unauthenticated requests, no user data.

## 2. Ranked mechanisms we lack

### 1. Renderer port 1: the title's 32-bit graphics heap in place — `VICTORY_PORT_CONSTANTS=1` — cost (a)
- Source: avatarmanz 86dc109 (`crates/victory-gpu/src/port.rs`, 401 lines, whole file; `inplace.rs` `stretch` :538-570;
  `shadow.rs` hooks), 4b82f6c + 9c23e8d (`victory-hle/src/modules/xmem.rs` `arrange`, `LOW_RESERVATIONS`).
- What: the routine at image+0x1e86440 reserves 185 MB below 4 GiB (at 0x34310000), one section; the 174 MB buffer at
  0x34e10000 is the constants ring and what skinning writes. The stretch is mapped once more at an address of ours and
  opened with `OpenExistingHeapFromAddress1`; title buffers/windows there become buffers placed in that heap.
  Author: "a host copy in the card's memory, ~17 MB a frame sent up in ~600 pieces, shader writes fetched back and
  compared, and ~6,000 constant fills a frame" -> "nothing tracked, sent up, fetched back or compared". README: city
  street 57-75 ms -> ~53 ms with ports 1+2 (7900 XTX, 64 GB).
- xmem half is needed: the title maps the five pieces out of address order ("Scattered"); `arrange` swaps fresh section
  slices so the reservation is one run; the reservation is recognised by kind 0x42000 (`kind & MEM_COMMIT == 0 && flags &
  ADDRESS_32BIT`).
- Ours: new `victory-gpu/src/port.rs`; `inplace.rs` add `stretch`/`stretch_in` after `cover` (:526), `counted`, call
  `crate::port::forget` in `title_memory_freeing` (:183); `shadow.rs:5273` and `:5580` try `port::place_for` first;
  `host.rs:7517`/`:7552` meters; `late_line` `host.rs:3836`; `xmem.rs:87` (record reservation) and `:1236` (`arrange`
  before mapping). Our in-place machinery (`inplace::place`, `Shadow::placed`, `From::Part`) is the same as theirs.
- Size ~700 lines. Risk medium: heap opened on our own view locks 185 MB; assumes CPU steps read what the title's CPU
  wrote (no wait for flights) — check against `VICTORY_LATE_WAIT`; never tile-map into it. Picture: none.

### 2. Title memory in place by default on NVIDIA, page-pool windows included — `VICTORY_INPLACE` auto, `VICTORY_POOL_INPLACE`, `VICTORY_STRETCH` — costs (a), RAM behind (b)
- Source: mgrein `nv/memory-inplace` / `nv/integration` cb65249a (`inplace.rs` +391, `xmem.rs` +52). Ports of DexNom
  `VICTORY_POOL_INPLACE` and Belobug 0434b3ac.
- What: pool sections made `SEC_COMMIT` so `OpenExistingHeapFromFileMapping` accepts them; a window that is one run of a
  section = buffer placed in the section's heap (opened once per section); a run >= 4 MiB of a reserved section goes
  through our whole view (`beside`). Unset `VICTORY_INPLACE` = 1 when the adapter is not AMD or RAM <= 40 GiB, after a
  probe. Author: "On the RTX 4070 the probe passes every check"; "a run made 3,434 [windows] (13.8 GB of the PC's memory
  shared with the card)... In place they cost nothing".
- Why for us: our windows are copies in PC memory (927 windows / 3.9 GB, 6.2 GB shared with the card) refreshed by the
  submit thread (refreshes 15 ms of the 36 ms preparing). In place: no copy, no refresh, no write-watch asking, less RAM.
- Ours: `inplace.rs:87` `level()` (default 0), `:136` `place_for`, `:303` `title_memory_section`, `:538` `beside`
  (already there); `xmem.rs` section creation (`SEC_RESERVE` today). **Zero-code first test: `VICTORY_INPLACE=1`** (private
  memory only) on our build.
- Size ~450 lines. Risk medium-high: committed sections charge commit at once (we are at 36.5 GB on 32 GB: measure the
  pool size first); in-place pages cannot be decommitted/protected by the title while placed; FQY55kai dd202bc5/59d53a22
  refuse every in-place placement under late waits — port that guard too. Picture: none.

### 3. Lock splits on the descriptor/retire paths — no switch in the source (suggest `VICTORY_LOCK_SPLIT`) — costs (a), (e)
- Source: avatarmanz 11511b8. `descriptor/host.rs`: `Queued` bitset (:1020) read without the texture queue's lock —
  "most of the 50,000 descriptors a frame... took that lock to take nothing out of the queue (12 ms a frame of waiting,
  run city1)"; `Made::LOCKS` 512 -> `1 << 14` with `index.wrapping_mul(0x9E37_79B1) >> 18` (:98) — "2 to 4 ms a frame of
  each"; `due()` sorts outside the lock. `host.rs:81` `RETIRED` in 16 parts by thread — "10,000 views a frame... 12 ms a
  frame for one lock".
- Ours: `descriptor/host.rs:89` (`LOCKS = 512`), `TextureQueue` insert/remove sites, `host.rs:78` (`RETIRED` one Mutex).
- Size ~200 lines. Risk low. Picture: none. Skip its `VICTORY_LANE_CORE` (off, unmeasured) and `FILL_TITLE=2` (measured
  slower, e000e02).

### 4. Submit-thread savings from the RTX 5070 fork — cost (a)
- Source: Rubenpannen/main e4822e14.
  - `shadow.rs` `CARDED` keeps each copy's range; `send_copied` holds only copies the range reaches: "5-16 times less
    work a copy with 20-400 copies on the card". Ours `shadow.rs:731` (walks `carded_copies()`).
  - `copy_mapped` returns whether the kernel copied all; `Placed::prepare` skips the read-back + compare: "0.35 ms a
    MiB, 3.2 ms for 8 MiB". Ours `host.rs:8593`, `host/place.rs:81-99` (always allocates, reads back, compares).
  - `watch::ask_ahead` returns at once when the submission's one asking stands (`journal().kept`). Ours `watch.rs:370`.
- Same fork 05dedfd1 is a correctness fix for cards <= 12 GiB budget (copies 8-16 MiB into one-sided host copies changed
  bytes under flights); f954d941 fixes `CARD_STEPS` + copy-at-place ordering (we rejected CARD_STEPS=2; if retried, take it).
- Size ~80 lines. Risk low. Picture: none. Switch: `VICTORY_SEND_BY_RANGE`, `VICTORY_COPY_NO_READBACK`.

### 5. Heap records read in place, AVX2 compare; spans' lock skipped — `VICTORY_SYNC_IN_PLACE`, `VICTORY_SPANS_QUICK`, `VICTORY_SYNC_FINE` — costs (a), (e)
- Source: avatarmanz 15ab94e, a284ca7, 8822ed4, 288af58 (`descriptor.rs:577-596`, `safecopy.rs` `changed`,
  `descriptor/host.rs:238-261`). Measured (heapsync-ab3): "13,300 of 13,500 such writes a frame left it alone; making
  views 1.7 ms a frame against 2.4, the pass 11.7-12.0 ms against 13.2-13.8; the picture the same".
- Ours: `descriptor.rs:297` `sync_heaps`, `descriptor/host.rs:231,243`. ~370 lines, medium risk, picture none.

### 6. Renderer port 2: descriptor stores report their slots — `VICTORY_PORT_DESCRIPTORS=1` — cost (a) (heap sync, watch askings)
- Source: avatarmanz 9997d25, 4cb90be (`port_descriptors.rs` 519, `victory-loader/src/descriptor_sites.rs` 318,
  `descriptor.rs:352-371`, `apps/victory/src/run.rs`). Six `vmovups [table+r], ymm0` stores in +0x4d9efd0 / +0x4eec8e0
  (image 1.0.2.1) are patched, original bytes checked, to set a byte per slot / per 128 slots; the title's
  memcpy/memmove imports mark too. `sync_heaps` takes marks instead of changed pages: before "~290 pages, ~600,000
  slots, 15-16 ms a frame of the heap-sync thread, 5-7 of it asking the watch". Fallback: watch asked every 8 frames, 16
  pages a frame looked at whole, misses counted.
- Ours: `descriptor.rs:297`; loader patch hook as theirs. ~850 lines. Risk medium-high (code patch of the title; a slot
  missed shows an old view until the fallback reaches its page). Picture: none intended; watch the "missed" counter.
  Our heap sync is already aside (`HEAP_SYNC_ASIDE`), so the lane gain is the watch-lock time, not the whole 15 ms.

### 7. Renderer port 5: constant buffers read raw in place — `VICTORY_PORT_CBV=1` — costs (a), (e)
- Source: avatarmanz 3920dc2 / 0dec5dc (`port_cbv.rs`, `descriptor/host.rs` `constant_raw_at` :3054 and
  `constant_twinned`, `draw.rs:3531`, `victory-shader/src/emit/{lower,mod,shader}.rs` `constants_raw`,
  `shader_cache.rs:133`). Console CBVs sit on 16 bytes, D3D12 wants 256, so every table CBV is copied at each
  submission: "~23,000 a frame of the city, ~9 MB, and as many steps of the submitting thread"; merge message: "~22,000
  fills a frame -> 0". Shaders read constants through the slot's raw twin (or a root SRV).
- Needs port 1 (source memory in place). Ours: `draw.rs:4034` (`defer_constants`), `descriptor/host.rs:2950` `twin`,
  `:4218` `make_twinned`, `emit/shader.rs:189`. ~480 lines. Risk medium-high: every shader re-translated and every
  pipeline rebuilt (cost (f) once; do the warm run first); raw loads instead of CBV loads may cost card time — no frame
  time was published, measure. Picture: none intended.

### 8. LDS read barriers shared on 32-lane hosts — cost (d)
- Source: david-ojah `port/152-wave32-lds` 2d5ac6c1 (port of gomeaw's avatarmanz PR #152), `emit/lower.rs`. A console
  wave64 program with LDS runs as two host waves on NVIDIA and gets a group barrier before every LDS access; a run of
  consecutive read-only `ds_read*` now shares one: "the barrier count falls from 7 to 4 (wave64 stays 1)".
- Ours: `victory-shader/src/emit/lower.rs:5542-5550` (one `barrier` op per access), block loop near :2425.
  28 lines + test. Risk low-medium (proof is per block, reset by any other instruction). Re-translation of the affected
  compute programs (bump the cache key). Picture: none if the ordering argument holds; compare a capture.

### 9. Command allocators renewed after long recordings — `VICTORY_ALLOCATOR_RENEW=<calls>` (512) — RAM/VRAM behind (b), (c)
- Source: Rubenpannen 5df4fb31 (`host.rs:1293-1311`, `list.rs`). "about 0.8 KB of process memory and 0.7 KB of video
  memory per recorded root constants and dispatch, none given back by a reset". We measured "lists' allocators 9 GB".
- Ours: `host.rs:6485` `reset`, allocator resets `:4477, :4637, :4903, :5212`. Complements our `LIST_POOL_IDLE_S`.
  ~70 lines, low risk, picture none.

### 10. Pipelines of earlier runs made on the title screens — `VICTORY_PREPARE` (on) — cost (f)
- Source: LloydNicholson a0629739 (`pipeline/prepare.rs` 449 lines, `make.rs` `shared`, `known.rs`). Book on by default;
  made "longest first, through the title's own workers and library, on all cores but two"; one `Pending` per key so the
  title and the warm-up never build the same pipeline twice; stuck list. "2,217 pipelines in 98 s on the title screens...
  nothing compiled in the world after".
- Ours: `pipeline/known.rs:40`, `apps/victory/src/warm.rs:37` (offline only). ~1,000 lines, medium. Picture: none.
  Related small ones: Marvisak 555c112e "pipeline library loads do not wait for a save" (#2179), pr/3071 "a pipeline
  waited for in vain is not waited for again"; avatarmanz 1162d38 shader cache keyed by `TRANSLATOR_VERSION` (no
  re-translation after every rebuild of ours).

### 11. Fewer global UAV waits around indirect clamps — `VICTORY_CLAMP_BEFORE_UAV=0`, `VICTORY_CLAMP_AFTER_UAV=0` — cost (d)
- Source: mgrein `nv/host-compat` 7c78ad73, b572c9f8 (`indirect.rs`, `host/rec.rs`, `indirect/batch.rs`); ASsDevpile
  7adf22cf `VICTORY_CLAMP_WAITS=0` (port of soldsawn 830ca05). The leading null-resource UAV barrier is skipped when
  nothing UAV-capable was recorded since the last one; the trailing one is replaced by the dst transition alone.
- Ours: `indirect.rs:523-529` (`first`/`second` both start with `uav_barrier()`), `:3016`, `:3845-3852`. ~280 lines.
  Marked "experiment, off", no frame time published. Risk medium (ordering). Picture: none if correct.

### 12. Smaller items
- Shader constant table starts after the instructions: Marvisak e04c0781 (#2143), `emit/lower.rs` `code_table_start`.
  Ours `lower.rs:1746` takes the table from the lowest offset any load names (up to 64 KiB per program). Measured gain
  is under vkd3d/NVIDIA Vulkan ("one 57 KiB table made it reserve GBs of video memory"); on Windows it only shrinks
  shaders. 50 lines, low risk, re-translation. Costs (c)/(f).
- MImi8superr b237173c: no VRAM budget query for uploads into existing tiles; d43a6666: mapped levels as a bit mask
  (`tiles.rs`, `resource.rs`). Cost (c), ~40 lines.
- Rubenpannen c8c10d9b `VICTORY_RECHECK_LIVELY`: picture-latency fix (repainted textures up to 11 frames late), not speed.
- mgrein 87274d99 `VICTORY_FRAMES_AHEAD` (`host/latency.rs`): input latency on NVIDIA, not frame time.
- Still open from the earlier review (not in victory-fox): `SHOW_NEW_LATER` (device-loss fix, 4cd2d13),
  `WINDOW_BUDGET_MB` (ffc2cf6; superseded by item 2 if that works), texture store (9386b82/7e82cf5), `UP_QUEUE` /
  `JOIN_BARRIERS` (d3a6c2c: "joining barrier calls -2.4 of 79.5 ms" in replay).
- Already ported since that review: `METER_THREAD`, `UNDER_FUSED`, `AHEAD_SEARCH`, `MEMORY_MAP`.

## 3. Ideas seen but not finished or not measured
- Coldtrain "Relay Next" R1-R13d (`relay_next.rs`, 3,500 lines, ~20 switches `VICTORY_RELAY_NEXT_*`, RX 6800): flights
  kept across scheduler looks until a real dependency (R12e), trailing compare recorded after the title lists are sent
  (R12b), hash-only refresh of card-resident copies (R6). No numbers in commits; R5 parallel landing turned off as costly.
- Marvisak pr/2425 (59ccd520): contiguous physical pages mapped as one view with `MEM_COALESCE_PLACEHOLDERS`
  (`map_run`, `coalesce_placeholders`) — fewer mapping calls for (b); written for Linux map counts, unmeasured, second
  commit adds a fault-retry window (risky). Ours maps page by page (`xmem.rs:1530-1570`).
- Marvisak pr/2494 (1a206866) import committed physical-page sections in place; pr/3035 write faults save bytes without
  the guard lock; pr/2914 marked pages written back on several threads; pr/3139 one diff pass for dirty heap ranges;
  pr/3144 replaced layouts released off the submitting thread. Other architecture; mechanisms only.
- mgrein 7725f06b `VICTORY_COLD_GUARDS`: big rarely-written ranges moved from kernel write-watch to guards — measured
  worse there (da160bc7: "290-380 MB sent up per frame and 8.3-9.2 fps... without 68 MB and 12.7 fps"). Do not port.
- LloydNicholson 85e089c3 `VICTORY_TOLD_HOLES` (115.1 -> 107.8 ms) is for Wine only; Windows already answers.
- FQY55kai d9c17525 `VICTORY_UNIFIED` (shared-memory buffers like the console), experimental, off.
- avatarmanz `VICTORY_LANE_CORE` (own core for the submit thread), `ASK_SPREAD`/`ASK_NORESET`: off, unmeasured or removed.
- Nothing found for the mesh programs' fixed 128/128 output counts, nor for VRAM fragmentation by moving live copies.

## 4. Not acceptable (picture traded for speed)
- avatarmanz: `VICTORY_AMBIENT` / `AMBIENT_SETTLE` (9576c45, b4ba4bb), `VICTORY_OUTPUT`, `VICTORY_RENDER_SCALE`,
  `VICTORY_UPSCALER`, `VICTORY_FFX_SCALE` (e151e71, 4d7185e), Series S two-plane path, ray tracing off by default.
- mgrein 5340e71f (largest levels of streamed BC textures left out on <= 12 GiB), PastiiJay 15a4c4d3 (textures capped
  at 1024 px), FQY55kai 6c79025b / ad3lre ef8383b1 `VICTORY_TEXTURE_MAX_DIM` (mips dropped), Steenax6 820bbaac.
- dkdknight `VICTORY_BVH_NO_HIT`, `VICTORY_RT_PASSES_OUT`, `VICTORY_SHADER_LOOP_LIMIT` (their own note: "sky slightly
  wrong"), `SHADER_NO_ARRAYS`/`FAST_MEM`/`FAST_FLOW` ("heads and the frosted glass went missing").
- YOGESH9139 `VICTORY_AMBIENT_FLOOR`, ASsDevpile "ray tracing mostly off" + `VICTORY_BVH_MISS`, DLSS/frame generation
  (AwkGrep256, PastiiJay, canalmaloqueru-design): generated or upscaled frames.

## 5. Suggested order
1. No-code A/B: `VICTORY_INPLACE=1` on the current build.
2. Items 3 and 4 (small, low risk, lane time). 3. Item 1 (port 1), then item 2's pool half after measuring commit.
4. Items 5, 9, 8. 5. Items 6, 7 (title patch / full re-translation), 10, 11.
