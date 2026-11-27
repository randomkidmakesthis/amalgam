# Merge of GPU-only pages (foxG) and known writes filter (foxJ) - par\foxM (coder M, 2026-11-26)

Tree: `C:\GTAVI\Tools\par\foxM` (clone of victory-fox, branch local/scarlett-fox). Remotes foxG, foxJ.

## Merge bases
- base victory-fox 6c86c22 (F6). foxG 93a312a: 13 commits on 6c86c22. foxJ 002a3b9: 9 commits on 6c86c22.
  merge-base(foxG, foxJ) = 6c86c22: both branch from the current base head, nothing of either is in the other.
- Merged foxG first (no conflicts), then foxJ.

## Conflicts and how resolved
- `watch.rs`, 13 hunks, all note points (wrote, title_memory_allocated, landing, landed_beside, writing,
  kernel_writing, KernelWrite::drop, title_memory_written, title_memory_remapped, title_memory_unmapping, VEH
  handler write fault + guard fault): each feature added its own call at the same line. KEPT BOTH, foxG's
  calls (`pagekinds::ours/known`, `loadmeter::*`) first, then foxJ's (`knownwrites::landing`,
  `lockmeter::note/probe_hit/fault`). Ledgers stay separate. In the VEH handler `pagekinds::write_fault` (a
  counter) runs before `lockmeter::probe_hit` (makes a probe page writable again); neither returns from the
  handler, so the order does not matter.
- Auto-merged but not compiling: both trees added `watch::asking_now()` (same body). One kept.
- `lib.rs`: both `mod` lines kept; foxJ's `pub use lockmeter::kernel_will_write` replaces
  `watch::writing as kernel_will_write` (it notes, then calls `watch::writing`, which holds foxG's calls).
- `shadow.rs`, `host.rs`: different places, auto-merged.

## Composition ("was this page written"), order of evaluation
Per refresh of a watched buffer copy (`Shadow::update_free`), both on:
1. KNOWN_WRITES filter (`Shadow::update_filtered`): pages event-covered and unmarked since their take are
   spared, never asked (its diag safety compare samples them before);
2. the other pages go to `watch::writes` in runs; there GPU_ONLY=1 answers a run lying wholly on GPU-only
   pages from the journal (`pagekinds::exempt_asked`: no write-watch ask, no guard);
3. the rest: today's write-watch / guard tracking.
A page spared by EITHER rule is not asked. Texture refreshes (`texture.rs`) see only rule 2 (KNOWN_WRITES is
buffer copies only).
Glue added (merge commit): `watch::note_known` (every GPU_ONLY journal note: our dstorage writes, mapping
changes, F25 alloc, block entering/leaving the rule, safety-net hit) also marks the KNOWN_WRITES ledger
(`knownwrites::note`, source "other copy of ours", after the asking so the mark is >= every take before).
Without it the ledger could spare a page the journal calls written, and its verify would count those notes
as "writes the filter missed" (demotions). The other direction needs nothing: KNOWN_WRITES events
(lock windows) are title CPU writes, which GPU-only pages never get; GPU_ONLY's safety net (texture copies)
and verify (`found_written` inside `watch::writes`) still see every asking that reaches them. Comments at
`Shadow::update_free` and `watch::writes`.

## LOCK_METER and KNOWN_WRITES
`VICTORY_KNOWN_WRITES=1` does NOT need `VICTORY_LOCK_METER`: `lockhook::install` installs the lock/unlock
hooks for either switch, `lockmeter::note/locked/unlocked` forward to the ledger before their own `on()`
check, and the "known writes" lines come from the same hle thread. LOCK_METER=1 is diagnostics only; what
it adds per frame: a mutex HashMap insert per 64 KiB page for every note point, the class/compare pass in
every `copy_stale` (title bytes of every found-written run read again and compared), and with
`VICTORY_LOCK_PROBE` (default ON with the meter) once a second up to 10 runs x 4 MB made read-only for one
frame (a VEH fault on first write, stack unwind). Not measured on its own (run-156 had both on).
For a speed run: `VICTORY_LOCK_METER` unset (or at least `VICTORY_LOCK_PROBE=0`).

## Always-on additions (all switches off)
Hook installs: all gated (hle `modules::pagekinds::install` on PAGE_KINDS||GPU_ONLY; `lockhook::install`
on LOCK_METER||KNOWN_WRITES; dstorage/xmem meters on their switches). Ungated but behaviour-neutral:
- `inplace::title_memory_view` / `views_gone`: TWICE bit set/clear per 64 KiB page of each view (foxG).
- `texture::Staging`: two atomic stores per refresh (`fill_began`, `filled_at`) (foxG).
- `loadmeter::title_image(base)` once at start (foxG).
- `resource::let_go_idle` restructured (closure `let_go`, last-pass list): with GPU_ONLY off `ruled` is
  false and `let_go_freed` returns 0, same releases as before (foxG).
- `kernel_will_write` goes through `lockmeter::kernel_will_write` (one more call + two `on()` checks) (foxJ).
- `shadow::put_changed` wrapper, `land::piece_at` direct span, `host::copy_mapped` note, `views_mapped`
  loop over other views: each a call returning on `knownwrites::on()` (foxJ).
- Every note point: calls that return on their mode check.

## Build and tests
- Commit 3350e0a (merge of foxJ on c7cd981 = merge of foxG). `cargo build --profile diag -p victory --features
  diagnostics --offline --locked`: OK, 5 warnings that were already there. Exe:
  `C:\GTAVI\Tools\par\foxM\target\diag\victory.exe`.
- victory-gpu lib: 419 passed, 3 failed, 34 ignored. The failures are the known ones:
  `indirect::tests::warp_batch_charges_every_execution`, `indirect::tests::host_list_budget_starts_empty_at_each_submission`
  (WARP), `hlehooks::tests::unmapped_is_counted_and_passed_on`. The knownwrites/shadow known_writes_tests,
  the pagekinds tests and the watch bits/note_known test pass.
- victory-shader lib: 306 passed, 0 failed.
- victory-hle lib: the test binary dies with STATUS_ACCESS_VIOLATION in
  `modules::xmem::tests::a_title_thread_goes_on_using_the_rest_of_a_view` every time (4 of 4). The SAME crash
  happens on the base 6c86c22, on foxG 93a312a and on foxJ 002a3b9 (each built in a temporary worktree), so it
  comes from the base and not from this merge. One more failure to add to the known list: because the process
  dies, the hle tests after it did not run on the base either. With `--skip a_title_thread_goes_on_using`:
  168 passed, 0 failed, 2 ignored.
