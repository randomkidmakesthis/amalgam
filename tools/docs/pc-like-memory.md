# PC-like memory

## Engine hook points (Ghidra analyst 10, 2026-11-26; static only, nothing run)

Detail and proof: ledger rows F23-F25, findings doc "Analyst 10". Addresses are for image 1.0.2.1 (base 0x140000000).

| Class of data | Hook | What the layer learns | What it could stop doing | Risk | Run-time check |
|---|---|---|---|---|---|
| Streamed texture levels (the bulk of card memory) | Page bits [0x1471bfb08] (u64 per 64 KiB page: bit 62 CPU-mapped, bit 63 GPU-mapped); or mapping kind at 0x1400f7c20 (edx: 2 = GPU view only) | These pages are never in the CPU view: only DirectStorage writes them | Write-watch, guards, fingerprint looks and compares on them; a RAM mirror beyond the pool page itself | Low: proven for the level streamer | Count watch/guard hits from title threads on pages with bit 62 clear: must be 0 |
| Texture level release / reuse | Free 0x1403716a0 (rcx GPU-view address, edx bytes); alloc 0x14034bc70 return (rax address, size was ecx) | Exact end of use; and that an address now holds a new level (small levels are slab blocks reused with no unmap) | Idle timers and "found unmapped" sweeps for card copies; stale copies keyed by address | Medium: hook under the title's lock 0x149c3e0e8, keep it short | Log each free; no draw may bind that address until the next alloc returns it |
| Texture level arrival | Request 0x1401941b0 (r8 dest GPU-view address, [rsp+0x28] bytes); publish 0x1443c4bf0 (F11) | Which bytes a request fills and when the title starts to use them | Upload on "mapped"; second upload after the write lands | Low | Uploads per level = 1 |
| Engine buffers written by the CPU | Inner lock 0x145121690 return (rcx R: +0x30 object, +0x3c mode, +0 pointer, +0x18 bytes); unlock 0x144e8abe0 entry (match by R+0x30) | Buffer written = whole [ptr, ptr+bytes) when mode != 1; final at unlock | Write-watch and compares on these buffers | Medium: no dirty range (whole buffer); windows can last a frame; completeness unproven | One run with watch + hooks: count watched-dirty ranges in engine buffers outside any lock window |
| Read-back rings | Same inner lock, mode 1 (via 0x14524dab0) | Which buffer the CPU reads this frame | Copy-back of ranges nobody locks | Low (F1 already measured) | Existing consumers meter |
| Per-frame arena | 0x1424749d0 (F21) | Chunks final at list close | Tracking of segment C | See F21 | See F21 |
| Movie planes | 0x14442f760 (takes 5 plane pointers, unlocks at once); Bink writes until BinkDoFrameAsyncWait returns | Plane addresses; final at the wait's return | Watching the planes | Low | Planes change only between the two Bink calls |
| Resource-file chunks (models, dictionaries) | Place call 0x142593e8e (rcx module, edx index, r8 -> {map*, id}); map: +0 virtual count, +1 physical count, +0x18 + 0x18*i {address, size} | Asset type (module), chunk list; virtual chunks are CPU-only pages (never GPU-visible); physical chunks are final after Place returns | Tracking virtual chunks at all; watching physical chunks after Place (if the check holds) | High for "physical never written later": not proven; CPU reads exist (vertex-buffer lock 0x1443a4ba0) | Count writes to physical-chunk pages after Place |
| Resource-file release | 0x144f8ed40 (rcx batch+0x38, rdx map) | Exact chunk list being freed | Sweeps | Low | - |

Reuse policy: virtual addresses come from a buddy allocator and return quickly; physical pool pages are FIFO, so a
re-mapped address shows a different pool page. A card copy keyed by pool page stays exact until that page's next
DirectStorage write; one keyed by address does not.

## Load-read meter (coder G, 2026-11-26) - PARKED
`VICTORY_LOAD_READ_METER=1` (diagnostics, default off; `victory-gpu/src/loadmeter.rs`, hooks in hle dstorage/xmem/runtime,
watch.rs handler + `kernel_writing` + `title_memory_unmapping`, descriptor/draw view records). What it measured: per
DirectStorage completion, the 4 KiB pages of the title's CPU view (pool pages minus GPU-view addresses; plain
destinations) got PAGE_GUARD after our write; the first access per page was taken in watch's VEH (title code + RVA, layer
code, read/write, time since load), released/rewritten/guard-lost pages counted; kind (texture / card buffer / neither)
from the title's views over all of a load's addresses at report time. Lines "load-read meter ..." every 10 s and at exit.
Failures: run-131 died at 2 s (a worker inflating into a 4 KiB page shared with a just-guarded neighbour faulted before the
table knew the page; fixed 183e66b: table first, then the guard). run-133 died at 6 s in title code (null read): the meter
changed what the title sees. Most likely a kernel-mode access to a guarded page (a syscall copy probing it returns
STATUS_GUARD_PAGE_VIOLATION instead of faulting into the VEH, so a title read/write through the kernel fails or a file
read lands short), or a `writable()`/VirtualQuery check of the layer that treats PAGE_GUARD as "not plain read-write".
A fault method on shared title views is unsound here. If the number is needed again (resource-file physical chunks),
use QueryWorkingSetEx accessed bits on CPU-view pages the layer never touches after the load, or static analysis.
F24 answers it for streamed texture levels statically: GPU view only, card-only data.

## Page kinds meter (coder G; table-based since 77ea728) - `VICTORY_PAGE_KINDS=1`
hle `modules/pagekinds.rs` + gpu `pagekinds.rs`. Classes come from the title's own page table [0x1471bfb08] (u64 per
64 KiB page of the streaming region: bit 62 CPU-mapped, bit 63 GPU-mapped; region CPU base [0x149e1dac0], bytes
[0x149e1dac8], GPU base [0x149e1dad0]); our notes of the views (`xmem::Shown::note/forget`) are printed beside for
comparison only (they said 0 MB GPU-only in run-134). The mapping call 0x1400f7c20 is hooked (calls/MB per kind 0-5).
Lines every 5 s: "page kinds ..." (pool MB GPU-only / CPU only / both / unmapped by the table; dstorage MB into each;
host copies of placed textures with all source GPU-only / some CPU-visible, and in which region their addresses lie;
layer work on GPU-only pages: write-watch asks, guard arms/faults, copies read; title-code writes on GPU-only pages)
and "texture levels (F25 ...)" (alloc 0x14034bc70 / free 0x1403716a0: slab/large, reuse with no mapping change, frees
with a host copy over them by age). Run-138: GPU-only 1.2-2.3 GB; copies all on GPU-only pages ~5,300 (1.6 GB) of
16,400 (7.0 GB); title-code writes on GPU-only pages 0.
Kind-0 touch sampling (coder K, step 5; part of `VICTORY_PAGE_KINDS=1`; `VICTORY_PAGE_KINDS_TOUCH_AT=<s>`, default 120,
0 = off): from that time on, every kind-0 ("both") page of the region is taken out of the process's working set once at
its CPU-view address (`VirtualUnlock` of an unlocked range: no protection change, no exception; the next access by
any thread soft-faults it back inside the kernel) and the working set is read every 5 s (`QueryWorkingSetEx`, valid
bit = touched since). Printed on the page kinds line: MB touched / never, by what the page holds (a texture copy, a
buffer copy, neither; either view). Our own reads through the CPU-view address count as touches, so the "texture copy"
and "buffer copy" classes are upper bounds for the title; "neither" is the title's CPU side (or a kernel read for it).
Our section views, the GPU view and our dstorage writes are other addresses and do not count.

## GPU-only pages (B) - `VICTORY_GPU_ONLY=verify` (G), `=1` (coder K, built)
verify (behaviour-neutral): every 64 KiB page the write tracking finds written on a GPU-only page is checked against
the legitimate changes (mapping change, our dstorage write) and our own writers (H's note points): run-138 since the
start 102,325 found, 51 missed, all with no host copy over them.
`=1`: a range is "under the rule" when every 64 KiB page is GPU-only by the title's table, no page is shown at another
title address (`inplace::shown_twice`: a write through that one would be unseen) and none was given back for good.
- `watch::writes` answers such a range from the journal alone: no write-watch ask, no guard arm, no compare (texture
  refresh hash turns and `texture::unchanged` compares are answered from the journal too).
- Known events note the range's untracked blocks in the journal as written whole (`pagekinds::known`): our dstorage
  writes (`kernel_will_write` / `title_memory_written` / end of a kernel write), shader writes landing (`landing`;
  `landed_beside` notes itself), mapping changes (`title_memory_remapped` / `_unmapping`), the level allocator
  returning the address (F25 alloc: a new texture, never "unchanged"), a safety-net hit.
- A block entering the rule, and one leaving it (lost GPU-only status, shown twice now), is noted written whole once;
  after leaving it is on today's tracking at once (never guarded while untracked, so the guard reports it written).
  Copies only partly on such pages stay on today's path whole.
- Lifetime (F25): a GPU-only level free (0x1403716a0) is queued; the next sweep (`resource::let_go_idle`) releases the
  host copies of placed textures that start in it, unless used again since the free (slab handed the address out to
  a texture of the same description: kept, its pages noted at the alloc) or still held by a list/slot (retried). The
  room rule skips copies under the rule (counted "would have evicted"; a skipped copy used again counts "made again
  avoided"), except past 97 % of the budget (counted). The RAM half stays (no discard).
- Safety net (diagnostics, always with `=1`): each second up to 16 MB of copies under the rule are digested and compared
  with what the host copy holds (`holds`; set for untracked refreshes too), skipping copies with a known write since
  the bytes were read; a mismatch is looked at again a second later; still different = "stale GPU-only copy found"
  (must be 0): logged, noted written (uploaded again), blocks tracked for good.
- Counters on the page kinds line, "; VICTORY_GPU_ONLY=1 (per frame over the line): ...".
- run-140 (079ab0e): no copy ever entered (asks answered 0; copies under the rule 56 -> 0) although 8,906 copies were
  all GPU-only by the table: the rule refused any page our record shows at a second address, and nearly all were.
  323622c: another view counts only if our record's view there really shows the page and it is not a CPU-view address
  the title's table calls CPU-unmapped (our runtime may keep it mapped after the title's kind-4 unmap); refusals by
  reason on the line. Lifetime in run-140: 108,000 GPU-only level frees, 93,153 copies (22.3 GB) released, 46,969
  "kept as used again after the free" (the copy was asked for after the free: any use, not proof of a new level),
  made again 10 GB as before; 323622c counts copies released at a free and uploaded again at the same address within
  5 s, and of them those with the same bytes.
- run-142 (323622c): still nothing admitted; every refusal "outside the region": the page is also shown at a title
  view outside both regions (inplace's record holds only the title's mapping calls). Released at a free and uploaded
  again at the same address within 5 s: 46,320 (10.6 GB), same bytes 60 (6 MB): reuse holds new textures, release at
  free is right. Kind-0 touch (from 120 s): texture-copy pages touched 74-121 MB / never 1.9-2.9 GB.
- Fallbacks (counted on the line): level-free queue full -> lifetime rule off for good; a freed copy held 60 s -> left
  to the room rule; safety-net hit -> blocks tracked for good; net look-again notes dropped at a wrap.
- run-148/149 (f7c9cac), read by coder K2: content-safe (stale 0, fallbacks 0; prison: touches 0). "let go past
  97 % of the budget 156,437 (28 GB)" / "331,101 (71 GB)" were NOT evictions: the valve went by the raw card use
  (counts the free space of our texture blocks; 16.8 GB of 15.0) and stood open nearly all run, so the rule held
  nothing against the room rule; the counter added each rule copy the sweep looked at, every sweep. Room-rule
  evictions 23.2 GB (run-146 without the rule: 47.7 GB). Made again 12.8 GB vs 11.2 GB: the difference is the
  release at free, not the room rule: 48,290 copies released at a free were made again at the same address
  within 5 s (11.3 GB), with the same bytes only 66: the slab handed the address to a new level and we made a
  new copy where without the rule the old one is uploaded into (content right either way; the cost is a copy
  made anew instead of re-filled).
- K2: `VICTORY_GPU_ONLY_HOLD=0|1` (default 0). 0: copies under the rule are in the room rule's own order like any
  copy; the release at the title's free stays (the only lifetime information the title gives). 1: the idle sweep
  keeps them; under room pressure the room rule takes every other idle copy first and copies under the rule last,
  least recently used first; no budget valve (the order is the valve). Default 0 because holding by class carries
  no information that a copy will be used again, and on this card (~3 GB holes, ~3 GB targets beside textures)
  every byte held displaces a more recently used copy; 1 to be A/B-measured. "made again by who let the copy go"
  (idle sweep / room rule / room rule's last pass / release at free / real unmap) on its own housekeeping line.
- run-153 (d1de22e, 3 min, HOLD=0): made again by release at the title's free 10,778 (3,172 MB) of 4,790 MB in
  all. 4f548e5: at a free the copy is marked dead, not released. A new level of the same description at the
  address is the same title texture (PLACED_TEXTURES key) and its copy is re-filled in place from the alloc's note:
  the no-rule path for a reused address, no destroy/create. Dead copies with no use are released after
  `VICTORY_GPU_ONLY_DEAD_S` (default 10 s), or at once under room pressure, before any live copy. A new level of
  another description is another title texture: the dead copy goes on the timeout. Counted: marked dead,
  re-filled in place (count, MB), released dead on the timeout / under pressure (count, MB).
- run-154 (4f548e5, dead, 180 s): let go for room 19.6 GB, made again 3.9 GB, against no-rule 9.5-10.5 / 0.8-1.1 GB
  (runs 145/146/151/152). Under constant room pressure the dead copies were released at once (44,654, 12 GB):
  they had been freed moments before and were the likeliest to be used again. Why no-rule re-makes so little: a
  free does nothing to our copy, and the slab gives the address to a level of the same description. That is the
  same title texture, whose copy is uploaded into again once the dstorage write is noted: an upload, never a
  "made again". Dead/re-fill can at best equal this; it can beat it only by freeing room early, and that loses
  while the pool carries ~3 GB of holes. 93a312a: `VICTORY_GPU_ONLY=1` is the no-tracking half alone (the alloc
  note kept); the lifetime half is behind `VICTORY_GPU_ONLY_LIFETIME=free|dead`, default 0. Dropped until VRAM
  holes are fixed.
- K2 linear-view check: all 79 run-148 touches came in one round (14:18:17), all in the first 16 MB of the view at
  0xf00000000, 6 s after a host-memory-pressure warning. f7c9cac watched the linear address taken when the block
  entered; a block remapped to another pool page since (the address stays under the rule) is watched at its old
  pool page, which belongs to someone else by then. Now each touch is noted written (the copy takes the page
  again) and classed: at a linear address the page no longer has (watched at its new one), our own read there
  (`host::read` / `read_sparse`), a known write there, a mapping change of the page in this round or the one
  before (the title's mapping executor works through the linear view, T13); only an unexplained one leaves the
  rule for good, logged with what the address is. "Not watchable" is split: the section has no linear view
  (nothing of the title's to watch; retried each second when one appears) vs no record of the page's section
  (cannot be checked: leaves the rule for good, counted). Mismatches seen once are classed by how the next look
  settled them (a known write had been noted since: a write in flight when the net read; bytes back to what was
  uploaded: a torn read; the copy uploaded anew).
- Kind-0 design (not built): sampling the working set alone is unsound (a write between samples is unseen for a
  period; section views cannot be write-watched). Sound and cheap: arm the guard once on kind-0 texture pages and
  answer from the journal while protected and clean (never re-arm, never compare); a title write faults once, is noted,
  and the page returns to today's tracking.

## Kind-0 texture pages: arm once, answer from the journal (K2, design only, not built) - `VICTORY_KIND0_ARM=verify|1`
Class: pages the title's table [0x1471bfb08] marks CPU- and GPU-mapped (bits 62+63; F24 kind 0, resource-file
physical chunks), 3.7-7.4 GB, ~10,000 texture copies over them; run-142/148 touch sampling: texture-copy pages
touched 74-122 MB of 1.9-2.9 GB (our own reads included: an upper bound), i.e. >= 96 % never touched by the title's
CPU. Admission is by live signals only: every 64 KiB block of the copy has bits 62+63 set, a placed texture copy
lies over it, and every title view of the page is one we can protect (`inplace::other_views` + the linear view;
a view we cannot protect = refused, counted).

Mechanism in fox's guard (`watch.rs`):
- `watch::writes` today asks the kernel first (`kernel_written`) and on view memory falls to `guard()`, which
  makes written blocks read-only again (`protect`) and notes them; a read-only block whose writes are all in the
  journal is answered by its bit (`published`, `PROTECTED`). For an admitted block: skip the kernel ask, `guard()`
  arms it once (PAGE_READONLY, never PAGE_GUARD), `publish` sets its bit; from then on asks, texture refresh hash
  turns and `texture::unchanged` compares are answered from the journal while the block stays protected and clean
  (as `unchanged_by_rule` does for GPU-only pages). Nothing is ever re-armed for it.
- A title write faults once into fox's existing handler (as for every guarded block today): the block is opened,
  noted written, and marked "left" (a bit set): it is on today's path from then on (re-armed by `guard()` as any
  written block, compared as today). So the ~4 % the title writes cost one fault each, then today's cost; the 96 %
  cost nothing per frame.
- Every view is protected: VirtualProtect is per virtual address, a write through the GPU view or the pool's
  linear view would not fault on the CPU view's protection. Arm all title views of the page; a fault at any of
  them notes all of them (`other_views`). Our own windows (dstorage) are ours: `writing` / `kernel_writing` note.
- Known events (our dstorage write, landing, mapping change via `title_memory_remapped`, F25 alloc, kind-3/4
  unmaps) note the block written and drop "armed": re-armed at the next ask (one VirtualProtect).

Cost per frame: arms only for blocks newly admitted or remapped: the title maps ~225 kind-0 calls/s (run-148:
1,125 in 5 s, 334 MB), i.e. ~13 calls/frame at ~17 fps, ~3-5 us each = <0.1 ms/frame, plus one per fault (~2,000
blocks over a run). Answers: a bit test per block (ns). Saved: the kernel asks and guard arms on these blocks
(run-148 CPU-visible: write-watch asks 1,355/frame 12.5 ms, guard arms 65/frame 8 ms; the share on kind-0 texture
pages to be measured by verify).

Failure modes and why this is not the PAGE_GUARD meter that crashed the title twice:
- PAGE_GUARD fires on any access, reads included, once, as STATUS_GUARD_PAGE_VIOLATION; a kernel read or write
  into such a page (ReadFile, ReadProcessMemory in our `host::read`, the console-instruction emulator's reads,
  VICTORY_PATCH_WAITS polls) fails instead of faulting into our handler: short file reads the title takes for file
  contents, failed copies, and a title thread taking a guard exception its own handlers do not expect. This design
  uses fox's PAGE_READONLY guard: reads never fault (title, ours, kernel); only writes do.
- Kernel writes into protected pages (file reads, DirectStorage) do not fault into the handler, they fail. Every
  kernel write in this process is one the runtime issues for the title (all syscalls are ours) and goes through
  `kernel_writing` (opens the range and keeps any refresh from re-arming it until the call returns, `g.kernel`) or
  `writing` (our dstorage): the same path every guarded page uses today. Fallback: a file read returning
  ERROR_NOACCESS / a short count on such a range opens it, retries once, counts, and the block leaves for good.
- Atomics / locked writes / the title's patched waits (PATCH_WAITS) that write: fault like any write (handled).
- VAD growth: protection at 64 KiB granularity splits views into regions; bounded by admitted blocks; counted;
  past a bound (e.g. 100,000 regions) no new admissions.
- Fallbacks (counted, never "clean"): view not protectable -> refused; arm failed -> today's path; fault storm on
  a copy (> N faults/s) -> that copy's blocks leave for good; verify mismatch -> leave for good.

Verify (`=verify`, behaviour-neutral, diagnostics build): admit and arm as designed but keep answering today's way;
at each ask compare: today's path found the block written while the journal says armed-and-clean -> "missed"
(must be 0); a 16 MB/s digest net over armed copies (as the GPU-only net) -> "stale" (must be 0); faults on armed
blocks by thread (title / ours / unknown); kernel writes that failed on an armed range (must be 0); arms per frame
and their ms; the share of today's asks and arms that =1 would spare. =1 only after a 6-minute route and the prison
scene show missed 0, stale 0, failed kernel writes 0.

## Discarding the RAM half (C) - `VICTORY_GPU_ONLY_DISCARD=1` (design only, not built)
Unchanged design, now on top of B=1 as built: discard a page only while it is under the rule (table GPU-only, shown
once, not given back) and its host copy holds what was uploaded (safety net clean); the refill guard is needed for a
page that leaves the rule (CPU view mapped). B's lifetime rule is in place: the room rule does not evict copies under
the rule below 97 % of the budget; past 97 % C would have to refuse the eviction of a discarded page's copy (or never
discard near the budget). Counters as planned: MB given back, refills, rewritten after discard, evictions refused.

## Later topic (user, 2026-11-26): where NPC physics and other simulation run
User: "we will also need to check later how npc's physics and other stuff are calculated by the game because it should
be on CPU if im not mistaken". To establish (Ghidra + a run-time meter), not assumed: which simulation runs on the
title's CPU threads (rigid-body physics, ragdolls, AI, animation blending, vehicles) and which on the card as compute
passes whose results the CPU reads back (candidates seen in logs: skinning / mesh deformation, cloth and hair, particle
simulation "ptfx", GPU culling / visibility, light clustering). Why it matters here: every card-side simulation whose
output the CPU reads is a copy-back for the layer - run-135/137 found 28-35 % of all written pages per frame are our
own landings of shader output, one 92 MB buffer alone taking ~18 MB a frame - and CPU-side simulation is bound by the
title's fixed thread layout (five recorder threads, console affinity masks). Questions: what is that 92 MB buffer
(copy #521/#603, 0xd6b610000); which read-back rings feed gameplay (F1: 27 rings); does any gameplay-visible result
depend on a read-back arriving late (two frames old by design on console).
