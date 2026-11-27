# Silent-defect audit: where the layer can hand a shader zeros, stale or altered bits

Read-only audit, 2026-11-26. Code: `Tools\par\V1\crates` (paths below are relative to it; `sh/` = `victory-shader/src`,
`gpu/` = `victory-gpu/src`). Counts: `bench\run-92-faces-warm\plain.txt` (run-90-combined in brackets where it differs).
"No log" means nothing is written at run time, so the log cannot clear the item.

## Read first

1. **How the halves bug really worked** (and what is left of it). Every typed buffer and every texture is declared to the
   host as FLOAT (`0x409`: `sh/emit/lower.rs:9453`, `:10604`). A `d16` access then runs `legacyF32ToF16` on what the
   view returned. Through an unsigned 16-bit view the "float" is a tiny denormal, so the conversion gives 0. Run 92:
   34 programs declare half places, yet the host made **0** unsigned twins — the views were already integer views; the
   cure was the shader side. The same pattern is still live for textures and for views picked by index (A1, A2).
2. `placed unordered texture view not mirrored dimension=3` is a **2D** texture (resource dimension 3 = TEXTURE2D), not
   3D, and the code refuses every placed texture UAV of any dimension (B1).
3. Draws dropped in steady state are only the ray-tracing builder's (188/frame; 56 of 56 "dropped … why" lines). No
   "given up", no "host pipeline refused", no missing-method call, no "title slot past the host heap" in either run.
4. Not verified by a run: every "visible?" entry is a judgement from code and counters, not from a picture.

## A. View formats and type conversions

| Instance | File:line | Effect | Trigger | Log + count | Visible? | Fix + size |
|---|---|---|---|---|---|---|
| A1 `d16` image load/store taken as float | `sh/emit/lower.rs:10104`, `:9570` | load of a 16-bit UINT/SINT texture gives 0; store writes converted values | any `image_load`/`image_store` with d16 on an integer texture | none | likely if any program does it (count unknown) | as halves: i32 path + unsigned view for textures; ~150 lines |
| A2 halves fix skips some places | `sh/emit/halves.rs:104-147` | float path kept (zeros as before) | view picked by index from the heap; table place also reached by another access; table the program also indexes at run time | none | possible | extend to heap-picked views (needs per-slot format); medium |
| A3 typed buffer load/store always float | `lower.rs:10529-10545`, `:10604` | integer views rely on the driver passing bits unconverted through a float-typed resource (undefined in D3D12) | every `buffer_load_format_*` on UINT/SINT view | none | works today on NVIDIA; fragile | pick i32/f32 by view format; large |
| A4 `VICTORY_NAN_GUARD` on typed stores | `lower.rs:3702`, `:9592`, `:10543` | stored bit patterns 0x7FC00000, 0xFFC00000 become 0; 0x7F800000/0xFF800000 become ±65504.0 | typed buffer/image store of integer data with exactly those bits (e.g. packed colour 0xFF800000) | none | rare | skip the guard on stores, keep it on colour exports; 5 lines |
| A5 NaN guard on float colour exports | `lower.rs:10912-10926` | +Inf written to a float target becomes 65504 | a pass that writes Inf on purpose (far distance, "no hit") | none | low | per-format guard; small |
| A6 no typed-UAV-load support check | `gpu/` (no `TYPED_LOAD` anywhere) | unsupported format would read undefined | card without the format | none | no on RTX 4080 | check once at start; 30 lines |
| A7 view of a typed texture in another format of its family takes the texture's format | `gpu/descriptor/host.rs:3354` (`fits`), `:3420` | sRGB decode lost (or added) | sRGB view of a UNORM host texture, incl. BC1/3/7 (not in `SRGB`, `gpu/resource.rs:613`) | none | unknown: colours washed or dark if the title does it | count first (5 lines); then castable formats |
| A8 B5G5R5A1 / B4G4R4A4 / R9G9B9E5 targets in stand-in formats | `gpu/resource.rs:254`; `gpu/list.rs:1004` | copies between such a target and title-format texels skipped | copy target <-> title-filled 16-bit texture | 3 stand-in INFO lines; "copy between a stand-in…" 0 | no (0 skips) | – |
| A9 16-bit vertex inputs not unpacked | `lower.rs:2597` | value "missing" (see D2) | vertex program reading an f16 fetch input | none | unknown | unpack as pixel inputs do; small |
| A10 f16 results rounded toward zero | `lower.rs:841-853` | last place one lower than console | every f16 op; `VICTORY_F16_NEAREST=1` exact | none | no (banding at most) | switch exists |

## B. Views the host refuses or leaves null

| Instance | File:line | Effect | Trigger | Log + count | Visible? | Fix + size |
|---|---|---|---|---|---|---|
| B1 placed unordered TEXTURE view not mirrored | `gpu/descriptor/host.rs:3676-3683` | shader stores go nowhere; texture keeps old bytes | UAV of a texture the title placed in memory itself | `placed unordered texture view not mirrored dimension=3`: 1 (logged once only; real count unknown), first at second 8 | **high suspect** (runtime-composed textures) | counter (5 lines), then mirror as `placed_texture` does for SRVs; ~60 lines |
| B2 buffer view the host would refuse | `gpu/descriptor/host.rs:3128`, `:3225`, `:3340` | reads 0, writes dropped | raw view not on 4 dwords, typeless without raw flag, stride > 2048, past buffer end, zero elements | `buffer views left null…: 22` [0 early]; `a program uses a buffer view the host refused (it reads zeros)`: 47 [12] over 15 programs (10 compute, 5 mesh) | medium; most records look like garbage (address 0x4/0x8) and `picked=true` | read-only own-copy form (existing switch) |
| B3 view not on an element of its host copy | `gpu/descriptor/host.rs:3209`, `:3321` | null view | offset not a multiple of the element | `buffer views off their copy's elements…: 0.7 asked/frame, 165 kept` (served by own copies) | low | – |
| B4 constant buffer not 256-byte aligned | `gpu/descriptor/host.rs:3500` | CBV null: constants read 0 | CBV at an odd address | `constant buffer not 256-byte aligned on the host address=0x1359490`: 1 (once only) | medium: one program family gets zero constants | counter; copy to an aligned scratch; ~40 lines |
| B5 address of a missing view | `gpu/draw.rs:1511` | shader gets address 0 | table slot empty when a program asks its address | `a shader takes the address of a buffer view that is not there: it reads 0 slot=1 dword=8`: 1 (once only) | unknown | counter |
| B6 host heap 1,000,000 vs title 1,245,184 | `gpu/descriptor/host.rs:1061`, `:2421` | title slots >= 737,856 read null | title uses a slot that high (highest seen ~720,000: 2 % headroom) | `host view heap capped…`: 1; `title slot past the host heap`: 0 | not now; will bite silently only via the warn | remap; large |
| B7 stale slots | `gpu/descriptor/host.rs:3775` | old texture shown up to 120 s, then null | view rewritten while its texture is not ready | per period: 67 kept, 31 replaced, 4 made null | pop-in, brief wrong texture | – |
| B8 UAV of multisampled texture, typed depth, other family | `gpu/resource.rs:337`; `descriptor/host.rs:3383`, `:3414` | null | as named | "view the host does not allow"/"another family": 0; `unordered views refused 0x` | no | – |

## C. Calls and flags dropped

| Instance | File:line | Effect | Trigger | Log + count | Visible? | Fix + size |
|---|---|---|---|---|---|---|
| C1 device 0x1c8 / 0x218 | `gpu/device.rs:359` | title "goes without this pipeline" | every call | 29 lines = 16 + 13 logged calls (log is at powers of two: >= 16,384 and >= 2,048 real calls); names not derivable from the words | unknown, potentially large | Ghidra at the two call sites first |
| C2 `DispatchX` flags | `gpu/list.rs:1733` | flags ignored (0x20 "counts are threads") | builder dispatches | none | only once the builder runs | pass through; small |
| C3 queries | `gpu/list.rs:1754` | occlusion = 1<<20 samples, binary = visible, statistics = 0, timestamps = clock | every resolve | none | coronas/flares never occluded; anything gated on a count always on | real occlusion queries; medium |
| C4 depth plane used before its twin pass | `gpu/host.rs:8173-8180` | depth test sees the plane as it was before this list's shader writes | list writes a depth plane's twin, then tests against the plane in the same list | `3305 times used before the pass`, `3304 times not given back to a twin` (about 1 per frame) | **high suspect**: tiled lighting tests "equal" against per-pixel material class | split the host list at that use; medium |
| C5 pipeline stream subobjects | `gpu/pipeline/host.rs:237-312` | kinds other than 8-17, 21 ignored | console extension subobjects | none | unknown | log unknown kinds once; 5 lines |
| C6 ignored list methods | `gpu/list.rs:2535`, `:2538`, `:2573` | PS depth order, shading rate, 0x470 | each call | none | no (quality/perf hints) | – |
| C7 heap samplers | `gpu/sampler.rs:34-45`, `:141-155` | compare func 2..7 makes a comparison sampler; aniso forces filter 0x55; secondary bias, aniso bias/threshold, depth filter dropped | per sampler | none | low; wrong if a plain sampler carries "less-equal" | – |
| C8 immediate samplers | `sh/emit/sampler.rs:67`; `lower.rs:6420` | custom border colour = transparent black; one sampler used both ways becomes comparison | per program | none | low | – |
| C9 indirect rectangles | `gpu/draw.rs:4234` | drawn as first triangle, or not drawn | pipeline without geometry stage | "first triangle": 0 | no | – |
| C10 whole-resource copies/states | `gpu/list.rs:831`; `resource.rs:331` | twin copies and barriers ignore subresources | mips/slices written separately | none | low | – |

## D. Translator shortcuts

| Instance | File:line | Effect | Trigger | Log + count | Visible? | Fix + size |
|---|---|---|---|---|---|---|
| D1 loop caps 128 pixel / 256 vertex+mesh / 4,096 compute | `lower.rs:11150-11162` | lane leaves "as killed": pixel discarded, rest of work lost | any loop past the cap (light lists, marches, morph loops) | none | **possible: holes/stipple** | `VICTORY_SHADER_LOOP_LIMIT` (diag build) |
| D2 joins that carry a value with no DXIL form | `lower.rs:11230-11240` | value is 0 "without a word" | 40 programs: vehicle paint pixel, foliage mesh | none at run time | **likely for those materials** | lower the "first half of an interpolation"; medium |
| D3 wave64 compute in two host waves, approximate forms | `sh/emit/mod.rs:541-720` | tile-wide result becomes two half-tile ones; LDS order loose | 64-lane compute not exact otherwise (NVIDIA wave 32) | stderr only with `VICTORY_WAVE_WHY` | possible: blocky lighting/culling errors | list programs offline; exact forms |
| D4 implicit-LOD sample outside pixel stage | `lower.rs:9797` | reads level 0 | compute/mesh `image_sample` | none | aliasing | – |
| D5 gradient samples | `lower.rs:9881` | no anisotropy, longer axis picks level | `image_sample_d` | none | blur at grazing angles | medium |
| D6 gather at a level | `lower.rs:9997` | level rounded, edges clamped | `image_gather4_l` | none | low | – |
| D7 fma / rcp / trig | `lower.rs:7206`, `:7409`, `:3662`, `:8158` | last-place differences | always | none | no | – |
| D8 cross-lane reads of inactive lanes, quad mask | `lower.rs:4714`, `:7465`, `:3806` | host value instead of 0 | such programs | none | low | – |
| D9 memory by address: page without a copy | `gpu/memory.rs:23-27`, `:197` | read 0, store dropped for that frame | pointer read out of memory | "touched pages that had no copy": 0 | no (builder off) | – |
| D10 untranslated: 202 | `docs/render-audit.md:23` | 195 `GS_VoxelizeGeneric`, 5 `CS_TraceShadowRays_*`, 2 `CS_TransmissionVolumetrics*` | – | never bound | no | – |

## E. Memory-emulation gaps (stale or zero data)

| Instance | File:line | Effect | Trigger | Log + count | Visible? | Fix + size |
|---|---|---|---|---|---|---|
| E1 textures not ready when bound | `gpu/draw.rs:2663`; `gpu/tiles.rs` | zero / placeholder / missing level | card full (14.4 of 15.1 GB) | per sample: ~3-4 % of bound textures "not uploaded", ~1 % null; `10802 waiting`, `7407 passed over unmapped`; `819 uploads left a held level without memory`; `passes that made none for lack of room: 11927` | **yes**: pop-in, low detail, black surfaces, gaps | VRAM plan |
| E2 texture changed after upload, unseen | `gpu/draw.rs` (rehash) | stale texels, median 16 frames (max 176) | title rewrites memory with no slot write | `a texture picked by index changed after its upload…`: 88 [106] | yes, briefly | hook mapping/storage writes |
| E3 host-written texture never re-read from title memory | `gpu/texture.rs:1108` | title CPU writes after a card copy are lost for good | texture that was once a copy destination | none | possible (atlases built both ways) | counter; compare pages |
| E4 copies with left-out levels dropped | `gpu/list.rs:1316-1368` | silent `return` | level without memory | none | possible with E1 | counter; 5 lines |
| E5 stores through picked views not marked | `sh/emit/lower.rs:6601-6625`, `:10350` | not copied back | vertex-stage stores; first store before a slot's first mark; only buffer stores mark | `UNMARKED_PICKED_STORES` is never logged; heap_marks line: `1 first marks`, `0 marks on read-only views` | low | log the counter |
| E6 fills ahead of flights | `gpu/host/fill_ahead` | constants one frame old | CPU fill reads bytes a flight then changes | `fills that read bytes a flight then changed: 85`; 64 WARN | low (one frame) | `VICTORY_FILL_WAIT=1` |
| E7 arena indirect record changed | `gpu/indirect/arena` | draw with old arguments | record rewritten after read | 2 lines (once each) | low | – |
| E8 mapping change mid-copy | `gpu/host.rs` | guarded: waits, reads again | – | `17 so far`, 5 lines | no | – |
| E9 depth and stencil planes never uploaded from title memory | `gpu/texture.rs:13-16` | planes keep host contents | title fills a depth texture by CPU | none | unlikely | – |

## Ranked: most likely visible now

| # | Finding | Open defect it could explain | Confidence |
|---|---|---|---|
| 1 | E1 textures not ready / levels without memory | texture pop-in, low detail, rectangular gaps in credits artwork, black faces in some shots | high (counted) |
| 2 | C4 depth plane tested before the twin's classes reach it | black NPC faces, bare palms, stippled cloth (per-pixel material class one list late) | medium: counted every frame, effect not shown |
| 3 | B1 placed texture UAV dropped | bare palms, black faces (composed skin/cloth textures), credits gaps | medium: once-only log, count unknown |
| 4 | D2 zero through joins in 40 programs | wrong vehicle paint, foliage | medium (named in code) |
| 5 | A1/A2 remaining float-typed 16-bit paths | anything still "frozen" or zero that the face fix did not reach; glowing eyes | medium: mechanism proven, users uncounted |
| 6 | D1 pixel loop cap 128 | stippled cloth, holes | low-medium |
| 7 | D3 half-tile wave results | unlit or blocky areas beyond the ray-tracing loss | low-medium |
| 8 | C1 pipelines 0x1c8/0x218 | low-detail stand-ins, missing effects | unknown |
| 9 | B2/B4 refused buffer views, unaligned CBV | single effects reading zeros | low-medium |
| 10 | A7 sRGB views read linear | colour/brightness errors | unknown (no counter) |

Known and not ranked: ray-tracing builder off (unlit areas black), C3 queries.

## Cheap experiments (existing switches)

| Tests | Run with | Expect if it is the cause |
|---|---|---|
| E1 | `VICTORY_LEVELS_ON_DEMAND=0`; separately lower resolution to free VRAM | gaps/pop-in change with free card memory; watch `left a held level without memory` |
| D1 | `VICTORY_SHADER_LOOP_LIMIT=65536` (diag exe; shader cache re-translates; card may hang, use the watchdog) | stipple/holes vanish |
| D2 | `VICTORY_STRICT_JOINS=1` | the 40 programs stop translating: their objects disappear, which names them |
| D3 | `VICTORY_WAVE_LOCAL=idiom` and/or `VICTORY_WAVE_LDS_LOOSE=0`; offline `VICTORY_WAVE_WHY=1` lists the programs | approximate programs drop out; defect area changes |
| A4/A5 | `VICTORY_NAN_GUARD=0` | differences show where the guard alters values (glowing eyes may return) |
| A1/A2 | `VICTORY_HALF_VIEWS=0` as the control | face freeze returns; anything else that changes also moves halves |
| A10 | `VICTORY_F16_NEAREST=1` | banding only |
| B2 | `VICTORY_VIEW_OWN_COPY_RAW=1` + `VICTORY_VIEW_OWN_COPY_RAW_UAV=0` | "host refused (it reads zeros)" count falls |
| B7 | `VICTORY_STALE_KEEP_MS=0` | stale-texture frames become black frames |
| C7 | `VICTORY_SAMPLER_SANITISE=0` | sampler handling as before |
| E6 | `VICTORY_FILL_WAIT=1` | one-frame constant errors vanish (costs frame time) |
| hash to name | `VICTORY_DUMP_MATCH=<dir>;#` | names the 15 programs of B2 and the 34 of the halves fix |

No switch exists for B1, C4, A7, E3, E4, B4, B5: each needs a counter first (about 5 lines each), since their only
log line is written once.
