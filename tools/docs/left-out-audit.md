# What the layer leaves out, fakes or approximates (analyst Z, 2026-11-26)

Read-only audit. Nothing built or run, no code edited.

- Tree: `C:\GTAVI\Tools\par\foxW`, branch local/w-shader, head b6a2019. Paths are relative to `par\foxW\crates`.
- Run evidence: `victory-data\bench\run-197-fox-signature-local\plain.txt` (ray tracing off, standing set).
- Ray tracing (`VICTORY_BVH_BUILDER`, `VICTORY_RT_RESOLVE`, `VICTORY_ACTIVATE_RAYS`) is known and parked; it is
  listed once for completeness and not discussed.
- Everything marked "candidate" or "estimate" is from reading code and one log. No cause of a picture defect was
  confirmed by a run.

## Answers first

1. **Every dropped draw and dispatch in run-197 is ray tracing.** The per-frame totals match the three ray-tracing
   reasons exactly (for example 2,226.0 dropped = 2,221.9 `CS_RaytracingResolve` + 4.1 `CS_ScreenProbes_ActivateRays`;
   the third reason is "a stage of the ray-tracing structure builder, which is off"). Pipelines failed since the
   start: 0. Pending: 0. No other program is missing for want of a pipeline.
2. **No second switch of the LEVELS_ON_DEMAND kind was found**: no inherited setting in our set turns a correctness
   feature off.
3. **There are always-on approximations with no switch in our set that can touch the picture.** The ones most
   worth a test are the shader loop cut (a pixel past 128 loop iterations is discarded), the stand-in textures
   shown while a texture is not there yet, and the NaN guard that covers colour exports only.
4. One correction to my switch audit: `VICTORY_POOL_CAP` is read at this head (it came in with the merge of foxV);
   it was dead at 70082e0.

## Ranked table

| # | What is left out / approximated | Switch | Evidence in run-197 | Picture effect to expect | Cost / risk | Verdict |
|---|---|---|---|---|---|---|
| 1 | Shader loops are cut: pixel 128 iterations, vertex/mesh 256, compute 4,096. A pixel lane that hits the cut is **discarded** (`victory-shader\src\emit\lower.rs:13101-13113`, `:13127-13136`). The values were 1,024 / 4,096 / 65,536 and were lowered after the card was lost in the opening cutscene. | `VICTORY_SHADER_LOOP_LIMIT=<n>` (one value for every stage) | None possible: the cut happens inside the shader, nothing counts it. | Holes or flicker where a ray march or search runs long: reflections, volumetrics, distant geometry, water. | Changes the translation, so the shader cache is rebuilt (first run is cold). Higher limits lost the card before. | **TEST**, own run: `=1024`. |
| 2 | A texture slot whose texture is not on the card yet shows something else: the texture it showed before (`VICTORY_KEEP_SHOWN`, default 1 = one of the same kind) or a 1x1 gray texture (0x80 in every channel) (`victory-gpu\src\descriptor\host.rs:315`, `:388`, `:4601-4612`). | `VICTORY_KEEP_SHOWN=0/1/2`; `VICTORY_SHOW_AFTER` (ours 1) | "slots shown after their texture's upload: 210,000 ... the neutral texture meanwhile in 248,500"; "went on showing another texture: 21,735 had the new one's bytes, 131 others, 22,727 not known; of others 0, 1,397, 10". Queue at the end: 10,363 slots, 7,968 waiting for their memory. | Wrong material for a while: gray reads as 0.5 in every mask or parameter channel, an old texture reads as another object's. Candidate for patches that stick until the upload lands. | `=0` shows zeros then gray: a diagnostic, not a setting to keep. | **TEST** as a diagnosis (see below). The real fix is textures arriving sooner. |
| 3 | NaN and infinity are removed on pixel colour exports only. Typed image and buffer stores from compute are not guarded (`victory-shader\src\emit\lower.rs:52-56`, `:7546-7560`). | `VICTORY_NAN_GUARD_STORES=1` (default 0) | No counter. | One bad texel in a history written by compute can persist and spread: white or coloured areas that last. Candidate for white water and sticky patches. | Rebuilds the shader cache. Known side effect: integer bit patterns in stores can be altered (a packed colour 0xFF800000 became -65504). | **TEST**, own run, picture check. |
| 4 | R9G9B9E5 render targets are made as R16G16B16A16_FLOAT, which keeps negative values and alpha; the console format stores negatives as 0 (`victory-gpu\src\resource.rs:261-269`). | none in our tree (the drop has `VICTORY_E5_TARGETS`) | "the host does not render to this format: a stand-in is used format=67 stand_in=10" (also 86 -> 28 and 115 -> 28). | A negative channel that survives reads as a colour cast (missing green is magenta). Candidate for purple. | Port from the drop is small; R11G11B10 has fewer mantissa bits. | **NEEDS CODE** (review doc, shortlist item 2). |
| 5 | Ray tracing programs and the structure builder. | `VICTORY_BVH_BUILDER=1` | 2,226-4,132 dropped a frame, all of them. With rays off, the screen-probe resolve leaves "what the probes' memory held" in tile blocks (`victory-gpu\src\ambient.rs:1-9`). | Unoccluded sky light outdoors, stale blocks. Candidate for glowing vegetation and purple patches. | Known: card time roughly doubles. | **KEEP OFF** (parked by decision). Use one RT-on run only to tell whether the purple belongs to it. |
| 6 | Two pipeline creation methods are not implemented and fail (`victory-gpu\src\device.rs:349-372`). The title then goes without that pipeline or makes it another way. | `VICTORY_PIPELINE_METHODS=1` is a meter only | 29 warnings, slot 0x218 past 16,384 calls, all within one second at load. Nothing per frame. | Unknown. A comment cites another fork: with the method failing the title made the same compute programs from ordinary containers. | Meter is cheap. | **TURN ON the meter** for one run to see what the title does next. Implementation: NEEDS CODE, low priority unless the meter shows no fallback. |
| 7 | Host view heap capped at 1,000,000 slots; the title's heap has 1,245,184. Title slots past 737,856 read a null slot (`victory-gpu\src\descriptor\host.rs:1505-1512`, `:1548`, `:1594-1600`). The comment gives the highest slot seen as about 720,000. | none (`VICTORY_DYNAMIC_SLOTS` moves the split) | The warning fires once. Nothing counts writes past the cap. | A texture or buffer view past the cap reads as nothing (black or missing). Margin is about 2 %. | None to measure; a counter is a few lines. | **NEEDS CODE**: count views written at or past the cap. |
| 8 | One indirect command signature is dropped: "signatures that change root arguments are dropped" (`victory-gpu\src\list.rs:1942-1943`, `:1984`). | none | One line: stride 12, args [2, 13, 0, 0], max 1, logged at 18:08:28 beside the first ray-tracing drop. Whether it is counted in the 601-648 indirect drops a frame is not shown per signature. | If it is a ray-tracing dispatch, nothing new. If not, some effect is missing. | — | **NEEDS CODE** (log the pipeline name with the first drop), or read it in an RT-on run. |
| 9 | Depth planes and their twins: a use can see the plane as it was before a pending pass (`victory-gpu\src\host.rs:1678`, `:6723`). | none | "2,709 times not given back to a twin, 2,710 times used before the pass" against 70,685 binds as depth. | A shader reads depth one step stale. Candidate for flicker at depth edges or far geometry. | — | **NEEDS CODE** to judge; note it as a flicker candidate. |
| 10 | Textures change without the layer being told: "a texture picked by index changed after its upload with nothing to tell of it: uploaded again" (`victory-gpu\src\draw.rs`). | none | 55 warnings in the run. Each is the safety net working, so some frames showed the old bytes first. | Brief wrong texture, then right. | — | KEEP (it self-corrects). Worth knowing for pop-in. |
| 11 | A mip that streams in is read again at most every 30 frames (`victory-gpu\src\texture.rs:1518`). | none | Not counted. | Pop-in up to a second later than needed at 30 fps. | Reading around unmapped pages is slow, which is why. | NEEDS CODE if pop-in stays after the texture memory work. |
| 12 | Meshlet culling reads its depth pyramid with the level rounded to a whole one and coordinates clamped (`victory-shader\src\emit\lower.rs:11884-11896`). | none | Not counted. | A meshlet tested against slightly wrong depth can be culled or kept wrongly. Candidate for flicker of far objects. | — | NEEDS CODE; lower priority than #1 and #9 for flicker. |
| 13 | Samples outside pixel shaders read a fixed level (no derivatives); cube samples with explicit gradients have no anisotropic filtering (`victory-shader\src\emit\lower.rs:11730-11738`, `:11791-11797`). | none | Not counted. | Slightly sharper or blurrier lookups in compute passes (G-buffer resolves, sky). | — | KEEP (small); NEEDS CODE if a resolve shows the wrong mip. |
| 14 | Half-float conversion uses the host's rounding. | `VICTORY_F16_NEAREST=1` (default off; "costs a dozen instructions per result", `lower.rs:9825`) | Not counted. | Last-bit differences in 16-bit targets. Unlikely to be visible. | Rebuilds the shader cache; slower shaders. | KEEP OFF unless banding is chased. |
| 15 | GPU queries never run: occlusion always answers "visible" (`victory-gpu\src\list.rs:1545-1551`). | none | — | Nothing is culled by a query, so no object goes missing; things hidden behind walls that depend on a query (coronas, flares) may show. | Costs draws the console would skip. | KEEP. |
| 16 | Pipeline wait budget. | ours `VICTORY_PIPELINE_WAIT_MS=20`, default 500 | "0.0 ms waiting for pipelines" with a warm library. | With 20, an object can be missing for some frames while its pipeline is made. | 500 can make long frames on a cold library. | **TURN ON the default (remove ours)** for runs whose picture is judged. |
| 17 | HDR output is not done; the title is told HDR is unavailable (`victory-hle\src\modules\display.rs:61`). | none | "HDR mode: disabled (SDR presentation)" 6 times. | SDR picture. Same as the console on an SDR display. | — | KEEP. |
| 18 | Audio: the DSP coprocessor's outputs are zeros (no convolution), speech is silent, rate conversion is missing, trigger motors are dropped (`victory-hle\src\modules\dsp.rs:15`, `speech.rs:4`, `apu.rs:163`, `gameinput.rs:1349`). | none | "audio DSP connected" once. | No convolution reverb, no synthesized speech. Not picture. | — | NEEDS CODE, outside the 30 fps goal. |
| 19 | Rectangle lists without a geometry stage draw half or nothing (`victory-gpu\src\draw.rs:4455`); sRGB views of UNORM textures read unconverted (`descriptor\host.rs:4077`); custom sampler border colours become transparent black (`victory-shader\src\emit\sampler.rs:79`). | none | No line for any of the three in the run. | None seen triggering. | — | KEEP (not triggering here). |
| 20 | Checked and clean in this run: tables refused 0; unordered views refused 0; buffer views left null 0; depth textures left without a twin 0; indirect records run empty for budget 0.00; passes without room 0. | — | fps line and "recording per frame" line at the end of the run. | — | — | Nothing to do. |

## The base's own gap markers

`shortcut:` is the base's marker for a known gap. Count at this head, tests excluded: **166**.

| Where | Count | What they are |
|---|---|---|
| `victory-shader` (translator) | 58 | Mostly exactness notes (rounding, lane order, fused multiply-add). Picture-relevant: rows 1, 12, 13, 14 and the border colour in row 19. |
| `victory-gpu` | 81 | Memory tracking limits, list ordering, depth twins, formats, queries. Picture-relevant: rows 2, 4, 6-11, 15, 19. |
| `victory-hle` | 22 | Audio, input, package and memory queries. Rows 17 and 18. |
| `victory-formats`, `victory-loader` | 5 | Texture layouts not needed for filled textures; code scanning. None triggers in the log. |

Other markers (non-test code): "left out" 120, "dropped" 199, "skipped" 98, "stub" 102, "placeholder" 128,
"stand-in" 63, "not yet" 31, "not implemented" 17, `todo!`/`TODO` 10. I read the ones tied to a log line or to a
picture path; I did not classify all of these one by one, most are counters and comments about past runs.

## Switches whose unset default leaves a correctness feature off or a cheaper path on

| Switch | Default | Turning it on | Why off | Risk |
|---|---|---|---|---|
| `VICTORY_BVH_BUILDER` (+ `RT_RESOLVE`, `ACTIVATE_RAYS`) | off | The title's ray tracing runs | "until a run has shown the whole builder ... working on a card" (`victory-gpu\src\pipeline.rs:257`) | Known, parked |
| `VICTORY_SHADER_LOOP_LIMIT` | 128 / 256 / 4,096 by stage | Longer loops before the cut | Card lost with the higher values | Card loss; cache rebuild |
| `VICTORY_NAN_GUARD_STORES` | 0 | Guard on typed stores | Damaged integer patterns | Wrong packed values |
| `VICTORY_F16_NEAREST` | off | Exact half rounding | Shader cost | Slower shaders |
| `VICTORY_KEEP_SHOWN` | 1 | 0 = no old texture shown; 2 = any of the same dimension | 1 chosen as the middle | 0 shows gray longer |
| `VICTORY_EARLY_TEXTURES_MS` / `VICTORY_BAKE_TEXTURES_MS` | 12 / 1,500 | More time for textures before they are asked for (the drop uses 40 / 10,000) | Frame time | Longer frames |
| `VICTORY_TEXTURE_BACK` | on for four water formats | — (already on) | — | — |
| `VICTORY_INDIRECT_REST` | off, and off is the safer side | — | — | — |
| `VICTORY_PIPELINE_METHODS` | off | Meter only | — | none |

Not found as a switch: anything for queries, HDR, anisotropy, the heap cap or the depth twins.

## The four known defects: candidates and how to confirm each with one run

| Defect | Candidates (in the order I would test) | One-run confirmation |
|---|---|---|
| Bushes and some trees glow purple | (a) rays off: sky light with nothing blocking it and stale probe blocks; (b) negative values kept by the R9G9B9E5 stand-in; (c) gray or old stand-in textures in material slots | (a) the same spot with `VICTORY_BVH_BUILDER=1`: gone means it belongs to the parked ray tracing. (c) `VICTORY_KEEP_SHOWN=0`: if the glow turns gray or black, or flickers with the "neutral texture meanwhile" count, it is stand-ins. (b) needs the `VICTORY_E5_TARGETS` port. |
| Purple patches that stick for seconds | (a) stale probe blocks, as above; (b) NaN or infinity from compute stores in a history; (c) stand-in textures while 8,000 slots wait for memory | (b) `VICTORY_NAN_GUARD_STORES=1` (cold shader cache): patches gone means stores. (c) watch whether a patch disappears at the moment "slots shown after their texture's upload" grows; `VICTORY_KEEP_SHOWN=0` changes its look. |
| Open water turns white after a scene change | (a) NaN or infinity in the water passes' textures (they are compute-written, and `texback.rs` copies four of them into title memory, so a bad value also reaches the CPU side); (b) "a texture the host copied into keeps the host's contents even if the title later writes its memory" (`victory-gpu\src\texture.rs:784`): a reset by the title after a scene change would not reach the card; (c) loop cut in the water shaders | (a) `VICTORY_NAN_GUARD_STORES=1`. (c) `VICTORY_SHADER_LOOP_LIMIT=1024`. (b) has no switch: NEEDS CODE (count title writes to host-written textures). |
| Far trees and buildings flicker | (a) pixel discard at the 128-iteration cut; (b) stale depth from the twins (2,710 uses before the pass); (c) meshlet culling's rounded pyramid read; (d) textures re-uploaded after an untold change (55) | (a) `VICTORY_SHADER_LOOP_LIMIT=1024`: flicker gone means the cut. (b) and (c) have no switch. |

## Proposed test set for one 3-minute run

Only switches that do not change the shader translation, so the run stays warm and comparable:

```
add:     VICTORY_PIPELINE_METHODS=1
add:     VICTORY_KEEP_SHOWN=0
remove:  VICTORY_PIPELINE_WAIT_MS=20      (back to the default 500)
```

Read afterwards: the line "pipeline methods 0x1c8/0x218 ..." (does the title fall back to another creation method),
the look of the purple spots (gray instead of purple points at stand-in textures), "ms waiting for pipelines".
`VICTORY_KEEP_SHOWN=0` is a diagnosis for this run only; put it back to the default afterwards.

Then, each alone because each rebuilds the shader cache:
1. `VICTORY_SHADER_LOOP_LIMIT=1024` (flicker, water, holes). Watch for a lost device.
2. `VICTORY_NAN_GUARD_STORES=1` (sticky patches, white water). Check packed-colour surfaces.
3. One RT-on run at the purple spots, only to assign the purple.

## Not verified

- No run was made. No picture was looked at. Every defect cause above is a candidate.
- The shader crate's 58 shortcuts were read from their two-line texts, not from the code around each.
- Whether the single dropped indirect signature is a ray-tracing dispatch.
- Whether any title slot past 737,856 is ever written.
