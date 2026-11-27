# Ghidra / static-analysis performance findings (running log)

Exe: game_scarlett_master.exe, image base 0x140000000. Tools: `Tools\ghidra-scripts` (peutil.py raw capstone helpers,
d.py annotated disasm, callers.py call/pointer xrefs, sweep.py whole-binary regex sweep in ~8 s, xs.py string xrefs,
gd.sh + Decomp.java = decompile addresses in `ghidra-proj2` to `ghidra-scripts\out\dec\<addr>.c`, ~7 s a batch).

## Engine structures established (reuse, do not rediscover)

- D3D device global: `[0x1475df048]` (ID3D12Device*). Device vtable +0x428 = resource at address (layer `create_at_address`).
- Engine GPU buffer object (vtable 0x1461e0360): +0x08 u32 element count; +0x0c u16 (&0xfff) stride; +0x0e u16 creation
  flags (0x80 = unordered access, 0x08 = has shader-resource view, 0x100/0x200 = gets a D3D resource object);
  +0x10 u16: bits0-1 kind, bits3-4 allocator type, bits7-12 format id; +0x18 CPU address; +0x20 ID3D12Resource* or 0;
  +0x38 GPU address. Byte size = count*stride.
- Buffer creation: 0x1406ebaa0(count, stride, kind, initdata, flags, type, fmt, ..) -> alloc 0x142474690 -> 0x14006dc20
  (thunk 0x144ec54f0) which calls device+0x428 only if 0x1455ac2b0(buf) says so; most buffers have NO resource object.
- Address alias: region [ [0x149e1dac0], +[0x149e1dac8] ) is translated to base [0x149e1dad0] when an address is handed
  to the GPU and back when the CPU locks (two views of the same memory).
- Generic buffer lock (CPU access, read or write): 0x145121550 (thunk 0x144db7900), unlock 0x144e8abe0 (thunk
  0x144e8abd0). Calls ID3D12Resource::Map/Unmap (vtable +0x40/+0x48) only when the buffer has a resource object;
  otherwise hands out the raw address. Takes a "multi-frame buffer" handle: [0..2] buffer objects, +0x18 flags
  (bit1: 3 buffers else 2; bit0: rotate once per frame), +0x20 (index, frame stamp).

## Findings

### F1 (T1) CPU read-back buffers are an explicit engine class, all tiny
See ledger. Constructor 0x141adc2c0 (name "CpuReadBackShaderResourceBuffer [n]"), wrapper 0x141d40f90, 22 call sites:
14036b92e 140cbda74 140cc9062 1415e41e1 1415e42b2 1415ee333 141af4271 141d1d377(x3 via 145aa2440) 141dfee53 141eed83c
14236eb57 142496bb3 143286353 1457011a3 1457461ed 145783c67 145783d02 145837ac6 145a8c7ab 145af6d0f 145af6dad 145d1592f.
Constant sizes seen: 4 B, 16 B, 48 B, 2 KB, 4 KB, 14 KB, 32 KB; five sites have run-time sizes.

### F2 (T4/T5) The five command-list recording threads are hard-pinned to logical processors 8-12
- Thread creation: 0x141ad0fd0(entry, arg, stack, priority, affinityMask, nameHash, flag) -> CreateThread suspended,
  SetThreadPriority, Get/SetThreadGroupAffinity(mask), ResumeThread. Wrapper 0x141ad0ef0 takes a core index
  (mask = 1<<index) or -1 (mask = all host logical processors, ~(-1 << n), n = global 0x1471be5c4 from
  GetLogicalProcessorInformation in 0x141f67680). If nameHash != 0 and the thread table is loaded
  ([[0x14abbeef0]+0x20]+0x4e8 count / +0x4f0 entries of 0x30 bytes, lookup 0x141b67d40), priority and mask are replaced
  from data: mask = OR of 1<<core for listed cores, OR of core groups {0: 0xff, 1: 0x3f00} (table 0x1463b72b8).
- 0x1443c38a0 creates exactly 5 threads "Subrender:%02u" (count global 0x149c3d660 = 5, entry 0x1443c4140, stack
  0x57800, priority 0) with the literal mask 0x1f00 (call at 0x1443c397f). Other literal masks: 0x3f00 at 0x1443adb8e,
  0x1443adc1b, 0x145cb178e; 0x3fff for Bink threads (0x14577fa9b).
- Live process check (NtQueryInformationThread on a running victory.exe, 166 s old): masks 0x1f00 x11 threads (149 s
  CPU; five of them 22-25 s each, priority 0 = the Subrender threads), 0xff x8 (67 s), 0x3fff x11 (46 s), 0x3f00 x3,
  0x2000 x1, 0xffff x80 (the layer's own, D3D background, dstorage). Whole process 546 CPU-s in 166 s = 3.3 of 16.
- Not machine-count driven: n logical processors is used only for the default mask and min(4,n) Bink threads.
  GlobalMemoryStatusEx is only in an info-string builder (0x1455fde00). Console type: 0x1455deb88 reads the registry
  value, 0x140075140 caches "is Series X" (== 6), 70 call sites.

## Submission / pacing map (T4, no finding yet)
- Device global used by the renderer: `[0x148759d00]`. Queues: `[0x148759ff8]` (CreateCommandQueue, graphics) and
  `[0x1475e7038/40/48/50]` (device+0x180). Per-queue "stream" objects: 0x1475e0e98 (queue 0) + 4 more made in
  0x142478440 by 0x145a86180: [0] queue, [1] index byte, [2] fence, [4] last value handed out, [5] value the submit thread
  has passed to Signal, +0x38.. ring of 256 commands x 0x50 bytes, +0x5038/+0x503c flushed/write index, +0x5048 "signal".
- Stream flush 0x1450bb280 -> batch enqueue 0x1401f6330 (ring of 0x60 batches x 0x48 bytes at 0x1475df2a8) -> game
  submit thread 0x142481e90 (created at 0x145caba60, name hash 0xee295129): command type 0 = command list (up to 256
  per ExecuteX = queue+0x118 with the frame token `[0x1475e0e88]`; a list with byte +0x28 set is submitted apart and
  followed by a fixed per-queue list from 0x1475e7300), 1 = callback, 3 = frame start (device+0x280 origin wait ->
  token), other = present (device+0x280 flip wait INFINITE, queue+0x100 PresentX). After each batch: queue+0x80 Signal.
- Flush call sites: 140057300 1401f6130 1402a42d0(x4) 140607530 1409796d0 141a85220 1424846d0 1450bb250 145335cc0
  145eb2a00(x11, "flush all").
- Frame interval: 0x14247ab10 (device+0x270 SetFrameInterval from table 0x1461ece60, then +0x278 schedule).
- Dead end: flip wait on the 33 ms grid in late frames — the layer already ends such waits at once (frame.rs).

## Scripts (C:\GTAVI\Tools\ghidra-scripts)
| Script | Use |
|---|---|
| peutil.py | library: PE sections, pdata function bounds, capstone disasm, RIP-relative xref scan (no Ghidra) |
| d.py `<va> [end or +len]` | annotated disassembly of the function containing va (imports, strings named) |
| callers.py `<va>...` | direct call/jmp sites, lea refs and data pointers to an address |
| refs.py `<lo> <hi>` | all RIP-relative refs to a data range (use for globals; refs2.py = stricter, misses leaf functions) |
| xs.py `<substring>...` | strings containing the text and the code that references them |
| sweep.py `<regex>...` | all functions whose disassembly matches every regex (whole exe in ~8 s, 4 processes) |
| devcalls.py `<globalVA>` | functions using a global COM pointer, grouped by vtable offset called |
| gd.sh `<va>...` + Decomp.java | decompile into out\dec\<va>.c using ghidra-proj2 (no analysis; ~7 s a batch; 4G heap) |
| ExportTopics.java, export-topics.sh `[cap] [Tn]` | model-free export per topic T1–T7 from the FULL project (read-only): call trees 2 levels up, decompiled text, string xrefs, indirect-call lists → out\topics\Tn.txt. Run only after the full analysis has ended; expect 1–1.5 h. Tested for compile/run on ghidra-proj2 (where it finds no xrefs, as that project has none). |

## How to resume
Read the ledger, then this file. For a new question: find an entry (import IAT address via d.py/refs.py, a string via
xs.py, a vtable offset via devcalls.py/sweep.py), decompile with gd.sh, follow callers with callers.py. Import IAT
addresses: Sleep 146793b98, XMemMapPhysicalPages 146793a40 (thunk 145f3fce0), XMemAllocatePhysicalPages 146793a10
(thunk 145f3fcd0), XMemVirtualAlloc thunk 145f3fcc0, D3D12ConfigureVirtualMemoryX thunk 145f3fd60, DStorageGetFactory
146793fb8, D3D12XboxCreateDevice thunk 145f3fd80.
- F1 addendum sent: ring +0x50 = packed write index/frame, +0x4c = CPU read index (previous frame's buffer); reader example 0x1401f48c0 (wind grid 'WindNew').

### F3 (T3) The streaming region has three views of the same pool pages; game CPU code never dereferences the GPU view
- Init 0x141e1cf60 / 0x141e6c110: size = 16 GiB on Series X ((isSeriesX ? 2 : 3) << 33). CPU view base `[0x149e1dac0]`
  = XMemVirtualAlloc(size, 0x20402000, 0x21, 4); GPU view base `[0x149e1dad0]` = XMemVirtualAlloc(size, 0x20402000, 3,
  0x801); size `[0x149e1dac8]`. Pool: XMemAllocatePhysicalPages -> linear pool view `[0x1471bfaa8]`
  (XMemVirtualAlloc + XMemMapPhysicalPages once), registered with device+0x250 (handle 0x1471bfcc8); a queue made by
  device+0x40 {type 3} at `[0x1471bfce8]` carries the mapping batches.
- Batch executor 0x140f2c470 (entered from 0x140f2c460 when the list is non-empty; called by the DirectStorage batch
  submit 0x1400e88f0 on `batch+0x38`; entries appended by 0x1400f7c20 from 0x1428c9910): entry u32 = kind (bits 29-31,
  0..5) | 21-bit virtual 64 KiB page (bits 8-28) | run-1 (bits 0-7). Per batch: ONE XMemMapPhysicalPagesScatter for
  the CPU view (address = page*0x10000 + CPU base, sorted, deduplicated) and ONE queue+0xf8 call whose destinations
  are page*0x10000 + GPU base.
- All 55 code references to the GPU base are rebase/range tests (22 add = CPU->GPU address handed to D3D, 9 sub +
  compares = GPU->CPU before CPU access); the generic lock 0x145121550 rebases even Map() results to the CPU view.
- DirectStorage wrapper ("fiDeviceDirectStorage", methods table at 0x1462fc760; factory global `[0x1478f5e78]`):
  batch object +0x18 queue, +0x30 status array, +0x38 mapping list, +0x64 request count, +0x78 submitted. Create
  0x1428c9460 (capacity n+63), enqueue 0x1428c9680 -> 0x1428c9960, submit 0x1400e88f0 (mapping batch, EnqueueStatus,
  Submit), blocking wait 0x1400e8960 (polls IsComplete with Sleep(1)), poll 0x144f65820.

## State at hand-back
Sent: F1 (+addendum), F2, F3. Open leads, none proven: (a) T7 scheduler above the DirectStorage wrapper (needs
xrefs from the full analysis); (b) texture read-backs through Map in the generic lock; (c) T4: which stream-flush
sites produce the ~58 submissions (needs a run-time counter); (d) T6 untouched beyond the entry 0x14247af60;
(e) number of Subrender threads is the constant 5 at 0x149c3d660 set in 0x1443c38a0 — raising it was not examined.
Dead ends: late flip waits (layer already handles); CPU-count or memory-size driven sizing (none exists).

## T7 work after resume (no finding)
- DirectStorage device vtable starts at 0x1462fc550 (set in 0x141d8e4d0). Slots: +0x210 create batch, +0x218 close,
  +0x220 enqueue, +0x228/+0x230 enqueue variants, +0x240 submit, +0x248 blocking wait, +0x250 add page mapping,
  +0x260 poll. Callers found by `sweep.py "(call|jmp) qword ptr \[r\w+ \+ 0x240\]"` etc. Functions calling submit AND
  wait AND poll: 0x140dabff0, 0x140dac540, 0x141c78860, 0x1426b1640 (loader with request arrays of 0x80-byte
  records at obj+0x10/+0x1020/+0x2030; called from 0x143fb6250), 0x144f657a0 (submit, then SPIN on poll with no sleep,
  then wait; callers 0x1413307e0, 0x1428cda50), 0x145716780 (via pointer 0x1465e5f30). None read through yet.
- Read-back rings by owner (name strings in the creating function): 0x140cbd670 EntityCoarseVoxelization;
  0x140cc89c0 imposter; 0x141dfeba0 visibility_buffer (ring global 0x1475eadc8, mirrors buffer 0x1475eadb8);
  0x1432860d0 terrain_virtual_texture (0x200 x 4 B, ring global 0x1487b2af0); 0x1457010f0 puddle detect;
  0x145a8c1f0 cascade shadow classify; 0x1415ee090 weather particles; 0x1457838d0 lightning; 0x14036b6e0 deferred
  lighting (1 x 4 B, likely exposure); 0x1415e91e0 vfx triangle clip; wind grid 0x14236e090; unnamed: 0x1415e3f90,
  0x141af3ff0, 0x141eea9e0, 0x145745f20, 0x145837770, 0x145af6ba0, 0x145d158a0, 0x145aa2440.
  Candidates for "feedback that drives detail": terrain_virtual_texture, visibility_buffer, imposter. Their CPU readers
  were NOT found (no reader reaches the ring through its global; only the binder 0x140fbf540 and destroy do).
- T2 note: buffer +0x14 = sub-allocation offset inside a shared resource when (u16 +0x10 & 0x2040); view builders 0x1406eb900, 0x145c4d730, 0x145b54c30 (all called from 0x142473850).

## Analyst 2 (after the full analysis finished)

Tool: `ghidra-scripts/q.sh` + `Q.java` on the FULL project, read-only, ~13 s a call: `x:ADDR` xrefs to an address,
`c:ADDR` callers + callees of the containing function, `u:ADDR` caller tree 4 levels up (data refs as @addr),
`d:ADDR` decompile to `out/q/<entry>.c`. Text goes to `out/q/xrefs.txt` (appended; delete to start clean).
Never use `start` from Git Bash (error dialog); priority is set by ghidra-priority.ps1, which q.sh starts.
WARNING on vtable-offset sweeps ("call [reg+0x240]"): they match unrelated classes. 0x1426b1640, 0x145716780,
0x140dabff0, 0x140dac540, 0x141c78860 are NOT DirectStorage users (checked: other vtables) = dead end.

### Read-back ring: how the CPU reads it (corrects "readers not found")
- Ring object (0x78 bytes): [0..2] buffer objects; +0x40 u32[3] stamp per buffer = titleFrame*2+1, set by
  "mark written" 0x1453f1950(ring, cmdctx) (17 callers = the producers; frame counter 0x14abce29c, or *[0x14abce2a8]
  on threads whose TLS +0x12d8 & 6 != 0); +0x4c CPU read index; +0x50 packed write index/frame; +0x54 flags = 2nd
  arg of ctor 0x141adc2c0 / wrapper 0x141d40f90 (seen: 0, 1, 2; bit3 would mean "flush the command stream at once
  (0x1409796d0) and read ungated" but no ring found using it); +0x58.. views.
- Reader 0x14524dab0(ring, record) -> 0x14524dbf0 (advance +0x4c to the next buffer only if its stamp is newer and
  >= 2 title frames old, 3 on non-render threads) -> inner lock 0x145121690(record). No fence, no wait: validity is
  assumed from frame count alone. 0x145121550 is a different wrapper of 0x145121690, so a hook there sees no ring
  read (run-33: "game locks 0" on every ring). Call sites of 0x14524dab0: 1403dc0bf 14082c80c 140873d38 140a33503
  140a9ab1f 140a9ad26 140d3bfae 140fc0147 1410734e2 1410749f5 141355256 1424e7afd 143277dc6 143277f6f 14328f42d
  143365807 1433690e3 1433692ce 145579961 145cdf134.
- visibility_buffer ring [0x1475eadc8] (4 elements): created, never read through the global = not a detail driver.
- Rings 0x14a92c440 (0x200 x u32, site 0x145837acb) and 0x14a925418 (0x200 x 0x1c, site 0x141af4276): read by
  0x140a9aad0 into double-buffered copies 0x14a92c450 (2 x 0x800) / 0x14a925420 (2 x 0x3800); consumer 0x140264220
  uses them as per-object GPU bounds for 512 slots (bitmask 0x14a92d500) — not streaming. Not followed further.

### F4 (T7) Terrain virtual texture = GPU feedback; nothing arrives in the layer
- Init 0x1432860d0 ("terrain_virtual_texture"): feedback target 240x135 fmt 0x2a (object 0x1487b2ae0, view
  0x1487b2ae8), ring [0x1487b2af0] 0x200 x u32 with CPU copy 0x1487b2b08 (0x800, zeroed), shader slot ids
  0x1487b3342/44/46, page-table dims 0x1487b3358 (w) / 0x1487b335c (mips), cache dims 0x1487a1158/5c, mip count
  0x1487a1160, 5-frame average window 0x1487b3318 = 5.
- Per frame 0x1409ad250 -> 0x14328c090 (first frame: 0x141749f80 sets byte 0x1487ba1a0 = 1) -> 0x140fbeab0:
  0x1454e1c70 ages cache pages (entry 0x38 bytes: +8/+0xc page x/y, +0x10 mip, +0x24 age, -1 = free; counts pages
  with age 0 into 0x1487b2a88, max 0x1487b2a8c); 0x140fbf540 3 compute dispatches (feedback target -> ring), mark
  written, read ring -> memcpy to 0x1487b2b08; 0x140fbeb10 count = [0x1487b3304] (element 0x1ff), min 0x1fe, entry =
  x<<20 | y<<8 | mip, each -> 0x140fbec90 -> 0x140fbed20 (cached: age = 0; else insert into request set 0x1487a1170)
  plus the parent mip; 0x140fbef30 adds pinned pages (set 0x1487ac190); 0x1454e1f00 turns requests into pending
  pages (0x140fc0330); 0x1454e1b40 collects pending into list 0x1487b09b8 (count 0x1487b09b4); 0x140fbeba0 sorts and
  takes AT MOST 25 (0x19) per frame into the build list 0x1487b29c0 (count 0x1487b29bc); 0x140fbed90 mip bias
  [0x1487b6614] = lerp([0x1487b3308], [0x1487b330c], min(1, avg(touched/cache) * [0x1487b3310])).
- run-33 meter: "feedback ring terrain_virtual_texture: landed 0 times" in every period -> count stays 0.

### T7 leads not finished
- The real streaming scheduler was NOT reached. Reliable anchors: mapping-entry appender 0x1400f7c20 (callers
  0x14014c640, 0x144f8ed40, 0x144f8ef80, 0x1452384c0 = resource placement, reached through file-device vtables
  0x1462fc7b8/c0 and ten sibling vtables @1462fd6e8.. -> 0x1428c7020/0x1428c7050); upward: 0x145014330 <-
  0x141962d10 (6 callers), 0x142590e60, 0x142499630. Global [0x1479f6d70] (+0x10 = the DirectStorage device) is used
  only by a small synchronous metadata reader (0x1428cda50 -> 0x144f657a0), not the scheduler.
- Pool sizes: tables at 0x1471bf9b0.. read in 0x141e1cf60 (per-pool sizes, not followed).

### F5 (T7) Streamer pump: frame-gated (replaces the "not reached" lead above)
- Found via the batch pool: `sweep.py 'call qword ptr \[r\w+ \+ 0x210\]' 'mov (edx|r8d|r9d), 0x1000$'` -> 0x1428cb7d0.
- DirectStorage batch ring 0x149c3e070..c1: +0x00 [0x149c3e070] bytes per batch, 0x149c3e078 bytes in flight,
  0x149c3e080 device (&0x1478f5e80), 0x149c3e088 batch array (16), 0x149c3e090 current batch, 0x149c3e098 serials,
  0x149c3e0b0 requests in current batch, 0x149c3e0b8 = 0xfff cap, bytes bc/bd/be/bf/c0/c1 = current / next /
  oldest-in-flight / count / in flight / free. Enqueue 0x1401941b0, submit 0x140194110, next batch 0x144f8eca0
  (blocks in 0x1428c8c90 when all in flight), budget 0x144f2a440/0x144f2a4d0, scope ctor 0x140054df0 (lock
  0x1478f6940, reserved requests 0x1478f6968, reserved bytes 0x1478f6970; 512 MiB cap), scope end 0x1450498a0.
- Loader: enqueue group 0x1443c50a0 (from 0x1401b2bc0 <- 0x1401b2b30 <- 0x141474e00 ...), pump 0x144fd22d0(force),
  issue 0x140122ce0, in-flight/finish 0x140121f20, lists 0x149c3d808 / 0x149c3d818 / 0x149c3d828 (tails +8), lock
  0x149c3e040, per-resource table [0x149c3e108] (0x10 bytes: +8 refs, +0xa/+0xb pending down/up, +0xd level, +0xe
  lowest level in flight), type table 0x149c3e1d4 (0x38 bytes per type, index = record u32 & 0xfff), pending-byte
  counters 0x149c3e148..168. Frame counter 0x14a9437ac. Stamp immediates: 0x1443c5409 `add eax,2`, 0x140122147.
- NOT done: who decides the target level (callers of 0x1401b2b30 / 0x141474e00: 0x141c82b20 with 24 callers,
  0x142596530, 0x141e10090, 0x1421b7970) and whether levels move one step at a time; the per-type flags in
  0x149c3e1d4 (bit 27 tested in the enqueue).

## Analyst 3

### F6 (T7) Streamer admission, budget and pressure rule
- Correction: 0x140075140 = device type == 6 = Series S (Lockhart), cached byte 0x1471b63b0 (NOT "is Series X").
- Correction: 0x149c3e120..168 = ten i64 BYTES committed per request category 0-9 (not "pending"); stream.rs
  multiplies by 0x1000 once too often.
- Request entries: pool [0x1498cc2d0], 0x10 bytes: +0 prev, +2 next (u16 index+1), +4 u32 (bits 0-14 index, 15-22
  table at 0x1498d4b40[..], 23-25 target, 26-28 current, 29-31 list id), +8 u64 (category = >>0x33 & 0xf).
  Lists: heads/tails u16 pairs at 0x1498d26a8 (q0) / ac (q1) / b0 (q2) / b4 (q3) / b8 (q4 = free pending).
  Move between lists 0x144dabe80; pop after enqueue 0x143e43980. Request level r -> internal r+1, 4 -> 0 (0x144f9a600).
- Group builder: streaming thread loop 0x142592c50 -> 0x1401b2b00 -> 0x1401b2b30(force) -> 0x1401b2bc0: alloc group
  0x1420cf700(budget-used), fill from queues 3,2,0,1 (0x144f9a3d0 -> can-add 0x1400fcf00, add 0x1400fcfb0), enqueue
  0x1443c50a0 (drop 0x1420cf7b0 on -1). Also once per frame from main update 0x1421b7970 -> 0x141474e00.
  Caps: [0x149c3e1c0]=0x80 first loads, [0x149c3e1c4]=0x200 entries, [0x149c3e1c8]=0x400 units; set by 0x143bdd9b0
  (arg 1 = loading mode: all 0x100000; switch 0x1406ed1c0). One pending op per resource (table +0xa u16).
- Force: 0x141474e00: > 100 ms since frame counter changed -> caps ignored + pump(1). Loading loops only.
- Pump callers: 0x14011fd60 (frame end, pump(0)), 0x1420cf800 (drain loop), 0x14227b6c0 (pump(1)), forced path.
- Stamp frame+2 at enqueue only when the group contains a DOWNGRADE below the lowest level in flight (bVar4);
  pure-upgrade groups have stamp 0 and issue at the next pump.
- Busy table [0x1471bfab0]: per-page count of outstanding DirectStorage batches (inc at submit 0x1400e88f0, dec at
  wait 0x1400e8960 / poll 0x144f65820 via 0x144f65910); 0x144fd2870(range) = "I/O still outstanding on these pages";
  gates issue of downgrades (0x144fd26e0) and finish (0x144fd2620); both stop the whole list (head-of-line).
- Budget: [0x149c3e060] 0x185c00000 (X) / 0xf8900000 (S), set in 0x142479690 with pools 0x1498d26e0.. ; used
  [0x1498d26d8], peak [0x1475ddb98], reserve 0x14221c0c0, release 0x14221c130, lock 0x1471bfad8. Over-budget flag
  byte [0x1498d26d0] (also set in 0x14014c640). Slots [0x149c3e118] / max [0x149c3e11c].
- Pressure: 0x142279a30 per frame latches/clears the flag; rising edge -> 0x14227a6b0 escalates (order table
  0x14619b5b0 = 0,5,1,2,3,4,6): float [0x149f36104+4*cat] = 6.0 = minimum score per category, tested in the entity
  scanners 0x1400967e0 and 0x14266a030 against score 0x144e7cd20 (object u16 +0x96 & 0xf = category);
  [0x1498cc2a6] = 1 -> eviction target [0x1498cc298] (0x143e446e0, 0x1401b2cd0). Reset on falling edge.

### T7 addendum: who sets the target (answer at streamer level)
- Per resource entry +8 u64 = four ref counters (bits 0-14, 15-26, 27-38, 39-50 = request levels 0..3; add/sub
  masks at 0x14655bd40 / 0x14655bd60). Apply 0x144dabcb0(handle, level) -> 0x144dabd20: target = highest level with
  a live reference, clamped to the type's level count (0x144de7e10 / 0x144de7da0); no reference -> 4 = unload.
  No global clamp, no memory term here. Queue choice 0x144dabda0: no refs -> q3; downgrade of a loaded resource ->
  q2; else q1; 0x143e42980 -> q0. Builder drains 3,2,0,1 (frees first, upgrades last).
- Request objects: array [0x1498cc230] count [0x1498cc22c] (state byte +0xc, 3 = dead), incoming lock-free list
  0x1498cc258..78, processed in 0x1401b2e60 -> 0x144dab690 -> 0x144dab6b0 -> 0x144daba20. Producers (game systems
  choosing a level per resource) NOT enumerated.
- Entity score 0x144e7cd20: frac = min(0.95, 1/(obj dist field +8)) plus class 1 (default) / 2 / 4 / 5 / 6 / 7
  (7 = visible and passes box test 0x144e7d1b0); compared with pressure thresholds 0x149f36104[cat].
- NOT done (Q1e): what a "stand-in" drawable is and what swaps it.

### F7 (T4) Submissions = unconditional context ends
- 0x140056cb0(ctx) (440 callers) -> 0x140056f60(ctx,&tok,1) -> 0x140057300 (close list 0x1402a42d0, flush queue-0
  stream 0x1450bb280) ; linked second context ctx+0x12188 (byte +0x121a4) flushed via 0x1456a4960 -> 0x140094460;
  0x1456a4940 (27 callers) same for the second context type. Token discarded by both wrappers.
- Fence wait 0x144fb0a50(token, flag) (queue index = token>>56; table 0x1468929d0), callers 0x1409796d0,
  0x142474740, 0x144fb0910, 0x145eb2a00.

### F8 (T6) ExecuteIndirect: count always 1
- ExecuteIndirect = command-list vtable +0x1e8 in this title (not 0x1d8). Sweep: `sweep.py 'call qword ptr \[r\w+ \+ 0x1e8\]'`
  (175 sites; the real ones load rdx from a 0x1475e75xx global and set r8d = 1).
- Signatures (0x142478440, device+0x148): [0x1475e75d8] 0x10 draw; [0x1475e75e0] 0x14 draw indexed; [0x1475e75e8] 0xc
  type 0x69; [0x1475e75f0] 0xc dispatch; [0x1475e75f8] 0xc dispatch mesh (type 10).
- Class A wrappers (buffer object + offset): 0x14248d000, 0x14248d350, 0x14248ca90, 0x144e33e10, 0x140094360.
- Class B wrappers (CPU pointer in shared arena [0x1475df108], base [0x1475df140]): 0x14248d1b0, 0x14248d500,
  0x14248cbe0, 0x14555e800, 0x14248cd20.
- All ten set bit +0x121a4 = 1 on the context (the "has work" flag tested by the context-end flush, F7).

## State at hand-back (analyst 3)
Sent: F6 (admission/budget/pressure), F7 (submissions), F8 (indirect count = 1). Not done / next steps:
- Q1e stand-in drawables: not started. Start from the entity scanners 0x1400967e0 / 0x14266a030 (what happens to an
  object skipped by the threshold) and imposter ring owner 0x140cc89c0.
- Q1b producers of streaming requests (who picks level 0..3 per texture/model): enumerate callers that fill request
  objects (array [0x1498cc230]); request apply 0x144daba20 / 0x144dabcb0.
- Streaming thread wake-up: event handle at thread object +0 (0x142592c50 waits in 0x142219570 when idle); who
  signals it was not found.
- T6: bounds of shader-written records; the [0x1475d5ac0] signature set.
- T2: arena [0x1475df108].
Dead ends (analyst 3): vtable +0x1d8 sweep for ExecuteIndirect (wrong slot for this title); looking for a
game-side condition on flushes (none); frame-time throttle in the streamer (none beyond the 100 ms force).

## Analyst 4

F6 meter (run-43): pressure never active (used 1.8-3.7 GB of 6.2). Budget accounting is not the cause of low detail.

### F9 (T7) Slot ceiling is a high-water mark
- Slot alloc 0x1455cc180: count [0x149c3e118]++, then [0x149c3e11c] = max(mark, count). Free 0x14516b6b0 (<- 0x14516b5d0
  <- finish 0x140121f20 / 0x140108700 when refs == 0). Group enqueue 0x1443c50a0 @1443c511f: new+count > mark -> -1.
  Direct load 0x1400e9be0 (6 callers) allocates with no test. Table init 0x143bddca0(n) <- 0x141d4b840:
  n = [[0x1498d26a0]+0xc] + 0x1000, stored [0x1492a7f4c]; free-list head [0x149c3e1b8].
- On -1 builder 0x1401b2bc0 drops the group (0x1420cf7b0) without popping queues -> rebuilt next pass.

### F10 (T3) Issue and mapping run on the frame thread
- 0x1401b2b30(p): 0x145074180; 0x1401b2e60 (incoming requests); 0x1401b2b80; 0x1401b2cd0; builder 0x1401b2bc0(p);
  pump(1) only if p != 0. Streaming thread passes 0 (lock 0x1498cc238 around it; main 0x141474e00 takes the same lock,
  p = "more than 100 ms since frame counter changed").
- Frame end 0x14011fd60 -> pump 0x144fd22d0(0): lock 0x149c3e040 entry..exit; loop issue 0x140122ce0 per due group
  (scope 0x140054df0 trylock 0x1478f6940 + budget 0x144f2a440: `cmp rdx,0x20000000` @0x144f2a483); then finish
  0x140121f20 (completion poll 0x1400065c0: oldest batch IsComplete vtbl +0x260, retire 0x1428c8c90 = vtbl +0x248
  wait); then to-finish list (stamp frame+2).
- Submit 0x140194110 -> vtbl +0x240 = 0x1400e88f0. Next batch 0x144f8eca0 blocks in 0x1428c8c90 when
  current == oldest (ring of 16 full). Executor 0x140f2c470 holds 0x1471bfad8 only while building lists.

### Scene scanner (Q1c, no finding)
- 0x1400967e0 (<- 0x144f2ab60, 3 callers) walks 0x20-byte visibility entries (entity ptr, flags), entities with
  +0x28 == 0 (no drawable) and archetype +0x20 (flags byte +0x84: 8 / 0x40), score 0x144e7cd20 vs threshold
  0x149f36104[cat]; appends to [0x149f160c8] (0x18 bytes: entity, id, score; capacity [0x149f160c0] = 0x3000, count
  [0x149f160c4]) or to 0x149f160f8 (16 bytes, cap 0x2000, count [0x149f160f4]). Consumer 0x14266a030 (<- 0x14089ff30):
  walks from the end (highest score), at most 20,000, stops below threshold; 0x144e68560 false -> sets archetype
  +0x84 |= 0x40; requests through 0x140004ce0(streamer, list, flags 2 / 0x23, priority 0 or 8). Clear 0x14080b8a0.
  No meaningful per-frame cap. Request objects: only creator 0x143e429c0 <- 0x1402eaeb0(out, id, level 0..3, ..)
  with 34 callers (list in out/q/xrefs.txt) — not classified.
- Streaming thread object [0x147bc5d90] (0xde78 bytes; +0 event, +8 thread); waits on the event only when its three
  work lists (+0x278, +0x2f0, +0x398) are empty; signaller not identified (readers of the global: 0x142590e60,
  0x1425948f0, 0x14258be00, 0x1409d94a0.. see xrefs.txt).

### F11 (T7) Texture level publish
- Finish 0x1443c4bf0(res, entry): slot = [0x149c3e108] + u16(res+6)*0x10; obj = slot+0; swap obj+0x18 <-> entry+8
  (new/old address), obj+8 size, slot +0xc level, +0xb pending-up count; then 0x14555eb70(obj, 2, firstMip).
- 0x14555eb70 (callers 0x140121c70, 0x1443c4bf0, 0x1443c4d90): obj+8 == 0 -> copy default record
  [0x1475e7540 + 8*byte(obj+0x3e)] (0x20 bytes) into slot [view+0x18] (view = obj+0x28; memcpy unless
  [[0x148759d00]+3] != 0 -> device vtbl +0xc0), then 0x144ecc830; else 0x144ecc510(view, firstMip, mode): SRV with
  MostDetailedMip = firstMip, MipLevels = byte(obj+0x42) - firstMip. Texture object: +0x3c u16 array size, +0x3e
  dimension, +0x3f format, +0x42 mip count, +0x44/+0x45 flags.

## State at hand-back (analyst 4)
Sent: F9 (slot ceiling = high-water mark), F10 (freeze mechanism), F11 (texture level publish + deciding meter).
Not done: Q1c classification of the 34 request producers (callers of 0x1402eaeb0); model stand-in swap; streaming
thread signaller; Q3 / T2 arena. Tool note: q.sh takes ~9 s per call with victory.exe running; `u:ADDR` = caller tree.

## Analyst 5

### F12 (T7) Group "drops" are empty groups
- Enqueue 0x1443c50a0 returns -1 on: (a) no entries (group+0x68 == ring+0xc), (b) slot test @0x1443c511f, (c) budget
  reserve 0x14221c0c0(group+0x7c << 16). Group: +0x24 budget-used in 64 KiB units, +0x60 entry ring ptr
  (&0x149c3e1a0: +0 base, +8 read, +0xc write, +0x10 size = 0x8000 entries of 0x10), +0x68 start, +0x6c end, +0x70
  entries, +0x74 units, +0x7c 64 KiB blocks to reserve, +0x88 stamp, +0x90 first loads.
- Fill 0x144f9a3d0(out count, group, queue, force): entry with current == target (bits 26-28 == 23-25) is counted
  only; can-add 0x1400fcf00 false -> entry rotated to the queue tail; add 0x1400fcfb0 returns 1 (does not fit
  budget-used; sets [0x1498d26d0]) or 2 (entry ring full) -> stop. Pop 0x143e43980(count, queue) only after a
  successful enqueue, so at-target entries stay queued until a non-empty group passes.
- run-48 (ceiling 44,095): drops in 150 of 270 s -> not (b); budget far -> not (c). <= 1 per frame from main
  0x141474e00; bursts of thousands/s from the streaming thread loop (spins while its work lists are non-empty).

### F13 (T7) Model path
- Scanner 0x1400967e0: entity+0x28 == 0; archetype+0x84 & 8 -> list B [0x149f160f8] (count [0x149f160f4]);
  else (& 0x40 clear) list A [0x149f160c8]. Visibility entry 0x20 bytes: +0 entity, +8 float (score = 1/x capped
  0.95 + class), +0xc byte (bit 0 = valid), +0x10 u64 visibility bits >> 8, +0x18 u64 (bits 33-36 = kind, mask 0x759e).
- 0x140c14b30 (thunk caller 0x140c14b20, job table 0x14ac2dbf0): list B -> create drawable 0x145292eb0(entity)
  (type byte +0x30: 1/5/6 -> 0x1404685a0, 4 -> 0x140468740, 10 -> 0x140468960, 0xc -> 0x145292ef0, 0xd ->
  0x1404689e0, 0xe -> 0x1435a33b0). Cap 0x333 per pass only when [0x14ab44324] & 0x1000 (or countdown byte
  [0x149f360fc]) and count >= 0x334; processed count [0x149f360f8]. 0x1408aadc0: entity vtbl +0x1f8 (e, 2), clear.
- [0x14ab44324] = scene-load request state word, 0x14227bd80 (<- 0x14227bba0) over requests [0x14ab442d0] (count
  [0x14ab442cc], state byte +0x28). Not frame-time driven.
- 0x14266a030: asked-ids set [0x149f16038] (size [0x149f16030], count [0x149f16034]); [0x149f16040] urgent count,
  [0x149f16044] consecutive-pass counter, byte [0x14875c029] = urgent missing asset exists. Asset known test
  0x144e68560 (u16 table [0x14abcc598], bitset [0x14abcc5a0]).
- Streaming thread 0x142592c50 = asset state machine: active items array obj+0x160 (count +0x15c), per item
  0x142593560(obj, item, batch{cap 0x800}, &advanced) with state item[2] & 0x1f (0..0xf; < 0xd loading); completed
  list obj+0xbe78 (count +0xbe74); then builder 0x1401b2b00, 0x140ac3f30; waits 0x142219570(event) only if
  obj+0x278, +0x2f0, +0x398 are all zero.

### LOD distance lead (analyst 5, not finished)
- Scan kick 0x14080d250 (<- 0x14080cea0 <- 0x143d83340): copies scan base info 0x1484dce40 (0x370 bytes) to job
  params 0x14854fe90, sets callbacks [0x148550a60] = 0x144f2ab60 (-> scanner 0x1400967e0) and [0x148550a68] =
  0x144f2ea50, result arrays 0x14940bff0 / 0x14957f170 (0xb98c), 0x1493f6820 (0x157c). The per-LOD distance test is
  in the scan job that fills these arrays, driven by the 0x370-byte block: writers of 0x1484dce40..0x1484dd1b0 are
  where a LOD-scale float would be set. NEXT: `refs.py 0x1484dce40 0x1484dd1b0`, keep the float writers.
- Not LOD scale: floats [0x14801daf8]/[0x14801dafc] (copied to 0x147528f60/64) belong to the two imposter managers
  [0x14928e5e0]/[0x14928e5e8] (0x1f00 bytes, resource "im", init 0x145b1b510; distance defaults 5.0 / 32.0 at
  0x14801daf0 / 0x14801dae8), changed only by tunables 0x14577ae10 (hash lookup 0x14501aca0) when [0x149aa53b8] == 1.

### Per-frame / per-pass caps met (Q3 list)
| Where | Cap |
|---|---|
| resource group builder 0x1400fcf00 | 0x80 first loads, 0x200 entries, 0x400 units of 64 KiB per group; one group per builder pass (main: one a frame; streaming thread: one a loop) |
| pump issue 0x144f2a440 | 512 MiB in flight |
| group entry ring | 0x8000 entries ([0x149c3e1b0]) |
| asset requests 0x14266a030 | 20,000 candidates a pass, list A cap 0x3000, list B cap 0x2000 |
| drawable creation 0x140c14b30 | max(819, visible) a pass, only in scene-load mode |
| asset state machine 0x142592c50 | batch array of 0x800 items a loop (state 3 collects into it) |
| terrain pages 0x140fbeba0 | 25 a frame |
Streaming thread wake-up: it does not need one while any asset is loading (it polls); it waits on the event only
when obj+0x278/+0x2f0/+0x398 are all zero. Signaller still not identified.

## State at hand-back (analyst 5)
Sent: F12 (drops are empty groups; F9 closed), F13 (model path + meters). Not done: LOD distance test / LOD scale
(lead above), states of 0x142593560, event signaller, T2 arena. Tool note: xs.py crashes on non-UTF8 strings.

## Analyst 6

### F14 (T7) List B is "visible, loaded, no drawable"; creation is refused, not churned
- [0x149f360f8] = list-B entries handled this frame (copy of count [0x149f160f4]; cap [0x149f160f0] = 0x2000, set
  in 0x145b64be0). Entry 0x10 bytes at 0x149f160f8: +0 entity, +8 f32 score.
- Pass: 0x140c14b30 -> 0x145292eb0 = PRE-ALLOCATION only (0x1404685a0 -> 0x140468640 (0x1b0 block, array
  [0x148d4aaf0] count [0x148d4aaec], spare counter [0x148d7ab40]) / 0x1404686c0 (0x150, [0x148d49ae0]) /
  0x140468a60 (0x180, [0x148d58bd0]); consumed by 0x1400b5c00 / 0x1400b5e30 with index [0x148d7ab00] /
  [0x148d7aafc]). Worker 0x1403447a0(start, stride): entity+0x28 == 0 -> vtbl +0x1f8 (e, 1), entity+0xc8 = score.
  0x1408aadc0: flag [0x147c18b80] = 1, vtbl +0x1f8 (e, 2) for all, count = 0. With the flag set 0x140f5eec0 queues
  (entity, result) into [0x14957f1c8] (count [0x14957f1c4], cap [0x14957f1c0]).
- vtbl +0x1f8: base 0x144f3fde0 (vtables 0x1462e3fe0, 0x1462e68f0, ...), derived 0x140f5eec0 (vtable 0x14633e180).
  Flag 1 = create: needs archetype+0x84 & 8; gate @144f3ff19: (+0x31 bit 7) or (u32 +0x34 & 0x4001) or
  (+0xb4 & 0x3e) in {0xe, 0xa}; model pointer 0x144e91330(archetype) (stores 0x14a92e7d0 kind 2, 0x14a92e8a0
  kind 1, 0x14a92e700 kind 3 + sub-index archetype+0x82) must be non-null; type switch 0x144f407b0 (1/5/6 ->
  0x1400b5b60, 2, 3, 4 -> 0x144f42a90, 8, 10, 0xc, 0xd, 0xe). Flag 2 = post-init, only if entity+0x28 != 0.
  0x144f40eb0 (entity not +0xb3&1, archetype+0x51&1, lod field 0x18) = shared-drawable case (0x144f40cd0).
- entity+0x34 bit 0 = "is visible": setter 0x140832d70 -> 0x1409ad760 pushes it to the scene-graph entry (handle
  entity+0xa4, containers [0x1498e6400]/[0x1498e6408], 0x1409ad870); 0x1409ad650 derives it from entity+0xe8.
  entity+0x31 bit 7: set by 0x141099260 (object vtable 0x1462e77d0), cleared by 0x141013330.
- Removal: vtbl +0x200 (e, flags); base 0x140151bb0 (flag bit 0: frees via 0x145567f70, entity+0x28 = 0); derived
  0x14501d420, 0x14501d330 (vtable 0x14633e180), 0x140536d80, 0x1416cd490, 0x140d895f0 all call the base. Callers
  are virtual (77 functions call some +0x200 slot): not classified; no per-frame sweep found.
- run-50: plateaus 176 / 118 / 114 for 10-25 s. The 8192 of the first 53 s is the counter's INIT value
  (0x145b64be0 sets [0x149f360f8] = 0x2000) = pass not run yet; not a full list.
- Post-creation queue 0x14957f1c8 (0x10: entity, flag byte; count [0x14957f1c4], cap [0x14957f1c0] = 0x800 set
  @142be73be, reset 0x1407ce890): pushed by 0x140f5eec0 for every list-B entry of its class when [0x147c18b80] == 1;
  full -> push skipped (0x140f5efdd -> 0x140f5f05e). Consumer 0x1402b7e30(start, stride) -> 0x142bf7ab0(e, flag, 1)
  (340 lines, not read). Outside the pass the same step is scheduled through 0x1402e5b80(6).
- Q3 (pools) not done beyond: entity pools table [0x14abbe7c0 + 8*type] (+0 base, +8 element size, +0x10 used,
  +0x20 free head, +0x28 generation/flag array, +0x30 mask); shared-drawable pool [0x14999b100] (same layout; an
  exhausted pool hits int3 in 0x144f40cd0); list A cap 0x3000, list B cap 0x2000, queue above 0x800.

### F15 (T7) LOD scale manager
- Object 0x14a15e080: +0x00 defaults (set by 0x145efeee0(isSeriesX)), +0x80 current block (0x14a15e100), +0x100
  render-thread copy (0x14a15e180, copied by 0x140229ca0), +0x180 base f32 [0x14a15e200], +0x184 override value,
  +0x18c final/base, +0x190/+0x194/+0x198 second blend (start, timer, duration; init 10.0), +0x19c..+0x1a8 remap
  (in lo, in hi, out lo, out hi), +0x1ac blend timer, +0x1b0 blend duration, +0x1b4 previous value, +0x1b8 flags u8
  (bit 0 cut active, 1 override, 2/4 second blend, 3 remap, 5 changed, 6 is Series X). Reset 0x145c7c840.
- Update 0x143e571f0: inputs [0x14ab4a20c] (multiplier; writers 0x140496bf0, 0x140497aa0), per-type overrides
  [0x14ab4a210..0x14ab4a22c], base 0x14045f460 (= [0x14a15e200], max 1.0 in scene load), FOV scale 0x143298360,
  state block 0x1498d7810..0x1498d7860 (bytes 7814/7818/7819/781a/781d/781e/781f/7820, f32 7844, counter 7840).
  Outputs: [0x14a15e100] global, [0x14a15e104], [0x14a15e108], [0x14a15e110] (= [0x14a15e090] x factor),
  [0x14a15e118..], [0x14a15e128], per-type [0x14a15e12c..0x14a15e144], [0x14a15e148..], [0x14a15e168..],
  [0x14a15e178], [0x1498e6440] (struct also read through 0x148fdbd80 on the render thread), [0x1498e6438].
- Scan kick 0x14080d250 @14080d8e8 loops LOD types reading manager +0xac + 4*i (main) / +0x12c (render) into the job
  params; also [0x148550a50] = [0x1484ddb90], [0x148550a54] = [0x146c69808] (camera-speed term, default 30.0, writer
  0x1404e5860), [0x148550a58] = 20.0. Job entry 0x1406ed210 copies 0x14854fe90 (0x370) and calls 0x145385f30.
- Scan base info 0x1484dce40: +0x60..0x8c camera matrix rows, +0x260..0x29c frustum planes (SoA), +0x2a0..0x32c
  derived plane data, +0x300 param, +0x330.. filled in 0x14097db60 (view setup, 1,723 lines). No LOD floats.
- 0x144f2ab60 (scan callback before the scanner): for entries of class (bits & 0x1c0000000) == 0x100000000 does a
  box-distance cull against radius f32 settings+0xe4 (+0x11c if entity+0x94 & 1), settings = 0x144f2b4a0()
  ([0x148527dc8]+0x140 main / +0x2b0 render, static default block 0x146ade5c0 all zero except +0x7c 4.5, +0x150
  10.0); clears the visibility bits of mask [0x1498e6428] only.

## State at hand-back (analyst 6)
Sent: F14, F15. Not done: Q3 pool list; removal callers; LOD compare inside the scan job. See ledger T7 row.

## Analyst 7

### F16 (T7) Console polarity closed; per-type LOD overrides are authored scene data
- 0x1455deb88: registry console mode -> device type (0x50->4, 0x58->5, 0x60->6, 0x64->7, 0x68->8, failure->1).
  0x140075140 = (type == 6) = Series S. Layer answers 0x64 (xsystem.rs) -> false. 0x145efeee0(isSeriesS): true ->
  [0x14a15e090] 0.5, [0x14a15e0c8] 0.3, [0x14a15e0dc] 0.8; false -> 0.8 / 0.5 / 0.9 (measured = Series X set).
  Manager flag bit 6 of [0x14a15e238] = is Series S (F15 text and the meter label have it reversed).
- [0x14ab4a210..228] writer: 0x140497aa0 (<- 0x140495020 <- 0x1452a99c0), only when byte [0x14aaec154] == 1 (set in
  0x144c64590 from the scene object's state, cleared by its flag word +0xb4 bit 3). Handle [0x148002edc] (copy of
  [0x148002ed8] made by 0x14122a800; claimed 0x143145be0, released 0x14024a990) -> object (0x144eda870) -> +0xab8 ->
  +0x140 or (+8)->+0x50: 14 floats, -1 = unchanged: [0] -> [0x14ab4a20c], [5..11] -> the seven per-type overrides.
  Fallback block 0x149f15eb4 is all -1 (0x14598a6d0). 0x140496bf0 blends [0x14ab4a20c..] toward targets
  [0x14ab4a284..] with factor [0x14ab4a280]. run-52: 1.0 x175 s, 0.7 x91, 0.6 x50, 0.76 x12 + blends = authored.

### Drawable removal (Q1): the cleanup pass (not finished; see ledger F17)
- Slot +0x200 callers with a constant flag 3 (scan in scratchpad, all 'mov dl, 3'): 0x1403373f0 (entity teardown,
  <- 0x1451f2bc0), 0x14033a790 x5 (<- delete queue 0x140336740 <- 0x1403365f0 list [0x147c1cc98] count
  [0x147c1cc94]; also 0x1409d9cc0, 0x1422a22c0), 0x140c89c20 / 0x140c89cd0 (delete queue [0x14763b418], entities
  with (+0xb4 & 0x3e) == 0x18, then destroys them), 0x1416ccdf0 x2 (entities with (+0xb4 & 0x3e) == 0x1e owned by
  a container), 0x142751700, 0x1451f28f0 (<- 0x1451f24d0, destructors), 0x142279e50 (THE cleanup, below).
  No call site passes a constant 1; ~96 sites pass a register (other classes share the slot).
- Object-class path: 0x141cb3070(child, value, 1) sets a component value and removes the child's drawable
  (0x14501d330(child, 3)); callers 0x1400b80d0 (child type byte must be 4, pool [0x14abbe2e8]) and 0x144f43ad0 <-
  0x1400b8be0 <- 0x1400b87b0 <- creation core flag-2 branch for type 0x0c parents with component [0x14999af8c]
  (runs once: guarded by component +0x20 == 0), 0x14501d5c0, 0x1420aaf10.
- Streamer cleanup = manager object M (same object as F6's pressure logic): 0x14110c770(M) (sync) or jobs
  0x14229cf40 (-> 0x1422a2540 -> 0x142279e50) under 0x141dddda0: 0x142279a30 (pressure state, M+0x4a4; budget
  scale M+0x4a0 = (1 + 0.2 x M+0xdf4) x M+0xe0c), 0x142279db0 (reset counts), 0x14227a320(M, i, i+1) for each
  entity of the list M+0x4d0 (count M+0x4cc), 0x142279e00 (sort), 0x142279e50 (remove).
  0x14227a320: entity eligible if +0x31 bit 7 clear, +0x35 bit 6 clear, and ((+0xb4 & 0x3e) == 0x18 or vtbl +0x50()
  true). score = f32 entity+0xc8; if the entity's nibble in the per-type flag table [0x14ab75fb0 + 16 x type]
  (4 bits per pool index) has bit 3 CLEAR: score = max(0, score - 1.0), stored back. Then: score < 0 -> list 1
  (M+0x4e0, count +0x4dc); score == 0 -> list 2 (M+0x4f0, +0x4ec); 0 < score < 6 and M+0x4a4 (pressure) and type
  category bit in M+0xe00 -> list 3 (M+0x500, +0x4fc). Entries 0xc bytes: handle, asset id (0x144d9edb0 of
  archetype+0x5c), score.
  0x142279e50: time budget = 3e-5 x M+0x4a0 x time since the last call (QueryPerformanceCounter; no budget when
  M+0x4a0 == 0); list 1: vtbl +0x200 for all; list 2: only if asset flag byte (0x14503f550: asset record +0x19) &
  mask == 0; list 3: flags 3, mask 0x71, at most 30 a pass. Return addresses in 0x140151bb0's caller chain:
  0x142279fc1 (list 1), 0x14227a0b5 (list 2), 0x14227a236 (list 3).
- Score writers: scanner 0x1400967e0 third branch (entity HAS a drawable and type in mask 0x101e = 1,2,3,4,12):
  entity+0xc8 = 0x144e7cd20(...) every scan; create worker 0x1403447a0: entity+0xc8 = list-B score. 0x144e7cd20
  returns base (1/x capped 0.95) + 1 / 2 / 4 / 5 / 6 / 7 (never below 1 for a listed entity).
- Bit 3 of the nibble ("seen in this scan") is set by the scan callback 0x144f2ab60 @144f2ae76 for every valid
  entry EXCEPT when (visibility bits & ([0x148fdc4e0] | [0x1498e6410])) != 0 and entry+0x18 bit 46 is set; bits
  0/1/2 = visible in phase masks ([0x148fdc4e0]|[0x1498e6410]), ([0x1498e6420]|[0x1498e6418]), ([0x148fdc4e8] or
  [0x1498e6428]). The table is indexed by the entry's kind there and by the entity type byte in the cleanup, so
  kind (entry+0x18 bits 33-36) == entity type.

### F17 (T7) Drawable churn = override scorer (score <= 0) vs cleanup; confirmed by run-54 return addresses
- run-54 removal meter: 0x14227a0b5 (list 2, score == 0) and 0x142279fc1 (list 1, score < 0) = ~90 % of
  1,087-6,633 removals a second; removed kinds mostly type 0x0c. List 3 (0x14227a236, pressure) absent.
- Override scorer 0x142282d20, used by 0x144e7cd20 when qword [0x1476d5420] != 0. Installed by 0x14266a7d0 /
  removed by 0x14266a7f0, both from 0x142282b20(prev state) <- 0x14227bd80: on when ([0x14ab44324] & 0xc0) != 0
  or bit 8; mode byte [0x14764a9f9] (also 0x142596760(mode)). For a removable entity ((+0xb4 & 0x3e) == 0x18 or
  vtbl+0x50()), entry kind != 2: 7.0 if asset in required set ([0x1485644e0]: +0x20 count, +0x28 table;
  0x142283000) or visible in the scanner's phases; 1.0 if visible in phases [0x1476d5590] / [0x1476d558c]; else
  if [0x14ab44324] bit 12 clear: -1.0 when (archetype+0x84 & 7) == 4, otherwise -(asset size via 0x144fd0d70 /
  0x141f8b140), and *skip = 1 only if byte [0x147455e44] == 1; bit 12 set -> normal scorer.
- Guard bool [0x147455e44]: one-time init (guard [0x147455e48]) = 0x1409d3e80(0x646ef585, 0): generic hashed bool
  getter (thunk 0x1409d3e70, 267 call sites), table = [0x1498fa218]+0x90 (+0x28 ptr, +0x30 count; lookup
  0x144f87b60), object set in 0x143ecaa50, cleared in 0x143ecac20; returns the default when [0x1498fa218] == 0.
  Source of the table not identified.
- Scanner: list-B score = max(scorer, 0) vs threshold [0x149f36104 + 4 x (entity+0x96 & 0xf)] (0 without
  pressure) -> score-0 entities are listed and created; cleanup removes score <= 0. Cycle = every frame.
- Not involved: time (QPC only budgets the remover), occlusion queries, read-back rings.
- Open: who fills M+0x4d0; meaning of entry+0x18 bit 46 (entries visible in ([0x148fdc4e0]|[0x1498e6410]) with
  it set are not marked seen); source of the config table; console value of bool 0x646ef585.

## State at hand-back (analyst 7)
Sent: F16 (Q2+Q3), F17 (Q1) + answer on run-54. Tool note: a stale dis.py in the session scratchpad shadows the
stdlib; run helper scripts from a sub-folder. Dead ends: 0x1400b87b0 slot rebuild as per-frame remover (one-time,
guarded by component+0x20); sweep for constant-1 flag callers of slot +0x200 (none); bit-46 sweep (noise).

### F18 (T7) Config table [0x1498fa218]+0x90 = cloud tunables, online only (analyst 7, after hand-back)
- O = [0x1498fa218] (0xe8 bytes; 0x143ecaa50 <- 0x143da1560 <- 0x1422082d0; ctor 0x143eca960). Request 0x14026a720(O):
  gates 0x14588d790([O+0x60]) pending, 0x145268620() user index <= 3, 0x145eff880 + privilege 5; id [O+0x60] =
  0x145b93cb0(0x18010009, descriptor 0x1498f9f28 (hash 0x1a098062), 0x400, 0x443). Handler 0x140d00eb0 (data ptr
  0x14ac34c58): event type 6: status [O+0x64], success -> decrypt 0x1456cdd20 (key 0x14655d4b0, 0x140613660) ->
  0x143ec9930 -> 0x143ec9aa0 -> 0x143ec9d10 (JSON 0x141a93210) -> 0x143eca2a0 (member "tunables", insert
  0x1458cc8d0); [O+0xe4] loaded, [O+0xe5] attempt over, [O+0x68] finished, [O+0x69] ok-or-404. No local file.
- Getters: bool 0x1409d3e80 (thunk 0x1409d3e70, 257 sites; defaults 183 true / 55 false), siblings 0x140555560 (9),
  0x140cac900 (11). Streaming-area keys: see ledger F18 / message (0x646ef585, 0x1dd3a337, 0x08910f87, 0xdbb175bc,
  0xb77d20f4, 0xde5544b2, 0xba243131, 0x7ad775a7, 0x23941acd; cached bytes [0x1476d5250], [0x1476d52f8],
  [0x1476d53a0]). Census script: session scratchpad a7/tun.py (callers.py + look-back for ecx/edx).
- Open: 0x145b93cb0 down to the host / Xbox call; who fills cleanup list M+0x4d0.

## Analyst 8

Topics: T2 (arena), T8 (recording phase), T9 (frame pacing). Ledger rows F19-F22. All static; nothing run.

### Arena map (F21)
- Reservation base [0x1475df188] (init 0x141e86440, sizes from 0x145c5ff00): descriptor memory, size [0x1475df190]
  (= (cfg [0x14a94393c] + [0x14a943940]) x increment + [0x14a943944] x increment), then the arena resource
  ([0x1475df108], device+0x428 at [0x1475df140]): C = chunk pool [0x1475df140] size [0x1475df168]; D [0x1475df148]
  size [0x1475df170]; E [0x1475df150] size [0x1475df178]. Engine buffer object over it: 0x1475df0e8.
- Descriptor heap (0x145cabb00) spans descriptor memory + C: (size A+B + C) / increment slots.
- Chunk pool (0x14247ac20): base [0x1475df1b8], count [0x1475df1c8], size [0x1475df1cc], total [0x1475df1d0],
  descriptors [0x1475df1c0] x 0x58: +0 prev, +8 next, +0x10 address, +0x18 fence token (-1 free, -2 in use),
  +0x20 allocations alive, +0x28 queue/frame tag, +0x30/+0x38 list links, +0x40 owner context, +0x48 callbacks,
  +0x50 frame, +0x54 flags. Free list head [0x1475df1e0], tail [0x1475df1d8], lock 0x1475df198.
- Context side (ctx = recording context; +0x12780 sub-object): +0x88 current chunk, +0x90 cursor (moves down),
  +0x98 room; +0x12198 -> quota word (byte0 limit, byte1 used, byte2 granted, bit 25 flushing, bits 28+ waiters);
  +0x121f8 tail of chunks in use, +0x12200 tail of retired chunks.
- Calls: alloc 0x144d9f3e0 / 0x14517a440 -> 0x144d9f450 / 0x14517a480 -> bump 0x144d9d9a0; new chunk 0x1400049b0
  (twin 0x140031b90) -> 0x144e9b470/0x144e9b480 -> 0x14005c280; release 0x144da9f50 -> 0x144da9f80; to free list
  0x1424749d0 (list close 0x1402a42d0, 0x140607530) and 0x141f75eb0 (submit 0x1401f6130); fence test 0x144dad000.
- Offset helper 0x144da8e10 (address - [0x1475df140]), 41 callers = users of arena-relative offsets.

### Chunk quota (F19)
- 0x1407d1550(count-4, threads): 20 % -> global reserve [0x149c3d7ec]; rest: 0.3/threads each + weights from
  0x1465f6880 (rows for 1..12 threads) -> [0x149c3d7f0 + 4*i]. Job quota set in 0x1455cc030, passed on at job end
  by 0x1450bb3f0 -> 0x144deb640; returned at context end in 0x140057300 / 0x140094460 / 0x142483f40.
- Waits: 0x140037a50 (semaphore [0x1475defc0 + 8*thread]); Sleep(1) loop in 0x1400049b0 (call 0x140004b01).

### Recording structure (F20)
- Render thread frame 0x1423cf4c0: wait main thread (0x1417bd6e0 on queue+0x4c0) -> vtbl+8 -> 0x1443c38a0 -> vtbl+0x10
  -> 0x1423cf5a0 (wait, time kept at +0xd0) -> 0x141f6e8e0. Loop fn 0x1423cf210, thread fn 0x1423cefe0.
- Job slots 0x149c3be60 (0x30 each, 0x80), job ring 0x149c3ba30 (lock 0x149c3be30, semaphore 0x149c3be58), thread
  structs 0x149c3d670 (0x40 each: +0 handle, +8 wake event, +0x10 done event, +0x28 context, +0x30 second context).
- State apply at draw: 0x140032130 (fast) writes tokens straight into the list's command buffer (list+0x18 cursor,
  +0x30 end, opcode table list+0x144) and calls the vtable (+0x120 table, +0x160/+0x170 root views, +0x140 root
  constants) only when the buffer is full.

### Pacing (F22)
- See ledger row. Timer object [0x14760aee0] (made in 0x145804360). Fence wait core 0x144fb0ad0.

## State at hand-back (analyst 8)
Sent: F19 (+ chunk-count check), F20, F21, F22. Not done: T7 leftovers (only: no LOD compare in 0x145385f30, go to
0x1406ed3b0; cleanup list M+0x4d0 untouched); writers of arena segments D/E; type table queue+0x530.
