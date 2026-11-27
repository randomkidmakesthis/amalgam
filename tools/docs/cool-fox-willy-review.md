# Review of the "cool_fox build 30e54b4" source drop (analyst Z, 2026-11-26)

Read-only review. Nothing from the drop was built, run or executed; no code of ours was edited.

- Drop: `C:\GTAVI\Tools\ref-forks\cool_fox-willy\src` (paths below are relative to it unless they start with `ours:`).
- Base: tag `coolfox-base` (f4b48ad) of `C:\GTAVI\Tools\victory-fox`, extracted to a scratch folder with `git archive`.
- Ours: `victory-fox` HEAD 6c86c22 (branch local/scarlett-fox), plus `par\foxM` 3350e0a, `par\foxV` 4913a26,
  `par\foxT` c436c00, `par\foxW` 4b456ad.
- Method: `diff -ruN base drop` (7,309 lines, 41 files), every non-test hunk read; our trees searched for the same
  switch and function names; two of our run logs (run-182, run-187) and the floor log (run-103) read for evidence.
- No text addressed to an AI or assistant was found in the drop, its README or its launcher.

The diff touches 41 files, all `.rs` except one `Cargo.toml` (two feature flags). Everything else is byte-identical
to the base: root `Cargo.toml`, `Cargo.lock`, `.cargo/config.toml`, both `build.rs`, `scripts/play.ps1`,
`Play-VIctory.cmd`, `xtask`, `tests`, `docs`, `rust-toolchain.toml`.

## The three things to know first

1. **The "41 -> 27 GB, each texture keeps only its used sizes" gain is not code. It is one launcher line: they
   stopped setting `VICTORY_LEVELS_ON_DEMAND=1`.** The drop changes nothing in `tiles.rs`, `texture.rs` or the
   texture part of `resource.rs`. That switch turns off the base's default mechanism (`VICTORY_HELD_LEVELS`, host
   copies made from the first level a view shows). **Our run set has `VICTORY_LEVELS_ON_DEMAND=1`, and our logs show
   the same state they describe**: in run-182, run-187 and the floor run-103, zero textures were ever made level by
   level and the held-levels path was off, so every placed texture took its whole chain; the card was over its
   budget (15,940 MB in use of 14,818 MB in run-187; 16,590 of 14,914 in the floor). Details in 3(a).
2. **Nothing in the drop touches the submitting thread's cost** (refreshes, write-watch, flights/landing, copies in
   card memory) or the between-pass gaps. Their README lists that thread as the remaining limit. Details in 3(c).
3. **Their "30 fps" is in Series S mode on a 24 GB card.** In the layer the mode only changes the device type the
   title reads and the memory figures reported; the title then picks its own resolution and budgets. Details in 3(e).

---

## 1. Safety check

**Verdict: no sign of anything malicious. The source is safe to read and, as far as static reading goes, to build.**
The one feature that loads outside code (`VICTORY_FSR_DLL`) is off unless the user names a DLL, and we will not use it.

| Item | Finding | Evidence |
|---|---|---|
| `crates/victory-gpu/build.rs` | Identical to base. Reads source files under the crate, hashes them, prints `cargo:rustc-env=VICTORY_TRANSLATOR`. No process, network or writes. | build.rs:8, :19, :38, :44 |
| `crates/victory-shader/build.rs` | Identical to base. Reads `src`, hashes, prints two `cargo:rustc-env` values. | build.rs:10, :45, :62, :64 |
| `tests/fixtures/hle-probe/build.rs` | Identical to base. Two linker arguments. | 4 lines |
| `scripts/play.ps1` | Identical to base. Clears `VICTORY_*` environment variables of the session, starts `victory run`. No download, registry or file deletion. | play.ps1:34, :38 |
| `Play-VIctory.cmd` | Identical to base. Sets switches, starts the exe. | 40 lines; :36 sets LEVELS_ON_DEMAND=1 |
| `outer\Play-cool_fox.cmd` (their launcher, not run) | Writes `game-folder.txt` next to itself and a marker file in the cache folder, makes the cache and data folders, starts `victory.exe shaders` then `victory.exe run ... --console series-s --execute`. No network, registry, PowerShell or deletion. | read in full |
| `xtask` | Identical to base. Spawns `git` only. | xtask/src/main.rs:36, policy.rs:60, :94 |
| Cargo manifests | Only `crates/victory-hle/Cargo.toml` differs: two more `windows-sys` features (`Win32_Graphics_Gdi`, `Win32_UI_Input`). No new crate, no git or registry dependency, no new build-dependency or proc-macro. `Cargo.lock` identical, and it holds no HTTP, TLS or socket crate. | Cargo.toml:23, :35 |
| Network | None added. The only WinHTTP code is the base's `victory-hle/src/modules/net.rs` (the title's own calls, loopback only), unchanged. | grep over all added lines |
| Process spawning | None added. `std::process::Command` for `dxc -dumpbin` is in a `#[cfg(test)]` helper that the base already had; only its temp file name changed. | victory-shader/src/emit/mod.rs:1078, :1093 |
| DLL loading | New: `LoadLibraryExW` of the path in `VICTORY_FSR_DLL`, and of `amd_fidelityfx_upscaler_dx12.dll` beside it. Only when the variable is set. This runs whatever DLL the user points at inside the process. | victory-hle/src/modules/fidelityfx.rs:68, :179 |
| File writes | New: `keys.txt` in the data folder, written once if missing, re-read every second. The existing shader dump write changed form only. | gameinput/binds.rs:289-291, :310; apps/victory/src/run.rs:102 |
| Registry | None. | grep |
| Patching code in memory | New: three places of the title's image are rewritten at load (`VirtualProtect` + copy), only with `VICTORY_FRAME_INTERVAL_MS` set, only if the bytes there match the expected ones. This patches the game, not Windows. | victory-loader/src/frame_rate.rs:113, :184, :207 |
| Input capture | New: raw mouse input registered with `RIDEV_INPUTSINK`, `GetAsyncKeyState` polling, `ClipCursor`. Used in-process for the stand-in gamepad; nothing is stored or sent. `RIDEV_INPUTSINK` means mouse motion is received while the window is in the background too; it is discarded unless the title window holds the mouse. | gameinput/mouse.rs:212, :284, :336 |
| Embedded data | `include_bytes!` only of `bcpack-tables.bin` (identical to ours) and `include_str!` of `victory.example.toml`. No other binary file in the tree; the two largest files are `host.rs` and `lower.rs`. The byte arrays in `frame_rate.rs` are x86 patterns to compare against, 25-58 bytes each, with a test that decodes them. | victory-formats/src/bcpack.rs:124; frame_rate.rs:113-143 |
| Obfuscation | None seen. Every added constant is commented with its meaning. | — |

Not checked: `victory.exe` in the zip (not extracted, by instruction), so whether the binary matches this source is unknown.

---

## 2. Every functional change against coolfox-base

"Have it" refers to victory-fox HEAD; the four side trees carry the same answers (same grep results in each).

| # | Area | Files | Switch, default in the drop | What the code does | Have it? | Verdict for us |
|---|---|---|---|---|---|---|
| 1 | Memory | launcher only (`outer\Play-cool_fox.cmd`) | `VICTORY_LEVELS_ON_DEMAND` not set (base launcher sets 1) | With it unset, `held_only()` is true: placed textures are made from the first level viewed. No code change. | Mechanism yes (same base code). Our run set turns it off by setting LEVELS_ON_DEMAND=1. | **TEST first.** Zero code. Expected: several GB less on the card, fewer passes without room, lower commit. Risk: our ports have never run on this path. See 3(a). |
| 2 | Picture fix | `victory-gpu/src/resource.rs`, comment in `list.rs` | `VICTORY_E5_TARGETS`, default `r11g11b10`; `r16g16b16a16` = base | R9G9B9E5 render targets stand in as R11G11B10_FLOAT (32 bits, no alpha, negatives stored as 0) and not R16G16B16A16_FLOAT. | No. Ours is fixed at R16G16B16A16 (`ours: resource.rs:269`). | **PORT behind the switch with our default unchanged, then TEST.** Candidate for our sticky purple patches. Risk: fewer mantissa bits than the console format (6/6/5 against 9), so banding must be looked for. See 3(b). |
| 3 | Picture fix | `victory-shader/src/emit/lower.rs`, `victory-gpu/src/shader_cache.rs` | `VICTORY_NAN_GUARD`, default on (base had no guard) | NaN -> 0 and infinity -> ±65504 on pixel colour exports to float targets (every NaN), and on typed image and buffer stores (only the default quiet NaN and infinities; skipped for integer-format buffer views and for halves). Shader cache keyed on the switch. | Yes, equivalent for colour exports (ours on by default). Stores: ours only with `VICTORY_NAN_GUARD_STORES=1`, default off, because a packed colour 0xFF800000 was turned into -65504 (`ours: lower.rs:52, :7546`). | **SKIP the store half as a default.** Their image-store guard (`lower.rs:11075`) has no format check, so it repeats the damage we measured. Their integer-view exemption for buffer stores (`lower.rs:12026, :12092`) is a refinement worth taking only if we ever turn stores on. |
| 4 | Picture fix (faces) | `victory-shader/src/emit/halves.rs`, `lower.rs`, `mod.rs`; `victory-gpu/src/descriptor/host.rs`, `draw.rs`, `shader_cache.rs` | none (always on) | PR #927: buffer accesses that move 16-bit halves use unsigned-integer handles; the host shows a 16-bit float or signed buffer view at those table places as its unsigned twin, in a table made afresh for the draw. | Yes, same origin: `halves.rs` is byte-identical to ours; host half is our commit 9b2debf; ours has a switch `VICTORY_HALF_VIEWS` (default on). | Already have. **One piece missing, see #5.** |
| 5 | Picture fix (faces), correctness | `victory-gpu/src/draw.rs:3746-3754`, test in `descriptor.rs:1588` | none | The fresh table is sized to include the half places even when they lie past the range starts the used-resource entries give ("review on #927"). Without it the twin is never made for such a place and the next allocation can land on that slot. | **No.** `ours: draw.rs:3860-3876` builds `selected` from the indexed entries only; the twin is made inside `0..n` (`ours: draw.rs:4039`). | **PORT.** About eight lines plus their test. No speed effect. Risk low. Conflict: our `tables()` reads the places from `HALF_PLACES` and keys twin tables with an XOR, theirs passes `half` as a parameter and adds a word per place to the hash; take the sizing lines only. |
| 6 | Picture fix (faces), speed risk | `victory-gpu/src/draw.rs:186` | `VICTORY_EARLY_TEXTURES_MS` 12 -> 40; `VICTORY_BAKE_TEXTURES_MS` 1500 -> 10000 | More milliseconds a frame for the pass that uploads textures picked by index before a shader asks, and up to 10 s of synchronous uploads before a character-bake draw. This is the README's "bakes get the time they need". | Mechanism yes; our defaults are the base's 12 and 1500 (`ours: draw.rs:182`). | **TEST by environment only, after #1.** Faces already work for us. The pass runs on the thread that records the draw, so 40 ms a frame can add to the thread that already bounds us; 10 s before a bake is a freeze by design. Possible gain: less pop-in. |
| 7 | Speed (light scenes) | `victory-gpu/src/frame.rs`, `victory-loader/src/frame_rate.rs` (new), `apps/victory/src/run.rs` | `VICTORY_FRAME_INTERVAL_MS`, unset = off; launcher sets 16.667. `VICTORY_TITLE_FPS` overrides the rate or `off`. | Our frame clock ticks at the asked interval whatever the title sets; three reads of the title's frame-rate setting are patched at load so its frame time, its own limiter and its shortest simulation step follow. | No. | **PORT behind the switch (off), TEST later.** No effect on frames longer than 33 ms. See 3(d). Conflict: our `frame.rs` has frame-log hooks at the same place; small hand merge. |
| 8 | Tooling | `frame.rs` (`mod pace`), `run.rs` (`timers`) | `VICTORY_TIMER_LOG=1`, off | Frame-to-frame time bands every 5 s; the title's timers once a second (diagnostics builds). | Equivalent: `VICTORY_FRAME_LOG` (ours). | SKIP. Take `timers` only together with #7, to check game speed. |
| 9 | Memory | `victory-gpu/src/shadow.rs` | `VICTORY_WINDOW_CARD_PERCENT`, default 80 = base behaviour | The share of the card's budget up to which a new window outside buffers gets its copy in card memory (50-95). Above it the copy stays in PC memory. | No. Ours has the fixed `budget / 10 * 8` (`ours: shadow.rs:511, :547`). | **PORT (trivial, default unchanged). TEST only after #1.** While the card is over budget no value helps. Their comment records commit growing 29 -> 35 GB once the card passed 80 %; our card sits above 100 %. |
| 10 | Memory | `victory-gpu/src/pipeline.rs`, `pipeline/host.rs`, `shader_cache.rs`, `host.rs` | `VICTORY_DROP_DXIL=1`, off | A translated shader's host code is let go once a pipeline is made with it and read back from the shader cache (BLAKE3-checked) for the next pipeline. | No. | **SKIP for now.** Our log counts 101 MB of translations kept (run-187), so the most it can save is about 0.1 GB of 34 GB. Adds a disk read on later pipeline creation. |
| 11 | Speed + picture | `victory-hle/src/modules/fidelityfx.rs` (new, 1,356 lines), `kernel.rs`, `mod.rs`; `victory-gpu/src/external.rs` (new), `list.rs`, `list/thin.rs`, `draw.rs:1311` | `VICTORY_FSR_DLL=<path>`, unset = off | The title's FidelityFX loader is not mapped; its five API calls go to a PC FidelityFX DLL, with the host device, host list and host textures swapped in. | No. | **SKIP.** Needs a downloaded DLL and replaces the title's upscaler with another one. See 3(b), last part. |
| 12 | Picture (stand-in only) | `victory-gpu/src/ambient.rs` | `VICTORY_AMBIENT_LEFTOVER`, default off | Inside the `VICTORY_AMBIENT` stand-in: what to do with the light the title's resolve left behind (keep / hue only / drop). | No. | **SKIP**: it only acts with `VICTORY_AMBIENT`, which we do not use. Its comment matters for our purple, see 3(b). |
| 13 | Input | `victory-hle/src/modules/gameinput.rs`, `gameinput/binds.rs`, `gameinput/mouse.rs` (new), `lifecycle.rs`, `run.rs:102` | `keys.txt` in the data folder; `VICTORY_MOUSE_SENSITIVITY` 1.0 | Keyboard and mouse drive a stand-in gamepad with two layouts (on foot, driving); presses are held until the title has read twice (`Latch`), so a tap is not lost at low frame rates; cursor hidden while the mouse is taken. | No keyboard/mouse layer. We have scripted input (`VICTORY_AUTO_INPUT`). | **SKIP for the goal.** Optional later for hand play. The `Latch` idea (gameinput.rs:118-132) is worth remembering if scripted taps are ever lost in slow frames. |
| 14 | Cosmetic | `lifecycle.rs:282` | none | The window is painted black, not white, until the first present. | No. | SKIP (harmless; take with #13 if ever). |
| 15 | Tooling | `victory-gpu/src/lib.rs`, `victory-hle/src/lib.rs` | none | `frames_begun()` exported for logs. | Equivalent (`ours: frame.rs` `pub fn current`). | SKIP. |

Launcher settings we do not use, as asked (described, not recommended):

- `VICTORY_AMBIENT=0.8`: replaces the title's ray-traced diffuse light, which is empty while the ray-tracing
  structure builder is off, with an estimate computed from the previous frame's on-screen light, at 80 % strength.
  A stand-in picture.
- `VICTORY_AMBIENT_AIR=1.5`: bounds the light the air scatters to 1.5 times the estimated light around. Part of the
  same stand-in.
- `VICTORY_THIN=1`: the title's list calls are written down and translated on another thread. In our floor notes
  this was left out of the floor set ("no THIN, no AMBIENT").

"Shaders translated ahead" (`victory shaders <shaders.rpf>`) is the base's existing command; the drop only calls it
from the launcher once per build. No code change.

---

## 3. Deep dives

### (a) Texture sizes on the card and `VICTORY_LEVELS_ON_DEMAND` (the claimed 41 -> 27 GB)

**There is no code change behind this claim.** The drop's `tiles.rs` and `texture.rs` are identical to the base, and
`resource.rs` differs only in the R9G9B9E5 stand-in and one helper. The only difference is in their launcher, whose
comment reads: "VICTORY_LEVELS_ON_DEMAND, which stays off: with it on, every texture took all of its sizes on the
graphics card (the card filled up, then textures waited)".

The base has two mechanisms for the same problem, and one disables the other:

- **Held levels** (`VICTORY_HELD_LEVELS`, on unless `0`): a texture the title places in memory gets a host copy made
  from the first level its views show (`resource.rs:1273` `held_only`; ours `resource.rs:1244`, used at
  `ours: resource.rs:1584-1595` to pick `base` and at `ours: texture.rs:774` to read from that level only). Its
  own comment measures the levels shown at 1.2 and 1.6 GB of 6.3 and 6.7 GB of such copies.
- **Levels on demand** (`VICTORY_LEVELS_ON_DEMAND=1`): reserved textures given tiles level by level (`tiles.rs`).
  `held_only()` returns false whenever this is on (`ours: resource.rs:1247`: `... && !crate::tiles::wanted()`).
  A texture is made the tiled way only if it is 1 MB or more and judged "partly held" when its copy is made, and
  only if the card accepts the tiling (`ours: resource.rs:1558-1580`). Every other texture then gets its whole chain,
  because `base` stays 0.

**What our logs show with our switch set (LEVELS_ON_DEMAND=1):**

| | run-187 (ours, RT off) | run-103 (floor) |
|---|---|---|
| start-up line "take the card's memory level by level ... on=true" | present | present |
| "textures with levels on demand" status lines (printed once one tiled texture exists) | 0 | 0 |
| "textures from the first level viewed" lines (printed when held levels is on) | 0 | 0 |
| host copies the title fills | 18,840 alive, 7,159 MB | 17,023 alive, 6,996 MB |
| card memory in use / budget | 15,940 / 14,818 MB | 16,590 / 14,914 MB |
| passes that made no host copy for lack of room | 672 | 454 |
| title textures with no host copy yet | 52,854 | 110,560 |

The same zero counts hold in run-118, run-129, run-166 and run-182. The last runs that printed the tiled line are
run-94 to run-98, on the old tree. So on the fox base, in every run we have, **the switch made no tiled texture and
only switched held levels off**: each of ~18,000 placed textures carries its full chain. That is exactly their finding.

Why no texture qualifies as tiled on this base I did not determine. The gate is the `partly` test and
`Tiled::make` at `ours: resource.rs:1558-1580`; one counter or log line there would answer it.

**What would differ on our tree if we drop the switch** (keep `VICTORY_HELD_LEVELS` at its default):

- Host copies of placed textures shrink to the levels viewed. By the base's own measurement that is about a
  quarter of the bytes, so roughly 7.2 GB -> about 2 GB on the card. This is an estimate from a code comment, not
  from a run of ours.
- The card should fall under its budget, which it has never been in a fox run. Likely follow-on effects, each to
  be confirmed by the run: fewer passes without room (pop-in), no demotion of card memory by Windows, and new
  windows going to card memory again (the 80 % line of `shadow.rs`), which lowers commit. Their 41 -> 27 GB is the
  sum of such effects in Series S mode on a 24 GB card; ours will differ.
- Uploads read only the held levels (`ours: texture.rs:774`). With the switch on, the upper levels' memory belongs
  to neighbouring textures, and the `held_only` comment says most of what was found "changed" and uploaded again
  was other textures' bytes. So texture churn should drop as well.
- New behaviour that our ports have not met: a copy is replaced by one with more levels when the title starts
  viewing a larger level (`grown`, the status line's "replaced by one with more levels"). That is extra make/let-go
  traffic through the texture pool, so watch the pool-hole figure (now ~2.3 GB), `VICTORY_POOL_CLASSES`,
  `VICTORY_PARTIAL_DELTA`, `VICTORY_BAKE_DELTA` and foxV's page pool, all measured only with the switch on.
- The floor (`baseline-coolfox`) was also measured with the switch on, so a floor re-run without it is needed for a
  fair comparison.

### (b) Picture fixes, and whether they explain our purple bushes and patches

**Faces: PR #927 (half views).** On the console a buffer's 16-bit channels move as raw bits through float and
integer views alike. The translator found the places where a shader moves halves (`halves.rs:21, :72, :104`: d16
accesses, and whole-dword format loads whose every use reads one half as a half float), emits integer loads and
stores there (`lower.rs:6584-6595`, `:11771-11790`, `:12018-12110`, `:12163-12190`), and lists the table places in
`HostShader::half_views` (`emit/mod.rs:359-373`). The host then shows a 16-bit float or signed buffer view at such a
place as its unsigned twin (`descriptor/host.rs:3510, :3530`) in a table made afresh (`draw.rs:3660-3690`, `:3916`).
A host test records why: moved by way of floats through an integer view, 1 pattern of 65,536 survived on the cards
tried. We have all of this except the table sizing fix (#5 in the table).

**Faces: bake timing.** Only two default numbers (`draw.rs:186`): 40 ms a frame for the early texture pass and
10,000 ms before a bake draw, against 12 and 1,500. Their note: the old values left one face of noise and one of
blocks in the first cutscene. The title bakes faces once, so a source texture that is not on the card at that
moment is baked as nothing for the whole run. Our faces work, so this is a pop-in experiment for us, not a fix.

**"No more purple or blue skin": `VICTORY_AMBIENT_LEFTOVER` (`ambient.rs:113-124`, `:350-358`).** This lives inside
the `VICTORY_AMBIENT` stand-in. With the ray-tracing builder off, the title's screen-probe resolve leaves "in blocks
of the picture's tiles, what the probes' memory held" (base comment at the top of `ambient.rs`). The old stand-in
kept the larger of that leftover and its estimate, keeping the leftover's hue; their note: "without rays the
leftover is whatever that memory held (Neptune-VE saw bright magenta on translucent surfaces): a sheet of paper ...
was a glowing magenta". The new default drops the leftover and writes the estimate alone.

**"No more blotchy painted walls and skin": two changes together** (their note at `resource.rs:310-319` and
`lower.rs:1129-1133`: fixed "with it and the NaN guard", run cf-col1):
- R9G9B9E5 targets as R11G11B10_FLOAT (`resource.rs:270, :320`). Like the console format it has no alpha (reads
  give 1) and stores a negative value as 0. The old stand-in, R16G16B16A16_FLOAT, keeps negatives and whatever a
  pass wrote to alpha.
- The NaN guard (`lower.rs:4008`), on colour exports (`:12502`) and typed stores (`:11075`, `:12054`, `:12092`).

**Could these explain our purple bushes and the purple patches that stick for seconds?** Three candidates, in the
order I would test them. All three are inferences from the code; none is confirmed by a run of ours.

1. **Ray tracing parked, no stand-in: stale probe memory.** We run with rays off and without `VICTORY_AMBIENT`, so
   the resolve output is exactly what the base comment describes: sky light outdoors with nothing blocking it
   (vegetation that glows) and, in tile blocks, whatever the probes' memory held (coloured patches; the resolve is
   temporal, so they would persist for a while). The drop's authors saw magenta from this same leftover. The drop
   offers no fix for this that is not the stand-in. Test: compare the same spot in a `VICTORY_BVH_BUILDER=1` run
   (run-166 exists). If the purple is gone there, it belongs to the parked ray tracing, not to a bug to hunt.
2. **Negative values kept by the R16G16B16A16 stand-in.** Magenta is what a negative or missing green channel looks
   like. A pass that exports a slightly negative channel to an R9G9B9E5 target gets 0 on the console and keeps the
   negative value with our stand-in; if that target feeds a history, the error lasts for seconds. Our NaN guard
   does not touch negatives. `VICTORY_E5_TARGETS=r11g11b10` tests this directly. If it fixes the patches but
   banding appears, the alternative that loses nothing is to keep the 16-bit stand-in and clamp negatives to 0 on
   exports to such targets (my suggestion; not in the drop).
3. **NaN or infinity written by compute stores.** Ours guards colour exports only. `VICTORY_NAN_GUARD_STORES=1`
   exists in our tree for this test, with the known cost that integer bit patterns in image stores can be damaged.

**FSR DLL (`VICTORY_FSR_DLL`): how it hooks in, and the picture.** The title upscales through AMD's FidelityFX API
from console DLLs that normally run as translated title code. With the variable set, the kernel module answers the
title's `LoadLibrary` of those DLLs with an HLE module (`kernel.rs:313`, `mod.rs:69`) whose five functions forward
to the PC DLL (`fidelityfx.rs:179`). For each dispatch it swaps the title's device, command list and textures for
the host's, patches formats and states in the description for the call and restores them after (`fidelityfx.rs:720,
:788`); the PC DLL records straight onto the host list between two unordered-access barriers
(`external.rs:401-415`), and the title's heaps, root signatures and pipeline are re-bound afterwards
(`draw.rs:1311`). If the PC DLL refuses a frame, a pass of theirs bilinearly scales the input into the output and
tells the title the upscale succeeded (`fidelityfx.rs:869`, `external.rs:200`).

Does it change the picture? The module's own header says the title's provider is "FSR 4 INT8 4.1.1" and the PC DLL
they read is upscaler 4.1.0, so it is a different build of the upscaler at the least; the launcher's "same picture"
is their observation, not something the code guarantees. Which algorithm a PC FidelityFX DLL selects on an NVIDIA
card I could not verify from this source. The bilinear fallback is a plain picture loss on any frame it fires. By
our rules: skip. A useful by-product is the knowledge that the title's own upscaler is a translated compute pass;
its cost on our card is worth reading from our pass times before deciding anything.

### (c) Submitting thread cost, refreshes, write-watch, flights/landing, card copies, between-pass gaps

**Nothing in the drop changes any of these.** No hunk touches the refresh path, write-watch asking, flights or
landing, relay copies, or the ordering of passes. Files such as `shadow.rs` and `list.rs` are touched only for the
window share (#9) and the FSR hooks (#11). Their README's known problems say the same: "one thread that hands every
frame to the graphics card is the limit".

Changes that touch the frame's cost indirectly:

- **#1 (held levels)** is the only one with a plausible effect on our measured costs: smaller uploads, fewer
  re-uploads of neighbours' bytes, and a card under budget. Whether part of our 15-20 ms of card time in gaps is
  the card working over its budget is unknown; the run will tell.
- **#6 (early textures 40 ms)** adds work on the draw-recording thread. For us that is a cost, not a gain.
- **#11 (FSR with `VICTORY_THIN`)** adds a wait of the title's thread for its list to be translated
  (`list/thin.rs:356`); irrelevant without THIN and without the DLL.
- **#7** removes a sleep, see next.

### (d) The frame interval: what throttles frames today

There are two throttles, and both follow the title's own setting of 30:

1. **The layer's frame clock** (`ours: victory-gpu/src/frame.rs:103-160`). The title calls `SetFrameIntervalX`
   with 33,333 us; the layer keeps absolute ticks at that interval and the title's frame wait **sleeps in the
   layer** until the next tick (`std::thread::sleep(wait)`). A frame that already ran past a tick does not sleep:
   the wait ends at once on the tick that passed (`ours: frame.rs:138-146`, unless `VICTORY_LATE_WAITS=1`). So the
   sleep only fills frames shorter than 33.3 ms and never rounds a 40 ms frame up to 66 ms.
2. **The title's own limiter** (their analysis, `frame_rate.rs` header): a wait on `QueryPerformanceCounter` at
   image offset +0x120530 until 33 ms have passed since the last frame, active once a flag is set.

So for the question "our frames under 1k draws sit at exactly 33 ms: the game's cap or the layer's?": **it is the
game's cap, carried out by a sleep inside the layer.** The layer adds no throttle of its own; it honours the
interval the title asks for. Our frame log already records that sleep as `PaceUs`, so the share can be read per
frame.

What the drop does: the layer's clock ticks at the asked interval regardless of the title (`frame.rs:84, :137`;
0 = no wait, `:140`), and three reads of the title's setting (+0x210 of its settings object) are patched at load:
two that derive the frame time (`mov eax, rate; nop`) and one that feeds the shortest simulation step
(`frame_rate.rs:113-143`). Without the third their menus ran at twice the speed at 60. The patch offsets are for
game_scarlett_master.exe 1.0.2.1 and are skipped if the bytes differ.

For us: no gain in any frame longer than 33 ms, which is every heavy scene. Light scenes would run faster, and
since the title paces streaming steps in frames, loading during light stretches could be quicker. Risks: three
byte patches in the title's code, timing of cutscenes and scripts at other rates (the title scales some per-frame
amounts by rate), and a new variable in every comparison against the floor. Not needed for "stable 30"; keep it
behind the switch.

### (e) What depends on Series S mode

In the layer, `--console series-s` changes three things and nothing else (`ours: victory-gpu/src/console.rs:18-34`,
`victory-hle/src/modules/xsystem.rs:164-168`; identical in the drop):

| | Series X (ours) | Series S (theirs) |
|---|---|---|
| Device type the title reads | 0x64 | 0x60 |
| Console name reported | Xbox Series X | Xbox Series S |
| Title memory reported (GPU-optimal, standard) | 10 GiB, 3.5 GiB | 0, 7,776 MiB |

Resolution, texture budgets, draw distances and effect quality are then chosen by the title from the device type.
The layer has no resolution or quality switch tied to the mode. So:

- Their frame rate (median ~30 fps, README) includes whatever the title cuts on Series S: lower render resolution
  and smaller budgets mean less card time and fewer, smaller textures. None of that carries over to Series X at
  1440p.
- Their memory figures (41 -> 27 GB) are for Series S budgets on a 24 GB card. The direction carries over (see
  (a)); the amounts do not.
- Their launcher also uses `VICTORY_AMBIENT`, `VICTORY_AMBIENT_AIR` and `VICTORY_THIN`, which we do not, and the
  frame cap lift only shows in scenes lighter than 33 ms.
- Mode-independent: the picture fixes (#2-#6), held levels (#1), the window share (#9), DXIL dropping (#10).

---

## 4. Ranked shortlist

1. **Run without `VICTORY_LEVELS_ON_DEMAND=1`** (table #1). No code. Change `run-auto.sh:14` and
   `run-auto-coolfox.sh` for the run, or pass the override. Re-run the floor the same way. Read: card in use against
   budget, "passes that made none for lack of room", "textures from the first level viewed" (copies, MB, replaced
   by one with more levels), pool holes, commit, frame medians, pop-in. Conflicts: none in code; every texture-side
   port of ours (pool classes, partial delta, bake delta, foxV page pool) needs its numbers taken again on this path.
   Also worth one counter at `resource.rs:1558-1580` to learn why nothing was ever tiled.
2. **`VICTORY_E5_TARGETS`** (table #2). Files: `crates/victory-gpu/src/resource.rs` (`target_format`,
   `e5_stand_in`, `e5_stand_in_for`, one test), comment in `list.rs` (`unlike`). Port with our default left at
   R16G16B16A16, test `r11g11b10` on the purple spots and look for banding. Conflict: our `resource.rs` differs
   from base by ~440 lines but not in `target_format`; clean hand merge. The shader cache is not affected; the
   pipeline library is (target formats change), so expect pipelines to be rebuilt in that run.
3. **Half-view table sizing fix** (table #5). Files: `crates/victory-gpu/src/draw.rs` (the `selected.extend` lines
   in `tables()`), test `a_fresh_table_holds_the_half_views_past_its_range_starts` in `descriptor.rs`. Conflict:
   our `tables()` has a different signature and key scheme (9b2debf); adapt the lines and the test call.
4. **Purple diagnosis runs, no port** (3(b)): the same spots with `VICTORY_BVH_BUILDER=1`, then with item 2, then
   with `VICTORY_NAN_GUARD_STORES=1`. This tells which of the three causes is real before any code is written.
5. **`VICTORY_WINDOW_CARD_PERCENT`** (table #9). File: `crates/victory-gpu/src/shadow.rs` (two call sites and three
   small functions with tests). Default unchanged. Conflict: our `shadow.rs` differs by ~415 lines; the two lines
   `budget / 10 * 8` are still there at 511 and 547. Test 90 only once item 1 has put the card under budget.
6. **`VICTORY_EARLY_TEXTURES_MS=40` as an environment test** (table #6) after item 1, for pop-in only. No port.
7. **Frame interval switch** (table #7). Files: `crates/victory-gpu/src/frame.rs`, new
   `crates/victory-loader/src/frame_rate.rs`, `crates/victory-loader/src/lib.rs`, `apps/victory/src/run.rs`.
   Off by default. Conflict: our `frame.rs` frame-log and hang-watch hooks sit in `wait`; foxM touches the loader.
   Low priority for the 30 fps goal.

Not on the list: FSR DLL, keyboard and mouse, DXIL dropping, ambient leftover, store-side NaN guard, timer log
(reasons in the table).

## What was not verified

- No run was made for this review. Every expected gain above is an estimate from code and from existing logs.
- The ~quarter figure for held levels comes from a comment in the base, measured by its author on two frames.
- Why no texture is made tiled on the fox base was not determined.
- The three purple explanations are hypotheses.
- Whether the shipped `victory.exe` matches this source was not checked (not extracted).
- The FSR module (1,356 lines) and the input modules were read for behaviour and safety, not line by line for
  correctness.

---

## Switch audit (follow-up, 2026-11-26)

Question: which switches of our run set differ from their launcher, and does any of them cost us speed the way
`VICTORY_LEVELS_ON_DEMAND=1` did. Read-only again; nothing built or run. Code read: `C:\GTAVI\Tools\par\foxW`
(branch local/w-shader, HEAD 70082e0); paths below are relative to `par\foxW\crates\victory-gpu\src` unless
stated. Our set: `run-auto-coolfox.sh:13-17` plus the standing extras, checked against the real dump
`victory-data\bench\run-194-fox-fill-from-placed-verify\env.txt`. Their set: `outer\Play-cool_fox.cmd`.
Costs marked "estimate" are from reading the code, not from a measurement.

### Result in short

- **Eight names in our set are not read anywhere in the tree we run.** They do nothing.
- **Nothing else in the built-in list bypasses a faster default the way LEVELS_ON_DEMAND did.** The differences
  that can cost us are in the diagnostics build and its always-on lines, not in the performance switches.
- **`VICTORY_THIN` is not a picture setting.** It is the base's answer to recording cost, and we run without it.
  It belongs in a separate A/B run with a picture check, not in the "skip" group.
- `VICTORY_JOIN_PARTS` appears twice in our set (1, then 8). The bench script exports in order
  (`bench-victory.sh:12`), so 8 wins. Same as theirs.

### Names that are dead weight in the tree we run (NO-OP)

| Name in our set | Evidence |
|---|---|
| `VICTORY_BACK_OWN_VIEW=1` | no match in `par\foxW\crates` or `apps` |
| `VICTORY_MIRROR_FAST=1` | no match |
| `VICTORY_QUEUE2=1` | no match |
| `VICTORY_COPY_PLAIN=1` | no match |
| `VICTORY_INDIRECT_GATHER=1` | no match |
| `VICTORY_GATHER_DISPATCH=0` | no match |
| `VICTORY_CARD_FILLS_BATCH=0` | never read; the name only appears inside a log text (`framelog.rs:249`) |
| `VICTORY_POOL_CAP=1` | not in foxW, victory-fox, foxM or foxT; only `par\foxV\...\pool.rs` reads it. It was noted as "adopted into the standing set", but in the build we run it does nothing. |

Remove them from the script; they cannot change a run, and they hide what the set really is.

### Same value on both sides (no difference to audit)

`JOIN_COPIES=1`, `CARD_REHASH_RUNS=1`, `INDIRECT_WINDOW=1`, `TEXTURE_NO_WAIT=1`, `RELAY=2`, `WIDE_BACK=1`,
`RELAY_COPY=1`, `CARD_STEPS=2`, `INDIRECT_BATCH=2`, `MAP_THREADS=1`, `SHOW_AFTER=1`, `JOIN_PARTS=8`,
`UPLOAD_AHEAD=1`, `CARD_BUFFERS_MIN_KB=64`. `WIDE_TEXTURES=1` equals the default (`host.rs:8217`: on unless `0`).
`ASYNC_SUBMIT=1`: they do not set it, but their `VICTORY_THIN=1` turns the same line of submissions on
(`queue/behind.rs:662`), so both sides have it.

### Switches that differ

| Switch | Ours | Theirs | What it does in foxW (file:line) | Verdict |
|---|---|---|---|---|
| `VICTORY_THIN` | unset | 1 | List calls only write down their arguments; threads of ours translate them beside the title (`list/thin.rs:1-28`, `:58`). Its header: translation inside the call was 160-190 ms of processor time a frame on five recording threads. Our run-194 logs 110-138 ms a frame of list calls on the recording threads. | **COSTS US (probably the largest single difference).** Not a picture switch by design. Our fork notes record defects with it in other forks (blocky patches on the player, dynamic slots run out on NVIDIA, bones a frame old without `PASSES_AHEAD`). Whether those hold on this base is unknown. Separate A/B with a picture check. |
| build + `--features diagnostics` | diag profile, feature on | release, feature off | Every title list call goes through `list::with_state` (`list.rs:153-168`), which with the feature takes two clock reads, then `copystep::list_call` (`copystep.rs:395-419`: two shared counters, a `try_lock` on one shared mutex, a third clock read) and `copystat::recorded` (`copystat.rs:1122-1138`: more shared counters). 30,000-43,000 calls a frame on five threads. The diag profile itself is release code generation (`Cargo.toml` `[profile.diag]`). | **COSTS US.** Estimate 0.3-0.5 us a call, 10-20 ms of recording thread time a frame, against 3.2 us a call measured in total. Also 91 diagnostics blocks in `draw.rs`, 226 in `host.rs`. `VICTORY_FRAME_LOG=1` works without the feature (`framelog.rs:367-372`), so a plain release build still gives frame numbers. |
| `VICTORY_MEMORY_MAP` | unset = 6 | not in their build | Diagnostics builds only: every sixth memory line (30 s) the whole address space is walked with `VirtualQuery` (`copystat.rs:528-540`, `:578`, `:786-792`). Its own comment: about 250,000 regions, 0.9 s, and other threads' write-watch askings and the title's faults wait 11-17 ms behind it. Run-194 shows the walk ran. | **COSTS US.** A stall source every 30 s on exactly the thread that bounds us. Set `VICTORY_MEMORY_MAP=0`. |
| `VICTORY_METER_THREAD` | unset = off | unset, but warn level and no diagnostics lines | The forty-odd lines of every 5 s are made on the presenting thread, "about 30 ms ... once every 5 s"; `=1` moves them to a thread of ours (`host.rs:9107-9115`, `:9274-9280`). | **COSTS US.** One long frame every 5 s. Set `VICTORY_METER_THREAD=1`. |
| `VICTORY_LOG` | unset = info | warn | 22,577 lines, 7 MB in run-194, written to stderr unbuffered (`victory-core/src/logging.rs:20-34`). About 60 lines a second, most from the 5-second block above. | Small by itself once the meter thread is on. Keep info while the tools need the lines. |
| `VICTORY_CARD_METER=1` | 1 | unset | Also turns the card stamps on (`host/spans.rs:68-73`): two extra command lists, each a timestamp and a `ResolveQueryData`, go around every submission (`host/spans.rs:1-8`, `:80-112`). Roughly 23-40 submissions a frame, so 46-80 extra lists a frame through the queue. | **COSTS US (small, unmeasured).** Needed for the card column of the frame log. Worth one run without it to learn its price. |
| `VICTORY_FRAME_LOG=1` | 1 | unset | Per-frame counters and timers (`framelog.rs:349-372`). Ten call sites in `draw.rs` and `list.rs`; a clock read per timed span. On by default in diagnostics builds anyway. | HARMLESS (small); it is our measuring tool. |
| `VICTORY_LATE_WAIT_LISTS` | 1024 | default 256 | The most host lists in the air before the flights are settled; until then their dynamic descriptor slots, upload memory and scratch chunks are held (`host.rs:2690-2709`). The comment tunes 256 for this base: 128 stopped the submitting thread twice a frame. | **Unknown direction, TEST.** Four times the base's value means four times as long a hold on upload memory and slots (commit), in exchange for fewer settles. Run-194 shows 0 tables refused, so 1024 is not failing; whether it is better than 256 here was never measured on this base. |
| `VICTORY_PIPELINE_WAIT_MS` | 20 | default 500 | Waiting allowed per frame, over all threads, for a host pipeline that is still being made (`pipeline/make.rs:268-279`, `:405-440`). When the budget is used up the draw goes without its pipeline for that frame. | HELPS long frames, costs picture for a moment: an object can be missing for some frames while its pipeline is made. With a warm library both sides log 0.0 ms waiting (run-194). Under the no-picture-loss rule 500 is the correct value for picture comparisons. |
| `VICTORY_CARD_AWAKE` | 0 | 1 | `=1` runs a dummy compute loop on the card for 250 ms after each submission to keep it clocked up (`host/awake.rs:54-66`, `:96-104`). | HELPS (ours): already measured and adopted, no loss. |
| `VICTORY_REFRESH_ONCE=1` | 1 | unset | A copy's range refreshed once in a submission is not asked again until a step of ours writes there (`host.rs:7935-7958`). Our port C2. | HELPS (our port; its comment gives the counts). |
| `VICTORY_WIDE_MERGE=1` | 1 | unset | Shader-written pieces merged into title memory on several threads (`host.rs:2244-2258`, used at `:5268`). Its comment: "Not with relay, whose landings do not come here". We run `RELAY=2`. | HARMLESS, likely unused on our path. |
| `POOL_DISCARD`, `FILL_TITLE`, `PARTIAL_DELTA`, `BAKE_DELTA`, `MAP_LOCK_FINE`, `GPU_ONLY`, `KNOWN_WRITES` | 1 | unset (not in their tree) | Our ports (`victory-hle\src\modules\xmem.rs:2390`, `host.rs:2904`, `texture/delta.rs:37`, `texture.rs:709`, `watch.rs:2880`, `pagekinds.rs:22`, `knownwrites.rs:81`). | HELPS / measured by us; not audited again here. |
| `VICTORY_FRAME_INTERVAL_MS` | unset | 16.667 | Not in our tree. | See section 3(d): nothing above 33 ms. |
| `VICTORY_AMBIENT`, `VICTORY_AMBIENT_AIR` | unset | 0.8, 1.5 | Stand-in light (`ambient.rs:97`, `:110`). | PICTURE: skip. |
| `--console` | Series X | series-s | Section 3(e). | Not a switch to copy. |

### Defaults they rely on that we change without setting anything

Our ports added switches that are on unless set to `0`. Found by diffing foxW against `coolfox-base`:
`HALF_VIEWS`, `NAN_GUARD`, `D16_BITS`, `D16_VIEW_KINDS`, `EARLY_DEPTH`, `PS_QUAD_READS`, `TARGET_VARIANTS`,
`SECTION_WINDOW`, `TEXTURE_SWEEP`, `TEXTURE_RAM_CAPS`, `HOUSEKEEPING_LOG`, `SPILL_STRICT`, `WAVE64_REFUSE`,
`WAVE_LDS_LOOSE`, `WAVE_PAIRS`, `RT_RESOLVE`, `WINDOW_ICON`. Also changed in place: `MEMORY_MAP` (was on/off, now a
count), `CARD_SPANS` (now also turned on by `CARD_METER`), `SHADER_OPT`.

Of these, the ones with a possible frame cost:
- `VICTORY_TARGET_VARIANTS` (`pipeline/make.rs:311-322`): a draw into views of other formats than its pipeline
  declares gets a second host pipeline. More pipelines to make; picture fix. Keep.
- `VICTORY_EARLY_DEPTH` (`victory-shader\src\emit\lower.rs:1283`): declares early depth where the console does. Should
  only save card time.
- `VICTORY_HOUSEKEEPING_LOG` (`resource.rs:2850-2866`): one more status line; cheap.
- `VICTORY_LOCK_PROBE` is on by default but only acts with `VICTORY_LOCK_METER=1` (`lockmeter.rs:48-50`, `:966`),
  which we do not set. No cost.
- The translator switches change shader code and the cache key; they were chosen for picture. Not a speed item.

`VICTORY_FILL_FROM_PLACED=verify` in run-194's dump is a verify mode of that one run, not part of the set.

### The hot-path question: execute_indirect and State::bind

`execute_indirect` (`list.rs:1942`) and every other list call enter through `list::with_state`
(`list.rs:153-168`). In a diagnostics build that wrapper is where the per-call cost sits: clock reads before and
after, a caller-location lookup, shared counters written from all five recording threads (the same cache lines),
and a `try_lock` on one process-wide mutex per call (`copystep.rs:410-411`). W10's finer timers inside
`execute_indirect` and `State::bind` (commit 4c8a472, marked "not built yet") would add seven and three more clock
reads per call if built. So yes: diagnostics counters, timers and a lock attempt sit on that path, per call.
A measurement of 16 us for `execute_indirect` taken with these meters includes them. The honest way to learn the
real figure is one run of the same commit built without the feature.

### Proposed test set for one 3-minute run

Against the current set (with `LEVELS_ON_DEMAND=0` already in). Ordered by expected gain. Items 1-3 change nothing
in the picture.

Add:
```
VICTORY_METER_THREAD=1
VICTORY_MEMORY_MAP=0
VICTORY_LATE_WAIT_LISTS=256
```
Remove (dead in this tree):
```
VICTORY_BACK_OWN_VIEW VICTORY_MIRROR_FAST VICTORY_QUEUE2 VICTORY_COPY_PLAIN VICTORY_INDIRECT_GATHER
VICTORY_GATHER_DISPATCH VICTORY_CARD_FILLS_BATCH VICTORY_POOL_CAP VICTORY_JOIN_PARTS=1 (the first of the two)
```
Keep for this run: `VICTORY_FRAME_LOG=1`, `VICTORY_CARD_METER=1`, the diag build (so the numbers stay comparable
with run-191).

Expected: fewer long frames (the 5-second and 30-second stalls go), a small gain in the median. The
`LATE_WAIT_LISTS` line is the uncertain one; if the run is worse, put 1024 back first.

Then, each as its own run because each needs its own comparison:
1. The same commit built without `--features diagnostics` (release), `VICTORY_FRAME_LOG=1` kept. Shows what the
   meters cost on the recording threads. Estimate 10-20 ms of thread time a frame.
2. `VICTORY_THIN=1` with a picture check on characters and hair. The largest possible gain and the largest risk.
3. `VICTORY_CARD_METER` removed, to price the stamps.
4. `VICTORY_PIPELINE_WAIT_MS` removed (500) for any run whose picture is compared.
