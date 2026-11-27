# How the game renders a frame and uses memory — facts, not guesses

Target: `game_scarlett_master.exe` 1.0.2.1 (Xbox Series X build) on Ryzen 7 9800X3D, 32 GB RAM, RTX 4080 SUPER 16 GB.
Layer: victory, NVIDIA fork (`Tools\victory-nvidia-8gb`). Tags: **[PC]** measured on this PC, **[notes]** measured by the
layer authors on other machines, **[code]** read from layer code, **[inf]** inferred.

## 1. Frame anatomy

- Pacing: each frame starts with a small submission and a fence signal; a game thread waits on that fence by polling
  memory. Frame time = the layer's burst of submissions + ~20 ms of the game's own work. [notes]
- ~30 command-list submissions per frame on this PC (27–30 in the frame log). [PC]
- Two GPU queues: graphics and compute; the compute queue waits for depth from graphics. [code]
- 5,000–6,500 draws per frame in the first rooms (warm), ~330 still dropped for lack of a pipeline. [PC]
- ~600 compute dispatches per frame; the game runs ~30 compute passes. [PC][notes]
- Geometry is mesh shaders with GPU meshlet culling against a depth pyramid; shading is deferred (tiled). [code]
- Ray tracing is all compute: static structures ship pre-built (`pack\*_bvh4`), dynamic ones are refitted every frame by
  52 programs from `XboxBvhBuilder.dll`. The layer has the builder off, so ray-traced bounce light/shadows are missing. [code]
- Render targets: swap chain 3840x2160; internal render size 2560x1440; a light mip chain 2560x1440 → 40x23; shadow
  maps 256²–2048². 2.3–3.1 GB of VRAM in render targets. [PC][notes]
- Pipelines: the game registers ~84,000 and a 5-minute session binds ~2,200 (1,010 compute, 638 mesh, 556 vertex). [PC]
- Shader pack: 25,953 programs (compute 2,734; mesh 8,765; pixel 12,067; vertex 2,192). 202 do not translate. [PC][notes]

## 2. Memory model of the game

- Console budget modelled as 13.5 GiB (10 GiB GPU-optimal + 3.5 GiB standard). The game never asked how much memory
  it has during a run: it sizes itself from the console TYPE, not from a memory figure. [PC][code]
- It allocates a 6.1 GiB pool of physical pages and maps 64 KiB pages at several addresses (one page, many views). [code]
- The GPU reads straight from game memory: vertex/index/meshlet/constant buffers, descriptor heaps written as raw
  bytes (~28,000 records mirrored per frame), and textures placed in game memory. [PC][code]
- Textures: the game places a texture's whole mip chain in its address space but only maps the levels it holds; the next
  texture is packed where the absent large levels would be. 2.2–3.0 GB of 4.3–5.5 GB of the layer's texture copies were
  levels the game does not hold. [notes][code]
- The GPU writes little that the CPU reads back: exposure metering and a few visibility read-backs. ~5 MB/frame. [PC]

## 3. Streaming

- DirectStorage, 17 queues, zlib then BCPack; data lands in the physical-page pool. 20.4 GB written in ~4 minutes. [PC]
- Texture database: 47 files, ~62 GB (`data\ttdb`). `levels.rpf` 76 GB.
- No evidence the game adapts its streaming to a reported memory or VRAM budget. [inf]
- In one 5-minute run: 222,279 texture uploads (91.5 GB cumulative), 102,204 texture copies released (14.8 GB). The
  game cycles textures hard; the layer re-creates copies each time. [PC]

## 4. Measured cost of unified memory on this PC (warm run, first rooms, ~10 fps)

| Part of a ~100 ms frame | Measured |
|---|---|
| Game memory copied RAM → VRAM | 75–140 MB, 20–35 ms |
| All submit preparation (one layer thread) | 40–60 ms |
| GPU results copied back | ~5 MB, 5–10 ms |
| Card idle, waiting for that thread | 70–100 ms |
| GPU busy | ~72 % |
| VRAM / committed RAM | 9.8 GB / 23 GB |

In that lighter view the limit is one layer thread, with the card idle 70–100 ms a frame.

### 4b. Heavier view, profiler + census + GPU meter (run nv8-3-census, 7,530 draws, 114 ms/frame) [PC]

| | Per frame |
|---|---|
| GPU in the game's own lists (229 lists) | 87.6 ms |
| GPU in the layer's send-and-hash / compare lists | 5.2 + 3.3 ms |
| GPU idle | 15 ms |
| Submit thread at work | 110.5 ms (42 waiting for the card, 43 preparing) |
| Recording, all 6 game threads together | 276 ms in 56,234 calls |
| Card copies: 99 buffers, 991 MB | 28 MB sent up (22.5 MB from ONE 182 MB buffer at 0xb10000, ~295 ranges), 5.7 MB fetched |
| Small per-submit copies | ~82 MB, 22 ms |
| Submit thread by function | relay_wait 39, shadow::wide 21, Batch::flush 21, watch::writes 18 (11.5 lock wait), sync_heaps 15, copy_mapped 11.5, ReadProcessMemory 6.8 |
| RAM free | ~4.9 GB |

Here the GPU is the limit and it is the game's own rendering at 2560x1440 through translated shaders. Which side binds
depends on the scene; both are near their ceiling at ~9–10 fps.

Priorities that follow: (1) lower the internal render size via the globals; (2) trim the submit thread's serial work
(write-watch lock, descriptor sync, the 182 MB buffer); (3) mip levels on demand for VRAM and texture churn.

## 5. Knobs inside the game

| Knob | Status |
|---|---|
| Console type (Series X/S) | Series S: black screen (victory) / crash at 36 s (xcl, this PC) |
| Reported memory | never queried |
| settings.xml resolution / FSR quality | ignored by the console build |
| Display mode | ignored: still 3840x2160 swap chain, 2560x1440 inside |
| Render size | globals at 0x1499fecf8 / 0x1499fecfc, read by 0x141edb000 — untested lead |
| Dynamic resolution | none reacts to GPU timing |
| Setting names in the exe | none readable (hashed) — searched 214,796 strings |

## 6. Code locations worth opening in Ghidra (image base 0x140000000)

- 0x141edb000 + globals 0x1499fecf8/0x1499fecfc: render size
- 0x140f2c470: page-mapping batches (texture streaming core)
- 0x142477578, 0x1424800d5: callers of the two unimplemented pipeline-creation methods (0x1c8 / 0x218)
- caller of XMemAllocatePhysicalPages (the 6.1 GiB pool); reader of the `ConsoleMode` registry value
- 0x1424821dd: per-frame token submit; the fence-wait loop

## 7. Open questions being measured

- Which buffers make up the ~100 MB/frame, and how much of each actually changed? (`VICTORY_CENSUS=1`)
- Where the submit thread's CPU time goes by function. (`VICTORY_PROFILE=1`)
- VRAM saved, and picture cost, of mapping only the mip levels the game holds. (`VICTORY_LEVELS_ON_DEMAND=1`)
