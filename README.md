# foxhound + scarlett (merged)

A fork of [VIctory](https://github.com/Marvisak/VIctory), a compatibility layer
that runs an Xbox Series title natively on Windows from files **you** supply.
This tree merges two forks:

- **foxhound** ([readmet3xt-lab/foxhound](https://github.com/readmet3xt-lab/foxhound)):
  NVIDIA RTX tuning — DLSS Super Resolution and DLSS Frame Generation in place
  of the title's own upscaling, the Series X profile at a native 2560x1440,
  and fewer stalls at scene cuts.
- **scarlett** ([Op-lite/victory-scarlett](https://github.com/Op-lite/victory-scarlett),
  branch `scarlett-fox`): memory emulation, streaming, submit-path and picture
  fixes — texture re-uploads near zero, fewer freezes at scene loads, table
  and list bookkeeping savings, violet-vegetation fix, and the measurement
  tools that found them.

> **This repository contains no game files, code or assets.** You need your
> own legally obtained copy of the title. It is not affiliated with or
> endorsed by the title's developer or publisher, nor by NVIDIA.
> See [docs/asset-policy.md](docs/asset-policy.md).

## How the merge was made

Both forks descend from the same November 2026 ancestors (they share the
`cool_fox` lineage) but reorganised the GPU crate independently, so most of
the tree merged file-for-file and the rest was integrated by hand:

- **foxhound side kept:** DLSS / Frame Generation (`VICTORY_DLSS`,
  `VICTORY_FRAMEGEN`), the 1440p image patches (`VICTORY_NATIVE_1440P`),
  the ambient / reflection stand-ins (`VICTORY_AMBIENT`,
  `VICTORY_AMBIENT_SPECULAR`), early depth, soft views
  (`VICTORY_SOFT_VIEWS=copies`), levels-on-demand off
  (`VICTORY_LEVELS_ON_DEMAND=0`), the scripted auto-input system, the
  `asked_for!` switch framework with `VICTORY_AB` diagnostics.
- **scarlett side kept:** the shader pre-compile launcher
  (`Play-Scarlett.cmd` / `scripts/play-scarlett.ps1`), banded detiling
  (`Layout::detile_rows`), the NaN-origin listing (`VICTORY_NAN_ORIGIN`),
  the stack symbolizer, and its full `tools/` measurement suite and docs.
- **scarlett hook/diagnostics modules carried in the tree but not yet wired**
  (they target scarlett's private internals and were not safe by scarlett's
  own measure): `shell`, `loadmeter`, `hangwatch`, `lockmeter`,
  `knownwrites`, `pagekinds`, `hlehooks`, `chunks`, `copystep`, and the hle
  side `readback`, `chunkmeter`, `lockhook`, `callhook`,
  `modules/{mapdefer,pagekinds}`.
- **scarlett's per-switch GPU optimisations still to port** (they sit in
  files both forks restructured independently): `VICTORY_TABLE_MEMO`,
  `VICTORY_LIST_ALIAS_EXACT`, `VICTORY_LIST_HOLD`,
  `VICTORY_TABLE_NAMED_ONLY`, `VICTORY_PROBE_RAYS`, `VICTORY_PLANE_NOW`,
  `VICTORY_SAMPLERS_MADE`, `VICTORY_TARGET_VARIANTS`, `VICTORY_HALF_VIEWS`,
  `VICTORY_D16_BITS`, `VICTORY_NAN_GUARD`, `VICTORY_POOL_DISCARD`,
  `VICTORY_MAP_LOCK_FINE`, `VICTORY_POOL_CAP`. Each is a focused patch
  against the corresponding scarlett commit; `tools/docs/fox-port-*.md`
  lists them.

Both launchers are present:

- `scripts\play-foxhound.ps1` — the NVIDIA/DLSS path (see below).
- `Play-Scarlett.cmd` / `scripts\play-scarlett.ps1` — drag your game folder
  onto it; it prepares shaders ahead of the first start.

## Build

```powershell
cargo build --release --locked -p victory
```

Diagnostics build: `cargo build --locked --profile diag -p victory --features diagnostics`.

You need Windows 10/11 x64, a CPU with AVX2, 32 GB of RAM or more, an SSD,
Rust (toolchain pinned in `rust-toolchain.toml`), the Visual Studio Build
Tools with "Desktop development with C++", and your own dump of the title's
Xbox Series build **v1.0.2.1** (the folder with `game_scarlett_master.exe`).

## Set up (foxhound / DLSS path)

1. Copy `victory.example.toml` to `victory.toml` and set `game_dir`.
2. DLSS (optional, recommended on RTX): download NVIDIA's Streamline SDK
   (tested `streamline-sdk-v2.14.16.zip`) from
   <https://github.com/NVIDIA-RTX/Streamline/releases>, unpack it **outside**
   this repository and point the layer at its `bin\x64` folder:

   ```powershell
   [Environment]::SetEnvironmentVariable('VICTORY_STREAMLINE_DIR', 'D:\sdk\streamline-sdk-v2.14.16\bin\x64', 'User')
   ```

   Check it without the title: `target\release\victory.exe upscaler-check --device`.
3. Translate the shaders ahead (once per new build):
   `$env:VICTORY_HOST_WAVE = '32'; target\release\victory.exe shaders`

## Run

```powershell
.\scripts\play-foxhound.ps1 -Exe 'D:\Games\MyDump\game_scarlett_master.exe'
```

| Option | What |
|---|---|
| *(none)* | Series X profile, 2560x1440, DLSS Quality (if Streamline is set up) |
| `-Dlss off` / `dlaa` / `balanced` / `performance` | the title's own upscaling, or another DLSS mode |
| `-FrameGen` | DLSS Frame Generation |
| `-SeriesS` | Series S profile: 1280x720 inside, faster, softer |
| `-Res 1080to1440` / `4k` | scene at 1080p upscaled by the title / stock 3840x2160 output |
| `-Build diag` | diagnostics build |
| `-More @{VICTORY_X='1'}` | add or override any switch |

Scarlett-style run (shader pre-compile included): drag the game folder onto
`Play-Scarlett.cmd`, or use `.\scripts\play-scarlett.ps1` (`-RayTracing`,
`-Hooks`, `-OnlyPrecompile`).

## Switches worth knowing

Every `VICTORY_...` switch is described in a comment where the code reads it.

| Switch | Launcher | What |
|---|---|---|
| `VICTORY_DLSS`, `VICTORY_DLSS_MODE` | `quality` | DLSS Super Resolution through the title's FidelityFX dispatch |
| `VICTORY_FRAMEGEN` | off (`-FrameGen`) | DLSS Frame Generation, picture before the HUD tagged |
| `VICTORY_NATIVE_1440P` | on | Series X renders and outputs 2560x1440 (image patched) |
| `VICTORY_SOFT_VIEWS` | `copies` | tiled-heap page mappings kept without Windows views: no freezes at scene cuts |
| `VICTORY_AMBIENT`, `VICTORY_AMBIENT_SPECULAR` | `0.8`, `1` | estimates of indirect light and reflections (no ray tracing) |
| `VICTORY_EARLY_DEPTH` | on | pixel programs keep the console's early depth test |
| `VICTORY_PRELUDE_UPLOADS`, `VICTORY_SHOW_NEW_LATER`, `VICTORY_TABLED_REREAD` | on | texture upload order and slot changes (flicker, device losses) |
| `VICTORY_SYNC_IN_PLACE`, `VICTORY_SPANS_QUICK`, `VICTORY_HEAP_SYNC_AT_SUBMIT` | on | cheaper descriptor-heap bookkeeping |
| `VICTORY_PORT_CONSTANTS` | off | the title's constants heap read in place (slower on 12 GB cards) |
| `VICTORY_FRAME_INTERVAL_MS` | off | experimental: the title's own 30 fps setting patched to a higher rate |
| `VICTORY_BVH_BUILDER` | off | the title's ray tracing (slow, broken) |
| `VICTORY_AUTO_INPUT` | — | scripted pad presses (`A, w3, Downx3`, `rep(A,1.5,d>=20)`, ...) for unattended starts |
| `VICTORY_GPU_ONLY` | off (`-Hooks`) | page-kinds hook: not safe yet, a few textures can draw stale |
| `VICTORY_KNOWN_WRITES` | off | lock-hook ledger: retired, kept in the source |
| `VICTORY_LOCK_METER`, `VICTORY_READBACK_*` | off | diagnostics hooks (title-code) |
| `VICTORY_NAN_ORIGIN` | off | lists the shaders that wrote NaNs |

## How it works, and the findings

- [docs/architecture.md](docs/architecture.md): the layer's parts.
- [docs/findings.md](docs/findings.md): measurements behind the choices.
- [docs/dlss.md](docs/dlss.md): DLSS and Frame Generation.
- [docs/fork-ray-tracing.md](docs/fork-ray-tracing.md): the title's ray tracing and the light estimate.
- [docs/replay.md](docs/replay.md): recording a frame and replaying it.
- [docs/asset-policy.md](docs/asset-policy.md): what may and may not be in this repository.
- `tools/docs/*` (from scarlett): the measurement notes, freeze analysis,
  picture-settings checklist and fork reviews.

## Credits

A tree of other people's work with more on top; the history is kept, and
commits taken from elsewhere name the commit and its author.

- **[VIctory](https://github.com/Marvisak/VIctory)** by Marvisak and its contributors.
- **[fxceless/victory-fork](https://github.com/fxceless/victory-fork)**, **[avatarmanz/VIctory-7900XTX](https://github.com/avatarmanz/VIctory-7900XTX)**, **[codec-d/cool_fox](https://github.com/codec-d/cool_fox)**.
- **[DexNom/VIctory](https://github.com/DexNom/VIctory)**, **[Belobug/VIctory-Belobug-AMD](https://github.com/Belobug/VIctory-Belobug-AMD)**.
- **[FQY55kai/ZJOU-VICTORY](https://github.com/FQY55kai/ZJOU-VICTORY)**, **[myokex/victory-amd](https://github.com/myokex/victory-amd)** (with Spexivus, joroyt and tightfall), Joshua Odom, **Geckomacabre**, **cxy0072019-sketch**.
- **[readmet3xt-lab/foxhound](https://github.com/readmet3xt-lab/foxhound)** and **[Op-lite/victory-scarlett](https://github.com/Op-lite/victory-scarlett)**, whose trees this merges.
- **[XCL](https://github.com/ivall/xcl)** (GPL-2.0-only) was read for how the same title behaves under another layer; no code of it is here.
- NVIDIA Streamline, DLSS and Reflex are NVIDIA's; they are loaded at run time from a folder you supply.

## License

GPL-3.0-or-later, as VIctory. See [LICENSE](LICENSE); the one file that is not
covered is named in [docs/asset-policy.md](docs/asset-policy.md).
