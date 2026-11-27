# Rendering, shader and simulation audit (victory NVIDIA fork, RTX 4080 SUPER, 2560x1440 internal)

Evidence: run logs and window pictures under `Tools\victory-data\bench`, the layer's translate report, and its code.

## Read first

1. **The measured runs used an experimental switch.** `VICTORY_SKIP_WC_COMPARE=1` (from the fork's own launch script)
   also turns on `SKIP_WC_BACK`: shader writes into write-combining game memory are NOT copied back. The code calls it
   "an experiment, not a fix" and says to expect artifacts. ~97 of 100 MB/frame of copy-back lives in that memory.
   `VICTORY_TEXTURE_DROP_MIPS=1` was also on (drops the top mip of every 2D texture; an 8 GB-card workaround).
2. The "~303 draws dropped without a pipeline" in warm gameplay are all ray-tracing structure builder stages.
3. The two unimplemented pipeline-creation methods are called at least 16,384 + 2,048 times at load, not ~29.
4. Shader hashes in the log can be mapped to names with `VICTORY_DUMP_MATCH=<dir>;#` (writes `name.hash.cdxil`).

## Shaders

| Stage | Translate |
|---|---|
| Vertex + pixel | 14,247 / 14,247 |
| Mesh | 8,765 / 8,765 |
| Compute | 2,675 / 2,682 |

202 failures: 195 `GS_VoxelizeGeneric`, 5 `CS_TraceShadowRays_*`, 2 `CS_TransmissionVolumetrics*`. None was ever bound in a run.
The run bound 2,545 unique programs (1,042 compute, 707 mesh, 1,001 pixel, 474 vertex first-binds).

## Frame order and status

| # | Pass | Status |
|---|---|---|
| 1 | GPU simulation and skinning: ped skinning, head blend shapes ("sticky lips"), hair strands, wind, particles | translates and runs; correctness unverified |
| 2 | Ray-tracing structure refit/build (52 programs) | DROPPED: 274–371 dispatches/frame in gameplay, 450–2,385 in cinematic scenes |
| 3 | Instance and meshlet culling, depth pyramid | renders |
| 4 | Depth / G-buffer mesh passes | renders |
| 5 | Decals, hair visibility and resolve | renders |
| 6 | Shadow maps (cascades, local lights) | renders |
| 7 | Ray-traced shadows: trace, denoise, resolve | wrong: walks unbuilt structures, loop cut at 1,024 |
| 8 | Tiled deferred lighting | renders |
| 9 | Ray-traced global illumination: irradiance cache, probes | effectively missing: shadowed areas black |
| 10 | Reflections | depends on 2; missing or garbage |
| 11 | Skin subsurface scattering | looks plausible |
| 12 | Water | not exercised in the first scenes |
| 13 | Volumetrics / fog | translates; unverified |
| 14 | Particles, transparents | unknown |
| 15 | TAA and post (DoF, bloom, exposure via CPU read-back) | visible; exposure exposed to item D |
| 16 | FSR 1440p→2160p | output is 3840x2160; unverified |
| 17 | UI, Bink video | renders |

## Not rendered or wrong, by visible impact

| # | What | Symptom | Cause | Fix | Adds cost |
|---|---|---|---|---|---|
| A | RT structure builder off | everything not in direct light is black; reflections wrong | `VICTORY_BVH_BUILDER` off; recorded crashes ~80 s into play and card hangs when on | large: find the bad pointer, make unmapped game pages readable by address | yes, large |
| D | Copy-back experiment on | blocky stale patches; CPU-read GPU results stale | `VICTORY_SKIP_WC_COMPARE=1` in our launch settings | remove the switch | yes: ~97 MB/frame of copy-back |
| E | Top mip dropped | softer textures | `VICTORY_TEXTURE_DROP_MIPS=1` | unset (VRAM has room) | VRAM and upload |
| C | Device methods 0x1c8 / 0x218 unimplemented | unknown: game silently goes without those pipelines | return E_NOTIMPL; ≥18,000 calls at load | Ghidra at 0x142477578 / 0x1424800d5, then translate raw shader code with synthesised metadata | unknown |
| G | Raw buffer views with odd offsets left null | reads of 0 in some buffers (218 cases) | view start not a multiple of 4 | route through the existing own-bytes path | tiny |
| F | Texture queue | late or blank surfaces after camera cuts; 227 re-uploads | fingerprint polling | hook page-mapping and storage writes | CPU only |
| I | Shader loop caps and sampling shortcuts | truncated marches, wrong mips | caps of 128 / 256 / 4,096 iterations | raise after A and D | GPU |
| J | Compute dispatches over 65,535 groups cut | lost compute work (0.2–0.8/frame seen earlier) | group cap | split dispatches; restore the counter | small |
| B | 5 shadow-ray programs untranslated | none yet (never bound) | descriptor read as data | extend the view-address path | if used |
| M | Eyes glow white/blue | seen in pictures | probably fed by reflections (A) | re-check after A | – |

Facial animation: the head blend-shape programs translate. Whether their results survive is tied to item D (GPU writes
not copied back) and to how GPU-written buffers reach later passes through other views — to be tested first by removing
the experimental switch.

## Simulation and physics

- GPU: hair strands, skinning and blend shapes, wind, particles and weather, shallow water / ocean / river, trails. All
  translate. No named cloth simulation exists in the pack. Results the CPU reads back (wind, grass, occlusion, exposure)
  are exposed to item D. Ray-traced particle collision needs A.
- CPU: the game's native code. Threads, timers and CPU features are the host's (8c/16t, AVX2); console-only instructions
  are emulated where they fault. Game job threads are mostly idle: CPU physics is not the limit.
- Unknown: whether the game steps time by wall clock or per frame (decides cutscene audio/video sync at low fps).

## Other forks

- fxceless: no rendering fixes for the above; performance work only. This fork is ahead on shader translation.
- rorodev6's missing heads/hair/beards do not apply here: those pixel programs translate and render.

## Order of work

1. Baseline without the two confounds (no SKIP_WC_COMPARE, no DROP_MIPS), clean save, warm cache.
2. Build the shader hash→name map.
3. Small fixes: G, J.
4. Ghidra on C.
5. A (ray-tracing structures) — prerequisite for RT shadows, GI, reflections, eyes.
6. B and loop caps.
7. Cutscene timing and audio sync check.
