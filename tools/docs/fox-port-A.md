# Fox port, coder A (hle, loader, core, pe, apps, gpu/frame.rs + own gpu files) - port log

Copy: `Tools\par\foxA` (clone of victory-fox @f4b48ad), commits on top (main cherry-picks):
8120ecd group 1 tooling; 55b119a map defer / pool discard / unmapped hook; d36624e dstorage oldest-first / workers;
79e07b7 remap same; 5a4cacf fox defaults + readback / chunk hooks.

Exe with C's host.rs patch applied (for runs until C merges it): `C:/GTAVI/Tools/par/foxA/bin-final-with-gpu-patch/victory.exe` (+pdb).
Tests: hle 167 + loader 45 + my gpu files 11, all pass.

Build (diagnostics is REQUIRED for the frame log, hang watch and the lines the scripts read):
`CARGO_TARGET_DIR=C:\GTAVI\Tools\par\foxA\target bash /c/GTAVI/Tools/cargo-bg.sh build --profile diag -p victory --features diagnostics --offline --locked`
(same command as the old tree's `victory-run` diag builds; Cargo.toml documents it under `[profile.diag]`).

Rule applied (user, 2026-11-26): cool_fox as it ran is the floor. Tooling ON; every port that changes fox's
default behaviour defaults to FOX's behaviour until main A/Bs it (column "changes fox default").

## Inventory and final status

| # | Switch / feature (ours) | Fox base | Class | Final status | Default in port | Changes fox default |
|---|---|---|---|---|---|---|
| 1 | `VICTORY_AUTO_INPUT` (+`_START_S`, `_GAP_MS`) screen-driven script | none (has timed `VICTORY_KEYS`, `VICTORY_ONE_PAD`) | lacking | PORTED (hle gameinput.rs; draws from gpu frame.rs `PRESENTED_DRAWS`) + 7 tests | off unless set | no |
| 2 | `VICTORY_AUTO_A` | none | lacking | PORTED (+test) | 0 (off) | no |
| 3 | Caption `Scarlett \| <game> \| F FPS \| M ms \| D draws` + logo icons (`VICTORY_WINDOW_ICON`) | `<title> \| VIctory F fps M ms/frame D draws/frame`: scripts' `\|`-field awk reads 0 draws -> input-check/run-compare fail | conflicts | PORTED: gpu shell.rs (mine) + hle d3d12x.rs (title dir) + lifecycle.rs (WM_NCDESTROY); host.rs call sites in `patches\foxA-needs-gpu.patch` (C) | on | caption text only (tooling) |
| 4 | `VICTORY_FRAME_LOG` ("frame times:", "usual frame", "long frame" every 5 s) | none | lacking | PORTED: gpu framelog.rs (mine); frame boundary = the title's frame wait (frame.rs), not the present; columns fed by host/queue read 0 until C wires `framelog::add(T::..)` | on in diagnostics | no (log only) |
| 5 | `VICTORY_HANG_WATCH_S` | none | lacking | PORTED: gpu hangwatch.rs (mine), started at the first frame wait, stacks via loader `dump_stacks`; progress = frames + draws; `hangwatch::set_state(fn)` for C's lock/flight state; sites `hangwatch::at(&"...")` for C to place in host/queue | 8 s (diag) | no |
| 6 | `VICTORY_PRESENT_SKIP` | none | lacking | PORTED as `shell::present_maybe`; the call in host.rs present is in the patch (C) | 0 | no |
| 7 | `VICTORY_AFFINITY=free` (F2) | none | lacking | PORTED (hle affinity.rs); `free` must be asked for | console (fox) | no (yes with =free) |
| 8 | `VICTORY_MAP_FAST` (joined GPU-view runs) | `VICTORY_MAP_THREADS` (8 threads side by side, page views), `VICTORY_MAP_LEAN` (own placeholders, commit once) | conflicts | NOT PORTED: fox's mechanism kept (rule: never replace a fox path) | - | - |
| 9 | `VICTORY_MAP_FAST_CPU` | - | rejected (run-45 crash) | not ported | - | - |
| 10 | `VICTORY_MAP_DEFER` (unmaps on "victory-map") | none | lacking | PORTED (mapdefer.rs + xmem: Scatter, map_alias, map_aliases, XMemMapPhysicalPages, VirtualQuery, shown_at); maps still go by the fox path (side by side); line "map deferral over the last S s ..." every 5 s; 3 tests | 0 (=1 on) | yes when on |
| 11 | `VICTORY_UNMAP_EXACT`, `VICTORY_WINDOW_UNMAP` (hle side) | none | lacking | hle side PORTED: xmem unmap()/change() call `victory_gpu::hlehooks::title_memory_unmapped(at,len)` for views really gone; gpu side (resource.rs / shadow.rs) = C, registers via `hlehooks::on_title_memory_unmapped` | counting only | no |
| 12 | `VICTORY_DSTORAGE_LINEAR_VIEW` | `Section::window` lasting whole-section view (`VICTORY_SECTION_WINDOW`) | present | fox mechanism | - | - |
| 13 | `VICTORY_DSTORAGE_PAGES_AT_SUBMIT`, `_LATE_PAGES`, `_STALE_MS` | requests start at the title's Submit (`VICTORY_DSTORAGE_HALF` off), `page_list` re-read | present (other form) | fox mechanism | - | - |
| 14 | `VICTORY_DSTORAGE_OLDEST_FIRST` | none | lacking | PORTED (`Lanes::start_as`) + test | 0 (fox order; 1 ours) | yes when 1/2 |
| 15 | `VICTORY_DSTORAGE_WORKERS` | every logical processor | lacking | PORTED: `=physical` (our default), `=N`; + test | unset = fox (logical) | yes when set |
| 16 | `VICTORY_DSTORAGE_OWN_HANDLES` | file handle per worker (fbc1830, `Io::Handles`) | present | fox mechanism | - | - |
| 17 | `VICTORY_POOL_DISCARD` (+`_BELOW_MB` 4096, `_DELAY_S` 0 adaptive) | none (fox's lasting window keeps pages resident: same RAM cost as run-49) | lacking | PORTED on fox's Section window (resident/discarded bits, "victory-pool-discard" thread, "pool discard: ..." line every 30 s) + test | 0 (=1 on) | yes when on |
| 18 | `VICTORY_COMMIT_ONCE` | `VICTORY_MAP_LEAN` + per-page committed bits | present | fox mechanism | - | - |
| 19 | `VICTORY_MAP_PREFAULT` (default 0 in ours) | none | lacking | NOT PORTED yet (default off in ours, unmeasured; needs gpu `prefault_touch`) | - | - |
| 20 | `VICTORY_REMAP_SAME` (Neptune) | none | lacking | PORTED for the CPU view (XMemMapPhysicalPages + side-by-side scatter); line "title memory mappings asked for ..." every 5 s | 0 (fox; 1 ours) | yes when 1/2 |
| 20b | `VICTORY_ALIAS_RUNS` (Neptune, GPU-view runs) | fox page-per-view + threads | conflicts | not ported (overlaps MAP_THREADS) | - | - |
| 21 | `VICTORY_LAZY_GPU_VIEW` (F3 meter) | none | lacking | not ported (needs gpu gpuview.rs, C) | - | - |
| 22 | `VICTORY_STREAM_METER` (F9/F14 hooks) | none | lacking | hle hooks PORTED (group drop 0x1420cf7b0, drawable removal 0x140151bb0), installed only with =1 (old tree: always); forwarded to `hlehooks::readback::Sink` (C registers the gpu stream meter) | off | no |
| 23 | `VICTORY_STREAM_ISSUE_WAIT_FRAMES` / `_FREE_WAIT_FRAMES` | - | rejected (F5) | not ported | - | - |
| 24 | `VICTORY_READBACK_SET` / `VICTORY_READBACK_LOCKS` (F1, F4) | none (no loader callhook either) | lacking | hle hooks PORTED (readback.rs, loader callhook.rs, tls::near pub(crate), apps run.rs install); calls go to `hlehooks::readback::Sink` (gpu readback module = C) | off (old tree: SET on) | no |
| 25 | `VICTORY_CHUNK_METER` + `VICTORY_CHUNK_COUNT` (F19-F21) | none | lacking | PORTED (hle chunkmeter.rs, gpu chunks.rs mine); the refresh side `chunks::refreshed(from,to)` needs one call in C's shadow.rs refresh | off / unset | no (COUNT is an experiment) |
| 26 | `VICTORY_TITLE_TIMELINE` | fox `VICTORY_TIMELINE` (other content) | gpu (C) | - | - | - |
| 27 | `VICTORY_FAULT_METER` | none | gpu faults.rs (C) | hle call sites not ported (no gpu faults module) | - | - |
| 28 | `VICTORY_PATCH_WAITS`, `VICTORY_PATCH_CLZERO` | same | present | - | - | - |
| 29 | `VICTORY_CLZERO_LOOP15` (myokex) | none | lacking | NOT PORTED (exception-path speed change, 365-line diff against a diverged console_insn.rs; A/B candidate) | - | - |
| 30 | `VICTORY_DYNAMIC_SLOTS` | present (gpu) | present | - | - | - |
| 31 | Crash-reporter / network block (submit.backtrace.io) | present (net.rs, + `blocked_lookup`) | present | - | - | - |
| 32 | Host DLL policy | present (kernel.rs) | present | - | - | - |
| 33 | Loader stack symbolizer (`set_symbolizer`) | none | lacking | PORTED (stacks.rs); no symbolizer registered (fox gpu has no PDB reader) | - | no |

## For coder C (gpu files I do not own)

`patches\foxA-needs-gpu.patch` (host.rs, applies to f4b48ad): caption via `crate::shell::caption_for`, `crate::shell::set_icons(hwnd)`
in `presenter`, Present through `crate::shell::present_maybe` (VICTORY_PRESENT_SKIP). Also, no patch yet (needs C's knowledge):
- framelog feeds (`crate::framelog::add(T::X, n)`): PrepareUs/CardWaitUs/Submissions (host SUBMISSION_US sites), CopyUs/CopyBytes,
  CopyBackUs/Bytes, CardIdleUs, PresentUs, ExecuteUs (queue::execute), Uploads/RefreshUs, Remaps/RemapUs; run-freezes/run-heavy
  read "preparing", "waiting for the card", "copy-back", "other lists", "card: idle", "remapping:".
- "memory housekeeping" line (live-status) does not exist in fox; "memory of textures" and "the queue:" do (copystat/host).
- hang watch: `crate::hangwatch::set_state(host_state_fn)`, `let _site = crate::hangwatch::at(&"...")` at submit/relay/queue waits.
- `crate::chunks::refreshed(from, to)` where a refresh copies title memory into a host copy (old tree: shadow.rs:4657).
- UNMAP_EXACT / WINDOW_UNMAP: register `crate::hlehooks::on_title_memory_unmapped(f)`.
- Read-back / stream meters: register `crate::hlehooks::readback::set(Sink{..})` once the gpu readback module is ported.
New gpu files of mine (mod lines in lib.rs): framelog.rs, hangwatch.rs, shell.rs, hlehooks.rs, chunks.rs; gpu Cargo.toml gained
`Win32_Graphics_Imaging`, `Win32_System_Com` (icons).

## Needs a game run to judge
Auto input reaching the scene with fox draw counts (thresholds from our tree; fox menus showed the same counts in run-100);
frame log lines with the frame-wait boundary; A/B each "yes" row above against the fox floor (MAP_DEFER=1, POOL_DISCARD=1,
REMAP_SAME=1, DSTORAGE_OLDEST_FIRST=1, DSTORAGE_WORKERS=physical, AFFINITY=free) plus render check.
