# Game streaming triggers vs. what the layer does

Question: does the game itself say when to load and when to release, and does the layer follow it?
Short answer: **yes, the game says both, through page mapping and DirectStorage; the layer hears both, but reacts
coarsely (it throws away more than the game released) and too early (it uploads on "mapped", before the data lands).**

Evidence: code in `Tools\victory-dev` (being edited: line numbers may drift), logs `bench\run-11-mem-tex` (379 s,
11.8 fps, ended in a host GPU failure) and `bench\run-12-merged` (417 s, 9.5 fps), the exe's import table and strings.
Tags: **[log]** counted in a run, **[code]** read, **[exe]** import table/strings, **[unproven]**.

## 1. The game's signals

| Signal | Meaning | Evidence | Rate (run-11 / run-12) |
|---|---|---|---|
| Placed view written into a descriptor slot (device 0x230/0x238, no resource object) | "this texture exists at this address" — NOT "I need it": views are written long before memory is there | [code] `descriptor.rs:1013`; [log] 118,463 / 116,105 title textures never got a host copy; 3,329 / 14,534 queued slots "waiting for their memory" | 2.8 M placed slot writes per run |
| `CopyPageMappingsBatchX` kind 0/1 (map pool pages at a texture address) and `XMemMapPhysicalPages` | **"I need this now"**: pages given to a texture level | [code] `queue.rs:1432`, `xmem.rs:681,788`; game code at 0x140f2c470 | remap calls per usual frame 186 / 306, ~1,100 in long frames, max 54,857 in one frame; 4.49 M / 4.21 M ranges noted under texture views (~11,000/s). **No split map vs unmap** |
| Same call, kind 2 (unmap), or mapping other pages over | **"I am done with this"** — the only release signal streamed textures have | [code] `queue.rs:1466`; [log] 2,948 / 3,110 made slots found unmapped and nulled | not counted separately: add pages per kind in `copy_page_mappings` |
| DirectStorage request into physical pages (17 queues, all priority 2, capacity 4,159) | **"the data is here"** (completion); priority carries no intent | [code] `dstorage.rs:1065-1101`; [log] 17 queue lines | 542,807 / 579,941 writes, 35.6 / 38.5 GB (~92 MB/s, ~1,400/s) |
| DirectStorage cancel | never used | [log] the stub warns on every call; 0 lines in any run | 0 |
| Descriptor slot overwritten with another view / stale | slot no longer shows the old texture | [log] 100,658 writes of another view; 222,852 / 247,683 slots made stale | — |
| Resource `Release` | only for texture *objects* (render targets, UI); streamed textures have none | [code] `resource.rs:3577` | not counted |
| `Evict`, `MakeResident`, `SetResidencyPriority`, reserved resources, `UpdateTileMappings`, `DiscardResource`, min-mip clamp, sampler feedback | **not used** | [log] every unwritten D3D12 slot logs once ("method not implemented"); 0 lines in all logs under `victory-data` | 0 |
| `XMemGetWorkingSetStatistics`, `XMemTransferMemory` | imported, **never called** in a run (first call logs) | [exe] xmem.dll imports; [log] 0 lines | 0 |
| `GlobalMemoryStatusEx`, `VirtualAlloc`/`VirtualFree` (decommit/reset) | imported, go straight to Windows, unhooked | [exe] | **[unproven]** whether called; add a hook that counts |
| `D3D12ConfigureVirtualMemoryX` | called once: flags 0, 4 page-table pages | [log] | 1 |
| Strings naming the streaming system | none (hashed). Only `fiDeviceDirectStorage`, `Prestream`, `platform:/data/heightmapStreaming` | [exe] | — |

The game does not ask how much memory is free in any recorded run; pools come from the console type (6.1 GiB of
physical pages). Order seen for a texture: view written → pages mapped → DirectStorage fills them → drawn.
Proof of "mapped before filled": 28,964 / 30,436 slots had already been uploaded when a DirectStorage write landed
under them [log "writes behind the guards"]. Time from request to first draw: **[unproven]**, no log line.

## 2. What the layer does at each signal

| Event | Layer reaction | Where |
|---|---|---|
| Pages mapped OR unmapped (same hook for both) | drops guards; notes the range for the texture pass (slots there due next frame); notes it as "unmapped" for the sweep | `watch.rs:1534`, `descriptor/host.rs:507,1365`, `resource.rs:2357` |
| Next sweep (once a second) | a placed texture's host copy under a noted range goes **at once** if no list and no slot holds it. More than 64 ranges since the last sweep → **every** placed texture counts as unmapped | `resource.rs:2092-2130`, cap `KEPT_RANGES` `resource.rs:2331` |
| Slot's memory found unmapped by the pass | slot made null, host texture retired | `descriptor/host.rs:1908` |
| DirectStorage write done | guarded blocks opened; slots **already uploaded are left as they were** (default `VICTORY_PAGE_WRITES`=guards); found later by fingerprint looks at 4, 12, 40, 120, 400 frames | `watch.rs:1278,1246`, `draw.rs:455,2167` |
| Slot overwritten | old host texture retired from the slot (`keep`) | `descriptor/host.rs:1927` |
| Resource `Release` | host copy retired, staging and views forgotten | `resource.rs:3577` |
| Heap `Release` | frees the layer's own block only | `resource.rs:1072` |
| Buffer window over memory that was unmapped | nothing; the window stays until idle | `shadow.rs:2457` |

The layer's own triggers: texture LRU with water marks 85 % / 75 % of the card budget (`VICTORY_TEXTURE_LRU`,
`resource.rs:1876`, never fired: "let go for room 0 MB" in both runs); 1 s floor; window idle 20 s
(`VICTORY_WINDOW_IDLE_S`, 5 s under 3 GB free RAM); staging idle 10/5/2 s by free RAM; texture queue budget
500 ms per second and 192 MB per frame (`VICTORY_TEXTURE_BUDGET_MS_PER_S`, `VICTORY_TEXTURE_STAGE_MB`), aged slots
first after 2 s (`VICTORY_TEXTURE_MAX_AGE_MS`); heap drain (off); level tiles given back (off, lost the device in
run-10); pipeline wait (`VICTORY_PIPELINE_WAIT_MS`, 20 in these runs).

## 3. Measured outcome

| | run-11 | run-12 |
|---|---|---|
| Streamed by the game (DirectStorage into pages) | 35.6 GB | 38.5 GB |
| Uploaded by the layer (textures picked by index) | 138 GB (3.9x) | 136 GB (3.5x) |
| Host copies let go by the sweep | 172,509 / 43.6 GB | 175,682 / 45.2 GB |
| ... of them for room on the card / for idle time | 0 / 0 | 0 / 0 |
| Made again at the same address and description | 55,516 / 15.4 GB | 61,907 / 17.2 GB |
| Largest single sweep | 9,859 copies | 7,220 copies (2,120 copies / 505 MB in the last second, card at 10.7 of 12.5 GB) |
| Time inside remap calls, usual / long frame | 39 / 205 ms | 53 / 241 ms |
| Buffer windows alive / let go on idle | 3.2 / 9.5 GB | 3.7 / 7.8 GB |
| Textures changed after upload, found by fingerprint (of the first 200 logged; total 200–999) | 64 % at 4 frames, 8 % only at 176 frames (~15 s) | 69 % at 4 frames, 4.5 % at 176 |

## 4. The gap

| # | Game signal | Layer today | Verdict | Consequence | Proposed change (switch) |
|---|---|---|---|---|---|
| 1 | Unmap of a texture's pages | copy freed within ~1 s — but mapping-in is noted the same way, and past 64 ranges/s every placed texture is treated as unmapped | **match, then over-reach** | 43–45 GB let go with the card under its mark; 15–17 GB re-made; 3.5–3.9x upload bytes; black clothes/objects while a copy is rebuilt (inferred) | `VICTORY_UNMAP_EXACT=1`: keep unmap ranges apart from map ranges, no "all" fallback (join gaps as `keep_texture_change` does); a map-in makes the slot due, it does not drop the copy |
| 2 | DirectStorage write lands | uploaded slots not re-queued; fingerprint looks up to 400 frames later | **late** | ~29,000 uploads of not-yet-filled memory per run, then a second upload; slow pop-in, stand-ins staying (inferred) | `VICTORY_UPLOAD_ON_WRITE=1`: a mapped slot becomes due at the write that covers it, or a few frames after the map if no write comes; fingerprint looks stay as fallback |
| 3 | Page mapping call itself | each call takes the page-table lock, a remap hold, unmaps and remaps whole views | **slow signal** | 39–53 ms a usual frame, 205–241 ms in long frames, on the game's streaming threads: the game's own loading is throttled | `VICTORY_REMAP_BATCH=1`: one hold and one note per `CopyPageMappingsBatchX` batch; first measure the existing `VICTORY_REMAP_SAME` / `VICTORY_ALIAS_RUNS` (in the tree, not in run-11/12's binary) |
| 4 | View written for a texture with no memory | queued, looked at 1, 2, 4... frames later | layer-invented polling | 3,329–14,534 slots waiting; pass time spent on looks | `VICTORY_QUEUE_ON_MAP=1`: such slots leave the queue and return on a map note |
| 5 | Unmap of pages under a buffer window | ignored; idle timer only | **ignored** | 3.2–3.7 GB of RAM windows; 7.8–9.5 GB let go on idle (how much was copied again: [unproven]) | `VICTORY_WINDOW_UNMAP=1`: a window wholly unmapped goes at the next sweep |
| 6 | Level of a texture unmapped (levels on demand) | tiles kept until the copy goes | **ignored** | VRAM not returned; 0 tiles taken back | fix `VICTORY_LEVELS_GIVE_BACK` (device loss in run-10) after 1 |
| 7 | none (card under its mark) | LRU, idle times | layer-invented, idle in these runs | none seen | leave |
| 8 | Memory query | game never asks | n/a | pool size fixed by console type | none |

Answers:
- **(a)** Both. The layer frees a host copy when the game unmaps it (note → sweep, within about a second, or when
  its slot is found unmapped). Timers and the LRU also exist but freed nothing in these runs.
- **(b)** Neither cleanly. For textures picked by index it starts when the pages are *mapped* (or 8 frames after a
  table slot is written) — before DirectStorage has filled them — and for table-read textures at the first draw.
  DirectStorage completion does not start or restart an upload by default.
- **(c)** Yes, by construction; the share is **[unproven]**. All 43–45 GB were let go under the "source unmapped"
  rule, which also fires on map-in and on the 64-range fallback; 35–38 % of those bytes were made again. Missing
  counter: copies let go under the fallback whose first page is still mapped.

Counters to add (one 5 s line, `VICTORY_STREAM_LOG=1`): pages per `CopyPageMappingsBatchX` kind; let-go reason (own
range unmapped / own range mapped-in / fallback, still mapped or not); slots re-uploaded after a DirectStorage write;
milliseconds from write to first draw; calls of `GlobalMemoryStatusEx` and `VirtualFree`.

Top three by expected effect on loading speed and frame time: 1 (`VICTORY_UNMAP_EXACT`), 2 (`VICTORY_UPLOAD_ON_WRITE`),
3 (`VICTORY_REMAP_BATCH`, after measuring `VICTORY_REMAP_SAME`).
