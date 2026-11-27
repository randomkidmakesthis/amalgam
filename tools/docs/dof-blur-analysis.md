# Cutscene "blur circle in the middle": analysis (2026-11-26, static, no code changed)

## Verdict in one paragraph
The disc is most likely NOT the depth-of-field / autofocus chain. It matches a second, cutscene-only lens effect: a
full-screen Gaussian-pyramid blur with a radial vignette mask (`PS_GaussianAberrativePrePassWithVignette`,
`PS_GaussianPyramidAnalysis`, `PS_GaussianPyramidSynthesisWithVignette`), first bound 49 frames after the cutscene
starts. On the console it blurs and colour-fringes the screen EDGES and leaves the centre sharp; here the result is
inverted (centre blurred, edges sharp). The family is identified with good confidence (~75 %); the mechanism of the
inversion is not decided statically. One diag run settles both (last section).

## What the shots show
- run-85-best-safe `shot_0275` (man on sofa): the wall is blurred behind the man at screen centre and sharp at the right
  edge (same depth); his right hand, nearer the camera and off-centre, is sharp; his body at the centre is heavily
  blurred. So the blur does not follow depth: it is a centred, soft-edged radial zone, radius ~450-500 px of 2560
  (~1/3 of picture height). Smooth Gaussian look, no bokeh shapes.
- run-89-visual `shot_0184`, `shot_0214` (car close-ups): face at centre blurred, ear / left side of the cap (further
  from centre) sharp, soft transition inside one mesh. Lights at the far left/right edges show red/cyan fringes
  (the edge chromatic aberration of the same effect is working and is where it should be).
- run-89 `shot_0154` (wide street shot): no disc visible. Whether the edge of the disc is steady while only the
  inside changes cannot be told from one PNG per 30 s.
- `VICTORY_TARGET_VARIANTS` did not fire in run-89: "0 pipelines made again for the render target formats" all run.

## The DoF / autofocus chain (established)
Hashes (container hash as in run logs); all compute, all bound from frame 1668 of run-89:
- `Autofocus:CS_AutofocusSceneDepthScanInitialReduction` 4996d173208d83eb: 8x8 groups, gathers a 2-channel min/max
  depth texture in a window (origin + clamp from constants), wave-sums (ds_swizzle + v_readlane 32: runs through the
  32-lane exchange path on NVIDIA), stores (mean min, mean max, count).
- `...FinalReduction` c3ed1564a33b2649: 16x16 groups, LDS tree sum with barriers, one texel per group.
- `Autofocus:CS_ComputeLensParameters` 2bfdc19dc1f25b3a: reads texel (0,0), target = (min+max)/2 clamped and blended
  with two scripted distances; a spring with STATE kept on the card (16-byte buffer: distance, velocity; created with
  90.0 m; read and written in place every frame; NaN/Inf in it resets); writes 40 bytes of lens parameters to a
  48-byte buffer the DoF shaders read (SRV, stride 48) and to a read-back ring.
- Ghidra: all three buffers are made in 0x145745f20 (state: 1x0x10 B, init 90.0; lens: 1x0x30 B; ring: 3x0x30 B).
  The ring is site 0x1457461f2 (ring=1). Sites 0x145783c6c/0x145783d07 are `vfx_lightningStrike`, 0x1415ee338
  `weatherparticle`, 0x1457011a8 `puddledetect`, 0x145a8c7b0 `cascadeshadows_classify`, 0x14036b933 deferred lighting.
- DoF: `ResetIndirectArgsBuffer` e080e6a7cfb63a5b, `DOF2TileClassificationDownsample8x8` 291822a279c07067,
  `TileClassificationDownsample3x3` 6aa0d2e1c65940f3, `...Neighborhood3x3` 2b17036bf3a1d615, `...NeighborhoodBoundary`
  f8a6133bfedc5759 (atomic tile lists + indirect counts), `PrefilterBoundaryColor` 44fdeb707f882b7d and `MainBlur`
  03c01507703d7449 (indirect dispatches over the tile lists). `CS_Composite`, `PrefilterColor`, the Skip* and
  SimpleMainBlur are never even created by the title. Circle of confusion in the tile pass = function of depth and the
  lens buffer only (physical formula, clamp +-23 px); the two other textures it reads feed a brightness maximum, not a
  radius. Nothing radial anywhere in it.

## The vignette blur (new finding)
- Pixel shaders, first bound at frame 1717 of run-89: prepass 43b4c00d725ac9c7, analysis dfa53500ff122e4e, synthesis
  5510a9fd27ceb4b1 (plain `GaussianSeparableHorizontal/Vertical` a5a79f925460d149 / b084b0f94910ed53 from 1668).
- Prepass and synthesis compute the same mask: t = saturate((r - inner) / range), r = rotated elliptical distance
  from a centre in the constants; amount = saturate(LUT1D(t).a * strength), LUT = a 1D texture, alpha channel.
  Prepass: amount < 1e-4 -> plain copy, else chromatic aberration along the radius. Synthesis: 3x3 tent upsample of
  the coarser level, output alpha = saturate(level(amount) - this level's index) (log2 scale), blended over the finer
  level. Intended: alpha 0 at the centre (keep sharp), 1 at the edges (take the blurred level).
- Observed = a*sharp + (1-a)*blurred. The mask itself is right (fringes sit at the edges), so the fault is after it.

## Hypotheses, ranked
- H2' (new, ~75 %): vignette-blur pyramid shown inverted. Candidate mechanisms, undecided: (a) source and
  destination levels swapped or aliased (levels are mips or pooled targets sharing title memory; views of a single
  level; the layer's aliased-view / levels-on-demand / copies-let-go-for-room paths) - this one also fits "sometimes
  in focus, then not" (a level copy that is fresh in some frames and stale or re-made in others); (b) blend factors
  of the synthesis draws (read verbatim from the title's state, so less likely); (c) a later pass taking the wrong
  level. Shader maths checked: v_sin/v_cos in turns, exp/log base 2, packed f16 outputs are translated as such.
- H1 (autofocus read-back empty/zero): against. Ring 1 lands 15-59 times per 5 s at 8-12 fps (about every frame),
  ~32 non-zero bytes each, 0.3-1.0 frames after the write. The DoF shaders read the card buffer, not the ring.
  A wrong focus distance would shift the focal plane by depth; it cannot blur a wall at the centre and not at the edge.
- H1b (state buffer on the card overwritten by stale title memory, resetting the spring towards 90 m): not excluded;
  would explain "focus on the background" and intermittent focus, but not the disc. Probe list below measures it.
- H3 loop cap: against. Compute cap is 4,096; MainBlur's loops are bounded by a 23 px radius; a cap would be steady.
  16-bit float views: not established for this chain (texture formats unknown statically).
- H4 depth input wrong: against as the cause of the disc (shot_0275: same-depth wall differs centre vs edge).
- H5 occlusion / timing queries: nothing in either chain uses them.
- Indirect arguments: clamped on the card from the card copy in the same list; tile counts are within host limits.

## The one diag run (diag build, same input script as run-89)
```
VICTORY_SKIP_PIPELINES=5510a9fd27ceb4b1
VICTORY_CAPTURE=C:\GTAVI\Tools\victory-data\bench\<run>\frames
VICTORY_CAPTURE_FRAMES=2000,2300,2600
VICTORY_LOG=victory_gpu::draw=debug,info
```
Read: the shots / captured frames of the close-ups, and the debug draw lines of the listed frames for pipelines
43b4c00d725ac9c7 and dfa53500ff122e4e (targets "WxH f<format>", blend enable/src/dst).
- Disc gone, picture sharp with edge fringes: H2' confirmed and the fault is in the synthesis draws. Next: same run
  without the skip, read the synthesis draws' blend factors and which level each samples and renders to; fix in
  `victory-gpu` where those views/targets are made (`descriptor/host.rs` texture views of single levels,
  `texture.rs` level copies) or the blend state (`pipeline/host.rs` `host_blend`). Until fixed, the skip is a
  usable workaround in diag builds (loses the edge blur only).
- Disc still there but changed (stale or blocky centre): blur is already in the levels the analysis writes: skip
  dfa53500ff122e4e next; look at level aliasing.
- Disc unchanged: H2' is wrong; then run the probe on the DoF chain: `VICTORY_PROBE=<file>` with the lines
  `2bfdc19dc1f25b3a lens`, `291822a279c07067 tiles`, `f8a6133bfedc5759 boundary`, `VICTORY_PROBE_FRAMES=1:600`,
  `VICTORY_PROBE_AFTER_S=150`. State buffer: `card_before` of frame N must equal `card_after` of frame N-1 (else
  H1b: the layer re-uploads title memory over card-written state); lens buffer max = focus distance, must be
  finite and follow the scripted distances in the constant buffer (`game_submit`).

## Method notes
Shader names -> hashes: records carved from `shaders.rpf` at the offsets in `victory-data\reports\shader-info.txt`,
inflated, hashed with `victory translate` + `VICTORY_DUMP_MATCH`; read with `victory shader-info --disasm`. No probe
output from earlier runs exists. Not verified: that the vignette passes run in every close-up frame (only first bind
is logged), the console's intended look (inferred from the shader), texture formats and level layout of the pyramid.
