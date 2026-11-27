# Ray tracing on victory-fox: where it stands (coder T, 2026-11-26, PARKED by user decision)

**Node tests per ray (run-177, `VICTORY_BVH_WALK=1`).** CS_Primary_GatherVBuffer_WithFar averages 1,213 node tests per
thread (1,032 box, 181 triangle). 48.7 % of its threads make 1,024 or more, and the most is 8,510.
RaytraceVBuffer averages 357, GatherAlphaVBuffer 587, IrradianceCache_Gather 474, CollisionRaytraced 176.

A well-built 4-wide BVH over a city-sized scene is about 12-14 levels deep. Typical traversal there is tens to about
a hundred box tests per ray and a few dozen triangle tests. Our primary-ray numbers are 10-30 times that, and half the
threads go past 1,000.

The meter counts per thread, not per ray. If the program traces several rays per thread, the figures are spread over
those rays. Even so, numbers this high point to boxes that are too big or overlap: structures not refitted after
skinning or movement, stale or garbage bounds, or instance data reaching the card late. They do not look like the
cost of a correct structure.

This fits run-172. There, COUNT32 changed the builder's output and traversal went from 29 to 230 ms per dispatch,
with black, speckled faces.

## What is in foxT (all off by default)
- `VICTORY_PASS_TIMES=1`: GPU timestamps per title command, logged by program and by family. Works with RT off too.
- `VICTORY_BVH_WALK=1`: node tests per thread for every program with `image_bvh_intersect_ray`.
- `VICTORY_BVH_COUNT32=1`: port of upstream 06fe22a. It made the two heavy builder programs (caa78b8ca62bc3d6,
  28918b7150a7aad5) about 20 times cheaper, but it breaks the picture.
  Likely cause, not tested: these programs reduce over the whole wave (`s_or_saveexec -1`, then
  `v_cndmask v, 0, x, saved_exec`, then a DPP reduction or `v_readlane`). Our translation treats the saved entry
  EXEC as TRUE for every lane, including the threads past the count. Those threads then put their values into the
  reductions and ballots, and the active threads get wrong totals and offsets.
  Fix to try: AND `partial_active` into every mask read as data (`v_cndmask`, ballots, reductions), not only into
  masks in `st.active`. Then test a WWM partial-group program on the card.
- `VICTORY_BVH_PAGE_REUSE=1`: exact, but run-174 showed no gain. The directory load is not the cost.
- The b0fc38d36f25b863 spikes (0.3-1.07 s): a 38-instruction structure-copy loop with a grid stride of
  threads x 16 bytes. Every element goes through a root-view directory lookup and a globally coherent load and store.
  The iteration count is size / (threads x 16).

## When RT comes back
1. Validate the structures first, with `VICTORY_BVH_WALK` and the screenshots. Check whether the title's
   refit/rebuild requests and the CPU-written instance buffers reach the card on time; coder W found the 64 MB
   structure buffer copied whole every frame.
2. Make COUNT32 exact (the fix above). Then check that tests per ray fall toward tens.
3. Only after that, tune traversal cost.
