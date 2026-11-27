# Review of the foxhound fork (readmet3xt-lab/foxhound), 2026-11-27

Read-only review. Nothing from the fork was built, run or executed. Nothing was downloaded. No code of ours was
edited. This file is the only thing written.

- Fork: `C:\GTAVI\Tools\ref-forks\foxhound`, head `02fc18a`. Paths marked `F:` are relative to it.
- Base in the fork: `545f890` "Publish the pareto-residency Windows build and portable launcher". Its tree hash
  (`832b1ea5`) is the same as the tree of our tag `coolfox-base` (f4b48ad). So the base is byte-identical.
- Ours: `C:\GTAVI\Tools\par\foxT`, branch t-on-w, head `2c0c0d2`. Paths marked `O:` are relative to it.
- The fork adds 96 commits over the base (75 plain, 21 merges). 68 files, +17,421 / -805 lines (README and docs included).
- Method: every commit message read in full; `git diff 545f890 HEAD` for the files named below; switch names
  compared across base, fork and foxT; the functions each side changed compared by hunk header.
- No text addressed to an AI or to an assistant was found (README, docs, scripts, source comments searched).
  The README says much of the fork was written with Claude Code; that is a credit line, not an instruction.

Words used below:
- **"verified"** = I read the code at the fork's head.
- **"message"** = taken from a commit or merge message, a code comment or the README. Not checked by a run.
- All their numbers are from one PC: 7800X3D, 32 GB, **RTX 4070 SUPER 12 GB**, Series X profile, 2560x1440
  output, usually with `VICTORY_THIN=1` and DLSS Quality (scene 1706x960). They are not our numbers.

## The five things to know first

1. **Their launcher does not show the console's picture.** It leaves two passes out, replaces the title's
   upscaler with DLSS at a lower scene size (1706x960), and adds three light stand-ins. Section C lists each.
2. **Their one large speed gain against freezes is `VICTORY_SOFT_VIEWS=copies`.** It removes the Windows
   map/unmap calls of the GPU page-mapping copies. That is our bottleneck 3 exactly. It is also the hardest
   port: about 1,360 lines in the same `xmem.rs` and `watch.rs` functions we changed.
3. **Their cheapest speed gain is the texture queue's lock.** Every descriptor slot a recording thread writes
   takes one global mutex. Our tree still does this (`O: crates/victory-gpu/src/descriptor/host.rs:2692`).
   Their fix is 250 lines in code we have not touched.
4. **Two picture fixes we lack could explain defects that differ from run to run**: pipelines with static
   samplers loaded from the pipeline library sample through the wrong sampler, and character bakes that read a
   stand-in texture keep it for the whole run. Both are candidates, not proven for our defects.
5. **Nothing in the fork touches our NaN sources, the stretching meshes or the dot clusters.** The fork has no
   NaN guard at all. Their answer to purple light is the `VICTORY_AMBIENT` stand-in.

---

## A. What the fork added over the base

"Launcher" = `F: scripts/play-foxhound.ps1` with no options. "Ours" = whether foxT has the same thing.
Of the 59 switch names the fork added, only `VICTORY_EARLY_DEPTH` exists in foxT (searched by name).

### A1. Submitting thread

| Item (commit) | What it does | Default | Launcher | Their gain (message) | Ours |
|---|---|---|---|---|---|
| Card copies from a list (441446d) | `card_dirty` ran once per submission (28 a frame) over all ~3,900 card copies. A copy now goes on a list when touched. Verified, `F: shadow.rs` `TOUCHED`, `carded`. | on, no switch | on | "2.5 % of the submitting thread in a gameplay profile" | No. `O: shadow.rs:624` walks all of `CARDED`. |
| Wake only needed threads (8c1fa5d) | `shadow::wide` woke all kept threads for any job count. Now one per job. Verified, 11 lines. | on, no switch | on | "2.4 % of the submitting thread's samples", 1.3 % under `wide` | No. `O: shadow.rs:1569` `notify_all`. |
| Host lists left open (a7b8667) | A new host list was closed at once and reset again by the title's reset. Now left open. | on, `VICTORY_FRESH_OPEN=0` undoes | on | "about 200 empty closes a frame ... 3% of its samples ... about 1 ms a frame" | No. |
| Batch kept (19720fe) | The submission's `Batch` and its hash tables are reused. One walk gives the spans for three users. | on, no switch | on | "hash tables growing and inserting ... were 2.4% of the submitting thread" | No. Our `Batch` differs (REFRESH_ONCE, REFRESH_PAGES, NARROW_WRITES, PREPARE_WIDE live in it). |
| Time split in release (ed252ec) | The submitting thread's split line in release builds too. Logging only. | on | on | none | We have our own split meters (T19, W16, W23). |
| Indirect merge (28f6e25) | Consecutive one-record `ExecuteIndirect` calls joined into one. | off `VICTORY_INDIRECT_MERGE` | not set | none given | No. |
| Two host queues (fa88dc7) | `VICTORY_ASYNC_QUEUE=plan`: a dry-run plan of a second (compute) queue. Counts only; "Nothing runs differently." | off | not set | none; context: "7.2 ms sent and not begun" | No. Not usable as it stands. |
| Helper priority (002cf4a) | Sets a priority for the threads a frame waits for. | off | not set | "four bench runs ... were within the noise" | No. Skip. |

### A2. Heap-record sync switches (all five set by the launcher)

The "round" mirrors descriptor records the title copied into its heaps as bytes. It runs on its own threads.
The submitting thread waits for it at the end of preparing.

| Switch (commit) | What it does | Default | Launcher | Their gain (message) | Ours |
|---|---|---|---|---|---|
| `VICTORY_HEAP_SYNC_FIRST` (66f7e54) | The round starts when the submission takes its lock, not after planning. | off | 1 | wait was "2.7-5.8 ms a frame of the submitting thread"; gain not measured | No. |
| `VICTORY_SYNC_IN_PLACE` (769fe52) | Records are read in place once with fault-safe AVX2 code and compared as read, not copied through the kernel first. Adds hand-written assembly (`F: safecopy.rs`, `global_asm!`). | off | 1 | in the source fork: "copy 7.4 and compare 5.3 ms a frame of six threads for 35 MB"; here "2.6-3.1 ms a frame" was the old cost | No. |
| `VICTORY_SPANS_QUICK` (96dfd9b) | A slot write with no placed view skips the heap's spans lock when a bit says the slot is not in them. | off | 1 | in the source fork: "4 of the 16 ms a frame the submitting thread waited for the round were its threads waiting on that lock" | No. |
| `VICTORY_SYNC_FINE` (96dfd9b) | Round jobs of 2 pages / 64 records, not 8 / 256. | off | 1 | none | No. |
| `VICTORY_HEAP_SYNC_AT_SUBMIT` (d2e24b3) | The round is asked for when the title submits, so it runs while the submission waits in the line. New `heap_marks.rs`. | off | 1 | source fork: "port 7 took 2.4 ms off" a 4.5-5.2 ms wait | No. |

The merge message for all of A1 (new ones) and A2 says: "Built and tested at this merge; **not yet measured in
the game**." All five change timing: a record the title writes after its submit is left to a later round.

### A3. Texture queue and texture pass

| Item (commit) | What it does | Default | Launcher | Their gain (message) | Ours |
|---|---|---|---|---|---|
| Sort outside the lock (2de963d) | The pass sorted ~40,000 due slots under the queue's lock. Now it sorts after. | on | on | "1-5 ms a pass" under the lock | No. |
| Queue marks (2ad55e6) | Per title slot, an atomic says which write the queue holds. Notes that change nothing skip the lock. Verified, `F: descriptor/host.rs:1050-1092`. | on, `VICTORY_QUEUE_MARKS=0` undoes | on | "Other threads waited 33-44 s for the lock in a run of 150 s". Chain z1, marks + batch together: "gameplay 33.9 ms p95 34.7" against "39.5 ms p95 54" with both off | **No.** `O: descriptor/host.rs:2692` `queue_texture` takes `self.textures.lock()` on every slot write. |
| Batched tells (abf1020) | The pass tells the queue 16 slots per lock take, and reads what the queue knows with the due slots. | on, `VICTORY_QUEUE_BATCH=0` undoes | on | "telling the queue was 143-400 ms [per 5 s], about all of it waiting for the lock" | No. |
| View read once (13dca45) | "A slot shown after its upload or flipped uses the view read at its turn's start". It read the same record twice, microseconds apart. This is a lock saving, **not a picture fix**. 9 lines. | on | on | "Other threads waited 86-383 ms per 5 s ... for the views table's lock" | No. |
| Staged ask only at title addresses (36c4c37, 6b3960d) | `QueryWorkingSetEx` is asked only for textures at a title address, and answered from the runtime's views where they cover it. | on | on | measured: "271 ms per whole run with the old behaviour, 91-189 ms with these commits: a real but small saving" | No. |
| `VICTORY_TABLED_REREAD` (8625c3a) | A table read later in a frame takes out a slot written after the frame's first read. The draw then makes the slot at once, not 8 frames later. | off | 1 | chain z1: 37.2 / 40.2 ms (no gain over A); "a candidate of the flicker at scene loads" | No. |
| `VICTORY_QUEUE_HOST_TEXTURES=0` (5dda41d) | Records of textures the host writes are not queued for the pass. | queued | 0 | chain z1: 36.4 / 41.8 ms (no gain over A); "58,978 of 90,699 slots taken in 5 s ... found nothing to do" | No. |
| Split lines, census fix (0b651fa, c1d1b4e, 4cee21f) | 5-second timing lines for the pass and the lock. Logging. | on | on | none | Ours has `VICTORY_LOCK_METER`. |

### A4. Texture upload order, pool, bakes

| Item (commit) | What it does | Default | Launcher | Their gain (message) | Ours |
|---|---|---|---|---|---|
| `VICTORY_PRELUDE_UPLOADS` (91df493) | A title texture's upload is decided at submission, after the check, and copied on the list's prelude. Before, a texture found changed at the check was uploaded by the next list: a frame late. | off | 1 | none; "A candidate of the flicker at scene loads" | No. |
| `VICTORY_SHOW_NEW_LATER` (ee3fb52) | A slot keeps what it showed while lists sent earlier may still read it; it flips when they finished. Against device loss with the pool. | off | 1 | none; source fork lost the device 6 times without it | No. Ours has `SHOW_AFTER` only (base). |
| `VICTORY_POOL_SMALL=0` (185bc4c) | Pooled textures of 64 KiB and less at 64 KiB multiples, not 4 KiB. | 4 KiB | 0 | none; "Not proven to be the cause" of device losses | No. Our pool is reworked (`O: pool.rs` +1,365). |
| Bake sources (b3d49db, 082ebaf, 2983a71) | Before a character bake, every slot the bake's constants and records name is checked. A source not up to date is uploaded on the bake's own list, whatever the room or budget. | on, `VICTORY_BAKE_SOURCES=0` undoes | on | none in ms; "what a bake reads wrong stays for good: coloured specks over a character's skin" | **No** (searched `bake_sources`: none). We have `BAKE_EXACT` (base), `BAKE_DELTA`, `BAKE_TEXTURES_MS=10000`. |
| Exact refresh without waiting (3cb7b8f) | A bake's "exact" upload reads the texture through the kernel first and waits for the remapping only if a page is away. | on, `VICTORY_EXACT_NO_WAIT=0` undoes | on | "808 of 866 ms of texture refreshes a frame in a cutscene second" were this wait | No. `O: texture.rs:1502` waits whenever `exact`. |
| Target reserve expires (21b040d), room line (ff5d931) | The 3 GB render-target reserve is given up once targets settle. The "no room" line is budget less 768 MiB or 5 %. | on | on | "160-990 MB stayed kept for targets that never came" on a 12 GB card | No. Ours has `POOL_CAP`, `VRAM_RAW_PCT` instead. |
| Card copy caps from the budget (2203538) | Budgets of 12 GiB or less: buffer copies up to 18 % of budget. From 8 GiB: windows get 9 % (max 1,024 MB) on the card by default. | on | on | "gameplay ran at 3 fps" -> "21-25 fps" on the 12 GB card | Partly. Our 16 GB class is unchanged by the first half. The second half equals setting `VICTORY_CARD_WINDOWS_MB=1024`, which our tree reads (default 0). |

### A5. Memory mapping

| Item (commit) | What it does | Default | Launcher | Their gain (message) | Ours |
|---|---|---|---|---|---|
| `VICTORY_SOFT_VIEWS` (fb838e8, needs f12996c) | Views of the title are kept in the runtime's books only. The host maps nothing until the title's CPU touches the memory (a fault makes the view real). `copies` = the views of `CopyPageMappingsBatchX` only; `pool` = the title's own `XMemMapPhysicalPages` views only; `1` = both. The GPU side reads such memory through the section's view and guards it in its books with no `VirtualProtect`. Verified, `F: xmem.rs:2194-2262`. | off | **`copies`** | Merge a83bc91, three runs each: cutscene median "39.2 / 39.3 / 40.9 ms ... 0 seconds over 100 ms" against "66.6 / 63.3 / 66.0 ms ... 20 / 4 / 11 seconds over 100 ms". Streaming log, ~140 s: "page-mapping copies 35.1 s -> 2.0 s; unmaps 322,253 (31.9 s) -> 155,387 (10.1 s); maps 419,724 (19.4 s) -> 219,805 (6.6 s)". "`pool` does not help yet." | **No.** We have other tools on the same calls: `MAP_LOCK_FINE`, `MAP_DEFER`, `POOL_DISCARD`, `GPU_ONLY`, `REMAP_SAME`. None of them removes the map/unmap call. |
| `VICTORY_LAZY_VIEWS` (f12996c) | Earlier form: the title's own views mapped on first touch. | off | not set | "ran at 2 fps" (message of fb838e8). Superseded. | No. Skip, but SOFT_VIEWS is built on its code. |
| `VICTORY_MAP_RUNS` (9cb55c7) | A run of pages that follow each other in the section is mapped through one view: 2 system calls, not 2n. | off | not set | none; "unmapping is the dearest call, about 110 us a view" | No. Our `unmap` differs. |
| `VICTORY_FAULT_UNLOCKED` (3f55056) | A write fault opens its guarded block without the guards' lock (a bit per block, settled by the next lock holder). | off | not set | old cost: "Of 281 ms the title's threads spent in the write-fault handler over 14 frames of play, 252 ms were waits for the guards' lock". Gain not measured. | No. |
| `VICTORY_GUARD_HOT=<ms>` (71fb272) | A block written again soon after its rest stays open that long; it is told as written at every asking. | off | not set | old cost: "321 to 1,017 blocks made read-only and 323 to 331 guard faults a frame". Gain not measured. | No. Base `VICTORY_GUARD_REST=<us>` (3,000) is in both trees. |
| `VICTORY_WC_NO_BACK`, `_MIN_KB` (50c15a1, 2f81d0c) | For card copies of write-combined title memory of at least MIN_KB: what shaders write is not hashed, compared, fetched or merged back. Verified: 11 lines in `Shadow::gpu_writes` plus a registry of write-combined regions. | off | **1, 1024** | "gameplay 38.3 -> 33.3 ms, cutscene median 47.3 -> 36.3 ms, p95 205 -> 142 ms" at 1 MB. Without the minimum: "cutscenes ... went near black". | No. `O: shadow.rs:3349` is the base form. |
| `VICTORY_WC_BAR` (50c15a1) | A write-combined title buffer gets one copy in CPU-mapped card memory (GPU upload heap). | off | not set | none | No. Our `WINDOWS_BAR_MB` is a related idea for windows. |
| "Renderer port 1" `VICTORY_PORT_CONSTANTS` (9075710, 35c57d6, 376f983) | The title's 185 MB 32-bit graphics heap (the ring of constants, 174 MB buffer) is mapped once more at our address and opened as a host heap. Buffers in it are placed there: nothing tracked, sent, fetched or compared. New `port.rs` (401 lines). | off | not set | old cost: "~17 MB a frame sent up in ~600 pieces ... ~6,000 constant fills a frame". Replay: "the title's own work +~3 ms on the card". README: "less CPU, but on a 12 GB card the GPU then waits (slower overall)". | No. |

### A6. Shaders and picture

| Item (commit) | What it does | Default | Launcher | Their gain / claim (message) | Ours |
|---|---|---|---|---|---|
| `VICTORY_EARLY_DEPTH` (41b4a05) | Pixel programs with `DEPTH_BEFORE_SHADER` get DXIL `ForceEarlyDepthStencil`. | off | 1 | Fixes the glass prepass writing depth and motion over a character. "not yet confirmed in a run". | **Yes, same rule, ours is on by default** (`O: victory-shader/src/emit/lower.rs:1267-1283`). Ours also excludes coverage exports. |
| Static samplers out of the library (7fa0e95) | A pipeline with static samplers is never loaded from or stored in `ID3D12PipelineLibrary`. Verified: `F: pipeline/library.rs:285`, `pipeline/host.rs`. | on, `VICTORY_LIBRARY_STATIC_SAMPLERS=1` undoes | on | "PS_CompositeNMB loaded so read its colour-grading tables unfiltered: posterized light". "Any loaded pipeline with static samplers could read through another's (filter, address mode, level, comparison)". Reproduced in a stand-alone probe on the 4070 SUPER. | **No.** |
| `VICTORY_FILTER_3D` (d246d10) | 3D table samples filtered in shader code. An experiment made before the cause above was found. | off | not set | none | No. Skip. |
| Volumetric light on 32-lane hosts (eeb17cc) | Two `CS_TransmissionVolumetrics_Tiled*` programs added to the "widened" list by digest. | on | on | "on NVIDIA their dispatches were dropped and there was no volumetric light or fog" | Different route: our paired form (`VICTORY_WAVE_PAIRS`, on). Our run-197 audit found no dropped program but ray tracing. Not compared line by line. |
| Wave64 groups of several console waves (4f49709, 89cfae2) | Host waves "meet" per console wave by spinning on an LDS counter (at most 4,096 polls; "a partner that never comes costs wrong values"). Makes `CS_DefragHeap_ComputeRelocations` translate. | on for listed digests | on, but the launcher then leaves that pass out | That pass then cost "11.85 ms of the card a frame" | Different route (paired form). Skip theirs. |
| `VICTORY_LEAVE_OUT=<hashes>` (672442b) | Pipelines of the named shader hashes are never made, in every build. | empty | **two hashes** | see C | No (`SKIP_PIPELINES` in diagnostics builds only, base). |
| "Light stand-ins" `VICTORY_AMBIENT_SPECULAR` (2db59d4 .. 9f6fb77), `VICTORY_AMBIENT_ONCE` (8cf0a75) | A pass of ours writes an estimate into the rays' image after each `CS_RayMiss` of opaque surfaces. ONCE removes the diffuse stand-in's own feedback. | off | SPECULAR=1 (ONCE not set) | "Expected cost under 0.15 ms a frame" | No. `VICTORY_AMBIENT` itself is base code, in both trees. |
| Stand-in fixes (d226814, bda6e59, 9b91d27, f89b79d) | "never a stencil plane as the depth", "placed texture views resolved", depth size checks. Verified: all inside the stand-in's own table lookup (`F: draw.rs:3775-3930`). | — | — | — | **Not general fixes.** They change nothing unless `VICTORY_AMBIENT*` is set. |
| Left-out pipelines named (f149e26), casting log (cb191f3) | Logging. | on | on | none | No need. |

### A7. Output size, frame rate, upscaling, input

| Item (commit) | What it does | Default | Launcher | Their gain (message) | Ours |
|---|---|---|---|---|---|
| `VICTORY_NATIVE_1440P` / `VICTORY_RENDER_1080P` (9a79132) | Image patches for a 2560x1440 output. See D. | off | **NATIVE_1440P=1** | "median frame 77 -> 67 ms, 95th percentile 506 -> 401 ms" (with volumetric light turned on in the same comparison) | No. |
| DLSS / Streamline `VICTORY_DLSS`, `VICTORY_DLSS_MODE`, `VICTORY_FRAMEGEN`, `VICTORY_FSR_PATH` (fc8b9ca, d11233a, 4fcaebd, 1d0fa87) | The title is patched onto its FidelityFX path (renderer 7, which stock Series X never uses). DLSS takes the upscale dispatch. Frame generation through Streamline's swap chain. | off | **DLSS quality if the Streamline folder exists**; FRAMEGEN only with `-FrameGen` | "gameplay 43.8 -> 38.1 ms, cutscene median 50.3 -> 41.3 ms" (scene 1706x960 instead of 2560x1440) | No. |
| Frame-interval patch `VICTORY_FRAME_INTERVAL_MS`, `VICTORY_TITLE_FPS` (bacb402, 45f6250) | Our clock ticks at the asked interval and three reads of the title's 30 fps setting are patched at load (`F: victory-loader/src/frame_rate.rs`). | off | not set (`-Fps` sets only `VICTORY_FRAME_INTERVAL_US`, our clock alone) | "not yet measured fairly" | No. Not needed for 30 fps. |
| `VICTORY_FRAME_INTERVAL_US`, `VICTORY_DISPLAY_REFRESH`, `VICTORY_KEYS_FILE` (2f0fbe1) | Pacing override, extra refresh rate in the mode list, a file a script writes to press pad buttons. | off | not set | none | We have `AUTO_INPUT`. Skip. |
| `VICTORY_PIPELINE_BOOK` | **Base code, not the fork's.** Pipelines are written down; `victory warm` makes them ahead. | off | 1 | README: first run of a build stutters without it | Same code in our tree; our launcher does not set it. |

---

## B. Our four bottlenecks: what of theirs targets each

"Lines" are the commit's own size. "Conflict" compares the functions both sides changed since the base.

### B1. Submitting thread in prologue / cutscenes (32-44 ms; refreshes 12-14, card copies ~6, asks ~4)

| Their change | Targets | How sure | Port effort onto foxT |
|---|---|---|---|
| `WC_NO_BACK` + `MIN_KB=1024` | The write-back of shader writes: hashes, compare passes, fetch, merge. That is part of "card copies". | Medium. Their gain is large (-5 ms gameplay, -11 ms cutscene median) but on their mix. It needs our write-combined ranges to carry the same share ("about 360 of 730 MB a frame" compared). | **Easy.** About 110 lines without `WC_BAR`: `shadow.rs` (registry + 11 lines in `gpu_writes`, a field), `xmem.rs` +4, `lib.rs` +2. Our `gpu_writes` is still the base form. Picture risk, see C. |
| Card copies from a list + wake only needed threads | `card_dirty` walk and wake system calls. | High that it is free time; small: together about 5 % of their submitting thread's samples. | **Trivial.** 47 + 11 lines in `shadow.rs`; our `card_dirty` and `wide` are the base forms. |
| `PORT_CONSTANTS` | Buffer refreshes of the constants ring: its sends, compares and ~6,000 fills. | Low to medium. It moves work to the card over the bus. They found it slower overall on 12 GB. Our T22 meter (refresh steps by asker) would say what share of our 12-14 ms is this buffer. | **Medium.** New `port.rs` 401 lines; `inplace.rs` +76 (we changed it, +62); `xmem.rs` +145 in reservation code; `shadow.rs` +8. Device-removal warnings in its own comments. |
| Heap-sync switches (5) | The wait for the heaps' records at the end of preparing. | Low for us: this wait is not in our top three. Not measured in their game either. | **Medium.** `descriptor.rs` +230 and +52 (ours changed 6 lines there: clean), `queue/behind.rs` +172 (ours +8: clean), new `heap_marks.rs`, new assembly in `safecopy.rs`, `host.rs` +142 in `impl Host` (conflicts). |
| `GUARD_HOT`, `FAULT_UNLOCKED` | Guard arming and faults under the guards' lock: the "guard/write-watch asks" part. | Low to medium. Right target for our 75-80 % figure, but no gain measured, and our `GPU_ONLY` + `KNOWN_WRITES` already take pages out of guarding. | **Hard.** 109 + 564 lines in `watch.rs` `Guards`, `open`, `guard`, `Bits`, `install`. We changed the same items (+964 lines in `watch.rs`). No-code test first: raise `VICTORY_GUARD_REST` (microseconds, base switch). |
| Host lists left open, batch kept | ~1 ms a frame, 2.4 %. | Medium, small. | Medium. `host.rs` `Batch` and `detach`; our `Batch` is heavily changed. |
| `SOFT_VIEWS=copies` | Guards of such views are kept in books: no `VirtualProtect`. The kernel lock the guards share with map/unmap is freed. | Medium. Their cutscene median fell from ~65 to ~39 ms. | Hard, see B3. |

### B2. Open world: the title's five recording threads (about 22 us per bind/draw set-up)

| Their change | Targets | How sure | Port effort onto foxT |
|---|---|---|---|
| **Queue marks** (+ sort outside the lock) | One global mutex taken for every slot record a recording thread writes. We build 85-104k table slots a frame; each takes it. | Medium to high that the lock is in our path (verified in code). The size of the gain is unknown: their 5.6 ms was with `THIN=1` and with the batch commit. Our `VICTORY_LOCK_METER` can measure the wait first. | **Easy.** 199 + 54 lines in `descriptor/host.rs` `TextureQueue` / `HostHeap`. Our hunks in that file are elsewhere (`ListView`, `page_written`, `null_view`, `from_base`, `dynamic_slots`). |
| Batched tells | The pass's own lock takes. | Medium. | Medium. 268 lines; 12 hunks in `draw.rs` `heap_textures`, where we have 9 hunks. |
| `SPANS_QUICK` | The spans lock, shared by the title's threads and the round's threads. | Low to medium. | Easy to medium. 76 lines in `descriptor/host.rs` `HostHeap`. |
| `TABLED_REREAD`, `QUEUE_HOST_TEXTURES=0` | Fewer slots queued. | Low. Their own A/B showed no gain. Both change behaviour. | Easy, but skip. |
| `FAULT_UNLOCKED` | Title threads waiting for the guards' lock in the fault handler (their 18 ms a frame summed over threads). | Medium for the target, gain unmeasured. | Hard, see B1. |
| `VICTORY_THIN=1` (base) | Moves the translation of list calls to our threads. | It is in our tree already and unset. Our notes record picture faults with it in other forks. | None. Not a port. |

Nothing of theirs touches the table building itself (the 61 % "named by the shader" figure). Our `TABLE_MEMO`,
`LIST_HOLD`, `BIND_SAME` have no counterpart in the fork.

### B3. Freezes: GPU page-mapping copies (one map + unmap per 64 KiB page)

| Their change | Targets | How sure | Port effort onto foxT |
|---|---|---|---|
| **`SOFT_VIEWS=copies`** | Exactly these calls. Their cost per view matches ours: "MapViewOfFile3 (45 us) and later an UnmapViewOfFile2 (93 us), both behind a lock of the kernel's". One copy was "467 ms of a 608 ms frame". | **High** for the mechanism. Medium for our gain: our class B freezes are 0.4-1.6 s single calls under memory pressure; fewer views should help, but our commit of 33 GB stays. | **Hard.** f12996c (408 lines) + fb838e8 (956 lines) in `xmem.rs`, `watch.rs`, `host.rs`, `texture.rs`, `lib.rs`. Both sides changed `map_aliases`, `map_alias`, `change`, `unmap`, `scatter`, `XMemMapPhysicalPages`, `XMemMapPhysicalPagesScatter`, `XMemVirtualQuery`. It forces one mapping thread and must be squared with our `MAP_DEFER` FIFO, `POOL_DISCARD` resets, `MAP_LOCK_FINE` ranges and `GPU_ONLY` page kinds. Better re-built on our books than merged. |
| `MAP_RUNS` | One view per run of pages: 2 calls, not 2n. | Medium. No gain measured. | Medium. 241 lines in `map_aliases`, `show`, `split`; conflicts with ours. |
| Exact refresh without waiting | Bake uploads waiting for a remapping to end. | Medium for cutscene hitches; `MAP_LOCK_FINE` may already cut most of that wait. | Easy to medium. 69 lines; `Staging::refresh`, where we have 6 hunks. |
| Target reserve, room line | Texture room on the card. | Low for a 16 GB card. | Skip. |

Nothing of theirs lowers commit. The 1440p output (D) lowers card memory, not RAM.

### B4. Picture defects

| Our defect | Their change that could matter | How sure |
|---|---|---|
| Intermittent missing / black faces, wet-looking skin | (a) **Bake sources**: a bake that ran while a source showed a stand-in bakes that in "for good". (b) **Static samplers out of the library**: a loaded pipeline may sample through another filter, address mode or comparison; which pipelines are loaded and which are made differs per run. | Candidates only. Both fit "differs from run to run with the same settings" in `faces-by-run.md`. Neither was tied to our shots. |
| Purple patches lingering some seconds | None as a fix. `TABLED_REREAD` and `PRELUDE_UPLOADS` shorten how long a slot shows an old or stand-in texture. Their own purple answer is the `VICTORY_AMBIENT` stand-in. | Low. |
| Meshes stretching 1-2 s | None. | — |
| Dot clusters on far vegetation | None. | — |
| NaN from `CS_DownsampleFarDistanceTargetDepth`, `CS_ScreenSpaceBentConesCheckerboard` | None. The fork has no NaN guard and no mention of either program. | Verified by search. |

Port effort for the two candidates: static samplers about 25 lines of code (`pipeline/library.rs`,
`pipeline/host.rs`; our hunk in `library.rs` `pipeline()` is nearby but small). Bake sources 430 lines
(`draw.rs` +292 in `impl State`, `resource.rs` +50, `texture.rs` +22, then +76 and +13); medium, our `draw.rs`
is heavily changed but the bake path is its own function.

### Top 5 to port, by expected gain / effort

| # | Candidate | Bottleneck | Expected gain | Effort | Why this rank |
|---|---|---|---|---|---|
| 1 | Queue marks + sort outside the lock (2ad55e6, 2de963d) | 2 | Medium; theirs 5.6 ms gameplay with the batch commit | ~250 lines, clean | Behaviour-neutral, on by default there, has an off switch, code we did not touch. Measure the lock wait first. |
| 2 | Static samplers out of the pipeline library (7fa0e95, code part only) | 4 | A correctness fix; may remove run-to-run shading differences | ~25 lines | Cheapest item in the fork. Cost: those pipelines are compiled each run (driver cache helps). |
| 3 | `WC_NO_BACK` with `MIN_KB=1024` (50c15a1 without `WC_BAR`, 2f81d0c) | 1 | Theirs -5 ms gameplay, -11 ms cutscene median | ~110 lines, clean | Large for the size. **Must stay off until fixed frames match**: it can return stale bytes to the title. |
| 4 | Card copies from a list + wake only needed threads (441446d, 8c1fa5d) | 1 | Small, ~5 % of their submitting thread | 58 lines, clean | Free and neutral. |
| 5 | `SOFT_VIEWS=copies` (f12996c + fb838e8), re-built on our books | 3 (and 1) | The largest: their page-mapping copies 35.1 s -> 2.0 s per 140 s | ~1,360 lines, every core mapping function conflicts | Highest gain, lowest gain / effort. The only thing of theirs that removes the calls our freezes sit in. |

Next in line: bake sources (picture, 430 lines), exact refresh without waiting (69 lines), `NATIVE_1440P`
(198 lines, see D), `SPANS_QUICK` (76 lines).

---

## C. What lowers the picture against the console's, and what fixes we lack

### C1. Launcher settings and defaults that lower or change the picture

| Setting in their launcher | Exactly what it does | Effect against the console's picture | Verdict for us |
|---|---|---|---|
| `VICTORY_LEAVE_OUT=02384a60fda6f2d0,731a39171b568298` | Pipelines of these two shader hashes are never made; their draws and dispatches are dropped (`F: pipeline/make.rs:59-80`). First: `CS_DefragHeap_ComputeRelocations`, the decal heap's defragmentation. Second: the heat-haze and in-scattering pass. | **Two passes missing.** No heat haze, no distance in-scattering; decal heap never compacted. README: "it smears blocks of pixels, also on water". | **EXCLUDE.** |
| `VICTORY_DLSS=1`, `VICTORY_DLSS_MODE=quality` (only if `sl.interposer.dll` is found) | Patches the title to renderer 7 (FidelityFX path, as Series S) and the scene to 2/3 of the output: **1706x960** for a 1440p output. DLSS replaces the title's own temporal upscaler. | **Lower scene resolution than the console (2560x1440) and another upscaler.** The stock Series X never runs this path. | **EXCLUDE.** |
| `-FrameGen` -> `VICTORY_FRAMEGEN=1` | Generated frames between real ones, through Streamline's swap chain. | Invented frames. | EXCLUDE. |
| `-Res 1080to1440` -> `VICTORY_RENDER_1080P=1` | Scene 1920x1080, scaled to 1440p by the title. | Lower scene resolution. Not the launcher's default. | EXCLUDE. |
| `-SeriesS` | Series S profile: "1280x720 inside". | Lower everything. Not the default. | EXCLUDE. |
| `VICTORY_NATIVE_1440P=1` | Output 2560x1440 instead of 3840x2160; scene stays 2560x1440. See D. | The title's 4K temporal upscale and all 4K passes after it are gone. The interface is drawn at 1440p. | FLAG. A change of output, not of the scene. |
| `VICTORY_AMBIENT=0.8`, `VICTORY_AMBIENT_AIR=1.5` (base code) | Replaces the empty ray-traced diffuse light with an estimate from the last frame's visible light; bounds what the air scatters. | **Stand-in**, not the console's light. Known to overexpose rooms (their 8cf0a75 message). | EXCLUDE unless we choose a stand-in on purpose. |
| `VICTORY_AMBIENT_SPECULAR=1` | Writes share x the estimate into the rays' image after `CS_RayMiss` of opaque surfaces. | **Stand-in.** README: "reflections are blurry, and in cutscenes they can still show the sky's bright colour". | EXCLUDE. |
| `VICTORY_TEXTURE_STAGE_MS=16` (default 250) | The texture pass stops after 16 ms a frame. | Textures arrive later: stand-ins shown longer. | FLAG. Speed for pop-in. |
| `VICTORY_TEXTURE_CATCHUP=0` (default on) | A backlog of 1,024 or more due slots no longer gets three times the bytes and time. | Longer black or stand-in surfaces after a scene change. | FLAG. Keep our default. |
| `VICTORY_WC_NO_BACK=1`, `_MIN_KB=1024` | Shader writes into write-combined copies of 1 MB or more never reach title memory. | No loss by design. Risk: the title's CPU reads old bytes; a page the CPU writes takes back shader writes in the same 4 KiB. Without the minimum their cutscenes "went near black" (exposure read back stale). | FLAG as a risk, test with fixed frames. |
| `VICTORY_RIPE_BLOCKS=0` (default 12) | New texture blocks are used at once, none made ahead. | Not a picture setting. The default exists against device loss; they cover that with `SHOW_NEW_LATER`. | Keep our default. |
| `VICTORY_SHOW_NEW_LATER=1`, `VICTORY_SHOW_AFTER=1` | A slot shows its new texture one or more submissions later. | A texture appears slightly later. | Neutral to slightly later. We set `SHOW_AFTER` too. |
| `VICTORY_TEXTURE_NO_WAIT=1` | A texture that arrives during a remap "may show a frame later". | Same as ours. | Same. |
| `VICTORY_THIN=1` (base) | List calls only write down their arguments; our threads translate them. Speed only by design. | None by design. Our notes record picture faults with it elsewhere. | We do not set it. Keep. |
| `VICTORY_LEVELS_ON_DEMAND=0` | Same as ours. | — | Same. |

Not set by their launcher, named in the request:

| Switch | What it does (verified in the fork's code, all base code) | Picture |
|---|---|---|
| `VICTORY_SKIP_PIPELINES` | As `LEAVE_OUT`, in diagnostics builds only (`F: pipeline.rs:505`). | Drops passes if set. Not set. |
| `VICTORY_LAND_ECHO` | On by default: what a landing wrote into title memory is not sent up again; `=0` sends it. | Speed only. |
| `VICTORY_UNDER_WIDE` | On by default: pages under flights' writes are compared on several threads. | Speed only. |
| `VICTORY_TITLE_PRIORITIES` | On by default: the title's "idle" threads run at "below normal"; `=0` as the title sets them. | None; streaming speed. |

Defaults the fork changed that touch the picture, without a switch being set: none that lower it. The wave
"meetings" (4f49709) poll at most 4,096 times and then go on with wrong values; they apply to two listed
programs, one of which the launcher leaves out anyway.

Also: the fork's base has **no NaN guard**. Ours has one on colour exports. Their tree is less guarded there.

### C2. Picture fixes in the fork that we lack

| Fix | What it is | Could it explain a defect of ours? |
|---|---|---|
| Static samplers out of the pipeline library (7fa0e95) | Verified in code. A stored pipeline keeps the places its static samplers had in the driver's table in the run that stored it. Loaded in a run that made static samplers in another order, it samples through other ones. | **Candidate** for anything that differs between runs: wrong filter (posterized grading), wrong address mode, wrong comparison sampler (shadows, which could read as wet or black skin). Their proof is for `PS_CompositeNMB` only. One-run test without a port: start with an empty pipeline library and compare. |
| Bake sources brought up to date (b3d49db +2) | Verified in code. Before each `PS_BakeDecorations_*` / `PS_DecorationBlend_*` draw, the slots its constants and 304-byte records name are checked and uploaded on the bake's list. | **Candidate** for intermittent black faces, dark patches over eyes, blotchy skin: a bake that read a stand-in keeps it. Our `BAKE_TEXTURES_MS=10000` gives the pass time, but the pass still skips slots it served before and refuses copies while the card has no room. |
| Views over host-written textures in bakes (2983a71) | R9G9B9E5 views the title places in R32_UINT textures the host writes are copied on the bake's list. | Part of the above. |
| `PRELUDE_UPLOADS` | A texture found changed at the submission check is uploaded in this submission, not by the next list. | Candidate for one-frame-old textures (video planes, last streamed level). Not for defects lasting seconds. |
| `TABLED_REREAD` | A slot rewritten after its frame's first table read is made by the next draw, not by the pass 8 frames later. | Weak candidate for flicker at scene loads (0.27 s at 30 fps). |
| "A slot shown after its upload uses the view read at its turn's start" (13dca45) | A lock saving. Behaviour-neutral by its own message. | **No.** Not a picture fix. |
| Stencil-plane and placed-view fixes (d226814, bda6e59) | Inside the light stand-in's lookup only. | **No**, unless we use `VICTORY_AMBIENT`. Then they matter: without them the estimate read a stencil plane as depth. |
| Volumetric light programs (eeb17cc) | Their route to translate two tiled volumetric programs on 32-lane hosts. | Probably not: our paired form covers wave64 programs and run-197 dropped nothing but ray tracing. Worth one check that `CS_TransmissionVolumetrics_Tiled*` run in our logs. |
| Early depth (41b4a05) | Same as ours. | We have it, on. |

---

## D. The NATIVE_1440P patch

Verified in `F: apps/victory/src/native_1440p.rs`, `F: apps/victory/src/run.rs:89-131`,
`F: crates/victory-gpu/src/draw.rs:4831-4880`.

What the stock title does on Series X: it presents 3840x2160. The scene is drawn at 2/3 of that, 2560x1440, and
its own temporal pass scales it up.

What the patch changes, in the image before it is mapped (four fields):

| Field (RVA) | Stock | Patched |
|---|---|---|
| SDR display mode (0x0615c100) | 3840, 2160, 60, 1 | 2560, 1440 |
| Scanout reservation (0x0247ad4a) | `mov ecx, 0x04000000` (64 MiB) | size of two 2560x1440 RGBA8 buffers, rounded to 2 MiB |
| Temporal scene scale (0x028f1e7e) | 2/3 | **1.0** (or 0.75 with `RENDER_1080P`) |
| Settings viewport load (0x058048d4) | loads the legacy 3840x2160 size | loads the selected SDR mode |

Plus, at run time:
- `VICTORY_DISPLAY_MODE=2560x1440` is set for the display mode list.
- One draw is corrected: the frontend's last triangle (vertex `0602c2384acbb3b2`, pixel `a78d088fa55aa495`).
  Where its constants hold the legacy 3840x2160 projection, the viewport is set to that canvas so the result
  lands on the 1440p target.
- The patched image is cached in its own folder (`images/native-1440p-sdr`), beside the stock one.

So:
- **Output size: yes, 4K -> 1440p.**
- **Scene size: unchanged, 2560x1440** (1.0 x 1440p equals 2/3 x 2160p). G-buffer, lighting, shadows and the
  scene's post passes keep their size.
- **What shrinks:** the temporal pass's output and every pass after it that works at output size (the title's
  upscale itself becomes a same-size temporal pass, then output-size post, the interface target and composite,
  the scanout buffers). 2.25 times fewer pixels in those.

Build-specific: **yes.** It refuses any image whose BLAKE3 hash is not
`b99f876b74d4a77f03af5f878223b27bd647b5c35a79d5decd9f716504239e93` and checks each field against the stock bytes.
**Our image has that hash** (our `victory-data\bench\run-29-f1-meter\plain.txt`, line 2). So it would apply.

What it would save:
- Their figure: "median frame 77 -> 67 ms, 95th percentile 506 -> 401 ms" on the 12 GB card, with volumetric
  light switched on in the same comparison. Not a clean number.
- For us: card time in the output-size passes and card memory for the 4K targets and scanout. I could not
  count the 4K targets from code. It saves **nothing on the submitting thread or the recording threads**, which
  are our limits. It may help the memory pressure on the card (our card ran over its budget in earlier runs).
- Picture: on a 1440p display the 4K image is scaled down by us anyway, so the visible loss is the title's
  4K reconstruction averaged down (slightly more stable edges) and a 4K-drawn interface. On a 4K display it is
  a real loss.

Port effort: easy. One new file (115 lines for `prepare`), 24 lines in `run.rs` (ours differs by 10 lines),
59 lines in `draw.rs`, one dependency line (`victory-formats`, a workspace crate). Do not take
`prepare_upscaler` (the DLSS path).

---

## E. Safety

**Verdict: nothing malicious found by reading.** No download code, no network code, no new crates.

| Item | Finding |
|---|---|
| Non-source files in the tree | `LICENSE`, `.gitignore`, `.gitattributes`, `.victory-root`, `.githooks/pre-commit`, `.github/workflows/ci.yml`, `Play-VIctory.cmd`, `scripts/play.ps1`, `crates/victory-formats/src/bcpack-tables.bin`: **all unchanged from the base**. New: `scripts/play-foxhound.ps1`, `scripts/fullscreen.ps1`, `scripts/faultbook.py`. |
| Binaries | None added. The only binary file is the base's `bcpack-tables.bin`. No `.exe` or `.dll` in the tree. |
| Manifests | `Cargo.lock` +1 line and `apps/victory/Cargo.toml` +1: `victory-formats`, a crate of the workspace. `victory-gpu/Cargo.toml`: two more `windows-sys` features (`Win32_Security_Cryptography`, `Win32_Security_WinTrust`). **No new external crate, no git dependency, no build-dependency.** All `build.rs` files unchanged. |
| `scripts/play-foxhound.ps1` | Read in full. Clears `VICTORY_*` variables of the session, sets the switches, makes `%LOCALAPPDATA%\victory\logs`, writes a log there, starts `target\<build>\victory.exe run <exe> --console ... --execute`. Tests for `sl.interposer.dll` in a folder. No download, no registry, no deletion. |
| `scripts/fullscreen.ps1` | Read in full. Compiles a small C# P/Invoke class (`Add-Type`, user32 only), finds the `victory` process window, makes it borderless and monitor-sized. No network, no files. |
| `scripts/faultbook.py` | Read: standard library only (`argparse`, `bisect`, `collections`, `struct`, `sys`); reads the files named on the command line. No network, no process. |
| Network in source | **None added** (searched added lines for WinHTTP, sockets, URLs, HTTP crates). The only URLs are in comments and README. |
| DLL loading | New: `LoadLibraryExW` of `sl.interposer.dll` from `VICTORY_STREAMLINE_DIR` (default `C:\dev\sdk\streamline-sdk-v2.14.16\bin\x64`), **only when `VICTORY_DLSS=1` or `VICTORY_FRAMEGEN=1`** (`F: streamline.rs:300-323`, `:514`). Before loading, `WinVerifyTrust` checks the Authenticode signature offline. It accepts **any validly signed file**, not NVIDIA's signer in particular. Streamline then loads its own plugins from that folder. |
| Process spawning | Only in `examples/lut3d_probe.rs`: runs `dxc.exe` (path from `DXC` or the Windows SDK) and writes an `.hlsl` file and a library file. Examples are not built by `cargo build -p victory`. |
| Patching the title | (a) Image bytes before mapping: `native_1440p.rs` (1440p, upscaler path), each checked against stock bytes. (b) In memory at load: `victory-loader/src/frame_rate.rs` with `VirtualProtect`, only with `VICTORY_FRAME_INTERVAL_MS` set. (c) Import hooks: the title's `amd_fidelityfx_loader_xbox.dll` imports bound to the fork's functions when Streamline is available; the title's `VirtualQuery` / `VirtualProtect` imports bound to the fork's when `VICTORY_SOFT_VIEWS` is set. All patch the game, not Windows. |
| Hand-written assembly | `F: crates/victory-gpu/src/safecopy.rs` (`global_asm!`): a fault-safe compare-and-copy, used by `SYNC_IN_PLACE`. Read at hunk level only, not instruction by instruction. |
| File writes | Logs; the patched image copy in the cache folder (the base's loader already writes the image copy); `VICTORY_STREAMLINE_LOG` folder if set. `VICTORY_KEYS_FILE` is read, not written. |
| Registry | None added. |
| New subcommand | `victory upscaler-check [--device] [--present]`: loads Streamline and makes a device and swap chain. Only if run. |
| Obfuscation, encoded blobs | None seen. Byte arrays in `native_1440p.rs` and `frame_rate.rs` are short x86 patterns with comments. |

Not checked: the history before the base (same as ours by tree hash), and whether any published binary of the
fork matches this source (none is in the repository).

---

## Doubts

- Every gain is theirs: 12 GB card, `VICTORY_THIN=1`, mostly DLSS Quality at 1706x960. Our limits are CPU threads
  at a 2560x1440 scene; their GPU-side gains will not carry over, their lock and system-call gains should.
- Their own notes say "not yet measured in the game" for the heap-sync switches and the submitting-thread
  commits, "not yet measured fairly" for the frame-rate switch, "not yet confirmed in a run" for early depth, and
  no in-game gain for `FAULT_UNLOCKED` and `GUARD_HOT`.
- The queue-marks figure (39.5 -> 33.9 ms) is one A/B chain with two commits on together.
- The `SOFT_VIEWS` baseline is noisy (gameplay 60.1 / 72.6 / 35.6 ms with the switch off).
- I compared which functions both sides changed, not every line. "Clean" means no overlapping hunk headers,
  not a tested merge.
- Whether bake sources or the static-sampler rule explain our faces is a reading of the mechanism, not evidence.
- The clone has a shallow boundary (4 entries in `.git/shallow`), but the base commit and all 96 commits after
  it are present.
