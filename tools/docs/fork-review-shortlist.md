# Fork review: what to bring into our tree

Reviewed read-only (clones in `Tools\ref-forks`): Trexies `VIwork`, WillyboPeep `victory-all-forks`, JoshuuDev
`victory-amd`, SnickeredMG `specific-assets-vctry`. None is usable as a base: all lack our NVIDIA pieces (paired wave64,
mesh link, async submit, copy queue). ShakiraRED-dt is being reviewed separately.

## Strongest lead for frozen faces / wrong focus / low LOD

Our own tree documents the gap (`draw.rs` ~1400): shader writes through views a shader picks by index (bindless) "stay
in the host copy" — they never reach game memory or other copies of the same bytes. If the face network hands results
between passes that way, or the GPU writes a value the CPU reads, readers get stale data or zeros.
Fix exists in all-forks: commits a1ce531 + 7cdc3ef + a498c0c (shader marks the heap slot before each such store; the
host declares those views written and copies them back).

No fork implements real GPU queries. Ours answers: occlusion = always visible; timestamps = wall clock at list end.
A quality governor reading GPU time from timestamps would see a GPU ~6x over budget — cheap to test with a switch.

## Shortlist, in order

| # | What | Source | Author's number | Addresses |
|---|---|---|---|---|
| A | Bindless store marks → copy-back of picked views | all-forks a1ce531, 7cdc3ef | replaces "960 MB for some 30 lists a frame" | faces / focus / LOD |
| B | Full-refresh check (find missed CPU writes) | Trexies a63b98f | diagnostic | faces (diagnosis) |
| C | Fold small buffers into their window | Trexies b7beac9, 1c8b2a5 | 179 → 160 ms; forced waits 4.5 → 1.0 | submit thread |
| D | Constant-buffer fills and buffer copies on the card behind flights | Trexies 25fbcce → 9dd3e8f → e6d37be | one 500-byte buffer cost a 52–80 ms wait | submit thread |
| J | Remap lock per byte range | all-forks c79c751 | frame 801 → 486 ms; textures 371 → 47 ms | texture pop-in |
| P | Longer pipeline waits while a world loads | all-forks 4eac58d | 130 one-shot passes dropped: "walls, floor, ceiling and faces never came back" | cold run, lost passes |
| I | Disjoint refresh ranges | Trexies 3cd984d | — | submit thread |
| N | Empty ray walkers while the builder is off | all-forks f54cb72 | GPU time 456 → 176 ms (4090) | GPU time |

A/B candidates: drop the two global waits around each indirect clamp (Trexies 6b358fe, "~73 % of the card's time");
halve refused write-watch ranges (dce5b97); banded detile (58c7bb0); compare 4 KiB pieces before sending (4d08198).

## Do not take

Series S mode / lower display mode; indirect window cap above 64 ("stair-stepped shoulders"); Trexies d8d475c (ours is
safer); native 0x218 ("picture turned grey"); `VICTORY_RT_OFF`; the present-planes export as is.

## myokex/victory-amd @48451e9 (closest relative: same ancestor 0b0074e as our base)

Take (hand ports; whole-commit merges would endanger our NVIDIA shader work):

| # | Piece | Why |
|---|---|---|
| 1 | Descriptor fixes: mirror the bytes that were checked, forget stale copied slots, zero-slot fix, records across a 64 KiB page | a slot stuck on an old view keeps feeding old buffers/textures to draws |
| 2 | Early depth flag for pixel shaders the console depth-tests first | a glass prepass overwrote depth and motion of the character in front of it — likely tied to the blurred centre in cutscenes |
| 3 | Pipeline waits while loading (without leaving ray shaders out) | one-shot passes dropped: "walls, floor, ceiling and faces never came back" |
| 4 | Streamed textures read from their first mip; unguarded/movie textures re-checked; read around missing pages | "109,638 views were taken for stale in 8 minutes and showed nothing" |
| 5 | DirectStorage page array read when the request runs | late streaming |
| 6 | Write-watch fix for blocks noted absent | missed writes leave stale copies |
| 7 | Join consecutive parts into one submission (29 → 9 a frame, ~6 %), 15-byte clzero loop, section-handle fix | speed; the handle fix is needed for copy-back through our own view |
| 8 | Indirect clamp ring / early (measured on an RTX 5090: clamps 33 → 19 ms of GPU time) | GPU time; heavy port |

Do not take: leaving ray shaders out, `VICTORY_TABLE_BUFFERS` (stale views by design), `VICTORY_HEAP_FAST` (never run),
2^19 dynamic slots (AMD-only), their wide hash / staging blocks (ours equal or better), `VICTORY_SECTION_MB` (opt-in at most).
Thin LTO + one codegen unit: unproven (their figure is confounded); A/B on the same route before adopting.

## Neptune_VI (zm0kiemedia) @3cdf588, clone in `ref-forks\neptune-vi` (read only, never built or run)

Safety: clean. No exe/dll/archive (`bcpack-tables.bin` is byte-identical to ours); 89 crates, all crates.io, same set as ours; build.rs only hash sources / call `git rev-parse`. `discord.rs`: local pipe `\.\pipe\discord-ipc-N`, no network. `screenshot.rs`: F12 writes a PNG to `%USERPROFILE%\Pictures\Neptune`. Scripts: `-Stats` starts a hidden PowerShell (`-ExecutionPolicy Bypass`) on hard-coded `D:\Neptune-VE\memwatch.ps1`; `-FrameGen` copies OptiScaler in as `dxgi.dll`.

Same code as our zip: first commit 347cac9 == zip. 7 later commits (2026-11-25 21:16–21:44): 1bf102d DRED report in release builds; fb0c93c `VICTORY_INDIRECT_RAW=3` (single DRAW/INDEXED records checked on the CPU, then run unclamped; mesh records stay clamped; RAW=2 hung the GPU on 9800X3D + 5070 Ti; no number); c25ee37 launcher default; 4 README commits.

### Why Neptune shows more fps (their default launch, `play.ps1`)

- Their own figures (i7-14700, 5070 Ti, 64 GB, release build): Series S 16–17 fps first room, 9–11 fps open world at 6,500–9,000 draws (90 ms, GPU 35 % busy); Series X 4K "about 14" first room. Ours: 115 ms at 5,000–7,500 draws, 1440p→4K, diag build with frame log + card meter. Like for like the gap is roughly 90 vs 115 ms, not a multiple.
- (b) Ruled out, named for honesty: default is `--console series-s` when a PC FSR DLL is found (1280x720 → 1920x1080, a quarter of our pixels; the game's upscaler calls go to the PC DLL, not bypassed) — by their numbers ~15–20 % in the first room. `VICTORY_INDIRECT_RAW=2` (zip launcher): no clamp of any indirect record — theirs measured 83–97 ms of GPU a frame for clamps before `CLAMP_AHEAD`. `ambient.rs` (on, 0.6) overwrites the probe resolve's output, skips no pass. Not used by default: `NO_COPY_BACK`, `ASYNC_SUBMIT`, frame generation (`-FrameGen` only; the overlay counts the title's presents, so generated frames are not counted). RT resolve shaders are made; builder off as ours.
- (a) Real speed-ups: the table below plus launcher values ours has but does not run with: `VICTORY_MERGE_RUNS` (theirs 100000: "64–73 ms a frame in settles" at the 512 cap — our ~40 ms "waiting for the card" fits this), `VICTORY_CARD_WINDOWS_MB=4096`, worker threads (theirs 12; ours cores/4 = 4 on this CPU, no switch).
- (c) Unknown: cost of our diag build + frame log + card meter (their figures are release); CPU (20 cores vs 8); why our game lists take 82 ms on the card when our own work there is 7 ms. Cargo profiles are identical (plain release, no LTO, no target-cpu).

### Missing in victory-dev (grep-checked), frame time first

| # | Piece (Neptune file:line) | Switch, their default | Author's number | Port / risk |
|---|---|---|---|---|
| 1 | Quiet 1 MiB blocks asked once a frame; wider askings (`watch.rs:568`, `:407`) | `VICTORY_ASK_QUIET` 0 (launcher 8), `VICTORY_ASK_WIDE_MB` 0 | ~3,000 askings over 11 GB = a third of the submit thread; 36 ms of page resets | medium / a write is seen a frame late; zm0kie suspects dark objects |
| 2 | Copy-back writes not re-read as title writes, reset together (`watch.rs:771`, `:851`) | `VICTORY_QUIET_BACK` off (launcher 1) | an eighth of the submit thread; 650 reset calls x 50 µs | medium / medium (ours has an uncalled `watch::forgot`) |
| 3 | Send only bytes that differ (`shadow.rs:1884`) | `VICTORY_COPY_CHANGED` off (launcher 1) | ~1,500 64 KiB pieces a frame round-tripped | medium / low |
| 4 | Single-record clamps in the list prelude (`indirect.rs:487`) | `VICTORY_CLAMP_AHEAD` on | 16–21 µs a draw, 83–97 ms GPU a frame at ~5,000 mesh draws | large / medium |
| 5 | Per-thread buffer look-up cache (`shadow.rs:3864`) | `VICTORY_LOOKUP_CACHE` on | a tenth of recording threads' time | small / low |
| 6 | Hot blocks left unguarded; following blocks opened (`watch.rs:126`, `:1952`) | `VICTORY_GUARD_HOT` 0, `VICTORY_GUARD_OPEN_AHEAD` 1 | a tenth of submit / recording threads on the guard lock | medium / medium |
| 7 | Worker count, list pool, hash interval (`shadow.rs:1446`, `list.rs:260`, `texture.rs:1092`) | `VICTORY_WIDE_THREADS` (12), `VICTORY_LIST_POOL_MOST` (2048), `VICTORY_TEXTURE_HASH_EVERY` (32) | list making a tenth of submit thread; hashing 5 ms a frame | trivial / low |
| 8 | Table waits for dynamic slots (`descriptor/host.rs:1154`) | `VICTORY_DYNAMIC_WAIT_MS` 30 | 55 mesh draws a frame not drawn; ours refuses at once (`:1201`) | small / low — picture |
| 9 | Idle time by queue room (`host.rs:196`, `:227`) | `VICTORY_VRAM_IDLE_BY_ROOM` on, `VICTORY_VRAM_IDLE_MS` (15000,2500,1000) | 46,803 copies / 11.4 GB re-made in 200 s, black until then | small / VRAM |

Ours under another name: `UNDER_WRITES_RUNS/MASKS/WORDS` = `MERGE_RUNS`/`MERGE_MASK`/`MERGE_GAP`; `CARD_COPIES` = `GPU_BUFFER_COPY`; `CONSTANTS_WAIT` = `FILL_WAIT`; `STAGE_POOL_MB` = `STAGING_POOL`; `RELAY_OWN_QUEUE` ~ `QUEUE2`. Identical: `query.rs`, device 0x1c8/0x218. Nothing on depth of field or the RT builder. Also theirs only: `texback.rs` (`VICTORY_OCEAN_READBACK`, off: GPU-written textures copied back by kind, for boats). Dead in their source: `VICTORY_DESC_DATA`, `VICTORY_WAVE_OWN_LDS_GROUPS`.

Faces: no fix; their notes say "facial animation does not play". Related: bake budgets 40 / 10,000 ms (`draw.rs:94`: 9,371 textures due at the first bake → 504; "a head left black, a hand of noise"; ours 12 / 1,500, same switches); pooled staging zeroed (`texture.rs:487`); DirectStorage stale array (being ported).

Page map/unmap: Neptune has no `note_unmapped` and no 64-range fallback — a remap only makes slots due; host copies go by idle time alone (row 9). Upload on DirectStorage write is as ours (`VICTORY_PAGE_WRITES` default guards only; `=1` adds slots).

Free tests (exists in ours; their value, our default): `VICTORY_MERGE_RUNS=65536` (512), `VICTORY_CARD_WINDOWS_MB=4096` (0), `VICTORY_BAKE_TEXTURES_MS=10000` (1500), `VICTORY_EARLY_TEXTURES_MS=40` (12), `VICTORY_WINDOW_TAILS=1` + `VICTORY_WINDOW_KB=1024` (off, 4096), `VICTORY_LATE_WAIT_LISTS=1024` (256), `VICTORY_TEXTURE_STAGE_MB=512` (192), `VICTORY_CARD_BUFFERS_MB=1536` (1024), `VICTORY_PIPELINE_WAIT_MS=2500` (500; our runs 20), `VICTORY_D3D12_BACKGROUND=0`, `VICTORY_PAGE_WRITES=1`. zm0kie's warnings: `WATCH_4K` black water, `SLOT_EVICT` crash, `FAST_COPY` faults on a guard page on Ryzen.

## Neptune rendering differences

Read-only diff of `ref-forks\neptune-vi` @3cdf588 against `victory-dev`, 2026-11-25. Nothing built or run. "Ours has?" was checked by grep for the switch or function in `victory-dev\crates`.

Short answer: Neptune has **one** thing that changes the lit picture and ours lacks — the ambient stand-in pass. Everything else that makes the picture (translator, blend, samplers, queries, RT builder, loop caps, dispatch parts, device 0x1c8/0x218) is the same code or ours is ahead.

### Picture items Neptune has and ours does not (by visible benefit)

| # | Item (Neptune file) | On screen | Switch, their default | Ours | Port / risk | Shader cache |
|---|---|---|---|---|---|---|
| 1 | Ambient pass (`ambient.rs`, 397 lines; hooks `draw.rs:2177-2300`, `pipeline.rs:64,119`) | Areas not in direct light get an even warm light with a depth-only crease darkening, instead of black. Overwrites the output of `CS_ScreenProbes_TemporalResolve_*` (UAV at root parameter 1, table place 176, R9G9B9E5) right after its dispatch; the title's own lighting then applies it. A stand-in for the missing ray-traced probes, not GI: their notes say ceilings can stay black, shiny surfaces sparkle, rooms and water look off | `VICTORY_AMBIENT` 0.6 (`off` = title output; `_TINT`, `_SHADE`, `_FILE`) | no (`ambient`, `ScreenProbes`, `resolves_light`: 0 hits) | medium: file is self-contained HLSL via FXC, but it records a pass of ours into the title's list and needs our list-state restore (their `after_our_pass` does not exist in ours); slot 1/176 is fixed per program variant / medium | no (victory-gpu only) |
| 2 | Dynamic-slot wait (`descriptor/host.rs:1154`) | 55 mesh draws a frame (of 7,800) no longer vanish for a frame: flicker of world pieces | `VICTORY_DYNAMIC_WAIT_MS` 30 | no (ours refuses at once, `descriptor/host.rs:1201`) | small / low | no |
| 3 | Pipeline wait default | One-shot passes (model unpack at load: "walls, floors, ceilings and faces missing for the session") are not lost | `VICTORY_PIPELINE_WAIT_MS` 12,000 window per kind in code, 2,500 in their launcher | code is there (500 + load wait 4 s / 12 s compute for 120 s); **our runs set 20** | setting only / frame hitches | no |
| 4 | Catch-up allowance for the texture queue (`draw.rs:446`) | Shorter stretch of black materials in a world's first minutes (512 MB + 1.5 s a pass for 60 s) | `VICTORY_TEXTURE_GATE_S` 60 | different design: ours paces by clock (`VICTORY_TEXTURE_CATCHUP`, `draw.rs:514`); not a gap, an A/B | – | no |
| 5 | Idle textures kept while the card has room (`host.rs:196`) | Textures do not come back black after a camera turn | `VICTORY_VRAM_IDLE_BY_ROOM` on | no (row 9 of the table above) | small / VRAM | no |
| 6 | Leaving out RT programs | None on the picture; shorter first load. `CS_RaytracingResolve` (~1,000 programs) and `CS_ScreenProbes_ActivateRays` skipped | `VICTORY_RT_RESOLVE=0`, `VICTORY_ACTIVATE_RAYS=0`; both made by default | no; ours instead has `VICTORY_BVH_HOLD` (drops dispatches that walk unbuilt structures) | small / low | no |
| 7 | `VICTORY_NULL_MAGENTA=1` (diag) | Missing texture shows magenta, not black: tells "texture not there" from "lit black" | off | no | small / none | no |
| 8 | Series S path + PC FidelityFX redirect (`victory-hle\fidelityfx.rs`, `external.rs`, `VICTORY_RENDER_S`, `VICTORY_FSR_DLL`) | 720p→1080p through a PC FSR DLL; another upscaler, not a fix | off unless launcher finds the DLL | no | large / medium | no |
| 9 | `texback.rs` | none (boats float: ocean height maps copied back for the CPU) | `VICTORY_OCEAN_READBACK` off | no | medium / low | no |

### Shader translator (`crates\victory-shader`)

Nothing in Neptune's favour. Six files differ; every hunk is ours-only except comments. `VICTORY_DESC_DATA` ("5 more shaders", launcher says 40 in `start-neptune.ps1`) and `VICTORY_WAVE_OWN_LDS_GROUPS=64,128` are set by their launchers but **no code reads them**: `emit\desc_data_tests.rs` exists and is not in any `mod`. `VICTORY_WAVE_WIDE`, `VICTORY_WAVE_PAIRS`, `VICTORY_WAVE_DEEP_SKIPS`, `VICTORY_SHADER_OPT`, loop caps: identical. `VICTORY_DROP_DXIL` (theirs only) is RAM, not picture. Cache key: both key on the translator's sources and switches; none of the rows above touches it.

### Checked and equal (no gain from Neptune)

(a) RT builder: same rule, off unless `VICTORY_BVH_BUILDER=1`, and their own doc says it "still stays off in a real run" (pipelines refused or untranslated); `VICTORY_BVH_QUARANTINE` identical; no crash fix. (c) shadows: no difference in shadow maps, trace, loop caps, bias, comparison samplers. (d) lighting: no tiled-light or exposure change. (e) integer-target blend / LogicOp off: both have it (ours `GraphicsState::host_blend`). (g) device 0x1c8/0x218 and `query.rs`: byte-identical. (h) formats/sRGB/samplers: no Neptune-only change. (i) depth of field, autofocus, TAA, motion vectors: zero hits in either tree. Compute dispatches over 65,535 groups: split in both. Old texture kept in a slot until the new one is uploaded, pooled staging zeroed, `HEAP_RECHECK`, `TEXTURE_SWEEP`, `VIEW_GUARD`: in both.

### Common to both, unsolved (Neptune's own words)

Ray-traced light, shadows and reflections do not run ("nothing is lit by ray tracing yet"); five shadow-mask trace programs untranslated; "facial animation does not play"; "objects and clothes can still turn black for a moment while the game swaps their textures"; "parts of the world are missing behind some doors and on some roofs"; "some shots of the opening cutscene come out white"; lamps dark in one run; frame time jumps while streaming. Nothing on cutscene focus/blur, stand-in NPCs or glowing eyes.

### Ours is ahead

`VICTORY_EARLY_DEPTH` (early depth-stencil declared: glass prepass no longer overwrites a character's depth and motion — bears on TAA/DoF inputs); `VICTORY_HEAP_MARKS` (shader stores through picked views copied back to title memory); `VICTORY_SAMPLER_SANITISE` (LOD clamps, bias, address modes); `VICTORY_BVH_HOLD`; `VICTORY_DISPATCH_PARTS` switch; mesh link; `Root::Written`; unmap handling.

## Fork updates checked 2026-11-25

Fetch only, nothing merged, built or run. "Ours" = grep of `victory-dev\crates`. New commits on the tracked branch: soldsawn `a14d8c8`→`7b7b952` 61 (mostly merged fxceless pull requests); fxceless `465b94f`→`9c184b3` 10; rorodev6 `4ddc0c1`→`8604369` 4, new branch `test-all-fixes`; JoshuuDev 0, new branch `watch-less` (3); myokex `48451e9`→`1544bd5` 4; Neptune `3cdf588`→`7af0fa5` 2 (Agility SDK, README); Trexies, all-forks, snick, ShakiraRED-dt, rexglue 0.

| # | Item (repo, commit, all 2026-11-25) | Switch, default | Author's number | Port / risk | Ours |
|---|---|---|---|---|---|
| 1 | Guards: clean blocks as a bit set; uncommitted runs kept a frame / until the runtime tells (soldsawn f48ce4b, cc642f9, 62fb89d) | none; `VICTORY_GUARD_HOLES_FRAME` off (1, 2), `_RECHECK` 16 | guards 18 → 11–12 → 5.0 ms; VirtualQuery 12,210 → 443 a frame; preparing 48–50 → 40–42 ms; frame 70.7–71.5 → 65.2–67.8 ms | M / =1 low, =2 medium (zeros up to 16 frames) | no (`clean: BTreeSet`, only `VICTORY_GUARD_HOLES` on/off) |
| 2 | GetWriteWatch and guard pass off the journal lock; view ranges skip the asking; `any_view_between` (JoshuuDev `watch-less` 3b270b1, 40f9ce3) | none | submit thread 58 % of its waiting on that lock; frame median 143 → 108 ms; 42–47 thousand askings a second left out | M / medium (overlapping askings must still queue) | no (`ask_kernel` holds `JOURNAL`) |
| 3 | Small title buffers on the card + queue of changed card copies (soldsawn 4126fea, 11924ce; fxceless c36971b) | `VICTORY_CARD_BUFFERS_MIN_KB` off (64 or 0), `VICTORY_CARD_HASH_LEN` off | title lists 68.0 → 49.8 ms of card time, frame 114.9 → 109.7 ms (fxceless); own lists 11.6 → 23.0 ms until HASH_LEN; without the queue `card_dirty` 352–370 ms per 10 s | S + S / low; ~1.2 GB VRAM | no (floor 1 MiB; `card_dirty` walks `CARDED`) |
| 4 | Refresh takes only 4 KiB steps that differ; spread over threads (soldsawn f8f583c, e946217) | `VICTORY_REFRESH_COMPARE`, `VICTORY_WIDE_COMPARE` off | sent to card 97 → 11 MB a frame; frame 72.0–73.6 → 69.9–72.2 ms; wide: within noise | M / low | no |
| 5 | Scattered indirect records clamped once per list; draws that store nothing keep the window (fxceless 465b94f; soldsawn 07d7d91) | `VICTORY_INDIRECT_GATHER`, `VICTORY_INDIRECT_WINDOW_DRAWS` off | 104 → 97 ms (9.6 → 10.3 fps); 7.9 million records re-checked, none differed | M (536 lines) + S / medium | no (compare Neptune `CLAMP_AHEAD`, same goal) |
| 6 | Alias copies once per source write, in the prelude (soldsawn eb0d648) | `VICTORY_ALIAS_ONCE` off | 78 copies a frame = 26–31 ms of card time (RTX 4050); 358 → 333 ms | S–M (178 lines) / medium | no |
| 7 | `changed_runs` 64 bytes at a time by mask (fxceless c36971b) | `VICTORY_CHANGED_RUNS` on | 3.8 ms a frame off the submit thread | S / low | no (8-byte walk) |
| 8 | Guarded blocks looked at again every N frames (soldsawn d55223b) | `VICTORY_VIEW_RECHECK` off (64) | lets `VIEW_GUARD=0` (~30 ms a frame cheaper) keep the wall lost in 7 of 10 runs; 69–76 ms | S, needs row 4 / medium | no |
| 9 | Textures picked by index uploaded on their own thread (soldsawn 233a65f) | `VICTORY_HEAP_WORKER` off (needs `LATE_WAIT`) | pass is up to 250 ms of a frame with a backlog; no result yet | L (539 lines, 10 files) / medium | no |
| 10 | One kept view per physical-page section for DirectStorage writes (soldsawn 912d5c5 + b4fbb74, ec89292) | `VICTORY_XMEM_PERSISTENT_VIEW` off | none (map + commit + unmap per write removed); holds 6.1 GiB of address space | M, `xmem.rs` only / medium | no |
| 11 | Card windows as one buffer in a GPU upload heap (fxceless 79cfdd9) | `VICTORY_WINDOW_UPLOAD` off (needs resizable BAR) | shared memory 5.7 → 3.8 GB, free RAM +2 GB; no speed gain | M / medium | no |
| 12 | Exact buffer views skip the whole 4 MiB window (soldsawn 7d175bb) | none | none | M (138 lines) / medium | partly (`shadow::exact`, no `find_or_exact`) |
| 13 | Title's interface plane presented: HUD, map, menus (rorodev6 8604369; soldsawn 8adb7aa) | `VICTORY_UI_PLANE` on | checked on RX 6700 XT only | M (238 lines) / low | no |
| 14 | DirectStorage reads into physical pages held until the title's Submit (fxceless 79cfdd9) | none | 3 cases, 2,079 reads held; "faces were right in that one launch", not proven | M / low | different (stale-array wait, Neptune port) |
| 15 | Refused pipeline library kept, not saved over (fxceless 79cfdd9) | none | 366 MB of 3,385 pipelines were lost once | S / none | no |
| 16 | Translator: heap-indexed buffer view addresses, 64-bit float wave sums (rorodev6 `test-all-fixes` a8166e0, 75c8bb4); loads four dwords a call (fxceless c36971b, `VICTORY_SHADER_OPT` bit 64, off) | — | 106 → 62 programs not emitting (water); buffer loads −38 %, card effect not measured | M / changes the shader cache | no (`SLOT_VIEWS`, `ActiveF64`); first-half pairs, split joins, POPS, mesh link: have |

Changed since last review: the water read-back (`texback.rs`) is now in fxceless as `VICTORY_TEXTURE_BACK` (f5c03f3) and myokex (1544bd5): boat no longer sinks, but that run later lost the device. fxceless measured and dropped: `VICTORY_COPY_CHANGED` (256-byte chunks, no gain), `WIDE_REFRESH`, `SPREAD`, `REFRESH_ONCE`, `ASYNC_PRESENT`, mapping pool pages as one view (remap 140 → 79 ms a frame but black/red faces; reverted). Already ours: parallel `CARD_COMPARE`, `CLAMP_WAITS`, `JOIN_PARTS`, `BACK_OWN_VIEW`, relay on by default, atomic `made`, `RwLock` on `BUFFERS` (not `LOOSE`), R9G9B9E5 stand-in. Not for us: fast-core pinning (a90ef37; no slow cores here), `VICTORY_RESOLUTION` 1080p cap (patches a title self-check), Series S stretch, borderless window, fps box. Nothing new anywhere on depth of field, the RT builder, stand-in NPCs, 0x1c8/0x218 or GPU queries.

Safety: myokex 3b50cdf adds `Manual Save.zip`, `asdadsada.zip` and four PNGs (unopened; do not extract). rorodev6 bdd68c2 / Neptune c554639: `cargo xtask agility` downloads the D3D12 Agility SDK package (SHA-512 checked) and new crates `flate2`, `zlib-rs`, `sha2` (crates.io); rorodev6 `play.ps1` now runs `victory.exe warm` before the game. soldsawn: only `windows-sys` features added; launcher sets ten more switches. No new git or path dependencies in any repo.

## Fork review 2026-11-26 (visuals + performance)

Fetch only; nothing merged, built or run. "Ours" = grep of `victory-dev\crates` for the switch / function. New on the tracked branch: soldsawn `7b7b952`→`c09a44e` 47 (mostly merges); fxceless `9c184b3`→`46bf535` 70 (half are run logs); myokex `vi-fx3` `1544bd5`→`aa8399e` 2, **new branch `vi-xtx` (391 commits = the history of avatarmanz/VIctory-7900XTX, a source never reviewed; its own summary: `docs/findings.md`, `docs/fork-ray-tracing.md`)**; Neptune `7af0fa5`→`b900c64` 12 (25fbf69 pulls an "upstream evening build" + avatarmanz pieces); rorodev6, Trexies, all-forks, JoshuuDev, snick, ShakiraRED-dt: nothing new. No fetch failed. (a)–(d) = our four bounds: recording / submit-thread preparing / card / freezes.

### Performance, new since 2026-11-25 (ranked against our bounds)

| # | Bound | Item (repo commit, date) | Switch, default | Author's number | Port / risk | Ours |
|---|---|---|---|---|---|---|
| 1 | a | List calls only written down, translated by threads of ours (avatarmanz 3a99ee6 11-25, `list/thin.rs`; Neptune port 2c415c1 11-26, 1,317 lines) | `VICTORY_THIN` off (Neptune launcher 1), `_THREADS` | 45–60 thousand calls a frame, 160–190 ms of recording threads; recording 55 of 65 ms; frame 75 → 68 ms (7900 XTX) | L / high (collides with our BIND_FILTER, CHEAP_DRAWS, timeline, late gather) | no |
| 2 | a | Host heap: made slot found in an atomic, `kept` behind 64 locks (fxceless 5d659e0 11-25) | `VICTORY_HEAP_LOCKS` off | slots' lock 79,142 → 10,240 takes, waits 53.3 → 15.1 ms, other lock 25.7 → 0.1 ms; recording −23 ms summed; frame unchanged there (submit-bound) | S–M / low | half (`MIRROR_FAST`; `kept` one lock) |
| 3 | a | A list makes a view its tables hold once, copies the descriptor (avatarmanz 4e77a87 11-25, 326 lines; in Neptune 25fbf69) | `VICTORY_LIST_VIEWS` on | 12,400 tables / 96,000 slots a frame; last recording job 24 of 65 ms | M / medium (stale view inside one list) | no |
| 4 | a | Per-thread memory of buffer look-ups (fxceless 32be36d + fix bc6ee68, 223 lines) | `VICTORY_LOOKUP_CACHE` off | run 293: frame −14 ±7 ms, recording −57 ms summed; 540,000 asks, 89 % lock-free; PR author 117.2 → 104.2 ms (3090 Ti) | S–M / low (take bc6ee68 with it) | partly (`CHEAP_DRAWS` skips root/VB/IB look-ups only) |
| 5 | a, b | `sync_heaps` / `heap_buffers`: slot flag bits, page-wise locks, no re-queue of own mirrors, no null re-make (soldsawn 5acc7c5 11-25, 393 lines) | none | none (comments: 40,000 descriptor writes, 42,000 records a frame took a lock each) | M / low | no (`slot_flags`, `mirror_heap_buffer`: 0 hits) |
| 6 | b | Heap records / buffer-view pass beside the preparing and the recording, on several threads (avatarmanz 53c5256, f672130, 6c17ce9, 44629de 11-25) | `VICTORY_HEAP_SYNC_ASIDE`, `_HEAP_PASS_ASIDE`, `_VIEW_THREADS` on | pass 12–17 ms of one recording thread; "preparing on a thread of its own, refreshes and records spread" 103 → 75 ms | M each / medium | no |
| 7 | b | Buffer-view pass looks only at marked slots (fxceless 895a23b, 761 lines with `LIST_REUSE`); pages as they were skipped (myokex aa8399e `VICTORY_SYNC_SKIP`) | `VICTORY_HEAP_PASS_MARKED` off | 84,027 slots / 178 ms a pass (run 286); run 297 with 3 others: preparing −8.9 ±4.7 ms, frame not | M / medium | no |
| 8 | a | Block opened by a write fault rests 3 ms before it is guarded again; fault opens one view (avatarmanz 7bc7c07; Neptune 25fbf69) | `VICTORY_GUARD_REST` 3000 µs, `VICTORY_OPEN_ONE_VIEW` | 586 faults a frame, 245 repeats; 69 → ~6 ms of title threads | M / medium (write seen at the asking, not missed) | no |
| 9 | d | DirectStorage pool = physical cores (soldsawn 0bba762 11-26, 69 lines) | `VICTORY_DSTORAGE_WORKERS` | 8 vs 16 workers on 8c/16t: copying back 2.4–3.5 vs 5.7 ms/MB | S / low | no (`available_parallelism` = 16 here) |
| 10 | c | Ray walkers get an empty directory while the builder is off: every read 0, nothing copied (Neptune 25fbf69 `memory::empty`) or answer "no hit" at translation (fxceless 96ff135) | `VICTORY_RT_WALK_UNBUILT` off / `VICTORY_BVH_NO_HIT` off | fxceless: those passes < 1 ms there; all-forks f54cb72 (same idea) 456 → 176 ms on a 4090 | M, host only / low; NO_HIT changes the shader cache | no (ours `BVH_HOLD` drops dispatches, off) |
| 11 | c | Pixel waterfall loops: a helper lane reads its own value, so the loop no longer runs to its cap (Neptune 25fbf69 `helper_reads_own`, upstream 0601955) | none | none | S, translator / new shader cache | no (0 hits `isHelperLane`) |
| 12 | b | Same range declared written once per list; unaligned root constants filled once; pipeline kept once; same render targets not set again (avatarmanz b32fb99, c96525d, ab86ef7, ca1d519; Neptune 25fbf69) | `VICTORY_DECLARE_ONCE`, `VICTORY_BIND_CACHE` on | none ("thousands of times a frame, each a step for the submit thread") | S each / low | no (BIND_FILTER does not cover targets) |

Measured at nothing by their authors (do not port for speed): `LIGHT_REFRESH` (copies −13.7 ms, frame +7), `FILLS_PLAIN` + `STEP_INDEX` + `LIST_REUSE` (run 297), `HEAP_SYNC_CUT`, `TABLE_STAGES` for time (frame +7.4), `MAP_THREADS` 16 (avatarmanz 4e99544), `BVH_SIZES_WAIT` (−20 ms, mended nothing), `SHADER_OPT=127` grouped loads (no gain on 7900 XTX; **blackened the water** in fxceless run 277), `HEAP_WORKER` (fxceless run 309: pass 96–116 → 33 ms on the recording thread, frames 9.3 fps as before), repeat page mappings (run 308: 0 of 103,024). Low for us: Neptune `CLAMP_AFTER_WRITER` / `CLAMP_BATCH` (4.6 → 8.4 fps against their per-draw CPU check; our in-list layer work is 2.4–5.6 ms), soldsawn 3c85889 section windows (`VICTORY_INPLACE_SECTIONS`: needs `SEC_COMMIT` sections, ours makes them on Wine only) and Thin LTO (no number), `REMAP_LEAN` / `REMAP_BATCH` (ours `MAP_FAST`).

### Visuals, new since 2026-11-25 (by defect)

| Defect | Item (repo commit) | Switch, default | Claim | Port / risk | Ours |
|---|---|---|---|---|---|
| Black where no direct light | Builder made to work: DispatchX flag 0x20 = counts are threads (stage ran 64x, stored over neighbours), fences at buffer atomics, bounded accesses and stores, streamed bytes taken at submit (avatarmanz 810efe5, 30f46c9, adf5706, 96bfd0e, d2a8f03, b558934; Neptune `VICTORY_BVH_BOUND`) | `VICTORY_BVH_BUILDER=1` still opt-in; `_THREAD_DIMENSIONS`, `_BVH_FENCES`, `_BVH_BOUNDS` on | "rooms get their indirect light" (runs light-10/11, n19) for +40 ms of card time; crash cause named (pointer 0xff800000ff800000); open: dynamic structures overlap (48 of 275 good) | L / high | no (0 hits) |
| same | `CS_ScreenProbes_ActivateRays` dispatches left out while rays do not run (Neptune 25fbf69, from Belobug 6986a571) | `VICTORY_RAYS` off = left out | "once it ran, with no rays behind it, the whole picture lost light, shadows and reflections" | S / low — cheap A/B | no |
| Black while textures swap | Slot whose view went stale keeps showing its texture until the new one is uploaded (Neptune 25fbf69 `stale_kept` + 61f2206 cap; avatarmanz d1c412c `VICTORY_KEEP_SHOWN`) | `VICTORY_STALE_KEEP_MS` 10,000, `_MAX_MS` 120,000 | 72,347 slots made stale in 280 s, 19,724 black up to 3 s; "black sky frames fixed" | M / VRAM held longer | no (0 hits `stale_kept`) |
| Texture pop-in | Extra passes over textures picked by index inside a frame (Neptune 25fbf69) | `VICTORY_TEXTURE_CATCH_UP_MS` 2 | 47 % of such textures were one frame late, 37 % same frame | S–M / low | no |
| Texture garbage / wrong mip | View moved to first level with memory must keep `ResourceMinLODClamp` (fxceless 41f9857, PR author tested on an RTX 4080) | none | lowered clamp lets a view sample a level the title held back | S / low | **no: `tiles.rs` `show_from` still lowers it** |
| Cutscene blur / focus | Draw into a target of another format than its pipeline declares gets a pipeline for the bound format (fxceless a35c557, 352 lines) | `VICTORY_TARGET_VARIANTS` on | debug layer, 7 times "while a full-screen zoom and blur effect ran": R10G10B10A2 pipeline on R11G11B10 target = invalid draw | M / low | no |
| Dark light buffers | Card-written texture copied into its aliased view even after an upload of that view (fxceless 19a45d1, 105 lines) | none | read back zero before, packed-light value after (RTX 5080); did not cure black shadows (run 291) | S / low | no |
| Eyes glowing, white shots | NaN → 0 and Inf → ±65504 on colour exports and typed stores (Neptune 25fbf69 `nan_guard`, upstream) | `VICTORY_NAN_GUARD` on | "one bad pixel does not stay in a history the title blends" | S, translator / new shader cache | no |
| Shadow holes, dashed edges | Pixel shaders read quad neighbours with quad ops (avatarmanz 1c8dd31; myokex 95e9571 `VICTORY_PS_QUAD_READS`; Neptune `helpers_read`) | on | AMD: holes in lamp shadows, lines on paint, glass, skin; NVIDIA unproven | S–M, translator | no |
| World parts missing | Dynamic-slot ring (262,144) full: up to 2,836 of 18–31 thousand tables a frame refused = draws dropped (fxceless run 300); shared tables (5d659e0) | `VICTORY_TABLE_STAGES` off | refused tables 1.79 → 0.48 %; search fix 4d0c634 found nothing to find | S / low | ours same ring size + 30 ms wait |
| Device loss | Slot shows a new texture only after its upload ran (avatarmanz 3bab23c; fxceless 4679615); constant views capped at 64 KiB, zero-size kept (fxceless 2ba6e1f, 619b2dc) | `VICTORY_SHOW_AFTER` off | "a loss about every 16 minutes ... none since" | M / S | no |

Nothing new anywhere on frozen faces (`compute_graph_ops`, head / skinning programs), autofocus, stand-in models, 0x1c8 / 0x218 or real GPU queries.

### Known on 2026-11-25, never ported, now relevant

| Item | Why now | State |
|---|---|---|
| `VICTORY_HEAP_WORKER` (soldsawn 233a65f; now fxceless d4a528b) | first number: pass off the recording thread 96–116 → 33 ms; frame unchanged on a submit-bound PC. Ours is recording-bound in part → A/B worth it | L, needs `LATE_WAIT` |
| `VICTORY_LOOKUP_CACHE` (Neptune row 5) | now measured twice (row 4 above) | take fxceless form |
| Empty ray walkers (shortlist N) | now host-only in Neptune (row 10) | M |
| `VICTORY_UI_PLANE` (rorodev6 8604369) | still 0 hits for the switch in ours | M, unchanged |
| Exact buffer views (soldsawn 7d175bb) | soldsawn now uses `find_or_exact` in `buffer_srv` / `buffer_uav` / `buffer_copy` (f5cd082, 831d260) | M, no number |
| `VICTORY_XMEM_PERSISTENT_VIEW`, `VICTORY_WINDOW_UPLOAD` | covered by our `DSTORAGE_LINEAR_VIEW`; window upload has no speed gain | drop |

Safety (flag only): no archive or binary added in any new range. soldsawn: `lto = "thin"` in the release profile, one `windows-sys` feature, launcher edits. fxceless: 868,000 lines of run logs under `diagnostics/` (text), CI file. Neptune: new workspace crate `apps/um-spikes` (standalone D3D12 / memory probes; crates.io `windows` only), `play-zm.ps1` now sets `VICTORY_THIN=1`, `CLAMP_AFTER_WRITER=1`, `WATCH_4K=1`. myokex `vi-xtx`: `scripts/play.ps1` clears `VICTORY_*` variables; b7ab9c4 `victory warm` starts child processes of itself; `placement.rs` (Marvisak) and `thin.rs` are GPL-3.0-or-later like the rest. No git or path dependencies, no download code.

## trustno1-coder/VIctory-NVIDIA

Clone `ref-forks\trustno1` @033be27 (2026-11-25 19:41 -0800), read only, nothing built or run. Safety: clean. 261 text files; only binary is `bcpack-tables.bin` (byte-identical to ours); 89 crates, all crates.io, same names as ours; no git/path dependency; 3 `build.rs` (source hash; a linker flag for a test fixture); no download, registry, startup or credential code. Scripts: 12 `.ps1`; `play-nvidia.ps1` copies `victory.exe` to `victory_sm.exe` inside its own `target\` folder (so a driver Smooth Motion profile applies) and clears the `VICTORY_*` variables of its window; `.githooks/pre-commit` runs `cargo xtask policy` (not installed by a clone).

Lineage: NOT our line. Common ancestor with our base is fxceless `a26859f` (2026-11-25 01:46 -0500), 4 commits BEFORE `0b0074e`: it has none of fxceless' four frame-rate sessions and nothing of soldsawn. On top: 389 commits of avatarmanz/VIctory-7900XTX (author `local`; AMD RX 7900 XTX; its own submit thread, relay, card steps, threaded list translation — never reviewed by us; myokex branch `vi-xtx` is that tree + 2), then 12 commits of trustno1-coder (2026-11-25 15:42–19:41 -0800). As new as anything reviewed above.

### What the fps figure measures (not like for like)

"15 ±" = README table: **14.9 fps on an RTX 5060 Ti 16 GB** (PCIe 4.0 x4, no display attached), 19.3 on an RTX 5070 12 GB; Ryzen 7 9800X3D, 64 GB; prison prologue indoors, one run each; `play-S-540p` = **Series S profile, world drawn at 960x540** (a title patch pins render scale 0.5: 1/7 of our 1440p pixels), the title's own upscaler to 1080p; diag build; title frames from the log. Driver frame generation (Smooth Motion) is on by default but not in that count. Their own Series X numbers on the 5070: **6.0 fps at full scale (GPU 110 ms), 7.2 fps at half scale** — below our 13.

| Launcher setting (`scripts\play-nvidia.ps1`) | Effect | Against our rules |
|---|---|---|
| `--console series-s`, `VICTORY_RENDER_SCALE=0.5`, `VICTORY_OWN_UPSCALER=1` (exe patches, `victory-loader\hooks.rs`) | smaller console's detail and budgets, quarter pixels, FSR replaced | resolution lowered |
| `VICTORY_SKIP_PIPELINES=<rt_off_hashes.txt>` | 1,000 `CS_RaytracingResolve` + 34 traced-shadow programs never run | passes removed |
| `VICTORY_BVH_MISS=1` | every ray node test is a miss | row 7 below |
| `VICTORY_AMBIENT=0.8`, `_AIR=1.5`, `_MOST=2` | stand-in for traced indirect light | faked pass |
| `VICTORY_PIPELINE_WAIT_MS=10000` + `VICTORY_PIPELINE_WAIT_ONCE` (code default on) | a pipeline is waited for once; after that its draws are dropped until it is made | draws dropped |
| `VICTORY_RELAY=2`, `VICTORY_CARD_STEPS=2`, `VICTORY_TEXTURE_NO_WAIT=1` | speed set; README: "stretched bodies that jump", "rainbow, pixelated skin"; `-Quality` undoes them for ~7 fps | picture faults |
| `victory_sm.exe` + NVIDIA Smooth Motion | generated frames | frame generation |
| `VICTORY_CARD_BUFFERS_MIN_MB=1`, `VICTORY_CARD_BUFFERS_MB=1536` | compares on the card: 11.0 → 21.8 fps on Series S (copy-back 27 → 10 ms) | real; ours has both (2048 was rejected in ours) |
| `VICTORY_THIN=1`, `JOIN_PARTS=8`, `UPLOAD_AHEAD=1`, `INDIRECT_BATCH=2`, `CARD_AWAKE=1`, `SHOW_AFTER=1`, `TITLE_PRIORITIES=0`, `MAP_THREADS=1`, `DYNAMIC_SLOTS=524288`, `COMPOSE=1`, `NAN_SCRUB=1`, `D3D12_DRED=light` | base fork's speed set + NVIDIA fixes | rows below |

### Missing in victory-dev (grep-checked: 0 hits in `victory-dev\crates`); rows 1, 2, 4, 9–12 are not in any section above

| # | Piece (their file / commit) | Switch, their default | Author's number | Our bound | Port / risk |
|---|---|---|---|---|---|
| 1 | **Faces: 16-bit channels moved as unsigned integers.** fxceless PR 927 (tippythetaidum), in 77e13a9: `emit\halves.rs` (148 lines), `lower.rs` (`half_views`, `table_place`, `bufferLoad/Store.i32`, component 0x405), `emit\host.rs` `View::HalfUintUav`, `descriptor\host.rs:3304-3360` (`half_view`), `draw.rs` `tables(.., half, ..)`. The face network's values went through float views and "came back as zeros on NVIDIA" | none (always on) | "faces move" (RTX 5070); no number | frozen faces | M (~900 lines + 487 of tests; every shader translated again) / medium: our `lower.rs` has diverged; ours has `View::HalfUav` only |
| 2 | Card's clocks kept up between submissions: one 64-thread compute group on a queue of its own, quiet 250 ms after the last send (`host\awake.rs`, 252 lines; 5f86fa3, 040aa27) | `VICTORY_CARD_AWAKE` off (launcher 1; `turns` = A/B inside one run) | a replayed frame: 30 ms sent in one go, 38–40 ms with 1.2–2.4 ms gaps (RX 7900 XTX) | (c) card time, idle gaps | S: self-contained + one call at send / low |
| 3 | `VICTORY_THIN` (`list\thin.rs`) — already row 1 of the 2026-11-26 table above; new fact from this fork only | launcher 1 | on NVIDIA it ran the dynamic slots out: 1,100–4,800 of 12,000–16,600 tables a frame refused, until row 4 | (a) recording | a port of THIN needs row 4 with it |
| 4 | Dynamic slot pool up to half of a view heap capped at 1,000,000 (`descriptor\host.rs:1430`; a9632bc) | `VICTORY_DYNAMIC_SLOTS` 2^18 (launcher 524288) | refused tables 1,100–4,800 → 0–33 a frame | dropped draws | S / low; ours is a fixed 2^18 (`descriptor\host.rs:981`) + `VICTORY_DYNAMIC_WAIT_MS`: read our log's refused count first |
| 5 | Builder fixes (`DispatchX` flag 0x20, bounds, fences) and bind cache / declare once — already in the 2026-11-26 section ("Black where no direct light", row 12); confirmed here: ours drops the flag (`list.rs:1733`, `_flags`) | — | their README: the builder is refused on NVIDIA (64-lane programs), so this fork never ran it; ours has paired wave64 | unlit areas | as listed there |
| 7 | Ray walks end at the root while the builder is off (`emit\intersect.rs:62,199`; 24a479d) | `VICTORY_BVH_MISS` off (launcher 1) | "about 10 ms a frame" (5070, Series S 540p) | (c) card time | S (23 lines + cache key) / changes light: every ray misses instead of walking unbuilt bytes for up to 65,535 steps — same idea as row 10 of the 2026-11-26 table (`BVH_NO_HIT`) and shortlist N; needs the user's ruling (pass kept, result fixed) |
| 9 | A landing's own bytes not sent up again; heap records compared on several threads; under-write compare with AVX2 on threads (e162982, 44629de, b620284) | `VICTORY_LAND_ECHO` on, `VICTORY_VIEW_THREADS`, `VICTORY_UNDER_WIDE` | 14.4 MB a frame of one copy re-sent (28 of 72 MB over the bus); under-writes 14 → 4–7 ms | (b) prepare | M / medium (their relay differs from ours) |
| 10 | Pipelines: slow ones on a quarter of the workers; screen passes with a wait of their own; `victory warm` in child processes (a30829c, a989668, b7ab9c4) | `VICTORY_PIPELINE_SLOW` on, `VICTORY_SCREEN_WAIT_MS` 10000, `VICTORY_PIPELINE_BOOK` off | 97 pipelines in 20 s; first 90 s at 0.5–1.6 s a frame, up to 2,900 draws dropped | (d) cold stalls | S–M / low |
| 11 | Title's idle / lowest threads raised to below-normal; process above normal (0d065d3, c278c67) | `VICTORY_TITLE_PRIORITIES`, `VICTORY_PROCESS_PRIORITY` on | loads done 1.1 s late on average, 4.8 s worst; loading 40–60 s vs 28–33 under CPU load | (d) streaming | S / low (check beside our `VICTORY_AFFINITY=free`) |
| 12 | Picture, small: NaN / infinity → 0 on pixel colours and image stores (`VICTORY_NAN_SCRUB`, off; black blobs on faces and in the air); video planes always record their copy, hot 1,024 frames (`VICTORY_VIDEO_PLANES` on; intro video order); pipeline per bound target formats (`VICTORY_TARGET_VARIANTS`); constant-buffer slot marks (`VICTORY_CONSTANT_MARKS`, off; fxceless PR 1216); `VICTORY_RELOOK_MS` 20000 (ours 4000, `draw.rs:2629`) | — | none | visual / texture pass | S each / the NaN scrub is not what the console does |

Not for us: Series S interface plane (`compose.rs`; only Series S presents two planes), `VICTORY_ADAPTER`, `VICTORY_NO_WATCHDOG` (hides stalls), `VICTORY_INDIRECT_BATCH` (same goal as our gather; their replay 84.6 → 75.3 ms), `CARD_STEPS` / `RELAY=2` (their README: bodies stretch), `MAP_THREADS` (their finding: no gain), `SHADER_OPT=127` (no gain on their card). Nothing on depth of field / autofocus, GPU queries, 0x1c8 / 0x218, low-detail models or mips on demand.

Their open list: common room beige and hazy; smearing at half scale; two orange glows on corridor walls; dim rooms dark (rays miss); Series S at scale 1.0 stops at a breakpoint at start (also RTX 2080); 7 programs refused; from the base fork: far plants flicker outdoors (suspect: one bad indirect record empties its list's budget — `VICTORY_INDIRECT_REST=0` is their experiment), textures late while streaming, 40–46 GB private memory, dynamic ray structures overlap (the title reads a compacted size 1–2 frames before our submission lands), five shadow-mask trace programs untranslated.

Free tests (no port): NVIDIA Control Panel power mode "Prefer maximum performance" for `victory.exe` (row 2's effect without code — untested guess); count refused dynamic tables in our frame log (row 4).

## Vertex / geometry fixes in the forks (2026-11-26)

Read-only; one fetch per clone (nothing new since the section above). `git log --all --grep` over 11 clones: 131 distinct subjects, 118 not in our base `a14d8c8`; those on vertex data read and grep-checked in `victory-dev\crates` and `par\V1`. Evidence: run 90 (`BIND_FILTER`/`CHEAP_DRAWS` on) and run 89 (both `=0`) logs; 10 shots viewed (89: 0093, 0154, 0275, 0336, 0366; 90: 0093, 0154, 0245, 0305, 0336).

| # | Item | Repo commit | Symptom it fixes | Switch + default | Do we have it | Do we show the symptom (evidence) | Port / risk |
|---|---|---|---|---|---|---|---|
| 1 | Mesh primitive export: corner index is 9 bits (`0x1FF`), bits 9/19/29 are edge flags | upstream c0228d9; all-forks e43d7d2, myokex 77b82c9, Neptune afbfcb5, vi-xtx (not in soldsawn / fxceless) | index + 512 = a vertex nothing wrote | none | **no**: `lower.rs:10832` and `par\V1 lower.rs:10985` mask `0x3FF`; ours then culls any index >= vertex count, so a flagged triangle is **dropped** (hole), not a spike | unknown: no counter; no holes in the shots viewed. Only matters if the title sets edge flags | 1 line + test; every mesh shader translated again — put it into V1 |
| 2 | Raw view not on 16 bytes gets a copy of its own bytes | myokex 240ea7a (vi-xtx `VICTORY_VIEW_BYTES` on) | view left null, shader reads zeros (parameter blocks "raw, 128 bytes at 8") | ours `VICTORY_VIEW_OWN_COPY_RAW` **off** (a run with it darkened characters: skinning stores went to the private copy) | yes, opt-in; `_RAW_UAV=0` serves only the read views | **yes**: "left null" 9 (run 90) / 14 (run 89), all `format=39 first=2 raw=true`; 12 / 19 "host refused (it reads zeros)", all `unordered=false picked=true`; programs incl. compute 17af15c4015cb4b6 (vi-xtx `card_steps.rs` names it a skinning pass) and mesh 97356b4fcf9fbb3f | free A/B: `VICTORY_VIEW_OWN_COPY_RAW=1` + `_RAW_UAV=0` / medium |
| 3 | Vertex / index buffer view ends with the host copy it shows; direct `DispatchMesh` over 65,535 groups cut | myokex ef556e4 | device loss, reads past the copy | none | no (`draw.rs:1596,1620` give `v.size`; `:3473` `DispatchMesh(x,1,1)`) | no (no device loss; largest mesh group count 850 / 851) | S / low — guard only |
| 4 | Unfilled indirect records run empty: draws >= 2^23 vertices, mesh > 65,535 groups also when clamped one by one | myokex a9c2e99, 8d6b1a6; rorodev6 1ca533d (zero + cap 16,384) | "65,535 groups of meshlets read past every buffer"; TDR | none | draws yes (`MOST_DRAW`, `indirect.rs:68`); mesh only when `ahead` (`:169`), else 65,535 groups run | no: max asked 850 / 851, "other" 0 in 282 / 306 lines | S (shader text + test) / low |
| 5 | Indirect window cap 64 | Trexies 47ff7b0 | "stair-stepped shoulders" at 512 | const | yes (`draw.rs:534 WINDOW_MOST = 64`) | no (silhouettes smooth in all shots) | – |
| 6 | Root views taken whole, not 1 MiB | base (`VICTORY_ROOT_VIEW_MB`) | "meshes without bodies, exploded triangles" | unlimited | yes | no | – |
| 7 | Refresh sends only changed dwords | myokex 6ae0696 `VICTORY_SEND_CHANGED` now off: "geometry exploded", light lost | off | relative: ours `VICTORY_REFRESH_COMPARE` (4 KiB steps), off, not in run 89/90 | no | **pitfall: keep ours off** until checked by eye on skinned characters |
| 8 | Relay second stage / card steps | trustno1 README, avatarmanz issue 13 | "stretched bodies that jump" (skinned meshes handed back early) | their launcher on | not ported (0 hits `CARD_STEPS`) | no | do not port |
| 9 | Mesh outputs capped to lane count, primitives past the counts culled | rorodev6 911267c, bd33acc, f95856d | uninitialised primitives on NVIDIA | none | yes (`lower.rs:4372`, `:10805-10840`) | no | – |
| 10 | Rectangle lists: normalized attributes, zero / overflowing stride, components past the stride | all-forks 8145918, myokex c0cb573, base f3c7f0f | wrong full-screen / UI quads | none | normalized + past-stride yes (`primitive.rs:56-102`); stride guard not found | no warning in the logs | S / none |
| 11 | Null vertex view arrays unbind; half-float rounding carry | base 31ce30f, 63df363 | stale vertex buffer; value halved | none | yes (base) | no | – |
| 12 | Signature fixes: interpolation mode per row, PSV0 names, f16 mix halves, mesh stage I/O | all-forks 76096ca, 7265f4b; Trexies 6d9b8bb; myokex b6ca119 | programs refused = draws missing | none | not as these commits (`sig_interp` 0 hits); ours validates by its own forms | 0 translation-failure lines; drops are the RT builder stage only (240 a frame mean) | not needed |
| 13 | Picked buffers read through raw twins; byte stride of indexed view addresses | myokex 1f58d91 (AMD device loss in meshlet culling); rorodev6 5603b45, f68b1f0 | wild reads in culling; 62 water programs | `VICTORY_RAW_VIEWS` on | no | once a run: "a shader takes the address of a buffer view that is not there ... slot=1 dword=8" | M, new shader cache / AMD-driven: skip |

Bind filter: **no fork skips unchanged vertex or index buffer views** (soldsawn, fxceless, vi-xtx, Neptune set them on every draw; avatarmanz `VICTORY_BIND_CACHE` covers render targets and pipelines only), so no fork documents a pitfall for it. The one related warning (ca1d519): compare what a handle resolves to, not the handle — ours compares host address, size, stride. Our resets cover a new recording, `replay`, and rectangles' own buffers (`draw.rs:4027`). In run 90: 43 vertex sets / 20 skipped, 31 index sets / 89 skipped a frame.

Shots: no stretched, spiky or stair-stepped geometry and no trailing limbs in either run; characters and cars keep their shape with the filter on (90/0336) and off (89/0154). Faults seen are shading, not vertices: two NPC faces black, stippled shirts and pillars, bare palm crowns (90/0336). Open: the credits picture 90/0245 has rectangular gaps in its lattice; no run-89 frame of the same moment to compare.

Own logs, apart from the forks: row 2 is the only vertex-data lead. Also one "arena indirect record changed after it was read" a run (kind 2 = a dispatch: zeros when read, `[0x282,1,1]` at submission; check only, nothing issued from it) and "constant buffer not 256-byte aligned" once.

## neptune-ve (2026-11-26)

WillyboPeep/neptune-ve @d1a8773, clone `ref-forks\neptune-ve` (push URL disabled; read only, nothing built, run or
extracted; git + grep only). "Ours" = grep of `victory-dev\crates`. File:line below are in the clone.

**Base.** Not a new line: Neptune (VIctory Edit.) source zips committed as received — 5faa16d = build 25c =
`ref-forks\neptune\src` byte for byte (0 differing files); 6180ad6 = 25l; 6853f41 = 25m — plus 10 commits of
WillyboPeep (2026-11-25). 25l is what Neptune_VI merged as 25fbf69 + 2c415c1 (against `neptune-vi` origin/main b900c64
it adds 126 lines, all diagnostics gating): already covered by the 2026-11-26 sections above. New to us: 25m (6 files,
+445 lines, all on the submit thread) and Willy's commits. Their PC: RTX 4090, 9800X3D, 32 GB, **Series S 720p→1080p**:
16–21 fps at 2,000–3,500 draws, ~10 fps at 5,000–6,500. Neptune's own 25m figure: 85 ms at 11,500 draws.

Safety: clean. Only binary `bcpack-tables.bin` (same blob hash as ours). Crate names in `Cargo.lock` identical to ours,
all crates.io, no git/path dependency. 4 `build.rs` (source hash, `git rev-parse`, Agility exports now only with
`VICTORY_AGILITY=1`). `.githooks/pre-commit` runs `cargo xtask policy` (not installed by a clone). 3 `.ps1`:
`play-neptune.ps1` sets variables and starts `victory.exe` (no download, no copy); `tools\start-neptune.ps1` has
hard-coded `D:\Neptune-VE` paths and starts a hidden PowerShell with `-ExecutionPolicy Bypass` on `memwatch.ps1`
(CSV of memory figures, calls `nvidia-smi`). No network code beyond the title's blocked WinHTTP shim and the local
Discord pipe (launcher sets `VICTORY_DISCORD=0`). No text addressed to AI assistants. Commits are co-authored by an AI.

### Port list, ranked (none trades picture for speed)

| # | Item (file:line) | Kind / our bound | Author's number | Ours | Port / risk |
|---|---|---|---|---|---|
| 1 | A range taken whole and not written since is not copied again: pages a range holds only in part were never noted and were copied at every refresh (`shadow.rs:42`, `:2626`, `:3557`, `:3578-3612`; 25l, from avatarmanz) | perf / (b) prepare, bus | 2,900 of 4,900 stale pages a frame; copies 205 → 105 MB a frame (`VICTORY_RANGED`, on) | **no** (0 hits; `shadow.rs:4578-4583` leaves a partial page at 0 = copied every refresh). Missed in the earlier Neptune reviews | S (~40 lines) / low: same write stamps as whole pages; check beside `REFRESH_COMPARE` |
| 2 | Staging upload pool cap 256 MB → 2 GB (`texture.rs:449-454`; launcher `VICTORY_STAGE_POOL_MB=2048`, Willy 39f5da3) | perf / (d) streaming freezes | first 80 s of the world: 29,000 buffers made and freed (14 GB) → 258; title's mapping calls 150–250 → ~50 µs; samples under 5 fps 8 → 3 (one run each) | fixed constant: `SPARE_MOST = (256 MiB, 8 MiB)` `texture.rs:120`, no switch | trivial (make the cap a switch) / +2 GB RAM. Read our "taken / made" staging counts in a streaming burst first |
| 3 | Section pages committed once; the title's mapping calls skip the commit for pages committed already (`xmem.rs:376`, `:473-489`, `:1348-1352`; Willy 62cda3d) | perf / (d) | view itself: 0.8 vs 29 µs per 128 KiB write; skip: count only | half: lasting view = our `DSTORAGE_LINEAR_VIEW`; **commit is still asked at every map** (`xmem.rs:1941`) | S / low–medium (a decommit must clear the mark; `MEM_RESET` discards keep the commit) |
| 4 | Presents of start / loading screens (< 64 draws) wait while host pipelines are being made, 1.5 s a present, 5 min a run (`queue.rs:1264-1309`; Willy 841f5ab) | cold-run stalls, lost one-shot passes / (d) | 2,300 pipelines compiled for 1–4 min, world's first frames up to 16 s; "not yet seen at work" | no (`making()` `pipeline.rs:514` and `drawn()` `draw.rs:885` exist) | S (45 lines) / low: 1.5 s is under the title's watchdog; helps picture on a first run, nothing on a warm one |
| 5 | Clock read only for deferred steps that copy (`host.rs:7431`; 25m) | perf / (b) | 14 of 400 samples of the submit thread; no frame-time change | no (`host.rs:9526` reads it for every step) | trivial / none |
| 6 | Process above normal priority (`runtime.rs:229-238`; 25l, avatarmanz) | (d) loading | loading 40–60 → 28–33 s under other CPU load | no (0 hits; trustno1 row 11 above) | trivial / check beside `builds-yield.ps1` |

Still missing from 25l, already listed above (no new row): `VICTORY_THIN` + `VICTORY_PASSES_AHEAD` (`draw.rs:108`; without
it under THIN "hair and whole people drawn from old bones"; 25m adds `wants_ahead`, `queue.rs:338`, `:634`, `:1283`, and
names an open fault: "the player is drawn with bright blocky patches" now and then — **picture risk, do not port
THIN as it stands**), `VICTORY_LIST_VIEWS` (`descriptor/host.rs:3947`), `VICTORY_DECLARE_ONCE` (`host.rs:2570`),
`VICTORY_BIND_CACHE` (`draw.rs:802`), `VICTORY_RT_WALK_UNBUILT` / `memory::empty` (`memory.rs:353`), `VICTORY_RAYS`
(`pipeline.rs:366`).

Already ours: `VICTORY_CARD_FLAGGED` (25m, `shadow.rs:245-275`; 27 of 400 samples) = our `CARD_QUEUE` (`shadow.rs:111`);
`TABLE_BUFFERS_LEFT` = `left_to_draws`; placement heaps (`placement.rs`) = `TEXTURE_HEAPS` + `HEAP_PACK`; stale slots
kept, slot re-check by fingerprint, `TEXTURE_CATCH_UP_MS`, `NAN_GUARD`, `helper_reads_own`, `helpers_read`,
`GUARD_REST`, `WIDE_TEXTURES`, efficiency mode off, the giga-fork sampling profiler (`profile.rs`).

Do not take. Measured at nothing by Neptune (25m, all off by default): `VICTORY_AHEAD_SPLIT` (`queue.rs:750`; textures
in the same frame 79 → 71 %), `VICTORY_AHEAD_EARLY` (`queue.rs:844`; "what streamed in flickered again"),
`VICTORY_WIDE_FILLS` (`host.rs:7599`; waits 53 → 122 of 400 samples), `VICTORY_WIDE_TEXTURES` (85.3 vs 85.2 ms),
`VICTORY_WIDE_SPIN_US` (`shadow.rs:1491`; 85.2 → 87.4 ms). Picture loss: `VICTORY_TEXTURE_POOL_MB` (`resource.rs:2398`;
off: "foliage on walls, trees lit green in the dark"), ambient sky / reflection clamps (`ambient.rs:96`, `:153`:
stand-in light, overwrites title output), `cores.rs` (no slow cores here).

### Default launch (`play-neptune.ps1`) — which settings cut quality

| Setting | Effect | Verdict |
|---|---|---|
| `--console series-s` + `VICTORY_FSR_DLL` whenever an FSR DLL is given (`-SeriesX` = 4K path) | 720p → 1080p, smaller console's budgets | **quality cut: not acceptable** |
| `VICTORY_AMBIENT_FILE` (ambient on in code: sky-following level, reflections held to it) | stand-in for traced light | faked pass, not the title's picture |
| code defaults: `VICTORY_RAYS` off, ray walkers get an empty scene | ray-activation dispatches left out; every ray misses | pass skipped / result fixed — user's ruling as before |
| `VICTORY_PIPELINE_WAIT_MS=2500` | draws whose pipeline is later than 2.5 s are missing until made | picture loss on a cold run |
| `VICTORY_THIN=1` | recording on threads of theirs | speed (97.8 → 80.7 ms); blocky patches on the player |
| `VICTORY_ASK_QUIET=8`, `VICTORY_WATCH_4K=1`, `VICTORY_TEXTURE_HASH_EVERY=32`, `VICTORY_TEXTURE_NO_WAIT=1`, `VICTORY_RELAY=2` | writes / texture changes seen later | speed; picture risk (zm0kie: dark objects, black water; trustno1: stretched bodies with relay 2) |
| `CARD_WINDOWS_MB=4096`, `CARD_BUFFERS_MB=1536` + `_MIN_KB=64`, `UNDER_WRITES_*`, `COPY_CHANGED`, `QUIET_BACK`, `WIDE_*`, `LIST_POOL_*=2048`, `LATE_WAIT_LISTS=1024`, `WINDOW_TAILS`, `WINDOW_KB=1024`, `TEXTURE_STAGE_MB=512`, `CARD_WINDOW_UPLOAD`, `D3D12_BACKGROUND=0`, `VRAM_IDLE_MS`, `BAKE/EARLY_TEXTURES_MS` | speed / memory | no cut (most are in ours under these or other names) |
| Willy's: `VICTORY_INDIRECT_RAW=0`, `STAGE_POOL_MB=2048`, `DISCORD=0`, `FRAMETIME_DUMP=1`; `OCEAN_READBACK=1`, `DROP_DXIL=1`, `AUTO_A=150` | safety, RAM, logs | no cut |

Findings worth keeping (no code): (1) `VICTORY_INDIRECT_RAW=2` hung an RTX 4090 for 12 s with a device reset in 3 of 7
launches (garbage record at world start) — agrees with our keeping RAW off. (2) Willy 218e5fd measured the title's
page mapping calls (8 µs before the world, 150–450 µs in it): they do **not** wait behind write-watch askings or
protection changes; they are slow while the CPU is saturated (2,300 pipelines compiling in `nvgpucomp64` + streaming)
and while staging buffers are made and freed (row 2) — bears on our F3 / F10. (3) Neptune 25m: with THIN the frame is
bounded by the longest single list, 66–96 ms to translate on one thread.

Nothing new on: lighting, shadows, the RT builder (still "crashes within seconds", opt-in), the shader translator
(25m: no change; 25l = known), the pipeline library, faces, stippled cloth, palms, glowing eyes, low-detail models.
New open fault named by this fork: mirrors show only sky (planar mirror pass runs, room missing from it).
