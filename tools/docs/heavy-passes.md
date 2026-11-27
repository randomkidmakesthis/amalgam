# Heavy passes: names, dissection of the translated code, translator plan (2026-11-26)

Read-only analysis. Sources: `run-72-pipelines/plain.txt` (18 tables; 12 of them gameplay, T7-T18), heavy-window
summaries of run-73/74/75/76/78, the emitted DXIL of 51 programs (own copy of `victory.exe`, `translate --list
--jobs 2` at Idle with `VICTORY_TRANSLATE_ONLY=<hashes>` so only those containers are translated; `VICTORY_DUMP_MATCH`,
`VICTORY_EMIT_DUMP`, `VICTORY_SHADER_CENSUS`, `VICTORY_WAVE_WHY=1`, `VICTORY_HOST_WAVE=32`, `VICTORY_VIEW_LIMIT=999999`;
disassembled with `dxc -dumpbin`), `victory-shader/src/emit/lower.rs`, `victory-gpu/src/{memory,shadow,draw}.rs`.
Dumps: `%TEMP%\claude\d245a4cb1a13aa0f\e2849ea2-69d8-4b13-8b7b-b7b25a33f808\scratchpad\hp\{dump,emit,ll,census.tsv}`
(scratch; a reboot may clear it; the command above remakes it in seconds). **[P]** proven (IR, code or log), **[I]** inferred.

## 1. Verdict
- [P] The two passes that stand out use NONE of the mechanisms named so far: no raw-address path (no directory load,
  no faults store), no wave pairs (native 32-lane programs), no ray-walk cap, and their loops are data-bounded.
  Translator clean-ups cannot make them several times faster; see section 3 for what is left.
- [P] Of the 48 rows per table only ONE program goes through the directory and the faults view
  (`ScreenProbes_TraceRays...BVH`, 0.3-0.6 ms a frame). The faults store is worth <= 0.3 ms today.
- [P] Run-72 never logged a missing page ("touched pages that had no copy": 0 lines): the unconditional store only
  ever wrote the sink.
- [P, one run each] `VICTORY_SHADER_LOOP_LIMIT=64` (run-75): title lists 66.2 ms against 50.2 (run-74, same switches
  without the pipeline meter): no gain, so loops do not run to their caps. `VICTORY_CARD_BUFFERS_MB=2048` (run-73,
  run-78): 51.8 / 50.2 against 50.2: no gain; the 1 MiB floor was NOT lowered in those runs. Skipping three passes
  (run-76): 50.2 -> 38.6.
- [P] Where run-72's card time is (T8-T18; the meter adds ~11 ms a frame, about 4 us a binding): 16-25 ms in 19-27
  passes of >= 200 us a binding; 14-34 ms in 1,200-2,200 small bindings at 11-15 us each (mesh + pixel pairs mostly).
  The tail is as large as the heavy block; every mesh program in it carries the same fixed cost (section 3, M).

## 2. Names (ms a frame in gameplay tables T7-T18; us per binding)
| Pipeline | Name (shaders.rpf) | ms | us each | Kind |
|---|---|---|---|---|
| compute cb6adcc6da2613ff | `indirect_diffuse_lighting:CS_ScreenProbes_EvaluateRayRequests_RayBinning` | 1.7-6.4 | 3,160-6,580 | screen-space ray march per probe ray |
| compute 5cf00dc39042edaf | `shadowmask:CS_GenerateShadowMasks_Checkerboard_NoHair` | 1.3-9.4 | 1,440-9,950 | per pixel, loop over lights, filtered shadow-map lookups |
| vertex f7f331c8 + pixel 3bfe730f | `VS_Bink` + `PS_BinkNoAlpha` | 1.7-5.6 | 5,400 | video frames (T1-T6 only: intro, not gameplay) |
| compute 0a5ecfee6b79e13b | `shadowmask:CS_GenerateShadowMasks_HairOnly` | 0.8-1.5 | 820-1,560 | as 5cf, 64-lane program, PAIRED |
| vertex 7f4b6212 + pixel 5ad3a82b | `vs_spot_light_proxy_conservative` + `ps_light_proxy_conservative` | 0.4-1.3 | 580-1,530 | light binning in a pixel program (71 atomics, 34 loops) |
| mesh b7a81f5b + pixel 43e24028 | `MS_VS_TransformD` + `PS_DeferredTextured` | 0.5-2.2 | 19-36 | G-buffer geometry, 27-81 bindings |
| compute 660814153af43185 | `GpuParticleUpdate:CS_UpdateParticles` | 0.3-1.7 | 24 | 64-thread groups, barrier form, 11-70 bindings |
| compute f8f50ab85e7b10bb | `GpuParticleUpdate:CS_DrawBatch` | 0.6-1.2 | 17 | 35-67 bindings |
| mesh 910b4be0 / 321d411b + pixel | `MSNC_VS_LinearDepthSkin` + `PS_LinearDepth(Opaque)` | 0.3-1.3 | 26-67 | skinned depth |
| compute 2dc364df5d3278b8 | `indirect_diffuse_lighting:CS_FinalGather_0_1_1_0` | 0.5-0.8 | 590-810 | 115,200 groups of 32 = one thread a pixel [P, indirect.rs:406] |
| compute 66d8f8f6 / ffdb4344 / 34fb51b8 / 17e73620 | `...CS_ScreenProbes_GenerateRayBins` / `GenerateShaderBins` / `TemporalResolve_ReflectiveCaustics` / `PrefilterRadianceMips` | 0.5-0.8 / 0.2-0.4 / 0.4-0.5 / 0.1-0.3 | 300-790 | probe chain |
| compute f22a76fcb967ee59 | `...CS_ScreenProbes_TraceRays_RayBinning_BVH_Mixed_ShaderBinning_WithFar` | 0.3-0.6 | 420-630 | the ray walker (raw addresses) |
| compute bad01f53f53248e5 | `tiled_lighting:CS_MaterialTilesInitial` | 0.4-0.6 | 610-680 | |
| compute 2cc4c197ee29e291 | `sr_tracing:CS_Tiling_Generate_Opaque` | 0.3-0.7 | 490-800 | PAIRED |
| vertex 25ebc920 + pixel 48e74d16 | `VS_TiledLighting` + `TL_PS_NoDir_PBR_MATERIAL_STANDARD` | 0.3-0.8 | 710-850 | full-screen lighting |
| compute 8f9eb68d / 3a79195f | `bvh_skinning:CSComputeSkinningRigidVelocity` / `ped_skinning_cs:CS_PedStandardSkinning_0` | 0.4-1.6 / 0.5-0.7 | 155-260 / 230-350 | skinning |
| compute ce30cb33 / 8bbcbda6 / 0184869d | `scattering_transmittance_system:CS_Gather_Classified_10 / _2 / _8` | 0.5-1.1 | 740-1,250 | fog march |
| compute 2561af95, 71883e40, df11dcd0, 0e90505e, 4c0ac499, 2f03a2df | `ptx_prerender:ptfx_CS_lightAtlas_Sprite` (PAIRED), `sr_denoising:CS_TemporalResolve2_Opaque_FrameStatistics` (PAIRED), `TAA2:CS_TAA2_Resolve`, `vfx_proj_decal_vbuffer:CS_ProjDecalClassify`, `postfx_splines:CS_BakeLUT_...`, `deferred_tiled:CS_DepthProcessingHBO` | 0.2-0.8 each | | |
| mesh adc5c689 / 89a62261 / df4b2c77 / 792314ea / 38ad582f / 30332cf8 | `MS_VS_PropFoliageDeferred`, `MSNC_VS_PropFoliageSkinDeferred`, `MS_VS_TransformTerrainNormalTint`, `MS_VS_Transform`, `MSNC_VS_Cable`, `MS_VS_Transform_PositionOnly` | 0.2-1.2 each | 21-160 | geometry |
- compute 14264103296aa43a = `indirect_light_probes:CS_Primary_RegisterAllocationRequests` (3.4 ms in run-12): [P] in no table
  of run-72 (under 0.15-0.3 ms). dfafaf38+06295fb1, 0602c238+a78d088f, 58d92608+edc1cad8: not in shaders.rpf.

## 3. Dissection (static counts of the emitted DXIL; "x" = host instructions per console instruction)
| Program | Group | Console / host insts | Loops (cap) | Raw-address sites | Paired | Texture reads | Buffer loads / stores / atomics | f64 |
|---|---|---|---|---|---|---|---|---|
| cb6adcc6 EvaluateRayRequests | 32x1x1 | 700 / 1,602 (2.3x) | 1 (4,096) | 0 | no | 5 sample, 7 load, 3 gather | 27+24 cb / 4 / 3 | 0 |
| 5cf00dc3 ShadowMasks NoHair | 4x8x1 | 5,746 / 12,777 (2.2x) | 5 (4,096) | 0 | no | 62 sampleLevel, 35 sampleCmp, 20 load | 226+16 cb / 3 / 5 | 0 |
| 0a5ecfee ShadowMasks HairOnly | 32x1x1 | 5,212 / 46,493 (8.9x) | 5 (4,096) | 0 | yes | 148 sample, 37 load | 411 / 5 / 9 | 0 |
| f22a76fc TraceRays BVH | 32x1x1 | 3,579 / 11,228 (3.1x) | 11 (1,024) | 126 (125 in the walk loop) | no | 0 | 372 / 137 / 2 | 0 |
| 2dc364df FinalGather | 8x4x1 | 1,766 / 4,109 (2.3x) | 6 (4,096) | 0 | no | 16 load, 8 sample | 74+26 cb / 0 / 0 | 0 |
| 5ad3a82b light proxy (pixel) | - | 8,202 / 15,149 (1.8x) | 34 (128) | 0 | no | 2 | 152 / 11 / 71 | 0 |
| 2561af95, 71883e40, 2cc4c197 | 32x1x1 | 4,930 / 43,498; 1,379 / 11,405; 422 / 3,543 (8.3-8.8x) | 3 / 2 / 1 | 0 | yes | | | 0 |
| mesh b7a81f5b ... 89a62261 | 128x1x1 | 505 / 1,229 ... 5,484 / 13,705 (2.4-2.5x) | 0-26 (256) | 0 | no | 0-67 | 71-628 raw loads | 0 |
No program of the 51 uses 64-bit floats [P]. Descriptor picks: 29-146 `createHandleFromHeap` per program, each with a
UMin clamp; only f22a76fc (125), the skinned mesh programs (1-2) and bvh_skinning (4) pick non-uniformly [P].

**cb6adcc6 (5-6.5 ms).** [P] Straight-line: three early-outs (ray index against a count; request.y >= 0; per-ray word
> -1), 13 single-dword loads of one small raw buffer, then ONE loop: 39 host instructions and one `SampleLevel` of a
depth-like texture per step, left on a hit or after `N - 2` steps (N = a constant-buffer word, also the step size
1/N); the cap adds 2 of the 39. After it 4 samples, 3 gathers, 2 atomics on a raw buffer (the bins), 3 stores.
[P] It is at most 65,535 x 2 groups (a split record: 4.2 M threads) and the final gather (3.69 M threads by the
comment at indirect.rs:406; 4,109 instructions) takes 0.6-0.8 ms, so cb6 costs at least 5 times more per thread
with 2.5 times fewer instructions: its time is NOT instruction count. [I] It is (a) rays x steps texture reads, with more rays active than on the console if upstream
request data are not thinned (untraced rays, history), and/or (b) loads and atomics on buffers left in the PC's
memory (under 1 MiB: `shadow.rs` `card_limits`; shadow.rs:323 measured 1.8 ms for ONE group of 170 such loads).
Dispatch size and early-out rates are recorded nowhere. Translator share: under 10 % [I].

**5cf00dc3 (1.3-9.4 ms, 7x by scene).** [P] Two nested loops over the lights of a tile (exits: a mask word == 0; an
index != 511), body 11,458 host instructions with 94 samples (62 `SampleLevel` + 35 `SampleCmpLevelZero` statically
in the program), 202 single-dword raw loads (60 of the program's 223 are textual duplicates, 17 invariant in the
outer loop), 38 handle creations, 32 `WaveReadLaneAt`, 31 ballots. The scene dependence is the light count [I]. What a
translation change can touch: duplicate and invariant loads, handles made again each iteration: 10-20 % [I].

**0a5ecfee and the other paired programs (2-3.4 ms together).** [P] 8.3-8.9 host instructions per console one against
2.2-2.4 unpaired: the body is emitted for both lanes with 7,148 guard selects and 11,726 bitcasts; `VICTORY_WAVE_WHY`:
"LDS in a group over 32, an LDS read and write in one lane-branch region, mask as data, v_mbcnt, v_readlane". One
host thread does two lanes, so the thread count halves; the excess over an ideal form is 30-45 % of them [I].

**f22a76fc (0.3-0.6 ms).** [P] Per raw access (lower.rs `memory`): 5 shifts/masks, UMin, the directory load, compare,
URem, add, shl, select, the faults store, add, a NON-UNIFORM heap handle + annotate, and, add: 16 instructions, one
extra load, one store, where the console has one load. 125 of them in the walk loop of 11,235 instructions: ~18 % of
its instructions, a third of its loads, half of its stores. [P] The directory (16 MiB) and the faults (16 KiB) are
`shared_buffer`s: in the PC's memory, reached over the bus. [P] At 0.3-0.6 ms a frame the walks do not run to 1,024.

**Mesh programs (the tail).** [P] Every mesh program declares `SetMeshOutputCounts(max, max)` (128, 128 here) and has
each lane first emit a culled triangle (0,0,0) (lower.rs `mesh_prologue`, marked "shortcut ... Pass the program's own
counts if that shows in frame time"): the card allocates and assembles 128 vertices x up to 9 attributes and 128
primitives per meshlet whatever the meshlet holds. Share of the 14-34 ms tail: not measured.

## 4. Where the time goes, per mechanism (today, heavy scene)
| Mechanism | Programs in the tables | Bound, ms a frame | Tag |
|---|---|---|---|
| Faults store + directory load | f22a76fc only | <= 0.3 of its 0.3-0.6 | [P] sites, [I] share |
| Walks to the trace cap | none (f22a76fc ends early) | ~0 | [P] |
| Loop caps as the exit taken | none | ~0; counter = 2 instructions an iteration | [P] run-75, IR |
| Wave pairs | 0a5ecfee, 2561af95, 2cc4c197, 71883e40 | 0.7-1.5 of 2-3.4 | [P] programs, [I] share |
| Duplicate / invariant loads, handles | 5cf, 0a5e, ce30cb33 (114 of 157 constant loads invariant), probes chain | 0.5-2 | [I] |
| Fixed mesh output counts | all 8,765 mesh programs | 1-4 | [I] |
| NOT the translator: data volume and memory placement of cb6 and 5cf | cb6, 5cf | 7-15 of their 7-16 | [I] |

## 5. Plan (every translator item changes the shader cache key: re-warm + one cold pipeline run)
Step 0 (S, no cache change, decides items 1-2 of the layer side): one diag run with the op listing or
`VICTORY_PROBE=<file>` for cb6adcc6, 5cf00dc3, 66d8f8f6 (`VICTORY_PROBE_FRAMES=1:600`): groups per dispatch, the
ray count word, N; and `VICTORY_CARD_BUFFERS_MIN_KB=64` TOGETHER with `VICTORY_CARD_BUFFERS_MB=2048` plus
`VICTORY_CARD_METER_PIPELINES=1`: if cb6/5cf fall, their cost is the bus (layer work: small buffers, directory and
constant copies on the card), not the translator.
| # | Translator change | Benefits | Expected ms | Size | Exact? Verify | Symptom if wrong |
|---|---|---|---|---|---|---|
| 1 | Mesh: the program's own vertex and primitive counts in `SetMeshOutputCounts` when the allocation request's operands dominate every output write and are wave-uniform (take them with `WaveReadLaneFirst` in the entry block); keep maxima + culled triangles otherwise | all mesh draws (tail) | 1-4 [I] | M | exact (same triangles); mesh tests in `emit/`, `VICTORY_CAPTURE` frames byte-equal on a fixed input script | missing geometry, or long triangles over the screen |
| 2 | Loads: one load per (read-only view, offset) per dominating block chain; hoist loads and handle creations whose operands are defined before the loop; only in programs with no store or atomic through a view that can alias (not the raw-heap writers) | 5cf, 0a5e, ce30cb33, FinalGather, mesh skinning | 0.5-2 [I] | M (`bitcode/opt.rs`) | exact under the no-alias rule; diff tests; probe `card_after` hashes of 5cf's outputs equal over N frames, old against new cache | stale light or shadow data in one tile |
| 3 | Wave pairs: skip the pair for programs whose cross-lane uses are provably within 32 lanes; else emit lane-invariant work once (loads, handles, uniform branches are per pair, not per lane) | 0a5ecfee, 2561af95, 2cc4c197, 71883e40 | 0.7-1.5 [I] | L | exact only with the proof; `wave64_tests.rs`, probe outputs | hair shadows, particle light atlas, reflections tiles wrong |
| 4 | Raw addresses: (a) store to the faults view only inside `if missing` (dword 0 is a sink `take_faults` never reads: exact); (b) keep (page -> copy, start) in SSA per base register and reuse when the page compares equal; (c) uniform handle when the copy number is wave-uniform | f22a76fc, 64 walker programs, the builder | <= 0.3 today; 1-3 once the builder runs [I] | S (a), M (b, c) | (a) exact; intersect tests + "pages that had no copy" count unchanged | missing-page notes lost: black ray results a frame longer |
| 5 | Loop counters: none where the exit compares an induction variable with a loop-invariant bound | all loops | < 0.2 | S | exact unless data is garbage: keeps the hang guard elsewhere | card hang on unmirrored data |
| 6 | Guard chains: one boolean per region instead of `select(a, select(b, false, x), x)` per compare | all | < 0.3 [I], the driver folds most | S | exact | - |
Not proposed: merging adjacent single-dword loads into 4-dword loads (fxceless 96ff135: a load reaching past the
view's end may read zeros for the whole call; their water went black, run 277).

**Unbuilt structures.** Neptune 25fbf69 (`memory::empty`, host only, no cache change): walkers get an all-zero
directory, every read 0, nothing copied or compared; a zero structure is an empty scene, every ray misses at its
first test. fxceless 96ff135 (`VICTORY_BVH_NO_HIT=1`, translation): the node test answers "no hit" and reads nothing.
Neither is exact against the console, and neither is provably equal to today's picture (today a walk reads the
title's bytes where a copy exists). Both give the same deterministic "no occluder" result. On the card they save
<= 0.5 ms here (f22a76fc already ends early); Neptune's saves the host-side copies of every numbered window per
dispatch, which is CPU and copy time, not card time. They do not change the ranking above.

## 6. Proven / inferred
Proven: names; all static counts; absence of raw-address code, pairs and trace caps in cb6 and 5cf; loop exits and
caps; fixed mesh counts; the per-access sequence; the one-switch run totals; that cb6 is not instruction-bound
(against FinalGather, given its group bound). Inferred: every ms in the plan; what cb6 and 5cf are bound by (no
dispatch sizes are logged); the mesh share of the tail; run-75/73 as single runs without a per-pipeline table.
