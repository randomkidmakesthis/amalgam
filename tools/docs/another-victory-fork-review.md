# another-victory-fork (kaibatsuu) — review, 2026-11-26

Clone: `C:\GTAVI\Tools\ref-forks\another-victory-fork` (push URL blocked; remote `upstream` = avatarmanz, fetch only).
Bundle: `C:\GTAVI\Tools\patches\another-victory-fork-full.bundle` (all 3 branches). Nothing built, run or extracted.

## 1. Safety: clean

- Its 18 own commits change only `.rs`, `README.md`, `docs/` (34 files, +2,372/-113). No binary, archive, script,
  `build.rs`, CI, hook, `.cargo` or submodule change against avatarmanz. `Cargo.lock` is byte-identical to
  `victory-fox\Cargo.lock` (89 packages, all crates.io).
- Process launch: only `victory.exe` starting itself (`current_exe`) as a child for pipeline-compile isolation
  (`pipeline/make.rs`, `apps/victory/src/translate.rs`), temp file written and removed. `LOCALAPPDATA` read for a cache
  path. No network, download, registry, credential code. No text addressed to AI assistants (README only credits Claude Code).

## 2. What it is

- Base: avatarmanz/VIctory-7900XTX. Shares `0a43566` with us (= cool_fox minus its 6 commits = our base under `coolfox-base`).
- `combined` (default, 59d6bc0) = base + 25 avatarmanz commits (to `85d5641`, 11-25 21:48) + **18 own commits**.
- `main` (9576c45) = plain avatarmanz snapshot, 146 commits past our base (upstream is 7 further). None are in victory-fox.
- `claude/brave-shannon-7bfuao` = avatarmanz branch + 1 commit (pipeline workers low-memory mode).
- Own work is about AMD driver crashes (pipeline isolation, crash bisect, fallback cache), an opt-in submit split,
  a DirectStorage worker count, memory notes. **No page-mapping, xmem, CopyPageMappings, map-lock or streaming code of its own.**
- README claims, in its own words: "about 53 ms a frame (about 19 fps)... single frames up to about 200 ms". It does NOT
  claim a lag-free prologue. Tested on one PC: 9800X3D, **RX 9070 XT, 64 GB RAM**, diag build, 34 GB private, commit limit 70.6 GB.
- Default launch (`scripts/play.ps1`): TEXTURE_NO_WAIT=1, RELAY=2, WIDE_BACK=1, CARD_REHASH_RUNS=1, JOIN_COPIES=1,
  INDIRECT_WINDOW=1, RELAY_COPY=1, CARD_STEPS=2, INDIRECT_BATCH=2, MAP_THREADS=1, TITLE_PRIORITIES=0, CARD_AWAKE=1,
  SHOW_AFTER=1, THIN=1, JOIN_PARTS=8, UPLOAD_AHEAD=1, D3D12_DRED=light, SKIP_PIPELINES=none, **AMBIENT=0.8**, and
  **no BVH builder** unless `-RayTracing`. All of these switches exist in victory-fox (base). Quality cuts: see section 6.

## 3. Why it probably "does not lag" there (verdict)

1. **64 GB RAM.** Our 48 s of slow page-mapping calls and the 37 GB commit are low-free-RAM effects; that PC never gets there.
2. **Pipelines made before the run**: `victory shaders`, `victory warm --used` after a `VICTORY_PIPELINE_BOOK=1` run, and
   `warm-done.txt`. victory-fox has all of it (`apps/victory/src/warm.rs:37`, `pipeline/known.rs:40`); check our runs use it.
3. **Picture cuts**: no ray tracing (about a third faster by its README), ~1,000 RT-resolve compute programs not compiled
   at the world's first frame, stand-in ambient light. Not acceptable for us.
4. **avatarmanz lane fixes we lack** (rows 1-6 below): periodic stalls of the presenting/submitting thread removed.
None of 1-3 is portable speed; 4 is.

## 4. Ranked port list (fork file:line → victory-fox file:line)

| # | What | Theirs | Ours | Switch | Their measured claim | Removes which cost | Size / risk | Picture |
|---|---|---|---|---|---|---|---|---|
| 1 | Memory-map walk rarer. Diag builds walk ~250,000 regions with VirtualQuery for 0.9 s every 30 s on "victory-memory-map", holding the process's address map; other threads' memory calls stall behind it | `main` fb28d49: `copystat.rs:598-609`, `:836` | `copystat.rs:532` (on/off only), `:747` (`% 6`) | `VICTORY_MEMORY_MAP=<n>` (theirs 60, later "every 5 min") | two askings of 11 and 13 ms each ended with a walk's VirtualQuery (16.6, 13.1 ms); without it none over 1.5 ms | part of the 48 s mapping calls (map/unmap need the same address-map lock) and of >150 ms frames. **Test first with no code: `VICTORY_MEMORY_MAP=0`** (we run `target/diag` without it) | 20 lines / none | none |
| 2 | Frame meter's 5 s and 1 s lines on their own thread | f36a0fc, 122d354: `host.rs:8700-8730` (`meter_lines`) | `host.rs:9007` (frame rate fn); fox-port-C housekeeping `host.rs:454` | `VICTORY_METER_THREAD` (on) | 30 ms every 5 s + 3 ms every second off the presenting thread; fb28d49 adds 10 ms of VirtualQuery in those lines asked once per address | periodic hitches on the lane (72 x 30 ms per run) | ~200 lines in host.rs, conflicts with our added log lines / low | none |
| 3 | Title bytes under flights compared and put in one pass | 6f3ede5: `shadow.rs:2198` (`under_fused`), `:3622` (`merge_under_writes`), `host.rs` timing line | `shadow.rs:3631`, caller `host.rs:3216` | `VICTORY_UNDER_FUSED` (on) | step was 7-13 ms a frame on the lane (190,000 runs in 400 pages); with RT 1.96 → 1.87 ms, frame unchanged in that run | read waits on title memory behind flights (15-24 ms/frame) | 220 lines, one file / medium (byte-exactness; A/B-able) | none |
| 4 | Heap records read in place, each once, AVX2 compare; spans' lock skipped by a bitset | 15ab94e, 8822ed4, a284ca7: `descriptor.rs:536-555`, `safecopy.rs` (`changed`), `descriptor/host.rs:229-248` | `descriptor.rs:297` (`sync_heaps`), `descriptor/host.rs:231,243` (`spans.lock()`) | `VICTORY_SYNC_IN_PLACE` (on), `VICTORY_SPANS_QUICK`, `VICTORY_SYNC_FINE` | on by default after run heapsync-ab2 (figures in 288af58) | lane CPU per frame; lock contention with refresh threads | 370 lines / medium | none |
| 5 | Asked-ahead ranges found by binary search; one hold for both parts of the asking | `main` c83e54c, e000e02: `watch.rs` (`ask_ahead`, `ask_once`) | `watch.rs:315`, `:530` | none | 10,000 lookups a frame each walking a few hundred ranges; asking 5.4-5.9 ms a frame; second hold cost 0.5 ms a time | lane CPU; time spent holding the watch state that readers wait on (near our `watch.rs` ~2395 lock) | 150 lines / low | none |
| 6 | New pooled texture shown in its slot only after earlier-sent lists ran | 4cd2d13, 91018a5: `descriptor/host.rs:335-350`, `pool.rs` | `descriptor/host.rs:285` (`VICTORY_SHOW_AFTER`), `:3284` (`mirror`) | `VICTORY_SHOW_NEW_LATER` (on) | "every device loss with the pool in the report" was this page fault | none of the lag costs; removes device losses in long runs | 150 lines / medium | none (slot shows old/neutral texture a few ms longer) |
| 7 | Idle title-window copies capped under RAM pressure | `main` ffc2cf6 (PR #77): `shadow.rs:2938` (`window_budget`), `host.rs` +140 | `shadow.rs:5184-5236` (window grid) | `VICTORY_WINDOW_BUDGET_MB` (off) | none given; memory.md: 927 windows hold 3,872 MB, 1.4-1.9 GB never asked for | commit 37 GB on 32 GB → fewer slow mapping calls | 240 lines / medium (evicted window is copied again) | none |
| 8 | Residency budgets as avatarmanz re-did them from cool_fox | `main` 706305b: `residency.rs` (233 lines) | our fox residency module (compare values only) | `VICTORY_RESIDENCY`, `_LIMITED` (auto at <=32 GB) | none | commit/RAM pressure, upload bursts (128 MiB a frame cap) | compare only / low | prefetch limits delay sharp levels: check against our rule |
| 9 | Submission thread split: one prepares and sends, one waits and settles | own 2a8148f: `queue/behind.rs:715-805`, `queue.rs:213`, `host.rs:2621,5271` | `queue/behind.rs` (with `VICTORY_ASYNC_SUBMIT=1`, which we run) | `VICTORY_SUBMIT_SPLIT=1` (off) | none | landing waits no longer hold up preparing the next submission | 270 lines / medium-high (ordering) | none |
| 10 | Texture store: larger levels kept on disk by digest of the smaller ones | `main` 9386b82, 7e82cf5: `texstore.rs` (294), `texture.rs` | none | `VICTORY_TEXTURE_STORE=<folder>` (off) | walking frames ~10 ms slower until moved to its own thread | repeat uploads / churn (43 GB let go, 9 GB made again): sharp levels without waiting for title memory | 400+ lines / high, up to 4 GB RAM | none claimed (shows sharper sooner) |
| 11 | Pooled textures at 64 KiB multiples | c278f0d: `pool.rs:432-450` | `pool.rs:314` (`grow`) | `VICTORY_POOL_SMALL=1` restores | none; fixes card reading past a small piece | none; **raises** VRAM use slightly (worse for our 16.3 GB raw) | 140 lines / low | none |
| 12 | Up queue for uploads, joined barrier calls | d3a6c2c, 53ed64e: `host/upq.rs`, `host/rec.rs` | none | `VICTORY_UP_QUEUE=1`, `VICTORY_JOIN_BARRIERS=1` (both off) | replay: uploads are 5-6 ms of the drawing queue; barriers -2.4 of 79.5 ms; live run showed the up queue unused | texture upload bursts (13 s) — unproven | 390 lines / high | none |

Already in victory-fox (no port): DirectStorage worker per physical core (`dstorage.rs:541-565`,
`VICTORY_DSTORAGE_WORKERS=physical`), pool growth back-off and per-heap bindless refresh (taken by them FROM cool_fox
ab75ac3, 18bdf79), `VICTORY_FILL_TITLE` (`host.rs:2869`; avatarmanz measured =2 as 60-71 ms against 64-65 off with the
lane and turned it off again, e000e02), RT-resolve skip (`pipeline/make.rs:71-86`).
Not useful here: `VICTORY_CACHE_CORES` (7c3d839, dual-CCD X3D only; a 9800X3D has one cache class),
`VICTORY_ASK_SPREAD` / `ASK_NORESET` / `ASK_GAP` (measured slower, removed), AMD compiler fixes, pipeline isolation
(first launch "stutters for a few minutes"; NVIDIA has no such driver crash), `VICTORY_FRAME_INTERVAL_MS`.

## 5. Visual fixes we lack (all on `main`, none on `combined`)

- Wave32 hosts (NVIDIA is one): tiled cascade shadows translated exactly (4fd34fd, PR #82); LDS-flag lane regions as two
  host waves (ad0f9dd, 25025d2, key in e29bfe2, `VICTORY_LDS_FLAGS`); first-lane reads (431cab5); uniformity proofs
  (7348b80, abbbd6b). Likely relevant to flicker/wrong shadows on our card.
- Ray-tracing structures: wave64 BVH builder programs counted right on 32-lane hosts (06fe22a, PR #66 "NVIDIA"),
  BVH prefix scans on wave32 (PR #142, 7bf91de). Worth a look for `VICTORY_BVH_BUILDER=1` on the 4080.
- Sampler heap 2,048 slots with a null sentinel (dd67ae7); structured buffer slots kept raw without descriptor twins (b68e65e, 888bf1e).
- Shaders: one interpolation mode per signature row (0dc5d1b), level query without array slice (41d3184), XDAT kind 0x0c
  (e70e031): "every shader the title uses translates (25,790 of 25,985)".
- Two-plane present compose (85a663d; from cool_fox, we have `present_planes`).
- Purple/glowing bushes, purple temporal artifacts, device methods 0x1c8/0x218: **nothing new**; no commit mentions
  them. README still says "trees and far buildings flicker in places".

## 6. Not acceptable (picture traded for speed)

- `VICTORY_AMBIENT=0.8` in play.ps1 and `VICTORY_AMBIENT_SETTLE` (9576c45, b4ba4bb): stand-in lighting.
- Ray tracing off by default; 904b52e leaves out `CS_RaytracingResolve` (unless `VICTORY_RT_RESOLVE=1` / BVH builder) and
  `CS_ScreenProbes_ActivateRays` with `VICTORY_ACTIVATE_RAYS=0`: skipped passes. (Same skip exists in our `make.rs:71-86`.)
- Crash fallback (353bc15, 4cfb8a3): a shader's culprit instruction lowered as "write 0" (`VICTORY_DISABLE_MNEMONIC`) —
  wrong output by design; pipeline isolation leaving pipelines out.
- Series S two-plane path, `VICTORY_RENDER_SCALE` / `VICTORY_UPSCALER` / `VICTORY_FFX_SCALE` on `main`: lower resolution.
- `VICTORY_TEXTURE_NO_WAIT=1` (we run it too) and residency prefetch caps: verify they only delay, never drop, sharp levels.

## 7. Next steps

1. Run the 6-minute route once with `VICTORY_MEMORY_MAP=0` (no build). If over-150 ms frames drop, port row 1.
2. Port rows 2, 3, 5 (small, picture-neutral, lane time). Then 4 and 6.
3. Row 7 with our low-RAM numbers; row 10 only if repeat uploads stay the top cost.
4. The real gap to that PC is RAM: the mapping lock (`watch.rs` ~2395) and commit work stay ours to solve; this fork has nothing for them.
