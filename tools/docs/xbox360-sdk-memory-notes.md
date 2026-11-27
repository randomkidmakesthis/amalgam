# What the Xbox 360 SDK (ReXGlue / Xenia design) does about unified memory

Source read: `Tools\ref-rexglue-sdk` (BSD-3-Clause; compatible with our GPL-3.0-or-later code if notices are kept).
The 360 has 512 MB unified; our game has 13.5 GB. Their approach looks cheap partly because of that scale.

## How it works there

- Guest memory is ONE 512 MB tiled GPU buffer; 4 MB heaps are bound only when a draw touches that range. No RAM
  duplicate: uploads go through a temporary ring reclaimed when the submission completes.
- CPU writes are detected only by page-protection faults (no polling). After an upload pages go read-only; the first
  write faults, clears bits in flat bitmaps, and unprotects up to a 4 MB block (fewer faults, less precision).
- Uploads are copies RECORDED IN THE GPU QUEUE, so work already in flight still sees old data and no CPU wait is needed.
- By default every CPU-supplied page is re-uploaded each frame it is used; faults alone are not trusted for buffers.
- GPU results back to the CPU: default is not to copy at all. `full` waits for the whole queue (serialises CPU and GPU).
  `fast` keeps two buffers per target and hands the CPU the PREVIOUS result while the new one is in flight (one result
  stale; not fence-checked). The shader-write path uses the same double buffer but checks the fence and falls back to a wait.
- Textures: keyed by address, invalidated by one-shot write watches, LRU with soft/hard byte limits and a 30 s lifetime.
- Pipelines: descriptions stored on disk, pre-created on many threads at load, priority queue, draw skipped while missing.

## What transfers to victory (ranked)

| # | Idea | Effect | Size | Risk |
|---|---|---|---|---|
| 1 | Uploads through a ring with queue-ordered copies; remove the "settle" wait | large: no forced waits | M | medium |
| 2 | Bitmap guard state, wide unprotect, short lock (replace BTreeSet + global mutex in `watch.rs`) | cheaper tracking | S–M | low |
| 3 | Fence-checked double-buffered copy-back for buffers the CPU reads; full wait as fallback | ~126 MB/frame off the critical path | M | medium (one frame stale) |
| 4 | Hybrid tracking: faults for rarely written ranges, write-watch for hot ones, once per frame | far fewer askings | M | medium (fault storms) |
| 5 | Pipeline description store + pre-creation + priority queue | cold first run | M | low–medium |
| 6 | Tiled buffer at a fixed guest-address offset with LRU tile eviction, replacing RAM duplicates | large RAM saving | L | high |

## What does not transfer

- Mirroring all of memory (13.5 GB will not fit a 16 GB card alongside everything else).
- Their fault tracking is not better in principle: victory already guards view memory the same way. Their advantage is
  data structures (bitmaps, 4 MB buckets) and generous unprotects.
- Their texture cache has no answer to a game that cycles its streaming pool; a content hash on reload would be ours.
