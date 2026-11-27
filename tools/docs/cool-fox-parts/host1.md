# cool_fox review — HOST part 1: codec_d commits e05d463 and 1ad6e67

Scope: every hunk of `e05d463` (13 files, +974/−129) and `1ad6e67` (3 files, +129/−51) in
`C:\GTAVI\Tools\ref-forks\cool_fox` (HEAD f4b48ad). Read-only review: `git show` of the two commits, `git show
HEAD:<path>` and `git grep` for context, Grep/Read on `C:\GTAVI\Tools\victory-dev\crates`. Nothing built or run.
"Theirs" line numbers are at HEAD f4b48ad unless a commit is named; "ours" are victory-dev as of 2026-11-26.

No text addressed to AI assistants was found in these two commits.

**Measured claims: neither commit carries a number.** The commit messages are one line each, there is no doc hunk, and
`docs/bug-journal.md` (written by later commits) measures only the later residency work (18bdf79, ab75ac3), not these
features. Every "gain" below is therefore "no number" from the author; where I say what it should save, that is my
reading of the code, not a measurement. Their card: RTX 5090, 32 GB RAM, **Series S mode** (README.md:12: "The launcher
selects Series S").

The commit title says "Merge VIctory-7900XTX (0a43566) with ...", but e05d463 has one parent's worth of diff: it is the
codec_d additions on top of the avatarmanz base, not a merge of two code lines.

---

## 0. Summary table (order = suggested port order)

| ID | Name | Kind | Ours has it | Size / risk | Addresses |
|---|---|---|---|---|---|
| H1-1 | TablePlan: per-shader table layout computed once | performance (recording threads) | no (0 hits `TablePlan`) | M (~200 lines, 2 files) / medium | bound (a) |
| H1-2 | Fallback view + sampler heaps | correctness | no (0 hits `fallback_view`) | S (~90 lines, 2 files) / low | device-removal risk; maybe black/garbage draws |
| H1-3 | Blocking fence wait (WaitOnAddress) + address wakes | correctness / performance (title thread) | no (0 hits `WaitOnAddress` outside profile.rs) | S (~35 lines, 2 files) / low | title thread stalls; `E_NOTIMPL` path |
| H1-4 | DirectStorage staging floor 256 MiB | performance (streaming workers) | no (0 hits `VICTORY_DSTORAGE_STAGING_MB`) | S (10 lines) / low | bound (d) |
| H1-5 | Residency guard: pressure by free RAM **and** commit | memory | partly (RAM only, no commit, no hysteresis) | S (~110 lines, new file + 3 hooks) / low | (d), out-of-memory safety |
| H1-6 | Present: skip same-state barriers; keep presented texture alive | correctness | no | S (10 lines) / low | debug-layer errors, none visible |
| H1-7 | `VICTORY_KEYS` scripted input in release builds | tooling | diagnostics builds only | S (delete 5 lines) / low | unattended runs |
| H1-8 | Probe: does Series X pass 2 planes? (log only) then two-plane compose | visual (HUD) | no | probe S (5 lines); compose M (~520 lines) | missing HUD, if any |
| — | `limited_host_ram` demotions (bury lag 16, spare cap, 5-minute idle) | memory | not applicable: ours lacks the base hoarding they undo | — | — |
| — | Card-copy barrier dedupe (`transitioned`) | correctness | not applicable: different code shape | — | — |

Nothing in these two commits trades picture for speed. One caveat on the two-plane compose (section H1-8): it forces
an 8-bit swap chain.

---

## H1-1 TablePlan — table layout of a shader computed once (HOST)

**Theirs:** `crates/victory-gpu/src/draw.rs:27-112` (struct + `new` + `TableGroup::entry` / `needs_snapshot`),
`draw.rs:3628` (`State::tables` takes `&TablePlan`), `draw.rs:1646` (caller), `pipeline.rs:133` and `pipeline.rs:226`
(built in `translate`). Commit e05d463.

**Problem it solves.** `State::tables` runs for every stage of every draw on the title's recording threads. Before, it
re-derived from the flat `used: &[UsedResource]` list, per draw: which root slots are tables, which tables hold a
constant buffer, which reach past the host heap, the entries of each table (a filter pass per table), the table's
length (a max over entries), and, inside the per-descriptor loop, "is descriptor k declared" by a linear search
(`entries().any(|u| u.table_offset_dwords / 8 == k)`), i.e. O(n × entries) per table.

**Mechanism.** At shader translation (once per shader, `pipeline.rs:226`):

```rust
pub(crate) struct TablePlan { groups: Vec<TableGroup>, direct: Vec<(usize, usize)> }
struct TableGroup { slot: u8, has_cbv: bool, has_uav: bool, max_dword: u32,
    entries: Vec<UsedResource>, indexed: Vec<RootIndexedView>,
    by_descriptor: Vec<(u32, usize)> }   // descriptor number -> first declaration, sorted, deduped
```

- `groups`: one per distinct table root slot, sorted by slot. `entries` in declaration order.
- `direct`: `(group, entry)` pairs in the original declaration order of `used`, so the non-fresh path
  (`table_entry`) records reads and transitions in exactly the old order.
- `by_descriptor`: `(table_offset_dwords / 8, entry index)`, sorted, `dedup_by_key` keeps the **first** declaration —
  same answer as the old `find`.
- `has_cbv` replaces the per-draw "any Cbv in this table" scan; `has_uav` replaces `entries().all(|u| u.kind != Uav)`.
- `max_dword` replaces the per-draw max; table length is
  `n = (group.max_dword / 8).max(selected.last().unwrap_or(0)) + 1`.
- `needs_snapshot(heap, host_count, table)`: tests only the last entry (`max_dword`) against the host heap's count;
  only if that handle is outside the heap does it fall back to scanning all entries. (Old code tested every entry.)

Per draw (`tables`): the set of tables to make afresh is a `[u64; 4]` bit mask by slot (no `Vec`, no `contains`),
`entries` is a borrowed slice, `group.entry(k)` is a binary search. The "reader" identity of the base's ready-once
cache changes from `(used.as_ptr(), used.len(), ..)` to `(ptr of plan, plan.direct.len(), ..)`.

**Ours.** `crates/victory-gpu/src/draw.rs:3719-3972+` is the same function in the older shape and allocates on the
heap per call: `fresh: Vec<u8>` (3727), `half: Vec<(u8,u32)>` clone (3758), `entries: Vec<&UsedResource>` per fresh
table (3787), `selected: Vec<u32>` (3801), `made: Vec` (3782), and the linear searches at 3883, 3945, 3990. Caller:
`draw.rs:2014` `self.tables(list, &t.used, &t.host.root_indexed_views, compute)`. `Translated` is at
`pipeline.rs:129` (`used` at 131). grep: `TablePlan` 0 hits, `TableGroup` 0 hits, `struct Few` 0 hits.

**What to change in ours.**
1. Add `TablePlan` / `TableGroup` to `draw.rs` (copy theirs, ~85 lines) and a `tables: crate::draw::TablePlan` field in
   `Translated` (`pipeline.rs:129`), filled where `used` is known (their `pipeline.rs:226`). Any other constructor of
   `Translated` in ours (tests, the shader cache load path) needs the field too — grep `Translated {`.
2. Rewrite the head of `tables` (ours 3726-3781) on the plan. Ours has two extra sources of "fresh" slots that theirs
   lacks and that must stay: `HALF_PLACES` (3758-3774, the `View::HalfUintUav` port) and `STAGE_SPANS` (3830-3837).
   Both are per-draw thread-locals, so they stay per-draw; set their bits in the same mask.
3. Replace `entries` (3787) with `&group.entries`, the three linear searches with `group.entry(k)`, `n` with
   `group.max_dword`, the `indexed.iter().filter(..)` at 3801 with `group.indexed`.
4. Optional, separate step: return type `Vec<(u8, u32)>` → a small inline vector. Their `Few<T, N>`
   (`draw.rs:4510-4551`, 40 lines) is base code ours lacks; it is self-contained and can be copied.

**Dependencies on base code ours lacks.** `Few` (40 lines, optional). Their ready-once cache (`FreshTable { first,
epoch, ready }`, `VICTORY_READY_ONCE`, `draw.rs:4553-4569`) is **not** needed: ours has its own `ready_tables`
(`draw.rs:3902`), keyed by table key, which does not use the `used` pointer.

**Switch.** Theirs: none (unconditional). Proposed: none at runtime either — it is a pure data-layout refactor and a
switch would mean keeping both bodies. A/B by build. If an A/B switch is required, keep the old function as
`tables_flat` behind `VICTORY_TABLE_PLAN=0` for one round, then delete.

**Verify.** Picture must be bit-identical (no behaviour change intended). Counters `TABLES[0..8]` (ours
`draw.rs:3876-3980`: ready / not ready / made / slots) must be the same per frame before and after on the same scene;
the recording-phase time of heavy frames (bound (a), 41–50 ms) should fall. Unit test to add (theirs has none for
this): build a plan from a `used` list with two declarations on the same descriptor and interleaved slots; assert
`entry(k)` returns the first declaration and `direct` preserves order.

**Picture:** neutral. **Shader cache:** none (host side; the plan is derived after translation, not stored — if ours
serialises `Translated`, derive the plan on load). **Claim:** no number. **Size:** M (~200 changed lines, `draw.rs` +
`pipeline.rs`). **Risk:** medium — the merge with `HALF_PLACES` / `STAGE_SPANS` is manual; a wrong order in `direct`
would change transition order. **Addresses:** bound (a). Expected size of the gain is unknown: measure the share of
`tables` in the recording threads first (our profile) — if it is under a few percent, skip.

One behavioural difference to keep in mind: `needs_snapshot` checks only the furthest entry unless its handle falls
outside the title heap. Since host index grows with the handle, "furthest entry is inside the host heap" implies all
are — equivalent to the old loop.

---

## H1-2 Fallback descriptor heaps (HOST)

**Theirs:** `draw.rs:1570-1611` (bind), `draw.rs:458` + `1270` (`host_heaps_set`), `host.rs:6122-6184`
(`fallback_view_heap`, `fallback_sampler_heap`), fields `host.rs:60-63`, init in `create`. Commit e05d463.

**Problem.** The host root signatures are created with `CBV_SRV_UAV_HEAP_DIRECTLY_INDEXED |
SAMPLER_HEAP_DIRECTLY_INDEXED`. D3D12 requires **both** a view heap and a sampler heap to be set on the list before
such a root signature is set. If the title's list has set only one heap (or none), the old code called
`SetDescriptorHeaps` with one heap (or with an empty array) and then set the root signature: invalid; debug layer
error, and on some drivers undefined behaviour up to device removal. Also, when the title's heaps were `[0, 0]`
(never set), `host_heaps == heaps` was true from the start and nothing was bound at all.

**Mechanism.**
- Two lazily made (`OnceLock`) shader-visible heaps of **1 descriptor** each on `Host`: a CBV/SRV/UAV heap whose one
  slot is a null buffer SRV (`DXGI_FORMAT_R32_UINT`, `D3D12_SRV_DIMENSION_BUFFER`), and a sampler heap whose one slot
  is point / clamp, `MaxLOD = f32::MAX`.
- In `bind`: the title's heaps are sorted into `view_heap` / `sampler_heap` by `title_heap.samplers()`; a missing one
  is replaced by the fallback; the array is always ordered view, sampler; `SetDescriptorHeaps` is skipped only if both
  are missing (heap creation failed).
- `host_heaps_set: bool` forces the first bind of a list even when the title's heaps equal the reset value `[0; 2]`.
  Reset to false wherever `host_heaps = [0; 2]`.

**Ours.** `draw.rs:1938-1953`: the old code exactly (filter non-zero, `filter_map(host())`, `SetDescriptorHeaps`,
`host_heaps = heaps`). Reset at `draw.rs:1676`. Root flags: `pipeline/host.rs:1155`, `victory-shader/src/emit/host.rs:219`.
`Heap::samplers()` exists (`descriptor.rs:75`). grep: `fallback_view` 0 hits, `host_heaps_set` 0 hits.
`docs/card-time-analysis.md:65` describes ours as "one view heap + one sampler heap per title heap" — true only when
the title set both.

**What to change.** Copy the two `Host` methods (~65 lines) next to ours `Host::rtv`-style helpers in `host.rs`, add
the two `OnceLock` fields and the `host_heaps_set` field, replace `draw.rs:1940-1953` with their block, reset the flag
at `draw.rs:1676`.

**Switch.** Theirs: none. Proposed: `VICTORY_FALLBACK_HEAPS` default on (`=0`: old behaviour) for one A/B round.

**Verify.** Add a once-per-run warning when a fallback is used (theirs is silent — add
`once!("a list set no sampler heap: the fallback is bound")` and the same for views). If neither warning ever appears
in a 6-minute run, the title always sets both and the port is only a safety net. With the D3D12 debug layer on, the
message about a directly indexed root signature without heaps must not appear.

**Picture:** improves or neutral (a draw that read through an unbound heap read garbage). **Shader cache:** none.
**Claim:** no number. **Size:** S (~90 lines, 2 files). **Risk:** low. A 1-descriptor fallback means an index above 0
reads past the heap — still wrong, but that is the title's (or our translation's) error, and no worse than before.
**Addresses:** correctness; a candidate cause for device removals, not for any listed defect specifically.

---

## H1-3 Blocking fence wait and address wakes (HOST)

**Theirs:** `fence.rs:118-145` (wait loop), `fence.rs:168-173` (wake in `set`), `queue.rs:840-841` (wake in
`Written::write`). Commit e05d463.

**Problem.** `ID3D12Fence::SetEventOnCompletion(value, NULL)` means "block until the fence reaches the value". The
base (and ours) settles outstanding submissions once and, if the fence is still short, logs "blocking fence wait not
implemented" and returns `E_NOTIMPL` — the title then goes on believing the card has finished. With submissions run by
a separate thread ("behind"), the signal can simply be a little later than the settle.

**Mechanism (title's calling thread).** After the settle, when `fence.value < value`:

```rust
let deadline = Instant::now() + Duration::from_millis(250);
loop {
    let current = fence.value.load(Acquire);
    if current >= value { return S_OK; }
    if now >= deadline { break; }
    let ms = (deadline - now).as_millis().min(10) as u32;
    WaitOnAddress(&fence.value, &current, 8, ms.max(1));   // sleeps until value changes or <= 10 ms
    crate::queue::release_held(false);                      // lets held submissions run
}
// last check, else warn "blocking fence wait timed out" and E_NOTIMPL
```

Constants: total **250 ms**, slice **10 ms** (min 1 ms). The other half: `fence::set` now drops the waiters lock and
calls `WakeByAddressAll(&fence.value)`, so the sleeper wakes at once instead of at the slice end. The same wake is
added after every bottom-of-pipe value write (`Written::write`): the fence's "GPU address" is the address of
`fence.value` (ours `fence.rs:140-142`), and the title can write/wait there through the queue as well.

Only the `crate::host::late()` branch changes; the non-late branch still returns `E_NOTIMPL` at once.

**Ours.** `fence.rs:81-111`: lines 95-103 are the place (settle, check, warn, `E_NOTIMPL`). `fence::set` at
`fence.rs:113-127`. `Written::write` at `queue.rs:1211-1232`. `release_held` exists (`queue.rs:410`).
`windows-sys` already has `Win32_System_Threading` (`victory-gpu/Cargo.toml:30`). Our settle reason `WHY_FENCE`
(`host.rs:3346`, printed as "a blocking fence wait" at `host.rs:5169` / `5206`) already counts how often this path is
entered. grep: `WaitOnAddress` / `WakeByAddress` 0 hits in ours outside `profile.rs:1936` (a stack classifier).

**What to change.** Replace `fence.rs:101-102` with the loop; add `drop(waiters)` + `WakeByAddressAll` at the end of
`set` (after the `retain`); add the wake after the write in `Written::write` (`queue.rs:1226-1231`). Keep the test at
`fence.rs:187` (`E_NOTIMPL` without a host) — it exercises the non-late branch and still holds.

**Switch.** Theirs: none. Proposed: `VICTORY_FENCE_BLOCK_MS` default 250 (`0` = old behaviour).

**Verify.** Before porting, grep a run log for "blocking fence wait not implemented" and read the "a blocking fence
wait" column of the settle line. If the warning never appears, the port is a safety net only. After: the warning text
becomes "blocking fence wait timed out" and should not appear; the title thread must not show 250 ms stalls in the
timeline (a timeout means a real missing signal — worth a log line with the fence and both values).

**Picture:** neutral/improves (the title no longer reads results it believes finished). **Shader cache:** none.
**Claim:** no number. **Size:** S (~35 lines, `fence.rs` + `queue.rs`). **Risk:** low; worst case a title thread
sleeps 250 ms once where it used to continue wrongly. `WakeByAddressAll` per value write is a cheap call with no
waiters. **Addresses:** correctness of fence waits; no listed bound directly.

Whether the title itself sleeps with `WaitOnAddress` on fence memory is the author's assumption (their comment: "wake
any guest thread polling/blocking via WaitOnAddress on the fence's GPU VA"); nothing in the tree proves it. The wakes
are needed in any case for the layer's own wait above.

---

## H1-4 DirectStorage staging floor 256 MiB (HOST, victory-hle)

**Theirs:** `crates/victory-hle/src/modules/dstorage.rs:230-246`. Commit e05d463.

```rust
let floor = env VICTORY_DSTORAGE_STAGING_MB (default 256) << 20;
scheduler.lock().staging = u64::from(size).max(floor);
```

**Problem.** The title calls `SetStagingBufferSize(32 MiB)`. The layer's read scheduler uses that as the cap on bytes
of compressed source held by running requests: a request is not started while
`staged > 0 && staged + its_bytes > staging`. With multi-megabyte compressed chunks, 32 MiB lets only a few requests
run at once and the other worker threads idle. A PC has the RAM to raise it.

**Ours.** `dstorage.rs:222-228` (`scheduler.lock().staging = u64::from(size)`); default `STAGING = 32 << 20`
(`dstorage.rs:390`); admission test `dstorage.rs:725-728` (`starved`, `refused += 1`); a request's staging bytes
= `source_size` for ZLIB / BCPACK / PAGE_DESTINATION requests not from memory (`dstorage.rs:852-859`). The refusals are
already printed: `staging_waits` in the debug line "dstorage: last N s" (`dstorage.rs:681`). grep:
`VICTORY_DSTORAGE_STAGING_MB` 0 hits.

**What to change.** The 10 lines above in `set_staging_buffer_size`. Budget: at most +224 MiB of RAM, transient,
only while that many reads are in flight (the buffers are per request, not a pool held for the run).

**Switch.** `VICTORY_DSTORAGE_STAGING_MB`, theirs default 256. Proposed for ours: same name; default **32** (off)
until measured, then 256.

**Verify (measure first).** Read `staging_waits` in an existing log during a streaming burst. If it is 0, the cap never
binds and the port does nothing. If it is high while `busy_workers` is below the worker count, raise the floor and
compare `mb_per_s`, `busy_workers` and the number of streaming freezes.

**Picture:** neutral (textures arrive sooner if anything). **Shader cache:** none. **Claim:** no number; comment says
"so multi-MB RPF chunks don't starve the remaining cores". **Size:** S. **Risk:** low; more parallel decompression
competes with the recording threads for cores (watch bound (a) during bursts). **Addresses:** bound (d).

Not the same thing as shortlist row "Staging upload pool cap 256 MB → 2 GB" (`VICTORY_STAGE_POOL_MB`,
fork-review-shortlist.md:304): that is the GPU upload staging pool in `texture.rs`; this is the file-read scheduler.

---

## H1-5 Residency guard — host memory pressure with hysteresis (HOST)

**Theirs:** new file `crates/victory-gpu/src/residency.rs` (93 lines at e05d463; 229 at HEAD after 18bdf79 / ab75ac3,
which belong to another part). Hooks at e05d463: `host.rs:374-380` (`ram_idle_ms`), `host.rs:443-446` (`sweep`),
`host.rs:497-502` (`bury_lag`), `host.rs:1935-1940` (`give_chunks`), `host.rs:1962-1978` (`trim_spare_chunks`);
helpers `shadow.rs:317` (`live_span_bytes`), `texture.rs:475` (`live_staging_bytes`).

### Every constant at e05d463

| Constant | Value | Where |
|---|---|---|
| "limited host RAM" | total physical RAM **≤ 32 GiB** (`ullTotalPhys`), sampled once | `residency.rs` `limited_host_ram` |
| enter pressure | free physical **≤ 4 GiB** OR available commit **≤ 6 GiB** | `next_pressure(false, ..)` |
| leave pressure | free physical **≥ 6 GiB** AND available commit **≥ 8 GiB** (stays while `< 6` or `< 8`) | `next_pressure(true, ..)` |
| sample period | once a second (the submission sweep) | `host.rs:443` |
| headroom | `min(ullAvailPhys, ullAvailPageFile)`; a failed sample keeps the last value | `refresh` |
| prefetch limit under pressure | `min(bytes, 16 MiB)` per frame — **defined and tested but not called anywhere in e05d463 / 1ad6e67** (wired in 18bdf79) | `prefetch_limit` |
| retire lag (`VICTORY_BURY_LAG`) default | **16** submissions on a limited host, else 64 | `host.rs:500` |
| spare scratch chunks cap | limited host: `SPARE_MOST` = **128 MiB**; else with keep-freed `SPARE_KEPT` = **1 GiB** | `host.rs:1852-1854`, `1935` |
| idle time of RAM caches | computed from `min(free RAM, headroom)`: ≥ 12 GiB and keep-freed → 300,000 ms; < 2 GiB → 2,000 ms; < 4 GiB → 5,000 ms; else 10,000 ms | `host.rs:382-401` |
| under pressure, every sweep | all spare scratch chunks dropped at once (`trim_spare_chunks`) | `host.rs:444-446` |

HEAD differs (for the other part's reviewer; listed so nobody ports the stale numbers): pressure enters at free RAM
≤ 3 GiB or commit ≤ 4 GiB and leaves at ≥ 5 GiB and ≥ 6 GiB; `critical_commit` = commit < 1,536 MiB or an
`E_OUTOFMEMORY` in the last 500 ms; upload reserve 512 MiB (256 MiB for a bake); growth reserve 1,024 MiB (256 MiB
bake); commit sampled at most every 32 ms; `prefetch_limit` = 16 MiB when critical, 64 MiB under pressure or on a
limited host with ≥ 1 GiB of live staging; `ram_idle_ms` passes `keep_freed() && !limited_host_ram()`.

### Mechanism end to end

1. Once a second the submitting thread's `sweep` calls `residency::refresh()`: one `GlobalMemoryStatusEx`, stores the
   headroom, flips `PRESSURED` with the hysteresis above, and logs on each flip. The warning names the consumers:
   `buffer_mirror_span_mib` (shadow copies of title memory), `texture_staging_mib` (live upload staging),
   `spare_scratch_mib` (`ALIVE[4]`), `game_filled_gpu_texture_mib` (`KINDS[1]`). Message: "host memory pressure:
   limiting texture prefetch and idle upload storage"; on recovery "host memory pressure cleared".
2. Under pressure the sweep frees all idle scratch chunks immediately.
3. `ram_idle_ms` takes the smaller of free RAM and the headroom, so **low commit** (page file nearly full while RAM
   still looks free) shortens the idle time of staging buffers, loose shadows and the mapped pipeline library.
4. The rest (`limited_host_ram`) turns the base's "keep freed memory" hoarding down on a ≤ 32 GiB PC.

**What it solves.** Memory, not frame time: the base (avatarmanz, 2026-11-25) introduced `VICTORY_KEEP_FREED`
(`host.rs:353-368`: memory once given to the card is kept — 64 empty texture blocks of 64 MiB, 5-minute idle for
staging and windows with 12 GiB free, 1 GiB of scratch chunks, retire lag 64) to stop device losses from shader reads
into freed memory. On a 32 GiB PC that hoard ran the process out of **commit** (their journal, later:
"Peak process commit was 34.42 GiB"; "4,241 MiB of live texture staging, 277 MiB of commit headroom and 7,648 MiB of
free physical memory" — docs/bug-journal.md:7,15). Note the lesson in that last quote: **free physical RAM was 7.6 GiB
while commit was 277 MiB**. A guard that looks only at free RAM never fires.

### Ours

- `host.rs:176-191` `ram_idle_ms(avail_phys)`: free RAM only, thresholds 2 / 4 GiB → 2 / 5 / 10 s. No commit, no
  hysteresis, no pressure flag, no log line. Callers: `host.rs:1865` (`trim_scratch`), `texture.rs:980`
  (`let_go_idle`), both with `available_physical_ram()` (`host.rs:140-148`).
- `host.rs:222-244` `sweep`: no memory sample.
- `host.rs:1786` `SPARE_MOST = 128 MiB`; `give_chunks` `host.rs:1921-1949` always uses it. `trim_scratch`
  (`host.rs:1861-1889`) frees chunks idle for `ram_idle_ms` and 4 frames.
- `host.rs:263-276` `bury`: no lag at all (drops when the submission finished).
- grep: `residency` 0 hits as a module; `limited_host_ram` 0; `keep_freed` / `VICTORY_KEEP_FREED` 0;
  `VICTORY_BURY_LAG` 0; `trim_spare_chunks` 0; `pool.rs` does not exist in ours.

So ours has **none of the hoarding** this commit turns down, and the `limited_host_ram` parts (bury lag 16, spare cap,
5-minute idle off) are **not applicable**. What is worth taking is the detector.

### Port-ready entry (the applicable part)

- **Where:** new `crates/victory-gpu/src/residency.rs` (their e05d463 file, minus `limited_host_ram` and
  `prefetch_limit` if unused); `mod residency;` in `lib.rs`; in `sweep` (`host.rs:238`) call `residency::refresh()`
  first; in `ram_idle_ms` (`host.rs:176`) take `min(avail_phys, residency::headroom())`; in `trim_scratch`
  (`host.rs:1861`) when `pressured()` use an idle time of 0 (ours already has `Spare::trim`, so no separate
  `trim_spare_chunks` is needed — and ours retires the chunks (`retire(chunk)`, `host.rs:1886-1888`) instead of
  dropping them at once, which is the safer form; keep it).
- The log line needs `shadow::live_span_bytes()` (ours: `CARD_BYTES[0] + WINDOW_CARD_BYTES` exist at
  `shadow.rs:381`, `594`; `WINDOW_MAPPED_BYTES` 0 hits — drop that term or use ours' equivalent) and
  `texture::live_staging_bytes()` (= `STAGED[1]`, ours `texture.rs:912`). `ALIVE` (`host.rs:686`) and `KINDS`
  (`resource.rs:1176`) exist.
- **Switch.** Theirs: none. Proposed: `VICTORY_RESIDENCY` default on (`=0`: no sampling, `headroom()` = None,
  `pressured()` = false); thresholds as `VICTORY_RESIDENCY_ENTER_GB` / `_LEAVE_GB` only if needed.
- **Verify.** The warning "host memory pressure" with its four sizes appears when commit runs low; port their two unit
  tests (`either_constraint_enters_pressure_and_both_must_recover`, `prefetch_respects_...`). In a normal run of ours
  it should **not** fire; if it does, the four numbers say who holds the memory — that alone is useful tooling.
- **Picture:** neutral at e05d463 (nothing is skipped; caches are trimmed sooner). The later use (`prefetch_limit`
  capping texture prefetch to 16 MiB a frame under pressure) delays textures — on our PC that would add pop-in, so
  treat that later wiring as "trades picture under pressure" and review it with the other part.
- **Claim:** no number in these commits. **Size:** S (~110 lines). **Risk:** low. **Prerequisites:** none.
- **Addresses:** out-of-memory safety and diagnosis; bound (d) only indirectly. **It does not help VRAM**: every
  threshold here is system RAM / commit. Our tight budget is the card (14.4 / 15.1 GB); for that the relevant code is
  ours `vram_idle_ms` (`host.rs:153-171`: 75 % → 2.5 s, 85 % → 1 s) and their `pool::room` (`pool.rs:44-71` at HEAD:
  64 spare blocks below 85 % of the card's budget, else 2), which ours has no equivalent of and which is base code,
  not part of these commits.

---

## H1-6 Present: no same-state barriers; presented texture kept alive (HOST)

**Theirs:** 1ad6e67, `host.rs` present path (HEAD ~9160-9172 and ~9212-9224); e05d463 `copy.keep(texture)`.

- A transition barrier whose before and after states are equal is invalid in D3D12. The old code always recorded
  `transition(texture, state, COPY_SOURCE)` and back; when the title left the texture in `COPY_SOURCE` (0x800 — also
  the layer's default guess when the state is unknown), both barriers were no-ops with equal states. Now both are
  pushed only `if state != D3D12_RESOURCE_STATE_COPY_SOURCE`.
- `copy.keep(texture)` (and `keep(over)`): the presenter's list holds a reference to the shown texture until the list
  is reset, so a title that releases the texture right after presenting cannot free it under a present still in the
  air.

**Ours.** `host.rs:11038-11046` and `11054-11061`: unconditional barriers. No `keep` of the texture on the copy list
(`host.rs:11036`); the behind-the-line present clones the texture into `Held::Present` (`queue.rs:144-147`), which
covers the wait in the line but not the time after the list is sent.

**Change.** Two `if state != COPY_SOURCE` guards and one `copy.keep(texture)` after `copy.reset()`.
**Switch:** none needed. **Verify:** debug layer no longer reports "before and after states are the same" at present;
no visible change expected. **Picture:** neutral. **Size:** S (10 lines). **Risk:** low.

---

## H1-7 `VICTORY_KEYS` in release builds (HOST, victory-hle, tooling)

**Theirs:** e05d463, `gameinput.rs` — removes three `#[cfg(feature = "diagnostics")]` gates (and the non-diagnostics
`Vec::new()` branch) so the scripted key list works in a release build (`gameinput.rs:1249` `scripted`).

**Ours.** Gates still present: `gameinput.rs:1555`, `1572-1575`, `1622`. README.md:443 documents `VICTORY_KEYS`.
Their `scripted` also has a different tuple `(Duration, Duration, u8)` (a hold length per key, base change) — ours is
`(Duration, u8)` with a fixed 0.2 s; that is not part of this commit.

**Worth it?** Only if our unattended runs are made with release builds without the `diagnostics` feature and currently
need an external key sender. **Size:** S (delete 5 lines). **Risk:** none (no effect unless the variable is set).
**Picture:** neutral.

---

## H1-8 Two-plane presentation (Series S) — describe; probe first (HOST)

**Theirs:** new `crates/victory-gpu/src/compose.rs` (325 lines, e05d463); `host.rs:8980-9230` `present_planes`
(e05d463 + 1ad6e67); `queue.rs:654-688` `RetailPlane` / `overlay_resource` (1ad6e67), `queue.rs:965-1040` (both
present paths), `queue.rs:1074` `show_planes`; `queue/behind.rs:394-399`, `576-586`, `822-830` (the job carries the
overlay); `resource.rs:2384` `is_resource` (1ad6e67).

### Mechanism end to end

1. The title calls `PresentX(planes, token, plane_parameters)`. On Series S it passes **two** planes: the scene and the
   interface (HUD, map, menus), which the console's display hardware scales and blends.
2. `present` takes plane 0 as before. For `planes > 1`, `overlay_resource` reads the **second** plane description at
   a **72-byte stride** (see game facts), copying memory with the fault-tolerant `host::read_into` rather than
   dereferencing, then checks the resource pointer really is one of the layer's resources (`is_resource`: its first
   word equals the address of the layer's resource function table). If it equals plane 0's resource it is ignored.
3. Each plane's host state is looked up (`host_transition(resource, COPY_SOURCE)` and put back). The overlay travels
   with the present job (`Job::Present(texture, overlay, shown)`) so the composite runs in order behind the lists.
4. `Host::present_planes`: if an overlay is present and both planes have a float-sampleable view
   (`compose::view`), the swap chain format becomes `R8G8B8A8_UNORM` and a `Composer` is made (root signature + PSO
   from HLSL compiled at run time with `D3DCompile`, `vs_5_1` / `ps_5_1`). Both planes go to
   `PIXEL_SHADER_RESOURCE`, the back buffer to `RENDER_TARGET`, a full-screen triangle is drawn:

   ```hlsl
   float4 s = scene.SampleLevel(linear_clamp, uv, 0);
   float4 o = over.SampleLevel(linear_clamp, uv, 0);
   return float4(saturate(o.rgb + s.rgb * (1 - saturate(o.a))), 1);   // premultiplied "over"
   ```

   Each plane is scaled to the back buffer by the sampler (bilinear), so planes of different sizes work. States are
   restored; back buffer ends in `COPY_DEST` so the diagnostics panel / capture code that follows is unchanged.
5. Descriptors: one 2-slot shader-visible heap **per presenter list slot** (`slot.map_or(0, |i| i + 1)`), so presents
   sent ahead do not overwrite each other's views.
6. Fallbacks (1ad6e67): if the composer cannot be made or cannot record, the function calls itself with
   `overlay = None` (base plane only). First success logs once: "display output: compositing two planes (overlay
   active)" with frame, size and both formats.
7. 1ad6e67's actual fix: e05d463 read plane 1 with `plane_parameters.add(1)` on the 24-byte `Plane` prefix, i.e. at
   offset 24 — inside plane 0's own tail — so the "overlay" was garbage or the base plane and the screen was black.
   The composer creation was also moved before the swap chain resize check (a format change resets it).

### Does Series X present two planes?

Not answered by these commits. The only statements are the author's: `compose.rs:1-3` "Grand Theft Auto VI presents so
on Series S", and our own shortlist line 247 ("only Series S presents two planes", from an earlier fork review —
another fork also has a `compose.rs`). Neither tree contains a measurement of the `planes` argument in Series X mode.
Ours reads plane 0 only (`queue.rs:126`, `1371`) and never logs `planes`. **Free test (5 lines, no behaviour
change):** in `present_now` (`queue.rs:1284`) add `once!(planes, "PresentX plane count")` — or warn once when
`planes > 1`. If a Series X run says 1, this whole section is not for us. If it says 2, our picture is missing the
interface plane and the port becomes a visual fix.

### Compared with `VICTORY_UI_PLANE` (fork-review-shortlist.md:147, 204)

Same feature (title's interface plane shown: HUD, map, menus), earlier implementation by rorodev6 8604369 / soldsawn
8adb7aa, 238 lines, behind a switch (`VICTORY_UI_PLANE`, on), "checked on RX 6700 XT only", still 0 hits in ours.
codec_d's version: no switch (active whenever a second valid plane arrives), ~520 lines over 6 files, with the
72-byte stride fix, fault-tolerant reads, per-slot descriptor heaps for presents in the air, fallbacks, and a GPU
readback unit test (`composes_scaled_premultiplied_plane_on_gpu`, diagnostics builds). cool_fox itself has 0 hits for
`VICTORY_UI_PLANE`. If the probe says we need it, port codec_d's (the stride fix is the part that made it work), and
add our own switch `VICTORY_UI_PLANE` default on.

### Port notes if needed

- Ours has no `queue/behind.rs`; the equivalent carrier is `Held::Present(texture, state, frame)` (`queue.rs:144-147`)
  and the direct path at `queue.rs:1368-1388`; `Host::present` is `host.rs:10955`; `swap_format` `host.rs:10508`;
  `Presenter` `host.rs:10520`. `host::read_into` exists (`host.rs:10304`); `is_resource` 0 hits (8 lines).
- **Picture caveat:** with two planes the swap chain is forced to `R8G8B8A8_UNORM`. A 10-bit or 16-bit float scene
  plane is then written to 8 bits, and the shader does no transfer-function conversion; ours `swap_format` path may
  keep a wider or sRGB format. On Series X at 1440p this would be a (small) loss against the single-plane copy —
  check before enabling: keep the back buffer in the scene plane's `swap_format` instead.
- **Size:** M (~520 lines, 5 files). **Risk:** medium (present path, formats). **Shader cache:** none.
  **Claim:** no number; "Fix Series S two-plane black screen" (1ad6e67 title).

---

## Hunks not worth porting

| Hunk | Why not |
|---|---|
| `host.rs:497-502` bury lag 16 on ≤ 32 GiB (`VICTORY_BURY_LAG`) | Base feature ours lacks (ours `bury`, `host.rs:263`, has no lag). Nothing to turn down. |
| `host.rs:1935-1940` spare cap `SPARE_MOST` instead of `SPARE_KEPT` on ≤ 32 GiB | Ours always uses `SPARE_MOST` = 128 MiB (`host.rs:1786`, `1930`). Already the conservative value. |
| `host.rs:1962-1978` `trim_spare_chunks` | Ours has `trim_scratch` (`host.rs:1861`) with retirement; only the "under pressure → idle 0" rule is taken (H1-5). Theirs also drops the chunks immediately (`drop(freed)`) without waiting for running submissions; ours retires them — keep ours. |
| `host.rs:4299-4330` `transitioned` (one barrier per buffer in the batched card copies) | Fixes duplicate transitions of one shadow buffer listed twice in their batched "every copy first, then one barrier call" list. Ours records per shadow with a tracked `state` (`host.rs:6236-6269`): the duplicate cannot arise the same way. |
| `compose.rs`, `present_planes`, `RetailPlane`, `behind.rs` job change | Series S presentation; see H1-8 — probe first. |
| `lib.rs` `mod compose; mod residency;` | Follows the two ports above. |
| `shadow.rs` `live_span_bytes`, `texture.rs` `live_staging_bytes` | Log helpers for H1-5 only. |
| `pipeline.rs` +3 | Part of H1-1. |
| `draw.rs` `pub(crate) struct Few` | Visibility change of base code ours lacks; optional in H1-1. |
| `residency::prefetch_limit` | Dead code in these two commits (no caller until 18bdf79). Its later use delays textures under pressure: review with that commit. |

---

## Game facts

1. **PresentX plane parameters are 72 bytes each**, not 24. Layout (`queue.rs:654-667`, 1ad6e67;
   `const _: () = assert!(size_of::<RetailPlane>() == 72)`):

   | Offset | Size | Field |
   |---|---|---|
   | 0 | 8 | frame token |
   | 8 | 4 (+4 pad) | resource count |
   | 16 | 8 | pointer to the resource pointers |
   | 24 | 8 | pointer to source rectangles |
   | 32 | 8 | pointer to destination (scaling target) |
   | 40 | 4 | color space |
   | 44 | 28 | seven option words |

   The comment says "Older callers pass only the prefix for single-plane presents". Ours `Plane`
   (`queue.rs:1085-1090`) is the 24-byte prefix and is only ever read at index 0, so ours is correct as long as only
   plane 0 is read. Anyone indexing planes must use the 72-byte stride.
2. On Series S the title presents **two planes**: scene and interface; the display blends the second over the first
   as premultiplied alpha and scales each to the screen (`compose.rs:1-3` and the shader). Series X: unverified.
3. Plane formats the author prepared for (`compose.rs:195-211`), by DXGI number: typeless 9 → R16G16B16A16_FLOAT,
   23 → R10G10B10A2_UNORM, 27 → R8G8B8A8_UNORM, 90 → B8G8R8A8_UNORM; typed 10, 11, 13, 24, 26, 28, 29, 31, 67, 87, 91
   taken as they are. (A list of what is handled, not proof of what the title uses.)
4. The title asks DirectStorage for a **32 MiB** staging buffer "and stops if this fails" (`dstorage.rs:230`; same
   comment in ours `dstorage.rs:221`).
5. `SetEventOnCompletion` with a **null event** is a blocking wait and the title uses it (the base warning exists
   because it happens); a fence's GPU address is the address of its value (`fence.rs` `gpu_address`).
6. Lists exist that set a directly indexed root signature without both heaps set — implied by the fallback-heap fix;
   the commit gives no shader or pass name. Our fallback warning (H1-2 verify) would name the case.
7. The title's start screen listens to the pad, not the keyboard (`gameinput.rs` comment at the scripted-keys block;
   same text in ours `gameinput.rs:1551-1554`).

---

## Suspicious / to be careful with

- **No measurements.** A commit named "RTX 5090 fidelity & 32 GB RAM optimizations" has no number for any of its six
  features.
- `prefetch_limit` shipped unused; the first two-plane version (e05d463) could not have worked (wrong stride) and was
  fixed 12 minutes later — the e05d463 code was not run on the two-plane path before commit.
- `trim_spare_chunks` frees upload buffers immediately, with no wait for submissions in the air — the opposite of the
  base's own reason for `VICTORY_KEEP_FREED` (reads into freed memory lose the device). Scratch chunks in the spare
  pool are held by no list, so it is probably safe, but ours' `retire` form is safer.
- Fallback heaps hold one descriptor; they make the call legal, not the read correct.
- Two-plane compose forces an 8-bit back buffer (H1-8 caveat).

---

## Coverage

Commits examined: 2 of 2 (e05d463, 1ad6e67). Every hunk read in full (diffs of 1,575 and 289 lines).

| File | Commit(s) | Covered in |
|---|---|---|
| victory-gpu/src/compose.rs (new, 325) | e05d463 | H1-8 |
| victory-gpu/src/draw.rs | e05d463 | H1-1 (TablePlan, `tables`, `Few` visibility), H1-2 (heaps, `host_heaps_set`) |
| victory-gpu/src/fence.rs | e05d463 | H1-3 |
| victory-gpu/src/host.rs | e05d463, 1ad6e67 | H1-2 (fallback heaps), H1-5 (`ram_idle_ms`, `sweep`, `bury_lag`, `give_chunks`, `trim_spare_chunks`), H1-6, H1-8 (`present_planes`, composer), not-worth table (`transitioned`) |
| victory-gpu/src/lib.rs | e05d463 | not-worth table |
| victory-gpu/src/pipeline.rs | e05d463 | H1-1 |
| victory-gpu/src/queue.rs | e05d463, 1ad6e67 | H1-3 (wake in `Written::write`), H1-8 (`RetailPlane`, `overlay_resource`, both present paths, `show_planes`) |
| victory-gpu/src/queue/behind.rs | e05d463 | H1-8 |
| victory-gpu/src/residency.rs (new, 93) | e05d463 | H1-5 |
| victory-gpu/src/resource.rs | 1ad6e67 | H1-8 (`is_resource`) |
| victory-gpu/src/shadow.rs | e05d463 | H1-5 |
| victory-gpu/src/texture.rs | e05d463 | H1-5 |
| victory-hle/src/modules/dstorage.rs | e05d463 | H1-4 |
| victory-hle/src/modules/gameinput.rs | e05d463 | H1-7 |

Not read: nothing in scope. Outside scope and only consulted for context: HEAD versions of `residency.rs`, `pool.rs`
(lines 25-75), `host.rs` (345-420, 1840-1870), `draw.rs` (2515-2535, 2640-2660, 4510-4575), `docs/bug-journal.md`
(grep only).
