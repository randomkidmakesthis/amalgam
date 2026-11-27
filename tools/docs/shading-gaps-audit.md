# Shading gaps that are not ray tracing (analyst Z, 2026-11-26)

Read-only. Nothing built or run, no code edited, no picture looked at.

- Tree: `C:\GTAVI\Tools\par\foxT`, branch t-on-w, head 3c0f0c5. Paths are relative to `par\foxT\crates`.
- Run: `victory-data\bench\run-231-fox-journal-growth-6min\plain.txt` (8,690 frames, RT off, standing set).
- Cache: `victory-data\coolfox\cache` holds only binaries plus two pipeline name lists; no per-program report of
  which translation form was chosen. So "how many programs" is known only where a code comment or an older
  note of ours gives a count.
- "Confirmed hit" means a counter or log line in run-231 shows the mechanism fired. **The link from a mechanism to
  a symptom the user sees is my inference in every row**; none was checked against a picture.

## Re-confirmed: every dropped draw is ray tracing

Drop reasons in run-231: "left out by name: CS_RaytracingResolve", "left out by name: CS_ScreenProbes_ActivateRays",
"a stage of the ray-tracing structure builder", and "being made" twice (pipelines still compiling). Last report:
1,237.6 dropped a frame = 1,233.8 + 3.5 from the two families, the rest builder stages. Pipelines that failed
to translate or to be made, since the start: **0** in all 69 reports. No non-RT program is missing.

## Ranked table

### Confirmed hits in run-231

| # | Gap | Console / what we do | Evidence (run-231) | Symptom it could explain | Meter | Verdict |
|---|---|---|---|---|---|---|
| 1 | **Material-class depth plane read one step stale.** | The title's tiled lighting writes each pixel's material class into the depth plane of a depth-stencil texture, then "draws each class with the depth test equal" (`victory-gpu\src\resource.rs`, `Twin` comment). The console lets shaders write a depth texture directly. We write a colour twin and draw it into the plane with a pass of ours **when the list closes**; a depth use earlier in the same list "sees the plane as it was" (`victory-gpu\src\host.rs`, shortcut at "the depth plane's twin is not drawn into the plane here"). | "depth planes: 1.0 drawn from their twins per frame, **7,263 times used before the pass**" over 8,690 frames, growing steadily (939, 3,175, 7,263). About 0.84 a frame. | Per-class shading applied by the previous class map: wrong material response along moving silhouettes. Candidate for **magenta/blue streaks on characters, shiny "wet" skin, the dark band across the eyes**. Would not depend on RT. | Exists as a count (`PLANES[2]`). Missing: which pipeline's draw is the late user. One line naming the pipeline at the first 20 late uses would settle it. | **CONFIRMED HIT** (count); symptom link unproven |
| 2 | **Stale bytes drawn before a safety net caught them** (our memory hooks). | Console: shared memory, no copies. We skip refreshes for pages believed unchanged (`VICTORY_KNOWN_WRITES`, `VICTORY_GPU_ONLY`) and compare later. | 400 "FILTER MISS" lines, 32 "changed", **23 "bytes DIFFERED"** (buffer windows drawn with old bytes until taken), 1 "STALE BYTES FOUND ON A SPARED PAGE", 7 "stale GPU-only copy found" (textures), 102 "a texture picked by index changed after its upload with nothing to tell of it". | A vertex, bone or constant buffer one or more frames old: **brief mesh stretching**. A texture old for a few frames: pop-in. | The lines above are the meter. Launcher's `-NoHooks` is the A/B. | **CONFIRMED HIT** (23 + 1 + 7 + 102) |
| 3 | **Stand-in textures while the real one is not on the card.** | Console: a texture is there when its bytes are. We show the old texture of the slot or a 1x1 gray (0x80) (`victory-gpu\src\descriptor\host.rs`, `neutral`, `keep_shown`). | "slots shown after their texture's upload: 515,000, 0.85 frames after, the longest 3; **the neutral texture meanwhile in 729,439**"; kept another texture: 1,896 + 219 of another kind. Queue at the end: 13,604 slots, **12,847 waiting for their memory**, oldest due one written 566 frames ago. | **Texture pop-in.** Gray in a mask or parameter slot reads as 0.5 everywhere: candidate for **shiny skin (intermittent)** and **sticky patches** while a slot waits. | Counters above. `VICTORY_KEEP_SHOWN=0` changes the look and so tells. | **CONFIRMED HIT** |
| 4 | R9G9B9E5 render targets as R16G16B16A16_FLOAT. | Console format has no alpha and stores negatives as 0. Ours keeps both (`victory-gpu\src\resource.rs:269`). | "a stand-in is used format=67 stand_in=10" (also 86 -> 28, 115 -> 28). How many targets and draws use it is not counted. | A surviving negative channel reads as a colour cast: candidate for **sticky purple patches**, **violet dots**. | None for negatives. Port of `VICTORY_E5_TARGETS` is the test. | **CONFIRMED in use**; effect unproven |
| 5 | Three indirect command signatures dropped. | "signatures that change root arguments are dropped" and executions that do not run (`victory-gpu\src\list.rs`, `execute_indirect`). | Three first-time lines: stride 12 args [2, 13, 0, 0]; stride 12 args [10, 0, 4140721839, 32761]; stride 20 args [1, 32761, ...]. Two carry what looks like uninitialised words. Per-signature counts are not logged; the per-frame "indirect" drop total is dominated by RT. | Unknown: whatever those dispatches or draws produce is missing. | One counter per signature, with the bound pipeline's name. | **CONFIRMED HIT**, size unknown |
| 6 | Pipeline creation methods 0x1c8 / 0x218 fail. | `victory-gpu\src\device.rs`, `unknown_pipeline`: descriptions are console-native; the call fails and "the title goes without this pipeline, silently". A comment cites another fork: the title then made the same compute programs from ordinary containers. | 29 warnings at load (16 + 13). Nothing per frame. | Unknown. If there is no fallback for some, those effects never draw. | `VICTORY_PIPELINE_METHODS=1` exists and reports what the thread did next. Not on in this run. | **CONFIRMED HIT**; loss unknown until the meter runs |
| 7 | Water read-back could not be tiled once. | `victory-gpu\src\texback.rs`: shader-written water maps are copied back for the title's CPU. | 1 "a texture copied back could not be tiled e=Unsupported mip=0 slice=0"; 24 "texture readback landed". | One water map kind never reaches the CPU side: boats or foam, not shading on land. | The warning. | CONFIRMED HIT (1), low reach |
| 8 | View heap capped at 1,000,000 of the title's 1,245,184 slots. | Title slots past 737,856 read a null view (`victory-gpu\src\descriptor\host.rs`). | Warning once. Writes past the cap are not counted. | A texture or buffer reads as nothing. | One counter of views written at or past the cap. | POSSIBLE (cap confirmed, overrun not) |

### Possible (need a meter)

| # | Gap | Where | What happens | How many programs | Symptom it could explain | Meter needed |
|---|---|---|---|---|---|---|
| 9 | **A join value with no DXIL form becomes 0 "without a word".** | `victory-shader\src\emit\lower.rs:13225-13237` | "40 programs that translate today stop by name with [`VICTORY_STRICT_JOINS`] (vehicle paint pixel shaders ..., **foliage mesh shaders**); until those are lowered they keep drawing with that zero." | 40 by the comment | Foliage mesh shaders drawing with a zero: candidate for **white/violet dot clusters on hedges and far palms (also with RT on)**. Vehicle paint. | Offline: `victory shaders` with `VICTORY_STRICT_JOINS=1` lists them by name. At run time: count draws of those 40 pipelines. |
| 10 | **First half of an interpolation pair written as 0.** | `victory-shader\src\emit\lower.rs:10906-10928` | cool_fox's form is kept by default; `VICTORY_FIRST_HALF_RETRY=1` is the exact retry. The comment: writing 0 "changed 59 of the title's pixel programs". | up to 59 pixel programs | An interpolated input reading 0 in a pixel shader: candidate for streaks or wrong shading on specific materials. | Offline list of programs whose translation differs with `VICTORY_FIRST_HALF_RETRY=1`; then a watched run with it (re-translates). |
| 11 | **Wave64 compute programs in approximate forms** on a 32-lane host (NVIDIA). | `victory-shader\src\emit\mod.rs:646-920`: "local waves (APPROXIMATE)": "a tile-wide result becomes two half-tile ones"; "loose LDS (APPROXIMATE)"; "local waves (APPROXIMATE, any mask)". `VICTORY_SCC_LOCAL_ANY` stays off because "the frame gained black blocks and lost walls". | Not known for this tree. An older note of ours counts 36 approximate programs in a related tree, and names lighting compute among them. | Tile-wide lighting results split in halves: blocky light, light culling errors; candidate for **dot clusters** and **dark band**. | Exists offline: `VICTORY_WAVE_WHY=1` prints "WAVE64-RETRY ... (APPROXIMATE ...)" per program at translation. Needs a cold cache or `victory shaders`. `VICTORY_WAVE_LOCAL=idiom` and `VICTORY_WAVE_LDS_LOOSE=0` refuse them (their dispatches then drop) as an A/B. |
| 12 | **Shader loop cut with pixel discard** (128 pixel, 256 vertex/mesh, 4,096 compute). | `victory-shader\src\emit\lower.rs`, "a pixel loop past 128 iterations" | A lane past the cut leaves; a pixel is discarded. | Every program with a loop carries the cut; how many can reach it is not known, and nothing counts hits at run time. | Holes or dots where a light list or march is long: candidate for **dot clusters on far vegetation**, flicker. | No run-time counter is possible without changing shaders. Test: `VICTORY_SHADER_LOOP_LIMIT=1024` (re-translates). |
| 13 | NaN and infinity from compute stores unguarded. | `victory-shader\src\emit\lower.rs:52-56` | Only pixel colour exports are guarded. | 9,814 pixel programs carry the export guard (older note); stores: none. | Sticky patches, white water. | `VICTORY_NAN_GUARD_STORES=1` (re-translates). |
| 14 | Mesh shader outputs past the host's 32 KiB are emitted with lower maxima. | `victory-shader\src\emit\lower.rs:5026-5034` | Primitives past the lowered count are not emitted. | Not counted. `VICTORY_MESH_OUTPUT_STRICT=1` refuses them, which lists them. | Missing triangles in dense meshlets. | Offline list with the strict switch. |
| 15 | Buffers read through directly indexed heaps keep what their first use copied; title writes between lists are seen a frame late. | `victory-gpu\src\shadow.rs:13-16`, `draw.rs` ("seen a frame late") | Stale buffer for a frame. | Not counted. | **Brief mesh stretching**, besides row 2. | A count of such buffers whose bytes differ at the next look. |
| 16 | A texture the host wrote keeps the host's contents if the title later rewrites its memory. | `victory-gpu\src\texture.rs` ("keeps the host's contents") | Title-side reset not seen. | Not counted. | White water after a scene change. | Count title writes to host-written textures. |
| 17 | Meshlet culling gather: level rounded, edges clamped. Samples outside pixel stages read a fixed level. Cube gradient samples without anisotropy. | `victory-shader\src\emit\lower.rs` (gather and sample shortcuts) | Slightly wrong culling or mip. | 730 compute programs sample without derivatives (code comment). | Flicker of far objects; aliasing. | None cheap. |

### Checked and not hit, or equal to the console

| Item | Finding |
|---|---|
| Programs failed to translate at run time | 0 failed since the start. |
| Tables refused, unordered views refused, buffer views left null, depth textures without a twin | 0 each in the last report. |
| Rectangle lists without a geometry stage | No "no rectangle geometry stage" line. |
| Depth bounds test | Supported path; no "does not support" warning in the run. |
| Sampler state | Anisotropy is passed as asked (up to 16); LOD bias and clamps decoded. Approximations: half-border address modes "become their nearest"; a custom border colour becomes transparent black (`victory-shader\src\emit\sampler.rs:52`, `:79`). No count; low reach. |
| Formats 86 and 115 (16-bit packed colour targets) | Stand-in R8G8B8A8: exact for 4-bit, within 1/510 for 5-bit (`resource.rs` comment). |
| Minimum LOD of moved views | Kept as the title set it (ours). |
| Variable-rate shading | Ignored (`list.rs`, "shading rate" -> ignored state): full rate, so not less picture. |
| GPU queries | Never run; occlusion answers "visible". Nothing is hidden by it. |
| Target format variants, early depth | On by default in this tree. |
| Tessellation, geometry stages, stream out, dual-source blend, logic ops, conservative raster | I found no warning for any in the run and did not trace each state field through `pipeline\host.rs`. Not verified beyond that. |

## The depth twin, in full (row 1)

1. The title makes a depth-stencil texture that it also writes from compute. The console allows both uses on one
   texture. The host does not, so the layer keeps a colour "twin" for each plane.
2. The tiled-lighting compute pass writes a material class per pixel into the depth plane's twin and masks into
   the stencil plane's twin.
3. The title then draws once per class with the depth test "equal", so each class is shaded by its own program.
4. The stencil twin is copied into the stencil plane through a buffer. The depth twin cannot be copied into a depth
   plane; a pass of ours draws it there. That pass needs its own pipeline and target, "which a list still recording
   the title's commands cannot take", so it runs when the list closes.
5. A draw in the same list that tests against that depth plane before the close tests against the plane as it was
   left by the last pass: an older class map. The layer counts this ("used before the pass": 7,263).
6. What it shows as, if the late users are the per-class draws: pixels whose class changed since the older map
   are shaded by the wrong class's program or by none. In a still picture nothing; on a moving character, a
   fringe or band where skin, eyes, hair and background meet.

What is not known: which draws the 7,263 late uses are. If they are the class draws, this is a first-rank defect
and independent of RT. If they are some other depth test, it is harmless. The meter is one log line.

## Symptom map

| User's symptom | Candidates, strongest first | Deciding step |
|---|---|---|
| Magenta/blue streaks on characters (5 runs) | Row 1 (stale class map); row 10 (first-half zero); row 4 (negatives in the stand-in) | Name the late depth users (row 1 meter) |
| White/violet dot clusters on hedges and far palms, also with RT on | Row 9 (foliage mesh shaders with a silent zero); row 12 (loop cut discard); row 11 (approximate wave forms) | Offline `VICTORY_STRICT_JOINS=1` list: are hedge and palm programs in it |
| Shiny "wet" skin, intermittent | Row 3 (gray stand-in in a parameter slot); row 1 | `VICTORY_KEEP_SHOWN=0` changes it or not |
| Dark band across the eyes | Row 1; row 11 | Row 1 meter |
| Brief mesh stretching | Row 2 (23 buffer pages drawn stale); row 15 | `-NoHooks` run: gone or not |
| Sticky purple patches | Row 4; row 13; row 3; and the parked RT's stale probe blocks | `VICTORY_E5_TARGETS` port; `VICTORY_NAN_GUARD_STORES=1` |
| Texture pop-in | Row 3 (12,847 slots waiting for memory); row 2's 102 re-uploads | Texture memory arrival, not shading |

## Not verified

- No picture, no run. Counts are from run-231's log only.
- Which translation form each program got: the cache does not say in text. Counts in rows 9-11 come from code
  comments and one older note, not from this tree's translation.
- Pipeline state fields were not traced one by one.
- Whether the three dropped indirect signatures belong to ray tracing.
