# Critical path of a frame: three loops and two title gates (2026-11-26)

Read-only analysis. Sources: `victory-data/reports/profile-run77.txt` (+ `.folded`), frame logs of run-77-profile,
run-68-skip-piece, run-76-skip3, run-69-skip-predict, run-66-asyncpresent; `victory-gpu/src/{queue,frame,fence,host}.rs`;
raw disassembly of the title (`ghidra-scripts/d.py`, `out/q`, `out/dec`). Tags: **[P]** proven (report line, log
field, code, disassembly), **[I]** inferred. "Heavy" = usual-frame windows of 2,500-4,000 draws, weighted by frames.

## 0. Three things about the profile that change how it reads
- [P] It mixes the menu with play. Of 4,206 frames ~1,690 are menu frames of 33 ms (log 04:44:35-04:45:30). The
  10.5 ms "sleep in `frame::wait`" is the menu's 30 fps pacing: 45.0 s of "paced" over 4,236 frames = 10.6 ms; in play
  every usual-frame line says `paced 0.0`. Anything that happens only in play is **x1.67** per play frame
  (4,206 / ~2,516), and a play frame in the profile is 85 ms (long frames included), 70-78 usual.
- [P] Only 24 threads are followed (`profile.rs:336`, `most: 24`): of the six recording threads it has the title's
  frame thread (33964, started by title+0x23cefe0) and ONE Subrender worker (32632, entry title+0x43c4140).
- [P] The `.folded` file holds the 1,665 heaviest stacks only (RECORDING alone has 32,490): waits are complete in
  it, processor time is not. Processor shares below come from the report's tables.

## 1. What paces the frame
[P] In play no thread and not the card is saturated alone (run-77 heavy, frame 78.1 ms): victory-submit at work
73.4 (preparing 39.2, read waits 23.9, other 10.3), card busy 68.3 (title lists 53.6, ours 10.3), idle 9.8.
[P] A third bound exists: play windows of 500-1,000 draws run at 45-49 ms with the card idle 19-29 ms and
victory-submit idle 13 ms (runs 77, 68, 76). [P, disassembly] The title allows two frames in flight and ties them
with two gates in its end-of-frame function title+0x24846d0 (on the frame thread):
1. `0x140cd7320` takes the present semaphore `0x1475e0e78`, released by the title's submit thread when the present
   command of the frame before has been executed (= our `queue::present` returned).
2. `0x142474740` keeps the last two present tokens (ring `0x1475ddb60`) and waits for the OLDER one through
   `0x144fb0a50` -> `0x144fb0ad0`: a monitorx/mwaitx spin on the fence value read through fence slot 0x60
   (`fence.rs` `gpu_address`). So recording of frame N+2 cannot start before the fence of Present(N) is set, and the
   layer sets it only when every flight of frame N has run on the card AND landed (`queue::signal` ->
   `host::late_signal`, host.rs:4421).
[I] Hence, with R = wall time of one frame's recording and Lat = end of recording of a frame -> its fence set:
**frame = max( R + ~4, (R + Lat) / 2, card busy, victory-submit busy )**. Run-77 heavy: R ~ 63 (55 waiting for the
Subrender phase + ~8 own), Lat ~ 91 (3 + Present 58 + 30 until landed), (R + Lat)/2 = 77: the measured 78. The
same formula gives the gate spin (Lat - frame = 13 ms; measured 13.6) and explains run-76: three passes skipped,
card time -11 ms, frame only -7: a millisecond of latency is worth half a millisecond of frame.

## 2. The chain through one heavy frame (run-77; t0 = Present(N) returns; ms per play frame)
| # | Thread | Doing | ms | Kind | Basis |
|---|---|---|---|---|---|
| 1 | title main | update (waits for its own jobs, title+0x5098940 paths), then "wait for render thread" title+0x23cf6a0 (event 30.5 + Sleep(1) poll 9.5 a profile frame) | ~33 + ~45 | wait, overlaps all | report 919-922, 953-955 [P]; split play/menu [I] |
| 2 | title frame thread | Subrender phase of frame N+1: waits in title+0x43c38a0 for the 5 workers | 55 (33.1 x1.67) | wait for workers = R | report 303, 360 [P] |
| 2b | Subrender worker (1 of 5 sampled) | list calls of ours: 28 ms processor, 46 ms idle for a job, ~3 ms in our locks | 28 | work | report 21, 304; 23.7 "with" in ours [P] |
| 3 | title frame thread, end of frame | present semaphore (gate 1): ~0 in usual frames | 0-6 | wait | folded 11.77 via 0x43c38a0 minus spin [I] |
| 3b | same | fence gate (gate 2): spins until frame N has landed | 13.6 (8.2 x1.67) | wait, SERIAL with 2 of frame N+2 | report 220, 306 [P] |
| 4 | title submit thread (3252) | ExecuteX / Signal calls into us: return at once | 1.0 | work | folded `queue::execute` 1.01 [P] |
| 4b | same | waits on its batch semaphore for the frame thread (title+0x2481e90 > 0x6180 > 0x569f0) | 15-20 | wait | frame - present - ran; report 140 [P] |
| 4c | same | flip wait `frame::wait` | 0.0 | - | log `paced 0.0` [P] |
| 4d | same | `queue::present` -> `release_held_now` -> lock `RELEASING` (queue.rs:700-704): stands until victory-submit has prepared and sent all of N+1 | 55-60 (33.7 x1.67) | wait, ONE per frame | report 138, 195; log "lines and settling" [P] |
| 5 | victory-submit | preparing (processor) | 39.2 | work | log [P] |
| 5b | same | read waits: 1.5-2.2 a frame, each a full drain (12-17 flights, ~250 lists in the air) | 23.9 (21 card still running, 3 landing) | wait for the card | log "read waits per frame" [P] |
| 5c | same | other (sweep, merges under flights, sends); idle for work | 10.3; ~5 | work; wait | log [P] |
| 6 | card | title lists 53.6 in 215, ours 10.3, unexplained 4.4; idle 9.8 (7 while we prepare) | 68.3 busy | work | log [P] |
| 7 | relay | lands flights; fence of Present(N+1) set ~30 ms after Present returns | 30 | wait for card + work | spin + (frame - present) [I] |
Serial: 2 -> 3 -> 4d -> 5 -> 6 -> 7 -> 3b of the frame after next. Overlapping: frame N+1's recording runs beside
frame N's 5-7; main beside everything. Nothing is quantised: the 2,631 play frame times of run-77 spread smoothly
from 32 to 135 ms (most at 60-67), no peaks at 66.7 or 100 [P, "frame times: ... each:" lines].

## 3. Answers
**Q2 `queue::lock` (33.7 ms).** [P] It is the mutex `RELEASING` in `release_held_now`, held by victory-submit
(`queue::worker`) for the whole time it runs the queues' lines, preparing included. The title's submit thread
meets it once a frame, inside Present (queue.rs:1299), not at its flushes: with `VICTORY_ASYNC_SUBMIT=1` an
ExecuteX only detaches the lists and puts them in the line (1.0 ms a frame for all ~58), and a Signal goes behind
them. There are no 58 round trips. What the title needs back is nothing (ExecuteX is void; the fence value comes
later through `Held::Signal` / `late_signal`). Memory: the layer reads title memory when it prepares, which is
after the submit call and before the fence; the title's own line is gate 2 (it reuses frame N's memory from frame
N+2 on, after the fence of Present(N)), so reading at prepare time is inside what the console allows [P gate, I
that no other reuse rule exists]. Returning from Present at once (`VICTORY_ASYNC_PRESENT=1`) cannot shorten the
frame: gate 2 holds the frame thread until frame N has landed, which is later than Present(N) returning [P
structure]. Run-66 with that switch also shows a fault: menu frames alternate ~5 and ~305 ms ("copy and show
151.6", 6.4 fps). Not worth repairing for speed.
**Q3 recording threads (75 % waiting).** [P] The waits are the title's own: frame thread waiting for the
Subrender phase 33.1, worker idle for a job 27.8, present semaphore title+0xcd7320 13.2 (almost all menu), sleeps
4.2; the "work" of title+0x4fb0ad0 (6.4 on a processor + 1.5 sleeping) is gate 2 spinning. Waits inside OUR list
calls, both threads together, a profile frame: `HostHeap::mirror_from` 0.90, `CreateCommittedResource` (graphics
kernel) 0.80, `watch::guards` 0.45, `xmem::physical` 0.30, `HostHeap::keep` 0.27, `shadow::loose` 0.19,
`resource::texture_at` 0.14, `State::heap_buffers` 0.13, `texture::staged_from` 0.11, `hand_out_resource` 0.11,
`shadow::lookup_with` 0.10: 3.6 ms in all, not pacing. On a processor (28.3): ours 23.7 with callees, title 8.65
(6.4 of it the spin), driver 3.4, d3d12 5.3. Ours by function (with callees): `draw::State::bind` 16.5,
`list::execute_indirect` 11.4, `State::prepare_for` 9.2, `draw::readable` 6.2, `list::state::dispatch` 3.8,
`descriptor::host::make` 2.9, `State::heap_buffers` 1.9, `descriptor::host::buffer_srv` 1.6; kernel calls made
from there: NtQueryVirtualMemory 0.77, WaitForSynchronizationObjectFromCpu 0.86, NtMapViewOfSectionEx 0.58,
ReadVirtualMemory 0.56. The title's own recording code is under 10 % of the recorders' processor time: R is the
layer's list-call cost [P for the two sampled threads, I for the other four].
**Q4 `frame::wait` and title+0x569f0.** [P] `frame::wait` (frame.rs:120-152) is the flip wait before each
PresentX: ticks at start + k x 33.333 ms; a blocking wait ends on the next tick this event kind has not used; if a
whole tick went by unused (a late frame) it returns at once (`VICTORY_LATE_WAITS=1` = old sleeping). It sleeps
only when the frame's work is under 33 ms: no sleeping of late frames, no multiples of 33. On the console a late
frame flips at the next vblank; the layer is more lenient, not stricter. No pacing switch other than
`VICTORY_LATE_WAITS` and `VICTORY_PRESENT_SKIP` exists (no VICTORY_PACE / FRAME / SYNC_INTERVAL). title+0x569f0
is the title's semaphore wait (through title+0x6180): on the submit thread its batch-ring wait (idle for the frame
thread), on the frame thread the present semaphore, on main the wait for the render thread. All three are the
title's and needed.
**victory-submit's own locks** [P, report 143-147, 612-613]: `watch::guards_read` 1.64, `watch::journal` 1.17,
`shadow::wide` 1.18, `merge_under_writes` 0.54, `mirror_copied_page` 0.43 a profile frame (~8 ms a play frame); the
four wide workers wait 5.97 + 5.43 on the same two locks (~19 thread-ms a play frame) while doing ~34 of work.

## 4. Plan (ranked; no change of picture)
| # | Change | Why correct | Expected | Proves it first | Symptom if wrong |
|---|---|---|---|---|---|
| 1 | Read waits without the card cost. `VICTORY_FILL_PREDICT_CARD=2`: in `Host::fill_wants_card` (host.rs:8579) and `gpu_buffer_copy` (list.rs:1076) predict from what actually tripped, not from `shader_written` (true for the whole 174 MB arena: run-69 put 1,800-8,500 fills a frame on the card for 5-7 held pieces, title lists 46.7 -> 63.7 ms). Predicate: the exact byte runs `fill_check` found changed (per held piece), plus sources that made a submission wait (`read_wait::BLOCKED`) in the last 300 frames | same card path as today's `=1`: the fill or copy runs on the card behind its producer in queue order; `card_holds` kept; refused -> CPU fill and wait, counted | victory-submit 73 -> ~50, Lat 91 -> ~62; frame then card-bound: heavy 78 -> 68-70, medium 63 -> 52-56 [I] | run-69 already shows waits 0.1 and submit work 45 [P]; add to the read-waits line "on the card by prediction: held runs H / declared only D" and run once with `=1`: D must be >95 % | one-frame-old or wrong per-object constants, exploding instanced geometry (card copy is not the one the producer writes) |
| 2 | Recording cost R, in `State::bind` / `prepare_for` (draw.rs:1120-1668): set root constants, root views, vertex and index buffers only when changed since the last draw of the host list; answer `draw::readable` (draw.rs:4244) from a per-mapping-epoch cache instead of kernel calls per draw | D3D12 keeps root arguments and IA state across draws of one list; cache dropped at list reset, signature / heap / pipeline change, after every command the layer inserts (clamp, card fill) | R ~63 -> ~35 [I]. Alone only 78 -> ~74 (victory-submit's 73 binds); with #1 and the card work of card-time-analysis.md (title lists ~36 as in run-76): heavy ~50, medium ~40 [I] | profile again, play only and all recorders: `VICTORY_PROFILE=1 VICTORY_PROFILE_AFTER=80 VICTORY_PROFILE_MAX_THREADS=48`; read frame-thread waits at title+0x43c38a0 (= R), own time of title+0x4fb0ad0 (gate 2), each Subrender thread's processor share | wrong constants or geometry on single draws; a crash if a page is unmapped between the cached check and its use |
| 3 | victory-submit's remaining 49 ms: `host::sweep` / `let_go_idle` (3.2 ms a play frame inside `Host::submit`) to the late-wait thread; `watch::journal` and `watch::guards` split so wide workers do not queue behind one another and behind the fault handler's write lock | no ordering depends on either lock beyond the per-range data they guard [I, to be checked in watch.rs] | 3-6 ms of prepare [I] | report lines 143-147, 612-613 after the change | stale or missed write-watch pages = old texture or buffer content |
Not proposed: `VICTORY_ASYNC_PRESENT` (section 3), pacing changes (none active in play), returning from flushes
earlier (already immediate), a frame-depth cap (the title has its own: gate 2; "frames in the air" never above 2).

## 5. Meter to add (one line per usual frame; all are our entry points)
`title timeline: Present returned -> last ExecuteX before the next Present A ms (the title still recording) ->
Present called B -> returned C -> its fence set D ms later (landed); recording span (first to last list call of
the frame) E ms; longest recorder F ms in list calls (thread), all recorders G`. Then R = E, Lat = B + C + D, and
the formula of section 1 is checked on every line; E against F says whether R is one long thread or serial waves.
Existing-switch check of the R floor (timing only, never a result): `VICTORY_FILL_PREDICT_CARD=1` together with
`VICTORY_SKIP_PIPELINES=5cf00dc39042edaf,cb6adcc6da2613ff,14264103296aa43a` - if the frame stays near 62-64 with
both the card and victory-submit idle more than 10 ms a frame, R is the bound.

## 6. Proven / inferred
Proven: menu share of the profile and the pacing rule; no quantisation; the holder of `queue::lock` and that
flushes return at once; both title gates (code); the read-wait counts and their card share; the list of waits and
functions on the sampled recorders; busy/idle of card and victory-submit per scene class. Inferred: R and Lat in
ms (profile averages rescaled, one worker of five sampled), the formula, every expected frame time, that the four
unsampled workers look like the sampled one.
