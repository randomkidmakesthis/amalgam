# cool_fox, avatarmanz base: how it achieves its performance, and what of it victory-dev lacks (2026-11-26)

Read-only review. Their tree: `C:\GTAVI\Tools\ref-forks\cool_fox` at HEAD f4b48ad (base = avatarmanz commits
`a26859f..0a43566`, author "local", RX 7900 XTX). Ours: `C:\GTAVI\Tools\victory-dev\crates` as on disk today.
All paths below are relative to `crates/victory-gpu/src` unless a crate is named. "Theirs" = cool_fox, "ours" =
victory-dev. Every "ours has / has not" is a grep of `victory-dev\crates` done for this file (name and concept).
Nothing was built or run. No text addressed to AI assistants was found in what was read (commits carry
`Co-Authored-By: Claude Opus 5.5`; README: "This fork's changes were made with Claude Code").

## 0. What they claim, and under which settings

README.md:25-41 ("Where it stands (2026-11-25)"), one PC: i9-12900K, 64 GB, RX 7900 XTX 24 GB, Windows 11,
**1440p, Series X profile**, diagnostics build with a screenshot a second (docs/findings.md:3-6).

| Scene | Frame |
|---|---|
| Menus, videos, loading | 30 fps |
| Prologue indoors | 50-75 ms (13-20 fps) |
| First mission with the boat | 46-53 ms (19-22 fps) |
| Street chase, many cars and people | 100-165 ms (6-10 fps), single frames far longer |
| Outdoors on foot, the camp | 60-90 ms |
| Flying | 42-57 ms |
| Memory | 40-46 GB private, "64 GB of RAM recommended" |

"It is not there yet" (README.md:19): **no world scene reaches 33 ms**. Their heavy scene (street chase, 67,000
list calls, 100-165 ms) is slower than our heavy 80-90 ms; their light scenes (46-57 ms) match our 45-49 ms windows.
History of one prologue scene: 280 → 124 → 103 → 75 → 46-65 ms (findings.md:10).

Settings behind those numbers (`scripts/play.ps1:15-26`):
- always: `TEXTURE_NO_WAIT=1 RELAY=2 WIDE_BACK=1 CARD_REHASH_RUNS=1 JOIN_COPIES=1 INDIRECT_WINDOW=1 RELAY_COPY=1
  CARD_STEPS=2 INDIRECT_BATCH=2 MAP_THREADS=1 TITLE_PRIORITIES=0 CARD_AWAKE=1 SHOW_AFTER=1 AMBIENT=0.8
  D3D12_DRED=light SKIP_PIPELINES=none`;
- not with `-Safe`: `THIN=1 JOIN_PARTS=8 UPLOAD_AHEAD=1` (`-Safe` instead sets `UNDER_WIDE=0 LAND_ECHO=0`);
- `-RayTracing`: `BVH_BUILDER=1`, "about a third slower" (play.ps1:4).

Quality cuts inside the claimed numbers:
- **Ray-traced indirect light is off** (builder off) and `VICTORY_AMBIENT=0.8` puts a stand-in ambient term in its
  place. README.md:46: "Rooms with little direct light are too dark without `-RayTracing`". Not acceptable as a
  target for us: trades picture (same state as ours today: builder off).
- `SHOW_AFTER=1`: a slot shows a texture only after its upload ran (null / neutral meanwhile): later data. It is
  there for stability (device loss every ~16 min), not speed.
- `TEXTURE_NO_WAIT=1`: texture refresh does not wait for title memory to hold still (ours runs with it too).
- `CARD_STEPS=2`, `THIN=1`: earlier reviews: "stretched bodies" / "picture risk" (fork-review-shortlist.md:164,
  247). Their own README still lists "Trees, plants and far buildings flicker outdoors".
- Nothing is skipped (`SKIP_PIPELINES=none`), resolution is native, profile is Series X. The codec_d launcher on
  top (README.md:5-12, `Play-VIctory.cmd pareto-residency`) "selects Series S": not acceptable: trades picture.

Their own split of a 55 ms frame on the water (findings.md:13-24): preparing thread 48 ms ("It is the wall":
list steps 18 of which refreshes 10, card steps 7, waiting for descriptor records 6, write-watch asking 5, copies
in the card's memory 4, title bytes under shader writes 4); card about 41 ms busy; list calls 36,000 a frame =
230 ms of work over eight threads (67,000 / 430 ms in a street chase); 400,000 descriptor slots looked at, 40,000
mirrored; landing 16 MB a frame as dwords. Ours, heavy: submit thread 73-80 ms at work, card 57-68 ms
(frame-pipeline-analysis.md §1, critical-path-analysis.md §1). Same two walls in the same order.

## 1. Their frame path, step by step, against ours

### 1.1 The title's recording threads
| Step | Theirs (file:line) | Their number | Ours |
|---|---|---|---|
| List calls written down, run by 8 threads of theirs | `list/thin.rs:1-28` (why), `:45-58` (`VICTORY_THIN=1`), `:303` (`VICTORY_THIN_THREADS`, 8, clamp 1-32); a submission goes into the line by recording names, `queue/behind.rs` | 45-60 thousand calls, 3.5 us each, 160-190 ms on five recorders; recording 55 of 65 ms; frame 75 → 68 | none (`VICTORY_THIN`: 1 hit, a comment, `descriptor/host.rs:1012`). Already judged: picture risk, L, do not port as it stands (shortlist:164, 239). New facts: play.ps1 `-Safe` leaves THIN out ("the first thing to try when something is wrong"); it needs `queue::line_on()` (thin.rs:48) = their whole `behind.rs` line, which ours does not have in that form |
| Bind cache (targets, pipelines) | `draw.rs:1189` `VICTORY_BIND_CACHE` | — | already reviewed; ours `VICTORY_BIND_FILTER` (`draw.rs:44-57`) is wider (root constants, root views, tables) |
| A list makes a view once and copies its descriptor | `descriptor/host.rs:5869` `VICTORY_LIST_VIEWS` (4e77a87) | "the recording job the frame waits for last (24 of 65 ms) is 2,500 indirect dispatches, each with a table of its own that differs in a constant buffer" | already reviewed |
| Tables left alone when nothing changed in the list | `draw.rs:3715-3758`, `:4566` `VICTORY_READY_ONCE` (a1bdba4): `FreshTable { epoch, ready }` against `HostList::epoch` | profile waits-1 of five recorders: 21 % contended locks, 11 % `HostHeap::keep`, 17 % `readable`, 7 % buffer look-up, 8 % our clocks and counters | **has it**: `VICTORY_READY_TABLES` (`draw.rs:3959-3973`, `:5117`, Neptune) + `VICTORY_TABLES_BY_CONTENT` (`:5124`) |
| A thread's own answers about buffers | `shadow.rs:4775-4790` `VICTORY_QUICK_FIND` | 7 % of recorders | **has it**: `VICTORY_LOOKUP_CACHE` (`shadow.rs:5101-5143`, thread-local `FOUND` `:5183`) |
| A made slot told without the lock | `descriptor/host.rs:3243`, `:4569` `VICTORY_QUICK_MIRROR` (53c5256) | — | **has it**: `VICTORY_MIRROR_FAST` (`descriptor/host.rs:3050`, in run-auto.sh) |
| Views made beside the heap and copied in whole | `descriptor/host.rs:3414-3452` `VICTORY_VIEW_COPY` (02f36e4) | 51 ns a view against 149; 4 % torn reads → 0 (RX 7900 XTX) | **not** (0 hits `VIEW_COPY`; ours makes views in place, `CopyDescriptorsSimple` only at `descriptor/host.rs:1235-1241`). Row in §2 |
| Heap pass (buffer views picked by index) spread | `draw.rs:616-628`, `:892-902` `VICTORY_HEAP_SPREAD` (2aa8134) | 115 ms on a recording thread every 16th frame (72,000 slots); 11 ms a pass → 22 in lines with one | **not**: `draw.rs:1231` `else if *passes % 16 == 0 { ranges.clear(); host.buffer_slots() }`. Port entry P5 |
| Descriptor tables in dynamic slots | `draw.rs:3762-3773`, pool `descriptor/host.rs` `DYNAMIC` | — | same design (`draw.rs:3934-4036`), reviewed (shortlist:240) |

Verdict for bound (a): everything cheap they did to the recording threads is already ours under other names, except
the heap-pass spread (a spike remover) and making views off-heap. Their big lever is THIN, already judged.

### 1.2 Prepare and submit
| Step | Theirs | Their number | Ours |
|---|---|---|---|
| Submissions prepared by a thread of theirs, one line for all queues, presents in it | `queue/behind.rs:1-45`, `Lane::serve` `:223`; bounded by `VICTORY_SUBMIT_DEPTH` 64 (`:620`) and `VICTORY_SUBMIT_FRAMES` 1 (`:627`: "a present returns when the one before it was shown") | 77-79 ms frame: 58 ms in submit calls, card idle 37 ms behind them (before) | has `VICTORY_ASYNC_SUBMIT` in another form (`queue.rs` `release_held_now` 698-704, `queue::worker`): Present waits until victory-submit has prepared and sent the frame. Their rule is one frame looser (Present returns when the frame *before* is sent). We measured that returning earlier cannot shorten our frame (title gate 2, critical-path §3) |
| Parts of submissions prepared as one | `host.rs:3092-3097` `VICTORY_JOIN_PARTS` (8 in play.ps1), with `VICTORY_REFRESH_FRONT` (`:7683-7698`) | dorm: 12 of 28 parts joined, 54.8 → 51.4 ms of that thread; REFRESH_FRONT alone "measured at nothing" (17.1 vs 18.3 ms) | has `VICTORY_JOIN_PARTS` (`host.rs:4805`, fxceless 3973179; run-auto sets 1) |
| Copies shaders write refreshed together, ahead of the steps | `host.rs:7673-7679` `VICTORY_REFRESH_AHEAD` (dc2a3ad, 137 lines) | 580 copies a frame, 3.4-6.5 ms of the submit thread | partly: ours refreshes a group wide (`host.rs:10079`, `wide_refresh`/`prepare_wide`); no "ahead of writes" group (0 hits). Row in §2 |
| Heap records brought up to date on a thread beside the preparing, and between submissions | `descriptor.rs:183` (`HEAP_SYNC_ASIDE`), `:186-198`, `:230-255` `VICTORY_HEAP_SYNC_EARLY` 4 ms (f6db479, 33 lines) | "the first submission of a frame found all of them: 8 ms waited for (run cpu-2)"; findings: 6 ms of 48 | aside: **has** (`descriptor.rs:305-372`, "port of avatarmanz 53c5256"); early rounds: **not** (0 hits). Port entry P3 |
| Steps that read title memory under flights done on the card | `host/card_steps.rs:1-79` `VICTORY_CARD_STEPS` 0/1/2; `VICTORY_PLACED_ORDER` (`:88-103`) | findings: 7 ms of 48 are "steps done on the card"; n19: a whole submission refused for one placed copy stood 40-60 ms | judged before: stretched bodies (shortlist:247). Ours has its own card path (`card_constants`, `gpu_buffer_copy` `list.rs:1127`, `VICTORY_FILL_PREDICT_CARD` `host.rs:4219`). New fact only: `PLACED_ORDER` is off and "has only its tests behind it" |
| Title buffer copies at their place in the list | `host/place.rs:1-43` `VICTORY_COPIES_IN_PLACE` (on, 350f7f4, 738 lines) | correctness: "of copy, draw, copy, draw both draws read the second" | not as such; ours runs a title copy on the card only when a flight may write its bytes (`list.rs:1127-1138`). Row in §2 (picture: improves; not a speed item) |
| Each submission's sends are its own bytes | `host/send.rs:1-27` `VICTORY_SEND_OWN` (on, 18c0129) | test: 12 of 12 wrong without it | **not**: ours copies out of the copy's CPU side when the card reaches the send list (`host.rs:6243-6244`, `staging` at the same offset), while later submissions are already prepared. See §4 "Suspicious" |
| Snapshot memory, host lists, send memory reused | `shadow.rs:1283-1310` `VICTORY_SNAPSHOT_POOL` 1024; `list.rs` `IDLE_MOST` 128 → 512; `send.rs:130` sizes by doubles (52e0c03) | 30 % of the submit thread in the opening: snapshots 11, preludes 12, lists dropped 5, send memory 3 | lists: has (`list.rs:300`, 512). snapshots: **not** (`shadow.rs:1451` `pool.truncate(8)`). Port entry P4 |
| Landing on its own thread, on several threads, beside the guards | `host.rs:2440-2486` (relay), `land.rs` (whole), `host.rs:4665-4680`, `:4784-4810` | relay thread 58-60 ms of a 104 ms frame → 45 ms → **8 ms** (0d5e4c8) | relay: has. Several threads: **not on the relay path** (`host.rs:6772-6781`, `:6923-6931` serial; `VICTORY_WIDE_MERGE` only in the waited-for path `:7187`). Beside the guards: has (`VICTORY_BACK_OWN_VIEW`, `inplace.rs:485-600`). Port entry P1 |
| Fetch on a copy queue | `host.rs:2488-2505` `VICTORY_RELAY_COPY` (play.ps1 1), check `host/torn.rs` `VICTORY_FETCH_CHECK` | relay stood 48 ms of a 120 ms frame waiting for seven fetches of 94 MB | **has**: `VICTORY_QUEUE2` fetches (`host.rs:6575-6722`) with the `BESIDE` re-fetch notes (`:6549-6563`) = their torn check by another rule |

### 1.3 Memory tracking
| Step | Theirs | Their number | Ours |
|---|---|---|---|
| Write-watch asking once, ahead | `watch.rs` `ask_ahead`, `ask_rest`; a thread asks early `:385-403` `VICTORY_ASK_EARLY` (off) | early asking: "none the cheaper for its 650 a frame, 12 to 18 ms where 9 were" (3ead56e) | has `ask_once` (`watch.rs:312`), `ask_ahead`. Early thread: not, and their own number says skip |
| 4 KiB parts of a 64 KiB page | `watch.rs:40-88` `VICTORY_PAGE_PARTS`, `:786-787` `VICTORY_TAKE_PARTS`, `shadow.rs` `by_parts` (3ead56e, e8d3d91) | 29 MB written a frame were most of 114 MB copied, "and as much sent up and hashed again on the card" | **not**: `watch.rs:187` throws the 4 KiB away (`at / PAGE`, `dedup`). Port entry P2 |
| Holes of view memory known by bits | `watch.rs:1060-1070` `VICTORY_KNOWN_HOLES` (74045e4) | 231,000 regions; 111 ms a frame of all threads waiting for the guards' lock | has by another route: `Guards::parked`, `VICTORY_VIEW_SET` (`watch.rs:1015`), `VICTORY_GUARD_HOLES_FRAME` (`:1549`, off) |
| "Nothing new" known without asking | `watch.rs:813-826` `VICTORY_LIGHT_REFRESH`, `PUBLISHED` bits `:828-905`, `Shadow::nothing_new` `shadow.rs:3722`, used `host.rs:7926, 8019, 8037, 8090, 8177, 8209` (2aa8134) | 1,500 askings a frame on up to seven threads behind one lock: 94 ms a frame of all threads' time | **not** (0 hits `nothing_new`, `PUBLISHED`). Ours still pays `watch::guards_read` 1.64 + `watch::journal` 1.17 ms on the submit thread and ~19 thread-ms of wide workers on the same two locks (critical-path §3). Port entry P6 |
| Guard rest; one view opened at a fault | `watch.rs:1348` `GUARD_REST`, `:1326` `OPEN_ONE_VIEW` | 69 → ~6 ms of threads' time | has `VICTORY_GUARD_REST` (`watch.rs:933`); OPEN_ONE_VIEW already reviewed |
| Title writes put in beside the guard | `titlewrite.rs:1-31` `VICTORY_COPY_BESIDE`, `VICTORY_STORE_BESIDE` (both off), `VICTORY_WRITE_SITES`, `victory-loader/src/write_sites.rs`, `VICTORY_SAME_RECORDS` (`titlewrite.rs:177-185`, on) | 590 write faults a frame, a protection change ~5 us; a third the title's `memcpy`; no before/after | not (0 hits). Rows in §2: off at the author's, L, risky |
| Title bytes under flights' writes | `shadow.rs:2173` `UNDER_WIDE`, `land.rs:332-336` `VICTORY_UNDER_THREADS` 6, `send.rs:46-60` `VICTORY_UNDER_WORDS` | 14 → 4-7 ms; by bytes a submission with > 4,096 runs stood 4-14 ms | words: has (`VICTORY_UNDER_WRITES_WORDS` `shadow.rs:235`); wide: reviewed (shortlist:242) |
| Copies that fail instead of faulting | `safecopy.rs:1-12` `VICTORY_SAFE_COPY` (on) | 35,000 kernel calls a frame, 39 ms of kernel time on the submit thread | has: `plaincopy.rs` (`VICTORY_COPY_PLAIN=1`, in run-auto.sh). Theirs adds `merge` / `put_changed` (dword stores), used by P1 |

### 1.4 Uploads and refresh to the card
| Step | Theirs | Their number | Ours |
|---|---|---|---|
| Card buffers / windows, changed runs, rehash runs | `shadow.rs:232` (`CARD_BUFFERS_MIN_KB`), `host.rs:2147`, `:10146` (`SEND_CHANGED`) | already reviewed | has (`VICTORY_CARD_BUFFERS*`, `VICTORY_CARD_REHASH_RUNS`, `VICTORY_REFRESH_COMPARE` off) |
| A landing's block not taken again by the copy it went through | `land.rs:493-512` `VICTORY_LAND_TAKEN`, `Shadow::landed` | 900 blocks a frame of the 103 MB that went up | has: `put_back_own` notes the page current (`shadow.rs:3940-3949`) |
| Sends / windows / scratch in card memory the CPU maps (`D3D12_HEAP_TYPE_GPU_UPLOAD`) | `host/send.rs:127-135` `VICTORY_SEND_BAR`, `shadow.rs:482-495` `VICTORY_WINDOWS_BAR_MB` (both off, bd68f5e) | replayed yard frame 36.3 ms → windows 31.9, sends 32.7; lists 29 → 26 ms; but SEND_BAR costs the submit thread 8-9 ms instead of 3 | not (0 hits `GPU_UPLOAD`). Row in §2: A-B only, needs VRAM we do not have |
| Texture first uploads on a copy queue | `texture/ahead.rs` `VICTORY_UPLOAD_AHEAD` | — | left to the memory/visual pass |

### 1.5 Card work
| Step | Theirs | Their number | Ours |
|---|---|---|---|
| Indirect clamps in batches / windows | `indirect/batch.rs`, `draw.rs:341`, `:373` | replay 84.6 → 75.3 ms | reviewed; ours gather + window |
| Dispatch of more than 65,535 groups | `indirect.rs:268-280` `VICTORY_INDIRECT_WIDE` (AMD only: 2^20; "Other cards stay at 65,535") | 1,905 tiles lost at the bottom of the picture | ours splits such dispatches (`indirect.rs:141-173`, `:232-233`, `PART` `:399`): no port |
| Card awake | `host/awake.rs:66` | reviewed | has (`host/awake.rs`) |
| Pipelines: workers, slow ones, lowest priority, book, warm | `pipeline/make.rs:471-508` (`VICTORY_PIPELINE_WORKERS`: 2/3 of processors, clamp 2-16; `VICTORY_PIPELINE_LOWEST` on), `pipeline/known.rs`, `library.rs:763-768` `VICTORY_EARLY_ALL` | stalls: "270-660 s of a session were full of frames over 200 ms, almost all while new pipelines were made"; `EARLY_ALL` "ended it inside the driver's compiler" (AMD) | workers reviewed (shortlist:243). `PIPELINE_LOWEST`: row in §2 |
| Their own timestamps around each submission | `host/spans.rs` `VICTORY_CARD_SPANS` | — | has `cardmeter.rs` |

## 2. The unmentioned switches that concern speed, threading, tracking, copies, descriptors, tooling

Columns: default at theirs; number = the author's; "pic" = trades picture; bound = ours (a) recording, (b) submit
thread, (c) card, (d) streaming freezes, Lat = landing latency (critical-path §1: 1 ms of it = 0.5 ms of frame).

| Switch | What / how (their file:line, commit) | Default | Number | Pic | Ours (grep) | Bound | Size / risk | Verdict |
|---|---|---|---|---|---|---|---|---|
| `VICTORY_LAND`, `_THREADS` | landing in jobs of 256 KiB on a pool; one AVX2 pass a piece (`land.rs:44-56`, `:199-313`, `:357-411`, `:555-679`; `safecopy.rs:165-245`; 0d5e4c8, 1,961 lines) | on; threads = cores/4, clamp 1-4 | 45 → 8 ms of the relay thread | no | serial on the relay path | Lat, (b) | M / medium | **port (P1)** |
| `VICTORY_EXACT_BACK` | only the dwords that changed are stored into title memory (`shadow.rs:2085-2089`, `safecopy.rs:289`) | on | 16 MB a frame put as dwords of 98 MB fetched | no (closes a race) | 0 hits; ours stores first-to-last difference (`shadow.rs:5015-5020`, `:5078`) | Lat | part of P1 | port with P1 |
| `VICTORY_LAND_BESIDE` | pieces written through a view of ours, guards stay (`land.rs:58-62`, `inplace.rs:536`) | on | 18 ms of kernel faults + 5 ms protection calls gone | no | has: `VICTORY_BACK_OWN_VIEW` | — | — | skip (ours) |
| `VICTORY_LAND_ANYTIME` | beside the guards also while the runtime changes a mapping (`land.rs:458-476`, `:589-590`; 2c982ee, 31 lines) | on | 28 of 1,620 pieces went beside in a streaming run; 5,000 blocks opened, 7,000 guarded a frame; frame 105 → 140 ms without | no | ours `write_own` gives up only when *its own* 64 KiB is changing (`inplace.rs:534`, `VICTORY_MAPPING_RANGES` on) | (d) | S | A-B only if our log "stretches written the old way" (`shadow.rs:3955`) is high while streaming |
| `VICTORY_LAND_TAKEN` | landed block noted current for the copy (`land.rs:493-512`) | on | 900 blocks a frame not re-sent | no | has (`shadow.rs:3940-3949`) | — | — | skip (ours) |
| `VICTORY_LAND_SKIP` | pieces an earlier fetch holds left out (`land.rs:64-69`, `Had` `:413-456`) | off ("counted either way") | none | no | 0 hits | Lat | S | skip (author keeps it off) |
| `VICTORY_PAGE_PARTS` | the kernel's 4 KiB parts kept per 64 KiB page; refresh copies those (`watch.rs:40-88`, `:203-254`; `shadow.rs` `by_parts`; 3ead56e, 200 lines) | on | 29 of 114 MB a frame really written | no | not (`watch.rs:187`) | (b), (c) | M / low-medium | **port (P2)** |
| `VICTORY_TAKE_PARTS` | the same for tracked ranges (`watch.rs:786-787`; e8d3d91) | on | none | no | not (`watch.rs:755-785`) | (b) | S | port with P2 |
| `VICTORY_HEAP_SYNC_EARLY` | the heap-sync thread makes a round of its own every 4 ms while submissions come (`descriptor.rs:186-198`, `:230-255`; f6db479, 33 lines) | 4 ms | 8 ms waited for at a frame's first submission | no | not | (b) | S / low | **port (P3)** |
| `VICTORY_SNAPSHOT_POOL` | up to 1024 snapshot buffers kept, 512 MB in all; freed outside the lock (`shadow.rs:1283-1310`; 52e0c03) | 1024 | 11 % of the submit thread (opening, profile cpu-1) | no | not (`shadow.rs:1451`) | (b) | S / low (memory: capped at 512 MB as now) | **port (P4)** |
| `VICTORY_HEAP_SPREAD` | a sixteenth of the heap's buffer slots at every pass (`draw.rs:616-628`, `:892-902`, `descriptor/host.rs:2659-2663`) | on | 115 ms spike every 16th pass | no | not (`draw.rs:1231`) | (a), frame spikes | S / low | **port (P5)** |
| `VICTORY_LIGHT_REFRESH` | view memory "not written since asked" known by bits; a copy with nothing new is dropped before a thread is woken (`watch.rs:813-905`, `shadow.rs:3719-3745`, `host.rs:7926..8209`; 2aa8134) | on | 94 ms a frame of all threads' time | no | not | (b) | M-L / medium | **port (P6)**, A-B |
| `VICTORY_KNOWN_HOLES` | placeholders' holes known until the runtime tells of a mapping; other holes asked without the guards' lock (`watch.rs:1060-1070`, `:1163`) | on (needs LIGHT_REFRESH) | 111 ms a frame of all threads | no | has by another route (`parked`, `VIEW_SET`) | (b) | — | skip; read their "asked with the lock not held" when doing P6 |
| `VICTORY_ASK_EARLY` | a thread asks the write watch between submissions (`watch.rs:385-432`; 1a411ae) | off | "none the cheaper ... 12 to 18 ms where 9 were" | no | not | (b) | S | skip (author's own number) |
| `VICTORY_REFRESH_AHEAD` | copies shaders write refreshed together on threads ahead of the steps (`host.rs:7673-7679`, `:5625`; dc2a3ad) | on (needs wide refresh + LIGHT_REFRESH) | 3.4-6.5 ms of the submit thread, 580 copies | no | partly | (b) | M / medium; after P6 | A-B after P6 |
| `VICTORY_REFRESH_FRONT` | all of a submission's refreshes in one go in front (`host.rs:7683-7698`) | off (on with JOIN_PARTS > 1) | "Measured on its own at nothing" | no | not | (b) | M | skip |
| `VICTORY_UNDER_THREADS` | pool for the under-writes compare (`land.rs:332-336`) | 6 | with UNDER_WIDE: 14 → 4-7 ms | no | reviewed with UNDER_WIDE | (b) | — | as listed, shortlist:242 |
| `VICTORY_UNDER_WORDS` | under-write bytes go up as whole words, one dispatch (`host/send.rs:46-60`; 40580d7) | on | 4-14 ms of a steady frame gone | no | has (`VICTORY_UNDER_WRITES_WORDS`) | — | — | skip (ours) |
| `VICTORY_WIDE_SPIN` | wide workers spin up to N us before sleeping (`shadow.rs:1474-1479`; b620284) | 0 (off) | "woken in 50 us and more, 95 times a frame" | no | 0 hits | (b) ~5 ms of wake-ups at most | S | A-B, low priority (burns a core) |
| `VICTORY_SEND_OWN` | a submission's sends are copied into its own memory at preparing; short runs as (offset, value) pairs by one dispatch (`host/send.rs`; 18c0129) | on | 12 of 12 wrong in the test without | improves | **not** (`host.rs:6243-6244`) | defect: flicker? | M (send.rs ~300 lines + shadow hooks) / medium | **port for correctness**, see §4 |
| `VICTORY_CARD_ALL` | every title buffer copy gets two sides (`shadow.rs:360-375`) | off | none | improves | 0 hits | — | S after SEND_OWN | skip (memory) |
| `VICTORY_COPIES_IN_PLACE` | title buffer copies run in the list on the card (`host/place.rs`; 350f7f4, 738 lines) | on | none (order fix) | improves | partly (`list.rs:1127`) | defect class: one-frame-old constants | L / high | A-B after SEND_OWN |
| `VICTORY_WINDOWS_OWN` | windows get two sides too (`shadow.rs:527-533`) | off | costs 3.3 GB | — | 0 hits | — | — | skip |
| `VICTORY_PLACED_ORDER` | a placed copy blocks card fills only from its place on (`host/card_steps.rs:88-103`) | off, untested in play | n19: 40-60 ms stall it would remove | no | n/a (needs CARD_STEPS) | — | — | skip (CARD_STEPS rejected) |
| `VICTORY_FILL_TITLE` | a small fill does not wait if the title wrote the block since it was taken or no shader write landed there in 600 frames (`host.rs:2789-2809`) | off | "the first alone left two thirds of the wait" | **yes, possible** ("is read as it was before that flight") | 0 hits; ours `fill_ahead` + held pieces checks at landing instead | (b) read waits | S | skip: guesses; ours is checked |
| `VICTORY_RELAY_COPY` | fetch on a copy queue (`host.rs:2488-2505`) | off; play.ps1 1 | 48 of 120 ms of the relay | no | has (`VICTORY_QUEUE2`) | — | — | skip (ours) |
| `VICTORY_FETCH_CHECK` | hash per fetched piece against the compare's (`host/torn.rs`) | off | 3-4 ms for 96 MB | improves | ours: `BESIDE` notes (`host.rs:6549`) | — | M | skip unless QUEUE2 shows torn pieces |
| `VICTORY_VIEW_COPY` | every host slot made in a per-thread slot of a CPU-only heap and copied in (`descriptor/host.rs:3414-3452`; 02f36e4, 723 lines) | on | 51 vs 149 ns a view; torn descriptors 4 % → 0 (AMD) | no | not | (a) `descriptor::host::make` 2.9 ms; device losses | M / medium | A-B (the gain was measured on AMD's driver only) |
| `VICTORY_SLOT_HOLDS`, `VICTORY_BURY_LAG`, `VICTORY_WAIT_RESIDENT` | lifetime: a constant-buffer slot holds its scratch (`descriptor/host.rs:3061`), objects let go die 64 submissions later (`host.rs:496-501`), new memory made resident and waited for (`host.rs:210-220`) | on | device losses of 2026-11-25 (AMD page faults) | no | 0 hits each | stability, not speed | S each | port only if we see device removals with a page fault |
| `VICTORY_QUICK_FIND` / `_MIRROR` / `READY_ONCE` | see §1.1 | on | — | no | has all three | — | — | skip (ours) |
| `VICTORY_SAME_RECORDS` | a descriptor copied onto itself is not written (`titlewrite.rs:177-185`; e8d3d91) | on | none | no | 0 hits | (a)/(b): fewer write faults and dirty heap pages | S | A-B: first count how many `CopyDescriptors` writes are identical |
| `VICTORY_COPY_BESIDE`, `VICTORY_STORE_BESIDE`, `VICTORY_WRITE_SITES` | title `memcpy`/`memmove`/`memset` hooked; faulting stores decoded and done in the handler; code sites patched to look first (`titlewrite.rs`, `titlewrite/decode.rs` 697 lines, `victory-loader/src/write_sites.rs`; f19e55b 1,229 lines, 83a1bcf, e8d3d91) | off (asked for by name) | 590 faults a frame; no frame number | no by design; a mis-decoded store corrupts title memory | not; ours `VICTORY_GUARD_REST` covers the same faults | (a) title threads | L / high | skip (author keeps them off) |
| `VICTORY_SAFE_COPY` | §1.3 | on | 39 ms kernel time | no | has (`plaincopy.rs`) | — | — | skip (ours) |
| `VICTORY_SUBMIT_DEPTH`, `VICTORY_SUBMIT_FRAMES` | bounds of the line (`queue/behind.rs:620-631`) | 64; 1 | none | no | other mechanism | — | — | skip (critical-path §3: no gain behind gate 2) |
| `VICTORY_THIN_THREADS` | THIN's translators (`list/thin.rs:303`) | 8 | — | — | — | — | — | with THIN: rejected |
| `VICTORY_PICKED_WRITES`, `VICTORY_PICKED_MB` | buffers written through views picked by a computed index are found by a rolling compare and declared (`picked.rs`; b1da728) | 0; 32 MB | 37 compute programs; `82a2ad6d07417d36` writes 4.6 MB a frame of a 24 MB buffer undeclared | improves (costs hashing) | 0 hits | (c) cost; correctness | M | left to the visual pass with this note: it is a silent-defect class (writes that never reach title memory) |
| `VICTORY_SEND_BAR` | send memory in a GPU_UPLOAD heap (`host/send.rs:127-135`) | off | card 5.4 → 0.4 ms of copies, but submit thread 8-9 instead of 3 ms | no | not | (c) vs (b) | S | skip while (b) is our wall |
| `VICTORY_WINDOWS_BAR_MB` | windows in a GPU_UPLOAD heap, no CPU side (`shadow.rs:482-525`) | 0 | lists 29 → 26 ms, frame 36 → 32 (replay) | no | not | (c) | M / needs free VRAM (a fifth of budget) and resizable BAR | A-B later; not with 14.4 / 15.1 GB used |
| `VICTORY_INDIRECT_WIDE` | §1.5 | on (AMD) | — | — | ours splits | — | — | skip |
| `VICTORY_PIPELINE_WORKERS` | worker count (`pipeline/make.rs:471-492`) | 2/3 of processors, 2-16 (3/4, 2-8 with little RAM) | none | no | reviewed (shortlist:243) | (d) | S | as listed |
| `VICTORY_PIPELINE_LOWEST` | pipeline workers below the title's loading threads (`pipeline/make.rs:505-508`; c278c67) | on | none | no | 0 hits | (d) cold stalls vs streaming | S / low | A-B (check beside `VICTORY_AFFINITY=free`) |
| `VICTORY_EARLY_ALL` | every title pipeline started at creation (`pipeline/library.rs:763-768`) | off | "ended it inside the driver's compiler" (AMD); 84,000 pipelines | no | 0 hits | (d) | S | skip |
| `VICTORY_GPU_LOAD_MS` | what the title's four GPU timestamp pairs a frame show (`queue.rs:847-856`) | unset | none | **unknown**: the title may scale work by GPU time | 0 hits | — | S | do not set; game fact only (§5) |
| `VICTORY_AB` (+ `VICTORY_A`, `_B`, `_NONE_SUCH`: test names) | switches flip every N seconds at a present, all combinations in turn, frame mean/median/p90 logged per phase (`ab.rs:1-131`, `switch.rs:17-43` `toggled!`; a1bdba4) | off | — | no | not (ours has `alt` turns only for single switches, e.g. `host.rs:2562`) | measuring: scenes differ run to run | S (130 + 25 lines) / low | **port (tooling, T1)** |
| `VICTORY_TIMELINE`, `_AT`, `_FRAMES` | Chrome trace of a few frames + text chain "who waited on whom" from each present (`timeline.rs`, `timeline/{hooks,record,report}.rs`; 336f613, 2,506 lines, 25 files) | off | — | no | ours: `profile.rs`, `framelog.rs`, `framestream.rs` | measuring | L | skip; our profile answers the same |
| `VICTORY_RECORD`, `_AT`, `_FRAMES`, `_ALWAYS`, `VICTORY_REPLAY_GBV`, `_PEEK`, `VICTORY_TEST_D3D_DEBUG` | frame capture and `victory replay` with `--compare`, `--skip-shader`, `--pixel`, `--diverge`, `--upcopies` (`replay/*` ~15,000 lines, `apps/victory/src/replay.rs`; docs/replay.md) | off | recorder: +3-10 ms a frame, 5.4 s standstill, 9 GB | no | not (ours `capture.rs` only) | (c): per-pass card times without the title; pixel provenance for defects | L / low risk to the run, high effort; "Recording a frame in the world makes the title's own watchdog end it" (README.md:50) unless `VICTORY_POKE` | **worth a separate decision**: it is the only tool here that attacks bound (c) and the open visual defects (`--pixel`) |
| `VICTORY_CARD_SPANS`, `VICTORY_PANEL`, `VICTORY_FAULT_BOOK`, `_AT`, `VICTORY_BOOK`, `VICTORY_SLOT_CHECK`, `VICTORY_UPCOPY_CHECK` | meters: card's own stamps; on-screen numbers (F3; "costs some"); book of write faults; who owns card memory (AMD descriptor bits only); undeclared card writes (`host/spans.rs`, `host/panel.rs`, `faultbook.rs`, `book.rs`, `host/upcheck.rs`) | off | — | no | cardmeter / framelog / faults.rs | — | S-L | skip, except `UPCOPY_CHECK` (1,291 lines) if SEND_OWN is ported: it is its test |
| `VICTORY_BENCH_CS`, `_N`, `_S`, `_THREADS`, `_WARP`, `_HOLES`, `VICTORY_BLOCK_WAY`, `VICTORY_TEST_ROUNDS` | ignored tests / benches (`indirect.rs:966-1910`, `host.rs:10129-10455`) | — | — | — | — | — | — | skip |
| `VICTORY_POKE`, `VICTORY_SKIP_CPU_CHECK`, `VICTORY_TRANSLATOR`, `VICTORY_HOST_RDNA`, `VICTORY_RDNA_FLOAT` | debug poke of title bytes twice a second (`apps/victory/src/run.rs:256-263`); AVX2 check off; translator stamp in the shader cache key (`build.rs:44`, `shader_cache.rs:57`); AMD float rules ("0.8 ms", findings.md:75) | — | — | — | — | — | — | skip (AMD / tooling) |

Left to the memory/visual pass (not described here): `VICTORY_AMBIENT_AIR`, `_AMBIENT_SHADE`, `_BAKE_EXACT`,
`_BENCH_FILE`, `_BVH_BOUNDS`, `_BVH_FENCES`, `_BVH_SIZES_WAIT`, `_BVH_TRACE`, `_COPY_OVER_TITLE`, `_DSTORAGE_HALF`,
`_DSTORAGE_IO`, `_DSTORAGE_LOG`, `_DSTORAGE_STAGING_MB`, `_DSTORAGE_TRACE`, `_HELD_LEVELS`, `_HOST_LOCK_CHILD`,
`_IMAGE_WATCH`, `_KEEP_FREED`, `_LENT_HEAP`, `_MAPPED_BY_VIEWS`, `_MAP_LEAN`, `_MAP_THREADS` (their finding: 16
threads "no more pages mapped, frames longer"; play.ps1 sets 1), `_MEMORY_CARD_MB`, `_MEMORY_DUMP`, `_ONE_PAD`,
`_POP_ALL`, `_POP_FOLDER`, `_POP_WATCH`, `_READ_WATCH`, `_RECHECK_SOON`, `_RIPE_BLOCKS`, `_SECTION_WINDOW`,
`_STREAM_NOTICE`, `_STREAM_PRIORITY`, `_TEXTURE_AWAY_MS`, `_TEXTURE_FILL`, `_TEXTURE_HOT`, `_TEXTURE_KEEP`,
`_TEXTURE_ONCE`, `_TEXTURE_POOL`, `_TEXTURE_SAME`, `_THREAD_DIMENSIONS`, `_UPLOAD_AHEAD`, `_UPLOAD_LOW`,
`_WINDOW_MAX`.

Already-mentioned switches (missing-but-mentioned.txt), new measured numbers only: `VICTORY_INDIRECT_BATCH` none
new; `VICTORY_CARD_STEPS` 7 ms of a 48 ms prepare are "steps done on the card" (findings.md:16);
`VICTORY_PIPELINE_BOOK` 84,000 pipelines created, ~2,000 drawn with (`pipeline/known.rs:8`); `VICTORY_LIST_VIEWS`
2,500 indirect dispatches = the last recording job, 24 of 65 ms (4e77a87); `VICTORY_TEXTURE_BACK` boats (correctness);
the others: nothing new.
