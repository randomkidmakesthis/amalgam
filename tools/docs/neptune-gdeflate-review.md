# neptune-gdeflate review (dudegio/Neptune-GDeflate), 2026-11-26

Clone: `C:\GTAVI\Tools\ref-forks\neptune-gdeflate` (push URL disabled). Bundle: `C:\GTAVI\Tools\patches\neptune-gdeflate-full.bundle`.
Read-only review from code; nothing built, run or downloaded. Paths below are relative to the clone unless prefixed `fox:`.

## 0. Safety
- 242 tracked files, all text. No exe/dll/lib/archive, no submodules, no release assets fetched. `bcpack-tables.bin` of Neptune is REMOVED
  (must be supplied via `VICTORY_BCPACK_TABLES`, `crates/victory-formats/build.rs`).
- Cargo.lock: zero new crates vs `fox:Cargo.lock`, `neptune/src`, `neptune-vi`, `neptune-ve` (same 100 packages, all crates.io);
  only two local edges added (victory-hle -> tracing, victory-gpu; victory-loader -> victory-core).
- Not bundled, expected from the user: `fx1-storage.dll` (built from `tools/gdeflate/runtime.cpp` with MSVC + DirectStorage SDK),
  Microsoft `dstorage.dll` / `dstoragecore.dll`, `fx1_bc.dll`, optional nvCOMP (python `nvidia.nvcomp`). Loaded at run time with
  `LoadLibraryExW` from `VICTORY_GDEFLATE_BRIDGE_DIR` (`dstorage/native.rs:140-164`): a DLL-load surface if that folder is writable by others.
- Scripts: no network, registry, download or encoded commands. `play.ps1` starts `nsys.exe` (Nsight) and `nv-aftermath-monitor.exe`
  hidden if installed (`play.ps1:280-346`; `-NoNsight` skips the first). Python tools use `subprocess` only for their own scripts/tests,
  `ctypes` to load the DirectStorage DLLs. `.githooks/pre-commit` runs `cargo xtask policy` (not installed by clone; `core.hooksPath` unset).
- CI workflow: checkout pinned by hash, `contents: read`, cargo check/test only. No text addressed to AI assistants found.

## 1. What it is
- Source snapshot of Neptune "2026-11-25m" (closest to our `neptune-ve`; 43 code files differ, part of that being our own "(ours)" edits
  in neptune-ve) + GDeflate bridge by "SomewhereHigh" + facial-data fix (Vertex 3562acb / VIctory PR 927) + Cool Fox decoder fixes.
- 4 commits on main (8 with branch tips), 25-26 Nov 2026, one author, no shared history with our clones (fresh init). 1.8 MB.
- Unique size: `tools/gdeflate/` 4,500 lines (C++ bridge 687, python converter/tests), dstorage +467/-85 and 3 new submodules (660 lines),
  host.rs +303, descriptor/draw/shader face work ~700.

## 2. GDeflate path
- OFFLINE converter: `tools/gdeflate/repack.py` reads `*.ttpf` and RPF stream indexes, decodes each chunk (zlib, then BCPack through
  `fx1_bc.dll`, `repack.py:44-60`), recompresses the DECODED bytes with GDeflate (DirectStorage CPU codec or nvCOMP/CUDA), verifies,
  stores in per-archive SQLite sidecars; `publish.py` writes one pack + index; `verify-pack.py` checks it on the adapter.
  Key = SHA-256 of the original stored bytes + stored/decoded length + flags (`runtime.cpp:23`, `:576`). No author figures for pack
  size or time; the only hint is the guard "paused with less than 40 GiB of free disk space" (`repack.py:206`). Expect tens of GB.
- Run time (`dstorage.rs:1041-1062`): request (file, offset, sizes, zlib|BCPack flags) -> optional offset index gives the digest
  without reading the source (`dstorage/offset_index.rs`); else the source is read and hashed; `native::serve` -> bridge. Miss, busy
  lane, >4 MiB, or digest failure -> original CPU zlib/BCPack path (`StorageFallback`).
- Bridge = real Microsoft DirectStorage on the host: `IDStorageQueue2`, `DSTORAGE_COMPRESSION_FORMAT_GDEFLATE`, GPU only
  (CPU fallback refused, `runtime.cpp:424`), `DestinationType = DSTORAGE_REQUEST_DESTINATION_BUFFER` (`runtime.cpp:324-325`).

## 3. Where the data lands: (a), plus a round trip
- GPU decompresses into a 4 MiB default-heap BUFFER per lane (1/2/4 lanes), then `CopyBufferRegion` to a READBACK heap
  (`runtime.cpp:365`), `Map`, SHA-256 verify, `memcpy(dst, payload, r.decoded)` into the game's memory (`runtime.cpp:250-258`).
- `native.rs:295-314` decides only between memcpy straight into the title address (`into_title`, after `kernel_will_write`, then
  `title_memory_written`) and scratch + `copy_to_destination` for page-list destinations. Both end in title RAM.
- The normal texture/buffer upload path then runs unchanged. No texture destination, no skipped RAM page, no layout handling.
- So: no RAM or commit saving; it adds GPU decode work, a VRAM->RAM readback and a hash per block, plus 8-32 MiB of VRAM/RAM lanes.
  Neither hypothesis is proven by it; it confirms "GDeflate only changes who decompresses". The offline tools include a sampler-feedback
  + reserved-tile fixture (`streaming-test.cpp`, `feedback.hlsl`) that is NOT wired to the game (`tools/README.md`: "translated title
  shaders still use Neptune's existing texture residency path").

## 4. Author's numbers
- None for GDeflate (no load time, stutter, RAM, VRAM or fps figure anywhere in code, README or tools).
- The only figures are Neptune's own: RTX 5070 Ti 16 GB, i7-14700, 64 GB RAM, **Series S mode 720p -> 1080p**, 12-17 fps; Series X
  4K ~14 fps. Launcher defaults DLSS via OptiScaler, frame generation opt-in. NOT ACCEPTABLE for us as evidence: Series S assets and
  upscaling trade picture for speed, and 64 GB RAM hides the commit problem we have.

## 5. Unified-memory relevant unique code (ranked by expected RAM/commit and stutter benefit, 32 GB / 16 GB)
Base-Neptune items (windowup card windows without CPU twin, placement heaps, QUIET_BACK, WATCH_4K, DS_NOTIFY...) are already in
`neptune-gap-audit.md`; only what dudegio added on top is listed.

| # | Item (file:line) | What it does | In fox? (grep) | Author figure | Port |
|---|---|---|---|---|---|
| 1 | Texture staging cap under pressure: `victory-gpu/src/draw.rs:80-86`, used `:2222` (`VICTORY_TEXTURE_PRESSURE`, on) | background texture staging per frame capped to 64 MiB when free RAM < 3 GB or commit headroom < 4 GB, 16 MiB below 2 GB; bake exempt | No (`texture_pressure_budget` 0 hits). Fox has `residency::pressured()` but does not throttle the upload pass with it | none | ~30 lines, low risk; slower texture arrival under pressure (late, not lower quality) |
| 2 | Commit headroom as a signal: `host.rs:138-150` (`ullAvailPageFile`) | pressure decided on commit limit, not only free RAM | Partly: fox reads `ullAvailPageFile` in `residency.rs`/`copystat.rs` | none | fold into 1 |
| 3 | Idle window-upload slab trim: `windowup.rs:17-18,149-185`, called `host.rs:8556-8561` | slabs with no live chunk for 3 s released when free RAM < 2 GB, 4 kept | No `windowup.rs` in fox (`CARD_WINDOW_UPLOAD` not ported); fox `upload.rs` not checked for an equivalent | none | only with the windows port |
| 4 | Scratch spare limit by pressure: `host.rs:1682-1706` | spare scratch chunks cut to 32 MiB below 2 GB free, restored above 4 GB | Yes, coarser: `fox:host.rs:2050 trim_spare_chunks` drops all when pressured | none | nothing to do |
| 5 | Submit-time destination capture: `xmem.rs:565-634`, `dstorage.rs:839`, `:1299`, `:1401` (`VICTORY_DSTORAGE_SUBMIT_PAGES=1`, off) | resolves the page list to section references at Submit so a title that reuses/frees its page list while the read is pending still gets the write in the right physical pages; all views mapped before any byte is copied | No (`PageRuns`, `resolve_pages` 0 hits; fox resolves at execution, `fox:dstorage.rs:1762`) | none | ~150 lines, medium; correctness (wrong-page writes), no RAM effect |
| 6 | Completed writes always journalled: `watch.rs:1778-1805`; stamps monotonic (`fetch_max`, `watch.rs:337-371`) | a write through an independent mapping advances the journal even when no guard was open; a late publication cannot roll a stamp back | `written_through_view` exists (`fox:watch.rs:1077`); monotonic stamps not verified in fox | none | small; stale-texture fix, no RAM effect |
| 7 | Small-texture 4 KiB placement: `placement.rs:162-215` (`VICTORY_TEXTURE_HEAP_SMALL_ALIGN`, on) | textures <= 64 KiB placed at 4 KiB alignment; counter of bytes saved | Yes: `fox:pool.rs:721` | test prints BC1 32x32: 64 KiB -> 4 KiB | none |
| 8 | Batched tile mapping: `tiles.rs:532-592` | one `UpdateTileMappings` per heap per texture instead of one per mip | No (fox `tiles.rs:557` one region per call; path off by default, `VICTORY_LEVELS_ON_DEMAND`) | none | ~40 lines, low; only matters if levels-on-demand is used |
| 9 | Mask-aware under-writes budget: `host.rs:3255-3300`; relay batch cap `host.rs:2816`, `:2937` (`VICTORY_RELAY_CAP_BATCH`) | charges 4 KiB patch blocks instead of runs; lands a smaller fence-complete prefix when the relay is full | No (0 hits); fox has MERGE_RUNS/MERGE_MASK | none | medium; card/submit side, not memory |
| 10 | Diagnostics: `tools/Measure-HostAllocations.ps1`, `texture.rs:79` LargeStaging log, `-RamAttribution` | commit grouped by allocation base / write-combined bytes; logs >= 4096 textures detiled into staging | No | none | useful to attribute our 36.5 GB commit; read before use |

Nothing in the fork: avoids the RAM copy, opens title memory to the card, backs title pages lazily, drops host mirrors, changes
eviction or VRAM heap layout beyond row 7. Title memory mapping (`xmem.rs`) is otherwise Neptune's.

## 6. Verdict
- GDeflate bridge: do not port. Needs Microsoft DirectStorage runtime DLLs + a C++ bridge DLL + an offline pack of unknown size
  (author guards 40 GiB free) and conversion time (unstated), and it moves decompression onto the card, which is our busy side
  (90-96 %), while our CPU has idle cores. Expected for us: RAM duplication unchanged, commit slightly up, GPU load up during
  streaming, load time unknown (readback + SHA per block makes a gain doubtful).
- "Skip the RAM copy for card-only assets": not attempted here, with or without GDeflate. It stays a design of our own: classify
  destinations the game never reads back, leave those pages unbacked or decommit them after upload, keep write tracking for re-reads.
  GDeflate is not required for that; BCPack/zlib decode on idle CPU cores into an upload buffer does the same.
- Worth taking: row 1 (+2) first (direct lever on commit spikes and page-mapping stalls during texture bursts), row 10 to measure,
  rows 5 and 6 as correctness fixes if we see wrong or stale streamed data. Rows 3, 8, 9 only with their base features.
