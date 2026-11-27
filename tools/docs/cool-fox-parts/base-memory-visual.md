# cool_fox review — avatarmanz BASE: memory management and visual features

Read-only review of `C:\GTAVI\Tools\ref-forks\cool_fox` at HEAD `f4b48ad`, the 389 commits `a26859f..0a43566`
(avatarmanz/VIctory-7900XTX, author "local", RX 7900 XTX 24 GB, i9-12900K, 64 GB, 1440p, Series X profile). Nothing
built or run; git show/log/grep and file reads only. "Ours" = `C:\GTAVI\Tools\victory-dev\crates` (grep-checked on
2026-11-26; line numbers drift, other agents edit it). The six codec_d commits are in `host1.md`, `host2.md`,
`host3.md`, `translator.md` beside this file; where a base item was changed by them this part says so.

Text addressed to AI assistants: **none found** in the files read. (README.md:123 says the fork was "made with
Claude Code"; commit trailers name "Claude Opus 5.5" as co-author. Neither is an instruction.)

Their card has 24 GB and their PC 64 GB (README.md:27-39: "40-46 GB of private memory: 64 GB of RAM recommended").
**Every keep-longer rule of theirs was tuned with 7-9 GB of card memory free.** Ours runs at 14.4 of 15.1 GB. Items
that spend memory to be safe (`KEEP_FREED`, 64 spare blocks, `RIPE_BLOCKS` 12 blocks = 768 MB ahead) cannot be taken
at their defaults.

Verdict words: **port** (exact, no picture trade), **A-B** (switchable test, reason given), **skip**.
Bounds: (a) recording, (b) submit thread, (c) card, (d) streaming freezes / VRAM / textures not ready.

---

## A. cool_fox memory management

### A1. The six most valuable items that do not trade picture (summary; port-ready entries in section C)

| # | Item | Their file:line, commit | Their number | Ours | Size / risk | Bound |
|---|---|---|---|---|---|---|
| P1 | Small textures placed at 4 KiB, not 64 KiB, inside texture heaps | `pool.rs:510-520`, 9fea743 | allocation 310 µs → 6 µs a texture (pool as a whole); no VRAM figure | no (0 hits `SMALL_RESOURCE_PLACEMENT`; `resource.rs:3732` asks with the title's alignment) | S (12 lines) / low | (d) VRAM |
| P2 | Host copy of a placed texture starts at the first level a view shows (`HELD_LEVELS`) | `resource.rs:1240-1367, 1577-1614`, `texture.rs:735, 1836-1870`, `descriptor/host.rs:4242-4245`, 8d79927 | "the levels some slot shows are 1.2 and 1.6 GB of the 6.3 and 6.7 GB of such host copies" (two recorded world frames, 7900 XTX, Series X) | no (0 hits `base_level`, `want_level`, `held_only`); ours reads from the first mip (`VICTORY_TEXTURE_FIRST_MIP`, `texture.rs:106`) but still makes every level on the card unless the texture is in levels-on-demand | M-L (~450 lines of 866, 4 files) / medium | (d) VRAM, churn |
| P3 | New card memory waited for until mapped; objects let go counted against lists not yet sent, and buried late | `host.rs:209-351` (0322b39, f1d3eb2), `host.rs:462-519` (a0b4909) | device losses n08, n11, n15 = page faults in a texture made the same tenth of a second; none since | no (0 hits `CREATE_NOT_RESIDENT`, `MakeResident`, `bury_lag`; `host.rs:267-295` drops at once when nothing is in flight) | S (~130 lines, 3 files) / low | device loss; unblocks vram-plan #2 and #7 |
| P4 | Refresh copies the 4 KiB parts of a page that were written, not the 64 KiB page | `watch.rs:41-93, 127`, `shadow.rs:2619-2650`, 3ead56e; `watch.rs:784-800`, e8d3d91 | "29 MB a frame written so were most of 114 MB copied, and as much sent up and hashed again on the card" | no (0 hits `PAGE_PARTS`, `parts_written`; `watch.rs:157` folds the kernel's 4 KiB answers into 64 KiB pages) | M (200 + ~40 lines, 3 files) / medium | (b) |
| P5 | Texture change test with a 128-bit folded-multiply digest instead of blake3; one-shot staging asks no guards; an upload equal to what the copy holds is left out | `texture.rs:684-730` + `675-679` (2dfba4d), `texture.rs:1618-1648` (8d79927) | digest "0.035 against 0.2 ms a megabyte"; "of 787 repeated uploads of a frame that streams 398 were equal in every byte" | no (blake3 at `texture.rs:451, 1613, 1635, 1657, 1707, 1726`; 0 hits `fn unchanged`, `TEXTURE_ONCE`) | S (digest 50 lines) + M (same-bytes 120 lines) / low | (b), (d) |
| P6 | "How much of a texture is mapped" read from the runtime's own record of views, not `VirtualQuery` per 64 KiB | `inplace.rs:450-468`, `texture.rs:1770-1785`, c27a8ec | 146 µs → 1.4 µs (22 MB texture all there), 721 µs → 0.1 µs (1 MB of it there); 12,000 slots wait for memory during a load | no (0 hits `shown_bytes`; ours asks one page per 64 KiB block, `texture.rs:2122-2160`; `views()` exists at `inplace.rs:283`) | S (30 lines) / low | (a), (d) texture pass time |

Next in value, need an A-B: the room rule `vram_keep` (A4), first uploads on a copy queue (`UPLOAD_AHEAD`, A3).

### A2. RAM / VRAM budgets — theirs against ours

| Budget | cool_fox (file:line) | victory-dev | Note |
|---|---|---|---|
| Texture block size | 64 MiB (`pool.rs:30`); pieces over half a block (a whole block with ripe blocks) get an allocation of their own | 256 MiB heaps, pieces up to 32 MiB (`resource.rs:3273, 3737`) | ours packs better; theirs frees empty blocks sooner |
| Empty blocks kept | 2, or 64 (= 4 GiB) while card use < 85 % and `KEEP_FREED` (`pool.rs:32-34, 66-71`); staging blocks 8 | empty heap goes after `empty_heap_ms`, 1 s past the low mark (`resource.rs:3303`) | theirs would cost us headroom we do not have: skip |
| Rest before a freed piece is reused | 2,500 ms (`pool.rs:76`: "Windows resets a card that takes that long over one submission") | range returns when the last holder goes and flights land (`sweep_leaving`, `host::retire`) | theirs is time, ours is fences; ours is tighter but see P3 |
| Texture copies let go | none younger than 10 min while NET use < 88 % of budget; 88-95 %: idle 5 s, only enough bytes to reach 85 %, longest idle first; ≥ 95 %: idle 1.5 s (`host.rs:152-167`, `resource.rs:2034-2063`). NET = system use − free bytes in our blocks + room kept for buffer copies and targets (`resource.rs:2040-2042`) | early rule on RAW > 88 % / off < 82 %, idle 3 s / 60 frames, 128 MB a frame; fallback ≥ 256 MB, idle 1 s / 4 frames, sparsest heap first (`resource.rs:2034, 1951, 2026, 2393`; vram-plan.md §3) | theirs is the shape vram-plan.md #1 asks for (NET, bounded bytes, oldest first) |
| Copies of unmapped textures | slot made null 3 s after it was first found away (`descriptor/host.rs:2171-2199`, `VICTORY_TEXTURE_AWAY_MS`); the copy itself goes only by the rule above | exact unmap → next sweep; slot kept 10 s, cap 120 s (`stale_kept`, `descriptor/host.rs:1684-1745`) | ours follows the game's release signal; theirs does not need to on 24 GB |
| Title-buffer card copies | room kept 1 GiB at most (1f1a433: "was 5 GiB on 24 GB") | `VICTORY_CARD_BUFFERS_MB` 1,024 | same |
| Memory by address (ray structures) | 4 GiB on the card (`shadow.rs:5184-5200`, `VICTORY_MEMORY_CARD_MB`), 3 GiB of pages (`memory.rs:88`), arenas of 64 MiB (`memory.rs:84`), stops at 97 % of budget | old form, 4 MiB windows (`memory.rs` 322 lines) | only with the builder on; 4 GiB does not fit our card — see B1 |
| Windows of title memory | idle 5 min with ≥ 12 GiB RAM free (`host.rs:382-387`) | idle 20 s, 5 s under 3 GB free (`VICTORY_WINDOW_IDLE_S`) | theirs is RAM for safety; skip |
| Windows on the card over the bus | `VICTORY_WINDOWS_BAR_MB`, 0 (`shadow.rs:472-490`): `D3D12_HEAP_TYPE_GPU_UPLOAD`, no CPU side | no (0 hits `GPU_UPLOAD`) | A-B, see table A6 |
| Retired objects | dropped 64 submissions late (16 on ≤ 32 GiB RAM, codec_d) (`host.rs:497-503`) | at once / when flights finish (`host.rs:267-295`) | P3 |
| Host RAM guard | `residency.rs` (201 lines, codec_d e05d463) | RAM only | host1.md H1-5, host3.md H3-8 |
| Process private memory | 40-46 GB: lists' allocators 9 GB, windows 7, uploads 4 (findings.md:95-96) | 23 GB committed (game-render-memory-model.md §4) | ours is leaner |

### A3. Texture streaming, end to end

Their path for a texture picked by index (the bulk: ~24,000 slots when the world loads, `pool.rs:8-10`):

1. **Slot written** by the title (placed view, no resource object). The texture pass (`draw.rs` `heap_textures`,
   budget `VICTORY_EARLY_TEXTURES_MS` 12 ms, `draw.rs:165-185`) looks at due slots. How much of the texture is mapped:
   `texture::mapped_bytes` (`texture.rs:1770`) → `inplace::shown_bytes` (P6).
2. **Host copy made** from the first level some view shows (`resource.rs:1577-1590`, P2), cut from a 64 MiB block at
   4 KiB alignment for small ones (`pool.rs:497-560`, P1), from a block every list on the card has left behind
   (`VICTORY_RIPE_BLOCKS`, `pool.rs:87-125`), the block waited for until mapped (`host.rs:247-319`, P3).
3. **Staging**: a piece of a 64 MiB mapped upload block (`pool.rs:382-410`), detiled from title memory. Made for one
   upload, it asks neither write watch nor guards (`VICTORY_TEXTURE_ONCE`, `texture.rs:669-679`). If the runtime is
   remapping, the read goes through the kernel and takes the pages that are there (`VICTORY_TEXTURE_NO_WAIT` off by
   default there, `VICTORY_TEXTURE_FILL` on, `texture.rs:350-398`).
4. **Upload**: recorded into the pass's list (`texture.rs:735-882`), or, with `VICTORY_UPLOAD_AHEAD=1` (off;
   launcher of codec_d sets it), handed to two threads with a copy queue of their own (`texture/ahead.rs:186-283`):
   read, detiled and copied in batches of 16 MiB / 64 textures, at most 96 MiB on their way (`ahead.rs:35-37`).
   Only a texture no list has used: made in the last second, common state, never uploaded (`ahead.rs:96-120`).
5. **Slot shows it**: at once (default), or with `VICTORY_SHOW_AFTER=1` only when the submission with the upload has
   finished; a 1x1 grey texture meanwhile (`descriptor/host.rs:284-293, 314-425`), or what the slot showed before if
   it "can stand for it" (`VICTORY_KEEP_SHOWN` 1: same kind in another place or size; 2: same dimension;
   `descriptor/host.rs:4443-4487`).
6. **Change later**: guarded textures are hashed every eighth frame, every frame for 64 frames after a change the
   guard did not see (`VICTORY_TEXTURE_HOT`, `texture.rs:117-134`), next frame after an unnoticed change
   (`VICTORY_RECHECK_SOON`, `draw.rs:205-210`). An upload whose bytes equal what the copy holds is left out
   (`VICTORY_TEXTURE_SAME`, `texture.rs:1618-1648`). DirectStorage completion tells the GPU side
   (`xmem.rs:944-962`, `VICTORY_STREAM_NOTICE`).
7. **A view asks for a larger level**: `want_level` replaces the copy by one with more levels; slots go on showing
   the old one until the new upload has run (`resource.rs:1342-1367`, `descriptor/host.rs:258-282`).

Compared with ours (game-streaming-triggers.md §2, vram-plan.md §3):

| Step | cool_fox | victory-dev | Who is ahead |
|---|---|---|---|
| Start of an upload | at the look of the pass; DirectStorage completion re-queues | at map-in and at the write (`VICTORY_UPLOAD_ON_WRITE`, on; `watch.rs:1845`, `descriptor/host.rs:4881`); catch-up pass (`VICTORY_TEXTURE_CATCH_UP_MS` 2) | ours |
| Release | room rule only | game's unmap, exact (`VICTORY_UNMAP_EXACT`), then room rules | ours |
| Levels on the card | first viewed level on, plain smaller texture (P2); tiles (`VICTORY_LEVELS_ON_DEMAND`) exist but are off and switch P2 off (`resource.rs:1241-1245`) | tiles for 2D ≥ 1 MiB (`resource.rs:1421-1440`), everything else whole chain | **theirs for everything our tiles exclude** |
| Small-texture placement | 4 KiB (P1) | 64 KiB | theirs |
| Upload engine | pass's list, or copy queue + 2 threads (`UPLOAD_AHEAD`) | pass's list on the direct queue; `queue2.rs` lanes carry buffers only (`queue2.rs:30-34`) | theirs, optional |
| Hash | folded-multiply digest (P5) | blake3 | theirs |
| Equal upload skipped | yes (P5) | no | theirs |
| "Is it mapped" | view record (P6) | `VirtualQuery` per block | theirs |
| Unfilled pages while remapping | read around, zeros not put over a filled level (`TEXTURE_FILL`) | `VICTORY_TEXTURE_READ_AROUND` on (`texture.rs:471-480`) | equal |
| Changed behind the guard | `TEXTURE_HOT` | `VICTORY_TEXTURE_UNSEEN` on (`texture.rs:463-468`) | equal |
| Slot whose texture moved | `KEEP_SHOWN` by kinship of old and new view | `stale_kept` by time (10 s, cap 120 s) | equal in effect; theirs holds less VRAM (bounded by the new upload, not by a timer) |
| Bake | `EARLY_TEXTURES_MS` 12, `BAKE_TEXTURES_MS` 1500, `BAKE_EXACT` | first two ported (`draw.rs:411-428`), `BAKE_EXACT` no → host3.md H3-1 | host3.md |

**What of theirs would help "textures not ready when bound" without delaying data:**

- P6 and P5 make the texture pass and each upload cheaper, so more of the due queue (ours: max 8,440 due, 3.16 % of
  bound full-detail textures "not uploaded", vram-plan.md §4) is served per frame. Exact.
- P1 and P2 give room: 11,741 passes "made none for lack of room" (run-90) and 283-849 "held level without memory"
  are refusals; less VRAM per copy = fewer refusals and fewer re-makes (7.5 GB a run).
- `UPLOAD_AHEAD` (A-B): the first upload leaves the direct queue and the submit thread. It does **not** delay data
  if the direct queue waits for the copy queue's fence before the first list that names the texture (ours has that
  primitive: `queue2.rs` `Second::hold`). As written in cool_fox it relies on `SHOW_AFTER` flips instead, which do
  delay (below).
- **Not acceptable as they stand (later data):** `VICTORY_SHOW_AFTER` shows a grey 1x1 texture until the submission
  with the upload has *finished* ("{:.2} frames after they were asked for", `descriptor/host.rs:2511`;
  `ahead.rs:5-7`: "black or grey in that frame, and in the next"). It is a device-loss guard, not a speed-up; P3
  removes the same fault at its cause without the grey frames. `VICTORY_TEXTURE_AWAY_MS` / `KEEP_SHOWN` show
  an older texture in place of none — ours already does this (`stale_kept`).

### A4. Eviction and idle: the rule worth an A-B

`host::vram_keep` (`host.rs:152-167`) + `resource::let_go_idle` (`resource.rs:2016-2138`), 8d79927:

- candidates: copies of textures the title fills, held by nothing (no list: `Arc::strong_count == 1`; no slot:
  `references == 1`), idle `idle_ms` and 4 frames;
- with a byte limit: sorted longest idle first, stop when `bytes >= most` (`resource.rs:2052-2063, 2082`);
- without a limit and long idle times: 8,192 textures a look, the next 8,192 at the next
  (`resource.rs:2067-2088`; "a play has over 100,000"), 2dfba4d.

Their numbers: before, "88,000 of them (31 GB) went after 5 s with 7 GB of the card free, and 24,700 (12 GB) had to
be made and uploaded again" (session play-1125-0127); at 78-80 % "61,000 ... in the first 300 s ... of which 13,000
were made and uploaded again" (play-1125-0203). No after-figure is given.

For us this is vram-plan.md #1 with tested constants (88 / 95 / 85 % of NET). Difference to keep: ours must keep the
game-unmap release in front of it, and `targets_room_kept`. Verdict **A-B** (`VICTORY_TEXTURE_KEEP_NET=1`), proof
meters as vram-plan.md #1: fallback count, made again, "made none for lack of room", raw peak ≤ 14.4 GB.
The sweep-in-parts idea (8,192 a look) is a free speed gain for our once-a-second sweep spike ("sweep" in the
prepare line, `host.rs:238-258`): **port** with it.

### A5. Write tracking, page mapping, DirectStorage — compared

| Mechanism | cool_fox | victory-dev | Verdict |
|---|---|---|---|
| Pages mapped in runs | 4180818 `VICTORY_MAP_RUNS` — **gone at HEAD** (0 hits; replaced by groups of 64 pages on up to 8 threads, 565f4a5, `xmem.rs:1244-1320`) | `VICTORY_MAP_FAST` runs (`mapfast.rs`, `queue.rs:1477`), `VICTORY_REMAP_SAME` | ours has it |
| No split of own placeholder, no commit of committed pages (`VICTORY_MAP_LEAN`, `xmem.rs:447-470, 1040-1054`, 49d7126) | per-section bit per committed page | `MAP_FAST` joins placeholders; no committed-bit record (0 hits `commit_shown`) | already judged low (shortlist line 177); the committed-bit half is S and exact: **A-B** only if the map meter (`mapfast.rs:267` "split ms") still shows commit calls |
| Map on many threads (`VICTORY_MAP_THREADS`, 8; `auto`) | "16 threads map no more pages than one, and the groups hold the mapping lock: 260 ms a frame against 125" (dorm, 4e99544) | no | already rejected, see fork-review-shortlist.md line 177 |
| Streaming thread lifted above normal (`VICTORY_STREAM_PRIORITY`, on; `xmem.rs:1056-1075`) | title sets its one loading thread below normal | no (0 hits; `VICTORY_PROCESS_PRIORITY` / `TITLE_PRIORITIES` reviewed) | speed pass; note the game fact |
| One kept view per page section for DirectStorage writes (`VICTORY_SECTION_WINDOW`, `xmem.rs:392-443`, 9fea743) | per-page committed bits | `VICTORY_DSTORAGE_LINEAR_VIEW` on (`xmem.rs:550-578`) | ours has it |
| GPU side told when streamed bytes land (`VICTORY_STREAM_NOTICE`, `xmem.rs:944-962`, `dstorage.rs:1609-1616`) | `kernel_will_write` | `pages_written` (`xmem.rs:1116`) + `UPLOAD_ON_WRITE` | ours has it |
| Queue starts at the title's Submit, not at half capacity (`VICTORY_DSTORAGE_HALF`, off; `dstorage.rs:1293-1308, 1345`, 68e10e5) | 2,079 requests ran on stale page lists "three or four times a load" | `VICTORY_DSTORAGE_PAGES_AT_SUBMIT` on (`dstorage.rs:1019-1030, 1435-1455`): same finding, same addresses | ours has it |
| File handle per worker (`VICTORY_DSTORAGE_IO`, `handles`; `dstorage.rs:283-310`, fbc1830) | 185 → 990 MB/s (48 KiB random, levels.rpf); modes `overlapped`, `direct` | `VICTORY_DSTORAGE_OWN_HANDLES` on (`dstorage.rs:269-277`) | ours has it; `direct` (no file cache) untested by them: skip |
| Page look-ups do not wait for mapping calls (fbc1830: locks per page table apart from the mapping lock) | "2.7 million requests a second for 0.57" | meter exists ("page-table lock wait", `dstorage.rs:1806`) | speed pass: check that meter first |
| Staging floor 256 MiB (`VICTORY_DSTORAGE_STAGING_MB`) | codec_d e05d463 | no | host1.md H1-4 |
| Streaming log / trace (`VICTORY_DSTORAGE_LOG`, `_TRACE`; `dstorage.rs:709-760, 1047-1058`) | per request R/S/P/F lines, per-second map costs | `framestream.rs`, map meters | tooling; skip |
| 4 KiB parts (`VICTORY_PAGE_PARTS`, `_TAKE_PARTS`) | P4 | no | **port** |
| Holes of view memory known by bits (`VICTORY_KNOWN_HOLES`, `watch.rs:1060-1072`, 74045e4) | "111 ms a frame of all threads' time went into waiting for that lock" | `placeholders_told` (`watch.rs:1022`) | speed pass to compare; needs their `light()` refresh path |
| Light refreshes (`VICTORY_LIGHT_REFRESH`, `watch.rs:812-826`, 2aa8134) | 94 ms a frame of all threads' time | no | already rejected for speed (shortlist line 177: copies −13.7 ms, frame +7) |
| Early asking of the write watch (`VICTORY_ASK_EARLY=<ms>`, off; `watch.rs:388-403`, 1a411ae; turned off in 3ead56e) | first submission stood 8-10 ms in one call | no | skip: their own default is off |
| Guard over every view of a page | `VICTORY_VIEW_GUARD` | same (`watch.rs:1409-1446`) | equal |
| Title's writes into guarded blocks go in beside the guard (`VICTORY_COPY_BESIDE`, `_STORE_BESIDE`, `titlewrite.rs:70-103`, f19e55b); look-first stubs (`VICTORY_WRITE_SITES`, `titlewrite.rs:268-273`, `victory-loader/src/write_sites.rs:288`, e8d3d91); descriptors copied onto themselves not written (`VICTORY_SAME_RECORDS`, `titlewrite.rs:180-185`) | findings.md:44: "69 ms of threads' time → about 6" | no (0 hits `titlewrite`) | speed pass (patches title code: high risk) |
| Words of the title's image logged each second (`VICTORY_IMAGE_WATCH=name=0xoffset:bytes,...`, `image_watch.rs:21-60`, bd80bbe) | — | no | tooling, S (90 lines): **port** as a diagnostic for our render-size / streaming globals (0x1499fecf8) |
| `VICTORY_READ_WATCH` | **not at HEAD** (only named in `texback.rs:7` as the run that found ~30 places reading the ocean maps) | no | nothing to port |
| `residency.rs` | codec_d | — | host1.md H1-5 |
| `host/place.rs` (`VICTORY_COPIES_IN_PLACE`, on; 350f7f4) | title *buffer* copies done on the card at their place in the list; not a texture heap | no | speed pass (submit order), not memory |

### A6. Unmentioned switches in this scope (from `unmentioned.txt`), one row each

"Ours" is the grep of `victory-dev\crates` for the switch name: all are **0 hits** unless a file:line is given.
Picture: "no" = exact; "later" = shows data later or a stand-in.

| Switch | What + how (cool_fox file:line, commit) | Default | Author's number | Picture trade | Our defect / bound | Port size / risk | Verdict |
|---|---|---|---|---|---|---|---|
| `TEXTURE_POOL` | textures and staging cut from 64 MiB blocks, 4 KiB placement for small ones (`pool.rs:1-18, 82-85, 497-560`; 9fea743) | on | allocation 310 µs → 6 µs; the pass got through 350-500 of 24,000 queued per 250 ms turn before | no | (d) | ours has heaps (`resource.rs:3278`); only the 4 KiB part is missing: S | **port P1** |
| `RIPE_BLOCKS` | 12 blocks made ahead; pieces cut first from blocks every running submission has left behind (`pool.rs:87-125, 217-276`; 3b73cb0) | 12 | losses n08, n11 | no | device loss | S, but 12 x 64 MiB ahead; our 256 MiB heaps → 1 heap ahead | **A-B** after P3 (P3 is the cause-level fix; this is the belt) |
| `HELD_LEVELS` | P2 | on (off with tiles) | 1.2 / 1.6 of 6.3 / 6.7 GB | no | (d) VRAM 14.4/15.1 | M-L / medium | **port P2** |
| `TEXTURE_KEEP` | room rule A4 (`host.rs:141-207`, `resource.rs:2034-2046`; 1f1a433, 8d79927); `=1` older two-time rule, `=0` 5 s | on | 88,000 let go / 24,700 re-made; 61,000 / 13,000 | no | (d) churn 7.5 GB re-made | S-M / raw use past budget | **A-B** |
| `TEXTURE_AWAY_MS` | slot nulled only 3 s after its memory was first found unmapped (`descriptor/host.rs:2160-2199`; 1f1a433) | 3000 | "faces, books, posters and sleeves [black] for 0.3 to 2 s" before | shows older texture | black faces in some shots | ours: `stale_kept` 10 s (`descriptor/host.rs:1684-1745`) | have it |
| `TEXTURE_FILL` | a never-filled staging buffer takes the pages that are there (`texture.rs:379-398, 1425-1490`; 955c8b4) | on | 2,382 of 16,135 textures had zeros in the first level shown; 137 of 787 repeated uploads put zeros over a filled level | no (fixes black) | black textures | ours: `VICTORY_TEXTURE_READ_AROUND` (`texture.rs:471-480`) | have it; **verify** ours never uploads zeros over a filled level (their 137 of 787 case) |
| `TEXTURE_HOT` | guarded texture hashed every check for 64 frames (`texture.rs:115-134`; c5b12f1) | on | boot video cycled through three pictures, a new one every eight frames | no | flicker (videos) | ours: `VICTORY_TEXTURE_UNSEEN` (`texture.rs:463`) | have it |
| `TEXTURE_ONCE` | one-shot staging asks neither write watch nor guards (`texture.rs:669-679`; 2dfba4d) | on | part of "29 to 239 ms a frame, all threads" (run n04) with the no-wait bake read | no | (a), (b) guard churn | S / low | **port** with P5 |
| `TEXTURE_SAME` | upload left out if the copy was filled from the same bytes (`texture.rs:1618-1648`, `Holds` in `resource.rs`; 8d79927) | on | 398 of 787 repeated uploads equal | no | (b), (d) | M / low | **port P5** |
| `BAKE_EXACT` | bake reads title memory exactly (`draw.rs:187-198`; 8d79927) | on | coloured specks on the waitress's face, whole diner scene (run n01) | no | faces | — | host3.md H3-1, H3-2 |
| `RECHECK_SOON` | texture changed unnoticed is looked at next frame, not after four (`draw.rs:201-209`; 7130d52) | on | none | no | pop-in | S (10 lines) / low; ours looks at 4, 12, 40, 120, 400 frames (game-streaming-triggers.md §2) | **port** |
| `MAPPED_BY_VIEWS` | P6 | on | 146 → 1.4 µs, 721 → 0.1 µs | no | (a), (d) | S | **port P6** |
| `UPLOAD_AHEAD` | A3 step 4 (`texture/ahead.rs`, 474 lines; 350c686; changed by codec_d: `ahead.rs:107-108` gates, `drain`) | off (codec_d launcher on) | none (log line gives ms from asking to there) | as written needs `SHOW_AFTER` flips = later; with a queue wait: no | (b), (d) | L (654 lines, 7 files) / medium; needs host3.md H3-7 if bakes are to stay right | **A-B**, own design on `queue2.rs` |
| `KEEP_FREED` | freed card memory kept: 64 empty blocks under 85 %, windows and staging 5 min with 12 GiB RAM free, 1 GiB scratch (`host.rs:353-401`; 7d3e179) | on | "both losses were reads in memory given back seconds before" | no | device loss | spends VRAM/RAM we lack | **skip** (take P3 and `SLOT_HOLDS`) |
| `BURY_LAG` | retired objects dropped N submissions late; objects let go with nothing running wait too (`host.rs:462-519`; a0b4909) | 64 (16 ≤ 32 GiB) | none | no | device loss | S (26 lines) | **port P3**; ours ~30 submissions a frame → use 64 = two frames |
| `WAIT_RESIDENT` | heaps/resources made `CREATE_NOT_RESIDENT`, `MakeResident`, then a 16-byte copy on a copy queue of its own waited for (`host.rs:209-351`; 0322b39, f1d3eb2) | on | "a tenth of a millisecond to some" per wait; n11 was lost with `MakeResident` alone | no | device loss | S (~100 lines) | **port P3** |
| `SLOT_HOLDS` | a dynamic constant-buffer slot holds its scratch chunk (`descriptor/host.rs:3044-3064`; ac34b4d) | on | session n02 lost the device in three freed scratch chunks | no | device loss | S (15 lines) / low | **port** with P3 |
| `MEMORY_CARD_MB` | most bytes of memory-by-address copies on the card (`shadow.rs:5184-5215`; b558934) | 4096 | none | no | unlit areas (builder) | only with B1 | with B1; ours must be ≤ 1024 |
| `MEMORY_DUMP` | arenas written to a folder when a file `go` appears (`memory.rs:1190-1197`; bde604e; diagnostics) | off | — | no | builder debugging | S | with B1 |
| `LENT_HEAP` | a list without a view heap gets the title's biggest (`descriptor.rs:569-600`; b558934) | on | builder dispatches were dropped | no | unlit areas | S (40 lines) | with B1 |
| `UPLOAD_LOW` | list upload memory below 2^37 so 40-bit shader addresses reach it (`upload.rs:1-60`; b558934) | on | — | no | unlit areas | S-M (file of ~150 lines) | with B1 |
| `PLACED_ORDER` | a buffer copy at its place only blocks card steps behind it; submission cut behind it (`host/card_steps.rs:86-104`; 37332f4) | off ("only its tests behind it") | whole submission "stood 40 to 60 ms" (run n19) | no | (b) | needs `CARD_STEPS` | speed pass |
| `COPIES_IN_PLACE` | title buffer copies on the card at their place in the list (`host/place.rs:1-43`; 350f7f4) | on | none | no | (b) | L | speed pass |
| `WINDOWS_BAR_MB` | windows of title memory in card memory the CPU maps (`GPU_UPLOAD` heap; `shadow.rs:472-512`; bd68f5e) | 0 | captured frame: lists 29 → 26 ms of the card, frame 36 → 32 | no | (c), RAM (no CPU side) | M / needs GPU upload heaps (ReBAR; NVIDIA 40-series supports it) and a fifth of the budget free — we never have that | **A-B later**, after VRAM is freed |
| `WINDOWS_OWN` | windows get two sides (`shadow.rs:514-521`; 350f7f4) | off | costs 3.3 GB | no | — | — | skip |
| `CARD_ALL` | every buffer copy two-sided (`shadow.rs:357-376`; 18c0129) | off | — | no | — | — | skip (memory) |
| `SNAPSHOT_POOL` | 1,024 snapshot buffers kept, not 8 (`shadow.rs:1281-1295`; 52e0c03) | 1024 | 11 % of the submitting thread in the opening was `NtAllocateVirtualMemory` | no | (b) | S (10 lines) / RAM | speed pass: **port** candidate |
| `PICKED_WRITES`, `PICKED_MB` | finds buffers shaders write through views picked from the heap by hashing a 32 MB slice a frame (`picked.rs:1-76`; b1da728) | 0, 32 | — | no | silent defects | ours: `VICTORY_HEAP_MARKS` (shortlist line 127) | have another way |
| `PAGE_PARTS` | P4 (`watch.rs:41-93`; 3ead56e) | on | 29 of 114 MB | no | (b) | M | **port P4** |
| `TAKE_PARTS` | same for tracked ranges (`watch.rs:784-800`; e8d3d91) | on | none | no | (b) | S on top of P4 | **port P4** |
| `KNOWN_HOLES` | A5 | on | 111 ms a frame, all threads | no | (a), (b) | M | speed pass |
| `LIGHT_REFRESH` | A5 | on | 94 ms a frame, all threads | no | — | — | already rejected, shortlist line 177 |
| `ASK_EARLY` | A5 | off | 8-10 ms in one call | no | — | — | skip |
| `IMAGE_WATCH` | A5 | off | — | no | tooling | S | **port** (tool) |
| `MAP_LEAN` | A5 | on | 200-400 µs a page in the world; 2-4 s of every 5 s of the loading thread | no | (d) | S | **A-B** by meter |
| `MAP_THREADS` | A5 | 8 | 1,500 → 30,000 pages/s in a bench; dorm: worse | no | (d) | — | already rejected |
| `SECTION_WINDOW` | A5 | on | — | no | — | — | have it |
| `STREAM_NOTICE` | A5 | on | — | no | — | — | have it |
| `STREAM_PRIORITY` | A5 | on | none | no | (d) | S (15 lines) | speed pass |
| `DSTORAGE_HALF` | A5 | off | 2,079 requests on stale lists | no (fix) | missing / wrong textures | — | have it |
| `DSTORAGE_IO` | A5 | handles | 185 → 990 MB/s | no | — | — | have it |
| `DSTORAGE_LOG`, `_TRACE` | A5 | off | — | no | tooling | — | skip |
| `DSTORAGE_STAGING_MB` | codec_d | 256 | — | no | (d) | S | host1.md H1-4 |
| `POP_WATCH`, `POP_FOLDER`, `POP_ALL` | every presented frame in given seconds read back at 640x360, compared block by block with its neighbours: blips, steps, lone blocks counted; pictures saved (`popwatch.rs:1-46, 295-314`; d1c412c; diagnostics) | off | used to rank `KEEP_SHOWN` / `UPLOAD_AHEAD` by A/B phases | no | **measures** pop-in and flicker | M (~330 lines, self-contained; needs their `capture::png`) | **port** as the meter for our pop-in / flicker defects |
| `HEAP_SPREAD` | slots looked at again for no write: a sixteenth each pass, not all every sixteenth (`draw.rs:610-628`; 2aa8134) | on | 115 ms on a recording thread every sixteenth frame (72,000 slots) | no | (a) spikes | S | speed pass (check our `HEAP_ROUND` equivalent) |
| `VIEW_COPY` | per-thread staging heap for views a list copies (`descriptor/host.rs:3420-3460`; with `LIST_VIEWS` 4e77a87) | on | see shortlist row 3 | no | (a) | — | already listed, shortlist line 166 |

---

<!-- PART-2 -->
