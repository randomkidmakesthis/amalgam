# Frame pipeline: why the frame is prepare + wait, and what turns it into a max (2026-11-26)

Read-only analysis. Sources: `victory-dev/crates/victory-gpu/src` as of 04:40, runs `run-57-consumers` and
`run-55-f17-off` (current build, standard switches). Tags: **[P]** proven (log line or code), **[I]** inferred.
Heavy = frame-log windows of 2,500-4,000 draws, mean weighted by frames (run-57: 1,502 frames; run-55: 1,540).

## 1. Verdict
- [P] Heavy frame 81-82 ms. Submit thread at work 78-80 (prepare 34, wait 37-39, other 6). Card: own lists
  57-58 ms (title's 47-49 in 211, send-and-hash 6.2, compare 3.5), idle 19-21 (16-17 "behind us").
- [P] **Every millisecond of "waiting for the card" has one cause**: `title memory read` (WHY_READ). Line "of the
  waiting for the card in settles ... by what settled", 339 lines of run-57: read 33.6 ms mean (max 90), stale 0.02,
  all seven other reasons 0.00. 4.0-5.7 such waits a frame: 3.7-5.2 "a fill of upload memory", 0.2-0.8 "a buffer copy".
- [P] They are waits that the code was built NOT to make. `fill_ahead` (host.rs:3340) lets fills read ahead of
  flights and checks them at landing; before it tripped it passed 4,000-6,000 fills a frame and the wait was
  7-12 ms a frame. 11 s into the scene (run-57 02:27:46, run-55 02:10:3x) its ninth 64 KiB piece had a real miss
  and the global switch fired: WARN "every fill waits for flights again from here on, as with VICTORY_FILL_WAIT=1"
  (`HELD_MAX = 8`, host.rs:3352, 3418). From then on every fill that touches a declared range waits, for the rest
  of the run. `fill_check` since then: 9,420 fill waits, 1,466 (16 %) read other bytes afterwards (86,040 bytes in
  all); 1,638 copy waits, 948 with a changed source (4-10 bytes of 2,048).
- [P] Each wait is a full drain: `relay_wait(WHY_READ, blocked_by(line.written..))` (host.rs:6510) ends when no
  flight in the air declares the range; flights land in line order (`landable`, host.rs:2743) and `line.written` is
  rebuilt only in `relay_land` (host.rs:6050-6059) after fetch + compare + write-back. The usual blocker is the
  174 MB arena `0xb10000+0xae00000`, declared by most flights.
- [P] **But the card is busy during the waits.** Card busy by its clock = 81 - 20 = 61 ms; lists by their own
  timestamps 57. The waits hide no idle card; the only idle is the 20 ms. So removing the serial order gives
  `max(card ~61, submit CPU ~42)`, about **62-65 ms, not 33**. Sum-to-max is worth ~17-20 ms; the rest is card
  time (section 6) and then the single prepare thread (34 + 6 ms).
- [I] Third floor: process CPU 280-295 user + 75-78 kernel ms a frame = ~370 ms; on 8 cores that is >= 37-46 ms.

## 2. Timeline of one heavy frame (95 ms window, run-57 line 16602: 16 submissions, 4.5 waits) [P numbers, I layout]
```
title record threads  |== record N+1 (145 thread-ms, overlaps everything below) =====================|
title submit thread   |burst 1.4|---- in Present(N): waits for victory-submit (RELEASING lock) 87 ms ---|
victory-submit        |P P P P|wait~10|P P P|wait~10|P P P P|wait~10|P P P|wait~10|P P|wait| other 9  |
                       prepare 38 ms in 16 joined submissions, 4.5 full drains, wait 46 ms
card                  |.#######|######|.######|######|.#######|######|.#####|######|.###|###|..present |
                       '.' = idle after each drain while the next submission is prepared (15 of 19 ms idle)
victory-relay         |  wait card 56 ms in all, 12 landings: fetch 5.4 + compare/merge/write = 25 ms   |
```
- [P] The submit thread does prepare k+1 while k runs (16 sent ahead, 0 synchronous), until a fill or copy of
  k+1 reads a declared range; then it stands until everything sent has run AND landed.
- [I] After each drain the card has nothing until one submission is prepared: 15 ms / 4.5 = 3.3 ms a drain
  (prepare is 2.4 ms a submission). The other 4-5 ms idle ("title had not submitted") is the card finished
  while the relay still lands the last flight (`relay_wait` sets `working(false)`, host.rs:4044).
- [P] Copy-back is on the critical path only as the tail of each drain. That is why skipping 17 MB of it
  changed nothing: its time did not fall (22 vs 19 ms) and the card's work in the wait was untouched.

## 3. Every blocking wait on or around the submit thread
| # | Site | Reason it exists | Per frame (run-57/55) |
|---|---|---|---|
| 1 | host.rs:6506-6513 -> `relay_wait` 4037; test `blocked_by` 3243 (Fill 3274, Copy 3250, Texture 3293) | a CPU step at submission reads title memory that a flight's shaders may write; bytes are in title memory only after landing | **4.0-5.7 waits, 34-56 ms** [P] |
| 2 | host.rs:6576-6584 `stale_under_flights` -> settle WHY_STALE | title wrote a page a flight may write and no merge is possible | 0.0 (0.02 ms); `MERGE_MASK` avoids 1.55 a frame [P] |
| 3 | host.rs:6279-6289 `late_most()` (`LATE_WAIT_LISTS=1024`), 6611 `card_results_low` | cap on lists in the air (descriptor slots, scratch, result slots) | 0.0 [P] |
| 4 | host.rs:6283 WHY_WRITES; 3891 WHY_VALUE; fence.rs:97 WHY_FENCE | bottom-of-pipe write, memory-value wait, title fence wait | 0.0; 16-22 value waits met by a flight's write without waiting, 31 signals set behind flights [P] |
| 5 | host.rs:6504, 6527-6530, 9606, queue.rs:1283-1291 WHY_SYNC | synchronous submission; the 1.5 s sweep (needs nothing in the air); resize; present before stage 3 | 0.0 a frame; the sweep drain is one per 1.5 s [P] |
| 6 | queue.rs:1295 -> `release_held_now` 698-704 (mutex `RELEASING`) | the title's Present runs the lines itself, so it stands until victory-submit has prepared and sent the frame | 67-88 ms of the title's submit thread ("lines and settling"); not a card wait [P] |
| 7 | host.rs:4130 relay waits for the card; 5704/5733 piece fetch (copy queue / direct) | landing in order; fetch of flagged pieces | relay thread: 51-64 ms idle, 5.4-7.3 ms fetch, 25-31 ms landing; on the path only through wait 1 [P] |
| 8 | host.rs:2631 `demand_fetch`; inplace.rs:709; tiles.rs:1059; descriptor.rs:857 | skip-unread demand; in-place settle; tile upload; slot waiting for a texture | off or 0.0 in these runs [P]; no per-frame counter for tiles/inplace |
| 9 | host.rs:6498 `submitted` mutex | one submission at a time from prepare to send: snapshots and shader-write ranges are per copy | not a card wait; it is why prepare is one thread (34 ms) [P] |
| 10 | texture "waiting for memory to hold still", pipelines, remap readers, queue2 "direct queue told to wait" | - | all 0.0 [P] |
Missing counters: per wait, the split "card still running" / "landing after the card finished"; flights in the air at each send.

## 4. The game side
- [P] The thread that presents makes all submit calls (1.4 ms) and no list calls: it is the title's own submit
  thread (ledger T4), not a recorder. Recording (140 thread-ms) runs on other threads during the present call.
  The title is already one frame ahead of the layer; it is not record -> submit -> execute -> present in turn.
- [P] Present waits for "prepared and sent", never for the card (`late_free_present`, stage 3; WHY_SYNC 0.0).
- [P] victory-submit is at work 78-80 of 81 ms: it is the saturated stage. `VICTORY_ASYNC_PRESENT=1`
  (queue.rs:99, not in run-auto.sh) cannot help while that is so.
- [I] Once wait 1 is gone the layer is done in ~42 ms and the card needs ~61: flights pile up to the 1,024-list cap
  (about 4 frames, 250+ ms of latency). A depth cap is needed: Present waits until the frame before last has
  landed (`FINISHED`), i.e. 1-2 frames in the air, as a driver does.
- What assumes serial order [P code]: (a) CPU reads of title memory at submission (Fill/Copy/Texture steps): the
  one that bites; (b) one write-watch asking per submission (`watch::ask_once`) and the assumption at queue.rs:440
  that the title does not rewrite what a submission reads before we prepare it; (c) landing in line order with
  merges against a reference (`merge_under_writes`, masked merge): already order-safe; (d) `sync_heaps()` reads the
  title's descriptor heap at prepare time: already runs while the title records the next frame; (e) the sweep.

## 5. Change A: no CPU read behind a flight (turns the sum into a max)
Invariant today: a Step::Fill / Step::Copy copies title memory on the CPU at submission, so bytes a shader of an
earlier flight produces must first come down (card -> relay -> title memory). A GPU-to-GPU dependency goes
through RAM, and the card path that avoids it (`card_constants` host.rs:7869, `gpu_buffer_copy` list.rs:1054) is
chosen at RECORD time by `flights_write()` (host.rs:7812, list.rs:1064; draw.rs:3245), i.e. only if the producer
is already in the air while the consumer is recorded. The title records a frame and hands it over in one burst,
so the producer of the same frame is usually not yet sent [I: same cause as ledger F7's "producer unsent"].
Steps:
0. Meter (below). One run.
1. Smallest: per-piece holding only. `HELD_MAX` from `VICTORY_FILL_HELD_MAX` (default e.g. 4096), `WAIT` never
   global; `fill_ahead::on` for Step::Copy sources stays as is. Removes the ~84 % false waits. Exactness: unchanged
   rule, a missed piece is held after its first miss. Expect 4.5 -> ~1 wait a frame; ms gain uncertain (one late
   drain can still cost 20-30 ms): 81 -> 70-75 [I].
2. Full: the card path by prediction. `fill_wants_card` and `gpu_buffer_copy` also say yes when the source lies
   in a held piece or was declared shader-written in this frame or the two before (`shader_written_each`,
   host.rs:3126, exact ranges). The fill then runs on the card in queue order behind its producer: exact, no wait.
   `card_holds` must accept "no flight in the air" when the copy is the one the predicted writer writes; else keep
   the CPU fill and count it. Known true dependents: copies `0xdc07ea300/b00 -> 0xd1f32e/32f/36e000` (2,048 B),
   fills at `0xd6943f000`, `0xd6ab9f000`, `0xdc6ab3000`, `0xdc7d6a000`, `0xd61cdf000`.
3. Frame-depth cap at Present (section 4), `VICTORY_FRAMES_AHEAD=1`.
Expected: wait 37 -> ~0-3 ms; frame 81 -> **62-65** heavy (card-bound), 57 -> ~42 in the 3,400-draw windows whose
card time is 40 ms. Risks by symptom: one-frame flicker or jitter of whatever a small shader-written constant
buffer drives (lights, exposure, culling counts) = a fill read ahead of its producer (step 1; logged by the WARN);
missing or exploding instanced geometry = a predicted card fill read a copy that does not hold the writes
(`card_holds`); input lag = no depth cap; device removed / "tables refused" = too many lists in the air.

## 6. Change B: the card's 57-61 ms (the bound once A is in)
| Card time, heavy frame | ms | Whose | Basis |
|---|---|---|---|
| Send-and-hash lists (445 hash passes, 1,244 pieces, 120 MB up in 1,176 ranges) | 6.2 | layer | [P] card meter |
| Compare lists (213 passes, 26,123 pieces compared, 1,684 fetched) | 3.5 | layer | [P] |
| Idle after drains | 16-17 | layer | [P]; goes with change A |
| Idle, nothing submitted / landing tail | 3-4 | layer+title | [P] |
| Title's lists, 211, including what the layer records INSIDE them | 47-49 | split unknown | [P] total |
Inside the title's lists the layer adds per frame [P counts, ms not metered]: 2,133 indirect clamps (1,331 in
the list = a compute dispatch of ours between barriers and a graphics/compute switch; 802 window hits) + 1,394
gathered in preludes; **2,175-3,037 constant fills on the card** (`card_constants`: UAV barrier +
CopyBufferRegion + 2 transitions each, between draws); 69 masked merges; 7-20 buffer copies with 2 UAV barriers.
No cross-queue sync on the direct queue (queue2 "told to wait 0.0 times"). Unexplained busy (clock minus lists):
2-4 ms. [I] The ~2,200 card fills exist because whole-arena declarations make `flights_write` true for per-draw
constants; with exact declarations (job in progress) they fall back to plain upload fills that no longer wait.
Meter first, then: exact declarations -> fewer card fills, hash and compare passes; one clamp pass per list
(frame-budget row 1). Expected [I]: 10-20 ms of card time; card ~40-48 ms. 33 ms then still needs the big
full-screen passes (frame-budget row 2) and a second prepare thread (wait 9 in the table).

## 7. Meters to add first (all per frame, on the existing lines)
1. `read waits: N (fill F, copy C, texture T), W ms; bytes had changed after the wait: yes Y, no X; flights in the
   air at the wait A (lists L); of W: card still running Wc ms, landing after the card had finished Wl ms; held
   pieces H, global switch on/off; the waiting step was not on the card because: no flight declared it at record
   time R (of them shader-written in the 2 frames before: Rp), no copy holds it K, card_holds refused Hh, no room M`.
   Proves A: Wc + Wl = "waiting for the card"; Rp says how much step 2 catches.
2. `sends per frame S; found the card empty E; flights in the air at a send 0/1/2/3+; card idle: while preparing
   Ip, while landing Il, title had nothing It`. After A, E -> ~0 and Ip -> ~0, or the pipeline is still serial.
3. Card meter kind `Inserted` (timestamp pairs around layer commands inside title lists): `clamps n/ms, card
   constant fills n/ms, card buffer copies n/ms, masked merges n/ms, preludes n/ms; title's own remainder ms`.
4. `frames in the air at present: mean/max; present waited for depth D ms` (with step 3).
5. `process CPU per frame by thread group: recorders, victory-submit, relay, wide workers, dstorage, title other`.

## 8. Proven / inferred
Proven: the single wait reason and its counts; the tripped switch and its time; false-wait share; card busy 61 of
81 ms; the presenting thread's role; all zero rows of the table. Inferred: the per-drain idle model, the record-time
cause of the remaining true waits, all expected frame times, the layer's share of the title lists' card time.
