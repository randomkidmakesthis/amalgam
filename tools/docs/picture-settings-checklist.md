# Picture settings checklist: ours against Willy's cool_fox build 30e54b4 (analyst Z, 2026-11-26)

Read-only. Nothing built or run, no code edited, no picture looked at. Every "expected look" below is from code
and code comments, not from a run.

- Willy: `C:\GTAVI\Tools\ref-forks\cool_fox-willy\src` (paths marked `W:`), launcher `outer\Play-cool_fox.cmd`.
- Ours: `C:\GTAVI\Tools\par\foxW` head 474f973 (paths marked `O:`, relative to `crates`), launcher
  `C:\GTAVI\Tools\publish-fox\scripts\play-scarlett.ps1`.
- Speed switches and meters are left out unless they change what is drawn.

## Found while checking (read this first)

1. **`VICTORY_TABLE_MEMO` and `VICTORY_LIST_ALIAS_EXACT` are not read in `par\foxW` at 474f973.** Our launcher sets
   both. They exist in `publish-fox` (`crates\victory-gpu\src\draw.rs:891`) and in `par\foxT` (branch t-on-w,
   bcd4c90). So a build from foxW ignores them, and the alias fix they describe is only in the published tree.
   If foxW is what runs, the launcher's comment about the alias fix does not apply to it.
2. **Our launcher sets `VICTORY_PIPELINE_WAIT_MS=20`; Willy leaves the default 500.** With 20 an object can be
   missing for some frames while its pipeline is made. This is the one setting where our launcher gives less
   picture than his by choice.
3. **Willy has three picture changes we do not have**: R9G9B9E5 targets as R11G11B10, longer bake and early-texture
   budgets, and the table sizing fix from the review of #927. The first and third need a port; the second is two
   environment values.
4. With ray tracing off, neither tree shows the console's lighting. Willy covers the gap with an estimate
   (`VICTORY_AMBIENT`); we show what the title's passes leave. Section 2 says what each looks like.

## 1. Every picture switch or default, both trees

"Fuller" means closer to what the console draws. RT = ray tracing.

| Setting | What it does | Willy default | Willy launcher | Our default | Our launcher | Our effective state |
|---|---|---|---|---|---|---|
| `VICTORY_BVH_BUILDER` (+`RT_RESOLVE`, `ACTIVATE_RAYS`) | The title's ray-traced light. Without it ~1,000 resolve programs and the ray activation are left out (`O: victory-gpu\src\pipeline\make.rs:81-128`). | off | unset | off | unset (`-RayTracing` sets 1) | **Same** (both less than the console; parked by decision) |
| `VICTORY_AMBIENT` | Replaces the empty RT diffuse light with an estimate from the last frame (`ambient.rs:1-33`). Only acts while RT is off (`ambient.rs` `on()`). | unset = title's output | 0.8 | same | unset | **Different look**, see section 2 |
| `VICTORY_AMBIENT_AIR` | Bounds what the air scatters to a multiple of the estimated light (`ambient.rs:31-33`). | off | 1.5 | off | unset | Different look, see section 2 |
| `VICTORY_AMBIENT_LEFTOVER` | Inside the estimate: drop / keep / hue-only the light the title's resolve left (`W: victory-gpu\src\ambient.rs:113-124`). | drop | unset | not in our tree | — | Not applicable without `VICTORY_AMBIENT`; **missing if we ever use it** (ours would behave as his old "keep", the magenta case) |
| `VICTORY_E5_TARGETS` | Stand-in format for R9G9B9E5 render targets: R11G11B10_FLOAT (no alpha, negatives stored as 0, like the console format) or R16G16B16A16_FLOAT (`W: victory-gpu\src\resource.rs:270`, `:310-330`). | r11g11b10 | unset | not in our tree; fixed R16G16B16A16 (`O: victory-gpu\src\resource.rs:269`) | — | **Less correct in one way** (negatives and alpha kept), more precise in another (more mantissa bits). His note: with this and the NaN guard "walls and skin were no longer blotched". **No equivalent of ours.** |
| `VICTORY_NAN_GUARD` (colour exports) | NaN -> 0, infinity -> ±65504 on pixel exports to float targets (`W: lower.rs:4008`, `:12502`; `O: victory-shader\src\emit\lower.rs:7546`, `:12659`). | on | unset | on | unset | **Same** |
| NaN guard on typed stores | The same on compute image and buffer stores, quiet NaN and infinities only (`W: lower.rs:11075`, `:12026-12092`). | on (part of `NAN_GUARD`) | unset | off: `VICTORY_NAN_GUARD_STORES=0` (`O: lower.rs:52-56`) | unset | **Less guarded than his.** Ours is off on purpose: a packed colour 0xFF800000 was turned into -65504. His buffer-store guard skips integer views; his image-store guard does not, so it has the same risk. |
| #927 half views | 16-bit buffer channels moved as integers, float views shown as unsigned twins (`halves.rs`, identical in both trees). | always on | — | on (`VICTORY_HALF_VIEWS`, `O: lower.rs:43`) | unset | **Same** |
| #927 table sizing | The fresh table is sized to hold half places past the range starts (`W: victory-gpu\src\draw.rs:3746-3754`). | always on | — | **missing** (`O: victory-gpu\src\draw.rs`, `tables()`: `selected` is built from indexed entries only) | — | **Less correct.** A face-rig view past the range start would get no twin. Our faces work in the scenes seen, so it may not trigger; it is still a hole. |
| `VICTORY_BAKE_TEXTURES_MS` | Time the layer may take to upload a character bake's source textures before the bake draw. The title bakes once; what is missing is baked as nothing for the run (`draw.rs:163-175`). | 10000 (`W: draw.rs:186`) | unset | 1500 (`O: draw.rs:182`) | unset | **Less.** His note: 1,500 "left a face of coloured noise and one of blocks in the first cutscene"; 10,000 "baked both whole". This is the README's "bakes get the time they need". |
| `VICTORY_EARLY_TEXTURES_MS` | Time per frame for uploading textures picked by index before a shader asks. | 40 | unset | 12 | unset | **Less** (textures later; same note as above) |
| `VICTORY_BAKE_EXACT` | A bake finds its sources as title memory has them at that moment (`draw.rs:187-200`). | on | unset | on | unset | Same |
| `VICTORY_PIPELINE_WAIT_MS` | Wait allowed per frame for a pipeline still being made; past it the draw is skipped that frame (`O: victory-gpu\src\pipeline\make.rs:268-279`). | 500 | unset | 500 | **20** | **Less** (objects can be missing briefly) |
| `VICTORY_LEVELS_ON_DEMAND` / `VICTORY_HELD_LEVELS` | How many texture levels reach the card. Views start at the first held level either way. | off / on | unset | off / on | 0 | Same |
| `VICTORY_SHOW_AFTER` | A slot shows a new texture only once its upload has run; before that the old one or a gray 1x1 (`O: victory-gpu\src\descriptor\host.rs:283-292`, `:315`). | off | 1 | off | 1 | Same |
| `VICTORY_KEEP_SHOWN` | What a slot shows while waiting: 0 zeros then gray, 1 the old texture of the same kind, 2 any (`O: descriptor\host.rs:4601-4612`). | 1 | unset | 1 | unset | Same |
| `VICTORY_UPLOAD_AHEAD` | Uploads ahead of use (less pop-in). | off | 1 | off | 1 | Same |
| `VICTORY_TEXTURE_NO_WAIT` | A texture that arrives during a remap "may show a frame later" (`O: victory-gpu\src\texture.rs:398-404`). | off | 1 | off | 1 | Same (both accept the one-frame delay) |
| `VICTORY_TEXTURE_BACK` | Water height maps written by shaders are copied back for the title's CPU (`texback.rs:1-16`). | on, four formats | unset | same | unset | Same |
| `VICTORY_TARGET_VARIANTS` | A draw into targets of other formats than its pipeline declares gets a matching pipeline (`O: pipeline\make.rs:311-322`). | not in his tree | — | on | unset | **Fuller (ours)** |
| `VICTORY_EARLY_DEPTH` | Pixel programs the console tests before shading declare early depth (`O: victory-shader\src\emit\lower.rs:1283`). | not in his tree | — | on | unset | Fuller (ours) |
| `VICTORY_D16_BITS`, `VICTORY_D16_VIEW_KINDS`, `VICTORY_PS_QUAD_READS` | 16-bit loads through views without a kind word; quad reads in pixel programs (`O: lower.rs:59-70`). | not in his tree (his has the view-kind path always) | — | on | unset | Same or fuller (ours) |
| Minimum level of moved views | `ResourceMinLODClamp` kept when a view is moved to its first held level (`O: victory-gpu\src\tiles.rs`, `show_from`). | lowered by the shift (base behaviour) | — | kept | — | **Fuller (ours)**: his view can sample a level the title held back |
| `VICTORY_LIST_ALIAS_EXACT` | An aliased texture's view is made readable again within a list (`publish-fox\crates\victory-gpu\src\draw.rs:885-891`). | not in his tree | — | off; **not in foxW at all** | 1 | Fuller only in the published tree / foxT |
| `VICTORY_SHADER_LOOP_LIMIT` | Loop cut in translated shaders; a pixel past the cut is discarded (`lower.rs`, "a pixel loop past 128 iterations"). | 128 / 256 / 4,096 | unset | same | unset | Same (both approximate) |
| `VICTORY_F16_NEAREST` | Exact half-float rounding. | off | unset | off | unset | Same |
| `VICTORY_FSR_DLL` | PC FidelityFX DLL in place of the title's upscaler. | off | set only if the user adds a DLL | not in our tree | — | Ours keeps the title's own upscaler: **the reference picture** |
| `--console` | Series X or S. The title picks resolution and budgets from it. | X | **series-s** | X | X | **Fuller (ours)** |
| HDR | Not done in either tree (`victory-hle\src\modules\display.rs:61`). | — | — | — | — | Same |

Skin, colour and walls in his README map to exactly these rows: half views and table sizing (faces), bake and
early budgets (faces), `VICTORY_AMBIENT_LEFTOVER` ("no more purple or blue skin"), `VICTORY_E5_TARGETS` plus the
NaN guard ("no more blotchy painted walls and skin"). Nothing else in his diff touches the picture.

## 2. Willy's launcher settings we do not set

Classes: (a) a correction that brings the picture closer to the console's while RT is off, (b) a stand-in that
changes the look away from the console's, (c) neutral for the picture.

| Setting | Class | Why, with the code's own words |
|---|---|---|
| `VICTORY_AMBIENT=0.8` | **(b), covering a real fault** | It is declared a stand-in: "A stand-in for the indirect light the title traces rays for" (`ambient.rs:1`). What is wrong without it: "With the ray-tracing structure builder off ... the title's rays find nothing. Its program that resolves the diffuse light ... then leaves the sky's light outdoors, nothing indoors (walls are black where no lamp or sun reaches) and, in blocks of the picture's tiles, what the probes' memory held" (`ambient.rs:3-7`). What it assumes: "what is not on screen sends out what is ... No occlusion but the creases'" (`ambient.rs:22-24`). So without it: black unlit interiors, unblocked sky light outdoors, stale blocks. With it: plausible light that the console never computed. Neither is the console's picture; only RT on is. |
| `VICTORY_AMBIENT_AIR=1.5` | **(b), covering a real fault** | "The air of a room is lit by the same probes: without rays they hold the sky's light indoors too, a brown veil at sunset. `VICTORY_AMBIENT_AIR` bounds what the air scatters by a multiple of the light around" (`ambient.rs:31-33`). Without it: sky-lit haze indoors. With it: a bound of ours. |
| `VICTORY_THIN=1` | (c) by design | Threading only: "List calls that only write down their arguments ... what they mean is worked out by threads of ours" (`victory-gpu\src\list\thin.rs:1-2`). Our fork notes record picture faults with it in other forks; not a picture setting to turn on. |
| `VICTORY_FRAME_INTERVAL_MS=16.667` | (c) | Frame pacing and three patches of the title's rate (`W: victory-loader\src\frame_rate.rs`). No pass changes. Risk is game speed, not picture. |
| `VICTORY_LOG=warn` | (c) | Logging. |
| `--console series-s` | less picture | The title's own lower settings. Not to copy. |
| `VICTORY_PIPELINE_WAIT_MS` | he does not set it | His effective value is the default 500, ours is 20. See section 1. |
| `VICTORY_TEXTURE_NO_WAIT=1`, `SHOW_AFTER=1`, `UPLOAD_AHEAD=1`, `LEVELS` | same on both sides | Section 1. |

Both ambient switches switch themselves off when RT is on (`ambient.rs`, `on()`: "set, and the title's rays find
nothing"), so setting them does not disturb an RT-on run.

## 3. Off or approximated in both trees (needs code)

From `docs\left-out-audit.md`; one line each.

- Shader loop cut with pixel discard (128 / 256 / 4,096): needs code to raise safely; a switch exists for testing.
- Stand-in textures (old or gray) while a texture is not on the card: needs textures to arrive sooner.
- Pipeline creation methods 0x1c8 / 0x218 not implemented: needs code.
- Host view heap capped at 1,000,000; title slots past 737,856 read null: needs a counter, then code if hit.
- One indirect command signature dropped (changes root arguments): needs code.
- Depth planes read before a pending twin pass (2,710 times in run-197): needs code.
- Mips that stream in are re-read at most every 30 frames: needs code.
- Meshlet culling reads its depth pyramid rounded and clamped: needs code.
- Samples outside pixel shaders read a fixed level; cube samples with gradients have no anisotropic filtering: needs code.
- GPU queries never run (occlusion always "visible"): needs code.
- A texture the host wrote keeps the host's contents if the title later rewrites its memory: needs code.
- HDR output: needs code.
- Custom sampler border colours become transparent black: needs code (not seen triggering).
- Audio DSP outputs zeros, speech silent: needs code (not picture).

## 4. What to change to have every picture setting on

"Re-translate" means the shader cache is rebuilt on the next start; our launcher's own note applies ("faces can be
missing in that first run and are right from the second").

### TURN ON NOW (no code, no known picture downside)

| Change | Where | Re-translate | Note |
|---|---|---|---|
| Remove `VICTORY_PIPELINE_WAIT_MS = '20'` (default 500) | `play-scarlett.ps1` | no | Cost: possible long frames while a cold pipeline library fills. With a warm library our logs show 0.0 ms waiting. |
| Add `VICTORY_BAKE_TEXTURES_MS = '10000'` | `play-scarlett.ps1` | no | His value. Acts only at character-bake draws during loading; can pause up to 10 s there. |
| Make the launcher match the tree that runs: either build from the tree that reads `VICTORY_LIST_ALIAS_EXACT` / `VICTORY_TABLE_MEMO`, or drop the two names | tree choice | no | Otherwise the alias fix is not in the picture we judge. |

### TEST WITH THE USER WATCHING (changes the look)

| Change | Look at | Re-translate |
|---|---|---|
| `VICTORY_EARLY_TEXTURES_MS=40` (his default) | Texture pop-in after a cut and when turning; frame time, since the pass runs on the recording thread. | no |
| `VICTORY_AMBIENT=0.8` + `VICTORY_AMBIENT_AIR=1.5` (RT off only) | Interiors away from lamps (black without it), haze indoors at sunset, purple or glowing blocks on vegetation and translucent surfaces. Note: our tree lacks `VICTORY_AMBIENT_LEFTOVER`, so ours keeps the leftover's hue, which is the case his note calls "glowing magenta". Port that first (below) or the test is of his older behaviour. This is a stand-in; it was excluded by an earlier decision, so it is the user's call. | no |
| `VICTORY_NAN_GUARD_STORES=1` | Sticky coloured patches, white water; then check surfaces with packed colours for damage. | **yes** |
| `VICTORY_SHADER_LOOP_LIMIT=1024` | Flicker of far trees and buildings, holes, water. Watch for a lost device. | **yes** |
| `-RayTracing` once, at the purple spots | Whether the purple and the glow belong to the parked ray tracing. | no |

### PORT (code in Willy's tree that ours lacks)

| Port | Files in his tree | Size | Re-translate | Conflicts |
|---|---|---|---|---|
| `VICTORY_E5_TARGETS` (keep our default R16G16B16A16, test `r11g11b10`) | `crates\victory-gpu\src\resource.rs` (`target_format`, `e5_stand_in`, `e5_stand_in_for`, one test); comment in `list.rs` | ~50 lines | no (pipelines are rebuilt: target formats change) | none in `target_format` |
| #927 table sizing | `crates\victory-gpu\src\draw.rs:3746-3754`; test in `descriptor.rs:1588-1660` | ~8 lines + 70 of test | no | our `tables()` differs (9b2debf); adapt |
| `VICTORY_AMBIENT_LEFTOVER` (only if the ambient test is wanted) | `crates\victory-gpu\src\ambient.rs` | ~37 lines | no | our `ambient.rs` is the base's; clean |
| Integer-view exemption for the store-side NaN guard (only if stores are turned on) | `crates\victory-shader\src\emit\lower.rs:12026-12092` | ~40 lines | **yes** | our `lower.rs` differs widely; hand merge |

## Not verified

- No run and no screenshot was used. "Fuller" and "less" are read from code and comments.
- Whether the #927 sizing gap triggers on any of our shaders.
- Whether `r11g11b10` shows banding against the 16-bit stand-in.
- Which tree the published exe was built from; the launcher and foxW disagree on two names.
