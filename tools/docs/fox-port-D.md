# Fox port D: renderer ports 1, 2, 5 from upstream (avatarmanz) (2026-11-26)

## CURRENT: branch `ports-only` (route B), head 5ea734e in `par\foxD`
Tests on ports-only (unit tests only, no game run):
- gpu port/inplace/descriptor tests: 75 pass; the port tests 19 pass after 5ea734e.
- shader: the CBV test passes.
- hle: 168 pass, skipping the base xmem test that also crashes pre-merge.

- Exe: `par\foxD\target\diag\victory.exe` (diag, diagnostics).
- Built from pre-merge victory-fox e0149ec, with cherry-picks only:
  - 86dc109, 4b82f6c, 9c23e8d (port 1)
  - 9997d25, 4cb90be (port 2)
  - 3920dc2 (port 5, host + translator)
  - coder E's d95648d
  - c36f97a: port 2's heap hooks take no lock while the switch is off.
- Conflicts:
  - host.rs: `title_written_at` kept, port 1's `port::copied` added.
  - xmem.rs: `DEFER.before`, then `arrange`.
  - descriptor.rs: port 2 hand-fitted onto the pre-merge `sync_heaps`. The marked runs and fallback pages go into ours; `missed` and
    CHECKED are wired in; upstream's job restructure and in-place reading are NOT taken.
  - shader_cache.rs: B's key, with `VICTORY_PORT_CBV=1` appended only when on.
  - emit/mod.rs: `constants_raw` exported; `half_views` added in the CBV test.
- With every switch off the build is inert, checked by code review:
  - `port::wanted/place_for` return on `on()`; `copied/filled` see empty bounds.
  - `arrange` is behind `port_on()`; the hle import hook and the launcher patch are behind `on()`; `adopt` is behind `working()`.
    `adopted/let_go/line/log` check `on()`; `mark/faulted` see an empty mark map.
  - The CBV host paths are behind `port_cbv::on()`; the translator's paths are behind `constants_raw()`.
  - E's switches default off.
  - The only always-on addition: a 32-bit reservation without commit is recorded in a Vec at allocation.
- The translator source changed, so B's shader-cache key changed: the first run translates every shader (warm it first).
- The full merge (route A, below) is kept on branch `upstream-merge` (head 195ad0a + docs) for a later look. Main decided on route B
  because the merged build ran ~9 ms slower (see "Open").

---
# Route A record: upstream main merged (branch `upstream-merge`)

Copy: `C:\GTAVI\Tools\par\foxD`, branch local/scarlett-fox. Commits: 8018977 (merge), 5ed6678 (defaults gated back),
7133e3c (tests), 658d3c2 (coder E's d95648d cherry-picked). Exe: `par\foxD\target\diag\victory.exe` (diag, diagnostics).

## Route: A (merge), translator excepted
`git merge up-main` (avatarmanz main, 153 commits past 0a43566): 22 files conflicted, ~90 hunks, all in gpu/hle/shader.
- **Merged as upstream:** everything outside `crates/victory-shader` and the offline tool (`apps/victory/src/translate.rs`, `main.rs`).
  This includes the three ports, the heap-sync work, frame pacing/output/scale switches (all off), the texture store, the light probes,
  the replay/timing tools and the PR fixes.
- **Upstream re-ported cool_fox behind switches that are off by default** (residency budgets, fallback heaps, sweep per heap, XMA refill,
  DirectStorage 256 MiB floor, planes). Here cool_fox's own code is kept, so behaviour is as cool_fox ran: residency.rs, pool.rs, draw.rs,
  audio.rs and dstorage.rs are ours. `audio.rs` keeps cool_fox's `set_field(2,26,6,0)`; upstream writes width 5.
  Planes: upstream's `VICTORY_PLANES` is on by default, the same as cool_fox.
- **C3's hand ports dropped for upstream's code:** `VICTORY_METER_THREAD`, `VICTORY_MEMORY_MAP`, the AHEAD search (upstream: always on)
  and `VICTORY_UNDER_FUSED`. UNDER_FUSED uses upstream's code, but stays default OFF here (upstream: on). C3's `goes_as_words(0,4)` guard
  and the MERGED_FUSED counter are kept.
- **Translator = ours** (cool_fox wave32 lowering + coder B) **plus only renderer port 5's translator side (3920dc2).**
  Upstream's own wave32 translator line (LDS flag regions `VICTORY_LDS_FLAGS`, wave-uniform proofs, BVH/cascade wave32, etc.) is a
  second, independent implementation of what cool_fox+B do. Merging both would change shader output by default, so it is NOT taken.
  For the same reason these are not taken either:
  - 027e367: the view-address table. Its gpu side is removed too, which keeps the strides buffer at 8 MB.
  - 0a4253f: the preferred-adapter pick. The host keeps `D3D12CreateDevice(None)`, but the adapter log line is kept.
  - 1162d38: the version-keyed shader cache, which serves stale entries. B's source+switch key is kept, and `VICTORY_PORT_CBV=1` is
    added to the key. `stale_counts()` is a (0,0,0) stub.
  - The offline tool's upstream CLI additions (`--list`, host-wave alias).
- Conflict detail worth knowing:
  - texture.rs: upstream's texture-store prefill now runs inside cool_fox's `record_upload` (after the aliasing barrier and COPY_DEST,
    not for movie planes).
  - place.rs: upstream's `read_by` gained cool_fox's `Step::Plane`.
  - descriptor/host.rs: the `Flip` from cool_fox's pending flips gained `asked`.
  - host.rs Present: our `VICTORY_PRESENT_SKIP` feeds upstream's present status (S_OK when skipped).
  - host.rs: the meter takes upstream's per-second thread lines, with port C's three lines put back in.
  - Duplicate definitions were removed: fallback heaps, watch `Ahead`, C3's `meter_lines` and duplicate tests.

## Default changes the merge brings
Gated back to cool_fox's behaviour (5ed6678):
| switch | upstream default | here |
|---|---|---|
| `VICTORY_SYNC_IN_PLACE` | on | off (`=1` on) |
| `VICTORY_SPANS_QUICK` | on | off (`=1` on) |
| `VICTORY_CACHE_CORES` | on | off (`=1` on) |
| `VICTORY_POOL_SMALL` | off (textures rounded to 64 KiB) | on (no rounding; `=0` rounds) |
| `VICTORY_PLACED_READERS` | only readers after the copy | every reader, as before (`=1` upstream's) |
| `VICTORY_AMBIENT_SETTLE` | `250,3` | `0`, per frame (only matters with `VICTORY_AMBIENT=1`) |
| `VICTORY_UNDER_FUSED` | on | off |

On by default and kept:
- `VICTORY_METER_THREAD`: meter lines on a thread of ours.
- `VICTORY_SHOW_NEW_LATER`: a correctness fix; a slot is not switched to a pooled texture while earlier lists may read it.
- The watch's ahead search: the same answers.
- `VICTORY_MEMORY_MAP`: the walk runs every 60 lines instead of 6 (diagnostics builds only).
- Sampler heaps have 2,048 slots, with slot 2,047 a null sentinel.
- A guard page fails a copy and stays a guard page.
- A frame is counted only once Present accepts it.
- `VICTORY_WRITE_SITES` refuses a file inside the source tree.
- `VICTORY_SEND_OWN` and `VICTORY_RAW_VIEWS` were already on.

New switches, all off by default: `FRAME_INTERVAL_MS`, `TITLE_FPS`, `OUTPUT`, `RENDER_SCALE`, `UPSCALER`, `FFX_SCALE`, `TEXTURE_STORE`,
`TEXTURE_LOOK_MS`, `LIGHT_PROBE(_LUX)`, `UP_QUEUE`, `JOIN_BARRIERS`, `LANE_CORE`, `ASK_NORESET`, `ASK_SPREAD`, `SYNC_FINE`,
`WINDOW_BUDGET_MB`, `MEMORY_CATEGORIES`, `TIMER_LOG`, `PORT_*`. From coder E: `SEND_BY_RANGE`, `PLACE_NO_READBACK`, `ASK_AHEAD_EARLY`.

### Post-merge slowdown (runs 123-125 against run-121), and gates of 195ad0a
Measured by main on 5ed6678:
- Median frame times went from 62/49/55 to 56/71/84 ms.
- From the frame log at >2,500 draws, run-121 against runs 124/125:
  - card idle "while the title recorded": 3 -> 20-23 ms a frame
  - flights in the air at read waits: 6.4 -> 1.7
  - page faults: +35-50%
  - the title's remap ms: 10.7 -> 14-20
  - process kernel CPU: +25%
  - submit "at work" and preparing: flat
- So the title's own threads got slower, not the submit thread.
- Run-123 also started with a cold shader cache: 0 hits and 2,963 misses, because the translator patch changed B's source key.
- The merged runs also lacked `VICTORY_UNDER_FUSED=1` (and METER_THREAD=1, REMAP_METER=1), which run-121 had.

Suspects gated back to pre-merge behaviour in 195ad0a (`=1` gives upstream's):
- `VICTORY_TEXLATE`: upstream's texture-shown times. They take a global mutex on every texture view created (title threads) and the
  texture queue's lock per slot in the pass.
- `VICTORY_SHOW_NEW_LATER`: now default off.
- `VICTORY_ASK_ONE_HOLD`: e000e02's one hold of everything over both parts of a submission's asking.
- `VICTORY_GUARD_REARM`: fa11650's re-arm of guard pages after our copies.

Not confirmed by a run yet.

## The port switches
- **`VICTORY_PORT_CONSTANTS=1`** (port 1). The 185 MB 32-bit graphics heap at 0x34310000 (the constants ring at 0x34e10000) is opened
  as a host heap through a view of ours (`OpenExistingHeapFromAddress1`). Title buffers and windows in it become buffers placed in that
  heap. Port 1 is tried before cool_fox's card copies (`cards`). The xmem half (`arrange` and recording kind 0x42000) runs only with the
  switch.
  - Log lines: "renderer port 1: a stretch of the title's graphics memory opened as a host heap through a view of ours",
    "renderer port 1: title memory in place, read and written by the card where the title has it", and per second
    "renderer port 1 (VICTORY_PORT_CONSTANTS=1): N regions ... in place (M MB), K title buffers and windows placed".
    Refusals: "renderer port 1: title memory not in place, a copy as before" (why=Scattered / not committed / host).
  - Prerequisites: no hard ones. First A/B with cool_fox's defaults: `CARD_STEPS`, `FILL_TITLE`, `MAP_DEFER` and `POOL_DISCARD` off.
    Fills and CPU copies that touch ported memory still work and are counted on the port-1 line.
  - Limits, from upstream's notes:
    - The heap is a custom heap in **PC memory** (write-back, pool L0, cross-adapter shared; see the `inplace.rs` header). The card reads
      it over PCIe.
    - Upstream measured on a discrete 7900 XTX with 64 GB of RAM. Replay: "the title's own work +~3 ms on the card"; the 30 ms it saved
      were our compare passes. On this RTX 4080 run-122 (`VICTORY_INPLACE=1`) the card got slower with title memory in place, so watch
      card idle and card time.
    - Its 185 MB is locked into RAM while the heap stands, and the title cannot decommit or protect those pages.
    - Never tile-map into the heap (device removed).
    - It assumes the card reads what the title's CPU wrote before it submitted (no flight copy).
    - Card time per kind of list: `VICTORY_GPU_TIMING=1` (upstream 0140089; it runs every list timed on its own, so frame times change).
      The always-on line is "the card's clock per frame: ...".
- **`VICTORY_PORT_DESCRIPTORS=1`** (port 2). The launcher patches six 32-byte stores of the title (+0x4d9efd0, +0x4eec8e0; the bytes
  are checked first) and the title's memcpy/memmove imports, so they mark slots. `sync_heaps` takes the marks instead of the write-watched
  pages. Fallback: the watch is asked every 8 frames (`VICTORY_PORT_DESCRIPTORS_EVERY`) and 16 pages a frame are checked whole
  (`_CHECK`).
  - Log lines: six "renderer port 2: the title's descriptor record store at title+site patched..." lines (or "not patched, the title's
    bytes there are not the ones expected"), then "renderer port 2: the title's view heap's slots are told by its record stores...", and
    per second "renderer port 2 (VICTORY_PORT_DESCRIPTORS=1) per frame: N slots told ...". Also watch the "missed" view records.
    "not working, no store of the title's was patched" means the port is not active.
  - Prerequisites: none (works with the in-place sync off, our default). It is tied to image 1.0.2.1's code.
  - Limit: a missed slot shows an old view until the fallback reaches its page.
- **`VICTORY_PORT_CBV=1`** (port 5). Translated shaders read constant buffers as raw bytes, through the raw twin of their slot or as a
  root SRV. A table CBV in ported title memory gets a raw view in place, with no copy (`VICTORY_PORT_CBV_PLACE=0` copies again).
  - Needs `VICTORY_RAW_VIEWS` not 0 (on by default). It is fully effective only with `PORT_CONSTANTS=1`; without it the CBVs are still
    copied but read raw.
  - It changes the shader cache key, so every shader is translated again and every pipeline rebuilt. Warm first (`victory shaders`,
    `victory warm`).
  - Log line: per second "renderer port 5 (VICTORY_PORT_CBV=1) per frame: N tables' constant buffers read by the card in place ...".
  - Limit: upstream published no frame time for it; raw loads may cost card time.
  - Its translator test (`table_constant_buffers_read_as_raw_bytes_through_their_twins`) is ported onto our translator.

Suggested A/B order: PORT_DESCRIPTORS alone; PORT_CONSTANTS alone; both; then +PORT_CBV after a warm run.

## Tooling checked (code)
These stay as before:
- `VICTORY_AUTO_INPUT`: gameinput.rs, which reads `frame::PRESENTED_DRAWS`.
- The caption "Scarlett | game | F FPS | M ms | D draws": `shell::caption_for` in Meter.
- The frame log and hang watch: `frame_began` in frame.rs, next to upstream's pacing `pace::begun`.
- `VICTORY_PRESENT_SKIP`.
- B's shader-cache key: the "foxB 1" key, `switch_fingerprint` and half_views in the entry.

## Tests
`cargo-bg test --profile diag --features diagnostics`:
- victory-gpu lib: 435 pass, 4 fail.
  - `indirect::host_list_budget_starts_empty_at_each_submission` and `warp_batch_charges_every_execution` fail on pristine fox too
    (fox-port-C).
  - `hlehooks::unmapped_is_counted_and_passed_on` and `timeline::report::real_waits_are_recorded_with_who_ended_them` pass when run
    alone: they depend on test order and timing.
- victory-shader lib: 307 pass, including the ported CBV test.
- victory-hle lib: the process crashes (access violation) in `xmem::tests::a_title_thread_goes_on_using_the_rest_of_a_view`, an old
  base test where a title thread reads page 0 while pages 1-3 are remapped. It crashes the same way on pre-merge e0149ec, so it is
  pre-existing. Without it: 172 pass, 2 ignored.

## Open: the remaining post-merge gap (runs 126 against 127, same hour, same env)
- 127 is about 9 ms slower at 2.5-4k draws.
- Lists in the air at read waits: 8.1 -> 1.8. Card idle while the title recorded: 9 -> 23 ms.
- Faults, remap and preparing are equal. Kernel CPU +18%, user CPU -9%.
- So title threads are blocked, but not in a GPU wait.
- Suspects, always on in upstream, on title-thread paths: 11511b8 (lock-free queue bits, Made::LOCKS hashing, retire parts),
  dd67ae7 (2,048-slot sampler heaps), 888bf1e (slot masks), the sync_heaps job restructure.
- Proposed bisect builds (pre-merge + upstream up to 306722b; then the rest): declined by main (runs are noisy, 13-18 ms); route B
  taken instead.
- Insight for a later look: after the merge, title threads block in our code. Kernel CPU +18%, user -9%, game CPU 450% vs 542%, lists
  in the air 8.1 -> 1.8.
