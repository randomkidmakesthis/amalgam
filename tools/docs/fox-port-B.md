# Fox port, coder B: translator (victory-shader), victory-formats, shader_cache.rs

Copy: `Tools\par\foxB`, branch `portB` (commits on top of victory-fox f4b48ad). Sources of ours: victory-dev HEAD
(soldsawn base, which holds the paired form) + `git diff HEAD` (our work) + `par\D1` vs `par\D1base`.

The rule since 2026-11-26 (user): cool_fox's output is the floor for speed and picture. A ported change that
alters what cool_fox emits for a program it already translated defaults to cool_fox's behaviour (a switch turns
ours on). The exceptions are the confirmed picture fixes (HALF_VIEWS, NAN_GUARD, D16_BITS), which stay on.
"Additive" means the change only affects programs cool_fox refused or failed to translate.

**Program counts: not measured.** victory-shader has no offline corpus test. The only corpus tool is
`victory translate` (apps, coder A), and it reads the game's shader pack, which the brief rules out for me. So
every "N" below is unknown, or soldsawn's number for its own tree. To get real counts, run `victory translate
--validate` on victory-fox and on portB, once with all switches at their defaults and once per switch flipped,
and diff the DXIL hashes per program.

## Inventory and status

| Switch / feature | Origin | cool_fox | Status in portB | Default | Changes fox default output |
|---|---|---|---|---|---|
| `VICTORY_HALF_VIEWS` (halves.rs, `HostShader::half_views`, uint handles 0x405) | ours (fxceless #927) | lacking. Its d16 view-kind bits cover UINT/SINT views at table places and heap-picked views; they miss FLOAT views carrying integer bits and are undefined behaviour in D3D12 | ported. Halves places use ours; all other d16 accesses use cool_fox's view kinds. Host half: `patches\foxB-needs-gpu.patch` (C) | 1 | yes: programs with a d16 or halves-only format access at a fixed table place (face network, head, skinning). N unknown; counter `HALVES_PLACES` |
| `VICTORY_NAN_GUARD` (colour exports, strict) | ours (Neptune 25fbf69) | lacking | ported | 1 | yes: every pixel program with a float colour target (most pixel programs) |
| `VICTORY_NAN_GUARD_STORES` | D1 | – | ported | 0 | no |
| `VICTORY_D16_BITS` | D1 | lacking. cool_fox converts d16 image loads and gathers as floats | ported: d16 image load/gather, and buffer d16 loads with no view-kind word (descriptor chosen by control flow) | 1 | yes: programs with d16 image loads or gathers. N unknown; counter `D16_BITS_ACCESSES` |
| `VICTORY_D16_VIEW_KINDS` | D1 = cool_fox f4b48ad | present (always on) | switch added on cool_fox's path | 1 | no |
| `VICTORY_HELPER_OWN` | ours | present (same switch, `first_lane`) | kept as cool_fox's | 1 | no |
| `VICTORY_PS_QUAD_READS` | ours | present (always on, `helpers_read`) | switch added on cool_fox's path | 1 | no |
| Mesh corner 9-bit index (c0228d9) + 2 tests | upstream | present | – | – | no |
| `VICTORY_WAVE_PAIRS` paired wave64 form | soldsawn 26600c6 | lacking | ported. Used only where nothing else translates a program (cool_fox refused it) | 1 | no (additive) |
| `VICTORY_WAVE_PAIRS_FIRST` | portB | – | the paired form before the approximate forms and before cool_fox's wave-wide default (two scene kernels, driver loops) | 0 | with =1: yes (soldsawn: 36 approximate programs + the wide-default ones) |
| `VICTORY_WAVE64_REFUSE` | D1 = cool_fox 8cc31ab | present (always on) | switch added | 1 | no |
| `VICTORY_SPILL_STRICT` | D1 = cool_fox 8cc31ab | present | switch added | 1 | no |
| `VICTORY_TRACE_GUARDS` (T1) | D1 = cool_fox f4b48ad | present (always on) | kept as cool_fox's, no switch | – | no |
| `VICTORY_WAVE_WHY` WAVE64-FORM/-WIDE lines (T5) | D1 = cool_fox | present | – | – | no |
| `VICTORY_WAVE_LDS_LOOSE` | soldsawn 09b95ca | lacking | ported (approximate, last resort) | 1 | no (additive: programs cool_fox refused) |
| `VICTORY_SCC_LOCAL_ANY` | soldsawn 09b95ca | lacking | ported (opt-in) | 0 | no |
| 09b95ca: GroupInfo SGPR, s_pack_*, s_bfm literal, f64 sums per host wave | soldsawn | lacking | ported | – | no (additive: these were Missing/refused before). cool_fox's s_mov_b64 literal rule kept |
| float64.rs (one f64 implementation, v_add/mul/cvt_f64, wave sums) | soldsawn 26600c6 | cool_fox has the older pair (`active_f64_sum`) | ported; cool_fox's copy replaced, no switch | – | **yes: about 5 water volumetrics programs (soldsawn's count)**. Test `a_wave_sum_of_doubles_is_the_consoles` runs on the card against console lanes |
| idiom.rs, split_choices (`ir::split_joins`), ds_sub compare-exchange loop | soldsawn 26600c6 | lacking | ported | – | no (additive: these programs were refused, or NVIDIA refused their pipeline) |
| whole sections retry of a program that already translated | soldsawn 26600c6 | lacking | `VICTORY_WHOLE_SECTIONS` (the retry of failed programs stays on) | 0 | with =1: yes (the water optical depth program) |
| One interpolation mode per signature register | soldsawn 26600c6 | lacking | `VICTORY_ONE_MODE_PER_REGISTER` | 0 | with =1: yes (soldsawn: 70 programs) |
| First interpolation half: write 0 (cool_fox) vs error + first-half-pairs retry | soldsawn 26600c6 | cool_fox writes 0 | `VICTORY_FIRST_HALF_RETRY` | 0 | with =1: yes (soldsawn: 59 pixel programs differ) |
| LOD query of an array: layer undefined | soldsawn 26600c6 | lacking | `VICTORY_LOD_LAYER_UNDEF` | 0 | with =1: yes (array LOD queries) |
| Ray-walk loop cap 1,024 (`TRACE_LOOP_LIMIT`) | soldsawn 26600c6 | lacking | `VICTORY_TRACE_LOOP_LIMIT` (speed change, needs an A/B run) | 0 | with =1: yes (image_bvh programs) |
| Mesh linking (`emit::link_mesh`, `mesh_fits`) | soldsawn 26600c6 | lacking | translator side ported; inert until the host uses it (C: `VICTORY_MESH_LINK`, 26600c6 pipeline/host.rs) | – | no |
| POPS / rasterizer-ordered views | soldsawn 26600c6 | present (own mechanism: `ordered_section`, `pops_collision`) | kept cool_fox's; `s_setreg_imm32_b32` now also starts a section, which makes soldsawn's POPS test pass | – | yes, but only for programs cool_fox failed on (`src_pops_exiting_wave_id`): additive |
| Descriptors read as data (descriptor_data.rs) | soldsawn 26600c6 | present (own inline version + size_tests) | kept cool_fox's | – | no |
| `ir::with_shared_slots` | soldsawn 26600c6 | present (own split lives in `unspill`) | not ported | – | no |
| `VICTORY_EARLY_DEPTH` | ours (myokex c3723f3) | lacking | ported + test | 1 | yes: pixel programs with DEPTH_BEFORE_SHADER and no depth/stencil/coverage export (glass prepass) |
| `VICTORY_DISPATCH_PARTS` / `USER_GROUP_BASE` | ours | lacking | **not ported**: the host must supply the dword and split dispatches (C first) | – | – |
| `VICTORY_HEAP_MARKS` (`SLOT_HEAP_MARKS`) | ours (a1ce531) | lacking | **not ported**: host-dependent, and its slot 252 collides with cool_fox's `SLOT_TABLE_STRIDES=252` (would need e.g. 251) | – | – |
| `VICTORY_SHADER_LOOP_LIMIT`, `VICTORY_STRICT_JOINS`, `VICTORY_DUMP_MATCH` (apps) | base | present | – | – | no |
| victory-formats `Layout::detile_rows` + test | ours (Trexies 58c7bb0) | lacking (`bytes_from` present) | ported | – | no (API only, for C's banded detile) |
| shader_cache: half_views in entry, key bump, all translator switches | ours | partly (fixed switch list) | entry carries `half_views`; key "victory shader cache key foxB 1" + `emit::switch_fingerprint()` (every `VICTORY_*` the translator reads) | – | – |

## Wave handling: why cool_fox's order is the default

translator.md says the paired form is the better design for a 32-lane card: it is exact, and it puts no barriers
inside loops. cool_fox's widen form has a device-loss history in our runs 121-124. But the user set cool_fox's
output as the floor, so cool_fox's order stays the default (`VICTORY_WAVE_PAIRS_FIRST=0`). The paired form only
takes programs that nothing else translates. A/B for a run: `VICTORY_WAVE_PAIRS_FIRST=1`. The count
`emit::WAVE_PAIRED_BEFORE_WIDE` says how many programs moved off the widen form.

## Counters / log lines

- `emit::HALVES_PLACES`, `emit::D16_VIEW_KIND_ACCESSES`, `emit::D16_BITS_ACCESSES`, `emit::WAVE_PAIRED_BEFORE_WIDE`
  (AtomicU64). The host (C/A) may print them once a run.
- `VICTORY_WAVE_WHY`: `WAVE64-RETRY paired lanes, before cool_fox's wave-wide default`.

## Needs a game run to judge

- Faces and the blurred disc, with the host half of HALF_VIEWS in place (FLOAT views at halves places).
  Without C's patch: run with `VICTORY_HALF_VIEWS=0` if such views exist.
- The f64 water sums (float64.rs replaced cool_fox's copy).
- A/B runs for `VICTORY_WAVE_PAIRS_FIRST=1` and `VICTORY_TRACE_LOOP_LIMIT=1` (card time, shadow mask).

## Glow: state when parked (2026-11-26)

- Symptom: bushes and some trees glow; palm trees do not. It shows in cool_fox (run-100), merged builds (runs 101 and 108) and with WAVE_PAIRS_FIRST=1 (run-108: "some still glow, some don't"). The old tree (run-98) is fine.
- Ruled out (offline DXIL diff, old exe victory.d1c against fox exe victory.coolfox; scratch `scratchpad\B\fol-*`, `full-*.md5`):
  - Foliage draw programs: all 301 PropFoliage / Imposter / TreeEntityMask programs, mesh and pixel. portB's pixel code equals the old tree's except how quad reads are built, which I checked gives the same result. Fox's mesh UNORM8/SNORM16 conversions are exact (all 256 / 65,536 values checked).
  - The NaN guard alone (run-101 had it). CARD_STEPS and SHOW_AFTER as the sole cause (run-101 ran without both).
- Small open differences in the foliage family:
  - PS_Imposter: fox interpolates 21 inputs through `attributeAtVertex` where the old tree uses `loadInput`.
  - MS_VS_ImposterViewVec*: 2 fewer handles.
  - CS_*_BatchCull_ImposterOnly: 1 fewer constant-buffer load.
- Whole pack, old against fox: 23,362 of 25,689 programs differ.
  - Fox does not translate 26 compute programs: hair shadow masks, water transmission volumetrics, particle emits, decal trails, triangle clip. portB translates all 26.
- Open leads:
  1. Lighting compute programs that fox runs in approximate or wave-wide forms. PAIRS_FIRST=1 changed part of the picture. `why\pairs_first.err` (per-program WAVE64 lines from a private translate build with PROGRAM markers; parse with `forms.py`) lists the forms that win with PAIRS_FIRST=1; `why\fox_order.err` the same without it.
  2. Host settings, as one-switch tests: CARD_BUFFERS_MIN_KB=64, INDIRECT_BATCH=2, RELAY=2 / RELAY_COPY=1, UPLOAD_AHEAD=1, JOIN_PARTS=8, WIDE_BACK. Main queued run-112 with none of fox's launcher switches.
  3. PS_Imposter's attribute interpolation.
- Per-switch program counts (portB against fox; whole pack offline, `--list` with VICTORY_HOST_MAX_WAVE=32):
  - NAN_GUARD: 9,814 pixel programs.
  - HALF_VIEWS: 77 (75 compute, mostly compute_graph_ops, plus IrradianceCache_Shade, ScreenProbes_Inject and 2 light-proxy pixel programs).
  - D16_BITS: 1,454 (1,450 mesh, 4 compute).
  - EARLY_DEPTH: run not finished.
  - All defaults together: 11,489 changed and 26 newly translated.

## Speed items (translator), 2026-11-26

| Item | Status | Switch (default) | Changes output |
|---|---|---|---|
| 2 Reads deduped and hoisted (`bitcode/opt.rs` HOIST) | built. A read no store of the module can change (through an SRV, or any read in a module that writes no memory) is reused across blocks (REUSE). Loop-invariant reads, handles and arithmetic move to the loop preheader. Not moved: samples, derivatives, LOD queries, gathers, divisions | `VICTORY_HOIST_LOADS` (0) | with =1: 9,851 programs change. Whole pack with `--validate`: validated counts identical on and off (14,220 / 2,675 / 8,673). Off: bit-identical to the build before. Counter `bitcode::opt::HOISTED`. Commits b492cba, 345fcd3, 778ec4a |
| 4a Faults store inside `if missing` | present in cool_fox (`lower.rs` `memory`, `begin_guarded(Some(missing))`) | – | – |
| 4b Directory entry cached per page | not built (<= 0.3 ms today) | – | – |
| 1 Mesh output counts | not built. cool_fox has the same fixed counts (SetMeshOutputCounts(max, max) + culled triangles). The console's counts come in m0 at a one-lane `s_sendmsg` GS_ALLOC_REQ, often computed after LDS culling. DXIL needs the call in group-uniform flow before any output write, so an exact version must carry m0 to every thread (e.g. through LDS after the request) or prove it is computed from group-uniform values in the entry | – | – |
| 3 Paired form: lane-invariant work once | not built (L) | – | – |
| `VICTORY_WAVE_PAIRS_FIRST=1` | changes 61 programs (whole pack, offline): lighting, shadow masks, denoising, decals, particles, light atlas. run-108 against run-106: card busy 88 -> 96 %, mean frame 92 -> 74 ms. Inconclusive for card time: the log has no per-pipeline card meter | – | – |

### Glow lead found after parking (validator)
With cool_fox's defaults, 53 programs fail validation with "interpolation mode that differs from another element
packed into the same row". Nearly all are foliage: PS_PropFoliageDeferred_Fire_ExportDepth 23,
MSNC_VS_PropFoliageSkinDeferredFire 8, MS_VS_PropFoliageDeferredFire 7, MSNC_VS_TransformUnlit 7, PS_Imposter 2,
OctaSLOD 3, PS_PropFoliageDeferredLod_Fire_ExportDepth 2, PS_InteriorDrawClearDepth 1. The old tree fixed this with
one mode per register (soldsawn 26600c6). In portB it is `VICTORY_ONE_MODE_PER_REGISTER`, default 0. A/B run asked of main.

## Glow: user observations added 2026-11-26 (parked; no guess runs)
- The glow is PURPLE (bushes, some trees; palms fine). Separately: purple artifacts in some scenes that stay on screen
  for a few seconds, then fade (user: "maybe upscaler or something").
- run-116 (ONE_MODE_PER_REGISTER=1 + WAVE_PAIRS_FIRST=1): bushes still glow. NaN guard on colour exports: no effect.
- Reading (not tested): lingering for seconds = a pass with frame history (temporal AA / upscale / denoise / irradiance
  cache) is fed bad values (NaN, negative or huge) or a missing input and carries them for some frames. Purple = red and
  blue survive, green does not. The layer has no magenta stand-in of its own (grep: none), so it is data, not a marker.
- Known missing pieces to fix first, then look again: ~820 draws/frame dropped (pipeline methods 0x1c8 / 0x218), one
  compute dispatch without a host pipeline, ray-tracing structures unbuilt, compute stores are not NaN-guarded
  (VICTORY_NAN_GUARD_STORES=0). Trace from what the game asks for (rendering-pipeline study), not by switch trials.
- User, 2026-11-26: "the purple artifacts may be due to missing lighting". Fits: on the old tree, areas lit through the
  unbuilt ray-tracing structures were BLACK; on this base the same gap may come out purple (different fallback/NaN
  path). First thing to check when this is picked up: which lighting passes get no or empty input (ray-tracing
  acceleration structures unbuilt, the dropped compute dispatch, the ~820 dropped draws), per pass, from the log.
