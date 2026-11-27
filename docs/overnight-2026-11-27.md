# Overnight report — 2026-11-27

## TL;DR

1. **One-home as implemented is not a win in this core.** Homes require the relay
   to be off (the reference memory core's own rule), and the relay is worth ~2×
   here (18.5 fps baseline vs 3.3-7.8 fps with homes+relay-off). Keep `-OneHome`
   experimental.
2. **The texture shimmer is explained and fixed.** Our `INPLACE` placed windows
   over texture memory without invalidating the placed textures after card
   writes. Two fixes: texture-safe placement by default, and the reference's
   after-wait rule (placed home writes feed `page_written`, so placed textures
   over them are re-uploaded two frames later).
3. **The "late copies" progress note describes this core already** (relay
   landings after submission, `late_writes()` on by default, read-settles
   0.0 ms/frame). The gap is direct memory, not more late copies.
4. **Ported tonight**: texture-safe INPLACE, `VICTORY_INPLACE_RELAY` guard,
   after-wait invalidation, `VICTORY_HOST_EARLY` (host GPU in 384 ms before
   title code), `-Keys` for unattended runs, `-OneHome` as a real direct-memory
   mode, `-OneHomeTextures`, plus a robust A/B harness.

## Numbers (Series S, RX 9060 XT / 5700X3D)

| Config | median ms | fps | draws | note |
|---|---|---|---|---|
| baseline, relay on, clean state | 54.1 | 18.5 | 5,637 | smoke run |
| baseline after ~25 min of continuous runs | 665 | 1.5 | 5,225 | GPU throttling (86 ms on GPU) |
| `-OneHome buffers` (relay off) | 128-132 | 7.6-7.8 | 5.1-5.3k | 75 windows / 356 MB placed, 0 errors |
| `-OneHome small` (relay off) | 212-303 | 3.3-4.7 | 5.0-5.4k | up to 195 ms preparing on one thread |

The first 6-run batch (`ab2`) was thermally confounded; the cooled re-run
(`ab3`) was cancelled after ~1 min because **your own frame-gen harness**
(`fg-low720c` → `play-royale-low.ps1`, using `A:\rust\target\diag\victory.exe`)
started cycling on the GPU at 10:01/10:09. I yielded the machine to it rather
than corrupt both sets of numbers. Its latest runs are at
`A:\victory-royale-data\logs`.

## What changed in the repo (branch `overnight`, pushed)

- `crates/victory-gpu/src/inplace.rs`: texture-safe placement; relay/late-wait
  guard (`VICTORY_INPLACE_RELAY=1` overrides); new census counters.
- `crates/victory-gpu/src/shadow.rs`: placed home writes → `page_written`
  (after-wait invalidation).
- `crates/victory-gpu/src/host.rs`, `victory-hle/src/modules/d3d12x.rs`,
  `apps/victory/src/run.rs`: `VICTORY_HOST_EARLY`, timed and logged.
- `scripts/play-amalgam.ps1`: `-Keys`, `-OneHome` = direct-memory mode
  (`VICTORY_RELAY=0`), `-OneHomeTextures`.

Builds clean (`cargo build --profile diag -p victory --features diagnostics`).

## Recommendation

1. Keep the relay on; it is the single biggest lever on this hardware.
2. Leave `-OneHome` experimental until R5-style landings or the reference memory
   core is ported (plan in `AMALGAM.md`).
3. The remaining CPU win is the Avatarmanz-lineage port patches
   (DESCRIPTORS/GUARD/CBV/JOURNAL), which your Victory-Royale preset already has.
   Porting those into this core is the next meaningful step.
