# Amalgam

The `foxvic-scarlett` build (foxhound + scarlett merged, compiles and runs) with
the settings that measured best across four VIctory forks, and the
single-home-buffer experiment exposed as one switch.

> **This repository contains no game files, code or assets.** You need your own
> legally obtained copy of the title. Not affiliated with or endorsed by the
> title's developer or publisher.

## The four sources

| Source | What it brings |
|---|---|
| [foxhound](https://github.com/readmet3xt-lab/foxhound) | the base: DLSS/FrameGen, native 1440p, ambient/reflection stand-ins, `WC_NO_BACK`, soft views, levels-on-demand off |
| [victory-scarlett](https://github.com/Op-lite/victory-scarlett) | merge partner: freeze fixes (`SOFT_VIEWS=copies`), bake/pipeline waits, shader precompile, measurement tools |
| [Victory-Royale](https://github.com/space-de/Victory-Royale) (`VicRoy-fork`) | AMD RDNA4 tuning: 8 translators, card snapshots, port patches, Series S + PC FSR 4.1.2 (FSR profile not in this core) |
| [hle-d3d12-7900xtx](https://github.com/LibsMQG/hle-d3d12-7900xtx) | `SKIP_WC_BACK` (same idea as our `WC_NO_BACK`), `VICTORY_CCD` thread placement, `HOST_EARLY`, `ASK_EARLY` |
| [Dark-Victory](https://github.com/darksidexc/Dark-Victory-RX6600-I714700KF-VI) | Series S/8 GB-VRAM caps (`CARD_BUFFERS_*`), low-graphics preset |

Both new forks are a **different VIctory family** (avatarmanz/7900XTX lineage)
from this tree (cool_fox lineage) and their histories are squashed, so they
cannot be git-merged. What is shared was ported by switch; what needs their
internals is listed under *Still to port*.

## One home per buffer

The recommended fix — one real location per buffer instead of a RAM copy plus a
card copy synced every frame — is implemented here as three placements already
present in the core, switched on together:

| Switch | Placement |
|---|---|
| `VICTORY_INPLACE=1` | a window of title memory outside buffers (private committed memory) is opened as a host heap and the card reads it in place: **one home, in RAM, GPU reads it directly** |
| `VICTORY_INPLACE=2` | title buffers too, except the big ones in card memory |
| `VICTORY_WC_BAR=1` | write-combined title buffers get **one copy in card memory the CPU maps** (`D3D12_HEAP_TYPE_GPU_UPLOAD`, resizable BAR): no CPU side, no send list, no hashes, no compare |
| `VICTORY_WC_NO_BACK=1` (+ `_MIN_KB=1024`) | **big buffers live on the card**: shader writes into write-combined copies are not fetched back |

Anything without a home still does the full round trip, exactly as described.

```powershell
# the three placements, conservative (memory outside buffers only)
.\scripts\play-amalgam.ps1 -Exe <dump>\game_scarlett_master.exe -OneHome small -Log run.log

# buffer placements too (do not combine with VICTORY_LATE_WAIT)
.\scripts\play-amalgam.ps1 -Exe <dump>\game_scarlett_master.exe -OneHome buffers -Log run.log

# plus windows in BAR memory (foxhound measured it a wash: +1.4 GB VRAM, no gain)
.\scripts\play-amalgam.ps1 -Exe <dump>\game_scarlett_master.exe -OneHome both -Log run.log
```

Each placement is a documented experiment in the core with its own risks:
the title's CPU reading what shaders wrote in card memory reads older bytes
(accepted for write-combined memory, which the CPU writes and the GPU reads),
and in-place windows stop the title decommitting/protecting those pages (the
probe shows nothing of the kind in 6-minute runs).

Measure with `-Log` and any frame-summary tool: the core logs
`N fps, N ms/frame (… draws/frame, dropped …)` lines.

## Defaults and flags

`scripts\play-amalgam.ps1` = foxhound's tested set + the cross-fork additions:

- `VICTORY_ASK_EARLY=2` (hle), `VICTORY_MEMORY_MAP=0`, `VICTORY_CARD_BUFFERS_MIN_MB=1`
  (Victory-Royale), `VICTORY_WC_NO_BACK=1/_MIN_KB=1024` (foxhound/hle),
  `VICTORY_SOFT_VIEWS=copies`, `VICTORY_LEVELS_ON_DEMAND=0`.
- On 8+ logical CPUs: `THIN_THREADS=8`, `LAND/UNDER/VIEW_THREADS=4` (Victory-Royale's
  8-core tuning).
- `-JoinParts`, `-StageMs`, `-InPlaceMb` expose the tunables; `-Safe` drops the
  aggressive switches (first thing to try when something is wrong).
- `-SeriesS`, `-Res 1440|1080to1440|4k`, `-Dlss`, `-FrameGen`, `-Fps` as in
  `play-foxhound.ps1` (DLSS is NVIDIA-only; on AMD it runs without).
- `-Cache`/`-Data` or `victory.toml` decide where cache and saves live.

## Still to port from the other families

These need the avatarmanz-lineage internals and are not in this tree:

1. **`VICTORY_CCD=far|near`** (hle `cores.rs`): thread placement on processors
   with two core complexes. On the single-CCD Ryzen 7 5700X3D only `far`
   (off the title's cores) could help; the fork measured -3.3 %.
2. **`VICTORY_HOST_EARLY=0/1`** (hle `host.rs`, `d3d12x.rs`): make the host device
   at load rather than at the title's first command.
3. **Renderer port patches** (Victory-Royale / Dark / hle): `PORT_DESCRIPTORS`,
   `PORT_GUARD`, `PORT_CBV`, `PORT_BUFFERS` — the descriptor stores and guard
   waits of that lineage; measured 55 → 40.5 ms of submit there.
4. **`VICTORY_SKIP_WC_BACK`** as such: covered here by `WC_NO_BACK` (same idea,
   same 1 MiB gate); hle's reservation-map variant is not needed.
5. **PC FSR 4.1.2 Series S profile** (Victory-Royale): needs the FidelityFX
   SDK and the fork's upscaler interpose; not in this core.
6. **`VICTORY_CARD_BUFFERS_MB=768`, `CARD_SMALL_MB`, `WINDOW_MAX`, `FULLSCREEN`**
   (Dark-Victory's 8 GB preset): partially present (`CARD_BUFFERS_MB`), the rest
   are their launcher's; use `-More` for the ones this core reads.

## The reference single-home implementation (sennecools/victory-fork, `combined`)

The person who recommended the one-home fix ships it in
[sennecools/victory-fork `combined`](https://github.com/sennecools/victory-fork/tree/combined)
(fxceless lineage) as a **memory core**, not as placements:

| Switch | What it does |
|---|---|
| `VICTORY_MEMORY_CORE=1` | title buffers and windows in private title memory are read/written by the card in place (a heap opened on the title's pages, one buffer per home); `VICTORY_INPLACE` is ignored |
| `VICTORY_MEMORY_POOL=1` | title pages in 256 MiB chunks, each a section and a heap with one buffer over the whole chunk; placed textures over card-written pages are re-uploaded |
| `VICTORY_MEMORY_OWN=1` | the memory handed to the title is a write-watched region below 128 GiB the core can place |
| `VICTORY_MEMORY_DIRECTORY=1` | raw-address shaders (ray tracing) resolve pool pages |
| `VICTORY_MEMORY_RELEASE_AT_END` (default on) | homes and placements let go before the process ends (their run leaked 3.4 GB commit without it) |

Their design differs from our `INPLACE` in the two things that matter:

1. **Texture freshness.** A placed page the card writes makes the placed
   textures over it re-upload; our `INPLACE` had no such rule, which is why
   textures shimmied under `-OneHome`. Amalgam now refuses to place windows
   that a descriptor slot shows as texture memory (`VICTORY_INPLACE_TEXTURES=0`,
   the default) — the safe half of their rule without their tracking.
2. **Chunk-level homes + release at end.** Ours opens per-window heaps and
   releases them when idle (`let_go_idle`), not before exit.

Port plan if this is taken further: their `home.rs` (core policy) and
`card_home.rs` (GPU-side homes) are ~2 000 lines against fxceless internals;
adapting them means mapping `home.rs` onto our `shadow.rs`/`inplace.rs`
placement book and adding the placed-texture invalidation to
`descriptor/host.rs` (`placed_texture`) + `texture.rs` refresh. It is a
multi-day port, not a copy.


## Build

```powershell
cargo build --profile diag -p victory --features diagnostics
# or: cargo build --release -p victory
```

Windows 10/11 x64, AVX2 CPU, 32 GB RAM, any D3D12 GPU with shader model 6.6 and
mesh shaders (an RTX 40+ card for DLSS/FrameGen). Rust toolchain is pinned in
`rust-toolchain.toml`; Visual Studio Build Tools with a C++ workload are needed.

## Credits

Everything in this tree comes from VIctory and its forks; the histories are
kept and the merge commits name both parents. See the README's credits.
GPL-3.0-or-later.
