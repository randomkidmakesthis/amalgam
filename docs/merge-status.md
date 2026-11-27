# Merge status and port plan

State of the merged tree (`foxhound` × `victory-scarlett`, branch `scarlett-fox`),
2026-11-26. The tree compiles (`cargo check --workspace` and
`cargo check -p victory --profile diag --features diagnostics` both pass) and
contains both forks' full history.

## Wired and working

- **foxhound core** (taken wholesale for the GPU and shader crates, where both
  forks restructured the same files independently): DLSS / Frame Generation,
  1440p image patches, ambient / reflection stand-ins, early depth, soft views
  (`VICTORY_SOFT_VIEWS=copies`), levels-on-demand off, the scripted
  auto-input system, the `asked_for!` switch framework.
- **Both forks' shared lineage** (already present on both sides before the
  merge): `VICTORY_LEVELS_ON_DEMAND=0`, `VICTORY_SOFT_VIEWS=copies`,
  `VICTORY_QUEUE_MARKS`, `VICTORY_BAKE_TEXTURES_MS`, `VICTORY_PIPELINE_WAIT_MS`,
  `VICTORY_FILL_TITLE`, `VICTORY_PARTIAL_DELTA`, `VICTORY_BAKE_DELTA`,
  `VICTORY_CARD_*`, `VICTORY_RELAY*`, `VICTORY_INDIRECT_*`, thin lists, and
  the rest of the cool_fox launcher set.
- **Static-sampler fix**: foxhound already implements scarlett's
  `VICTORY_SAMPLERS_MADE` intent on by default (`pipeline/library.rs`,
  `with_static_samplers`): static-sampler pipelines are made every run, never
  loaded from the pipeline library. Nothing to port.
- **scarlett side kept**: `Play-Scarlett.cmd` / `scripts/play-scarlett.ps1`
  (unknown switches are simply ignored by the core; the ones above work),
  banded detiling (`Layout::detile_rows`), the `VICTORY_NAN_ORIGIN` listing
  (`victory_shader::emit::code_tag`), the stack symbolizer, `tools/` and
  `tools/docs/`.

## Carried but not wired (scarlett hook/diagnostics modules)

These files are in the tree but not compiled in; they were written against
scarlett's private internals and were themselves off by default / "not safe
yet" in scarlett:

- gpu: `chunks.rs`, `copystep.rs`, `hangwatch.rs`, `hlehooks.rs`,
  `knownwrites.rs`, `loadmeter.rs`, `lockmeter.rs`, `pagekinds.rs`,
  `shell.rs`
- hle: `readback.rs`, `chunkmeter.rs`, `lockhook.rs`,
  `modules/mapdefer.rs`, `modules/pagekinds.rs`
- loader: `callhook.rs`

Wiring them needs these scarlett-internal APIs exposed/adapted on the
foxhound core: `watch::{NotesTimed, asking_now, guarded_block,
probe_protect, probe_unprotect, with_guards_locked, note_known, last_noted}`,
`resource::{placed_copy_in, placed_copies, rule_copies, rule_census,
texture_copy_over}`, `inplace::{key_of, shown_twice, other_views}`,
`shadow::copy_over`, `switch::Bits` (make public), `frame::current` (make
public). They are small, one-function-at-a-time ports; nothing here affects
the picture unless the matching `VICTORY_*` meter switch is on.

## Still to port (scarlett's per-switch optimisations)

Each switch below is written against scarlett's own restructured internals
and must be **rebuilt** on the foxhound core, not copied. Suggested order is
dependency order. `tools/docs/fox-port-*.md` documents each switch; the
commit named is where scarlett added it (use `git show <sha>` for the
original).

| # | Switch | scarlett commit | scarlett files | landing zone (ours) | value / risk |
|---|--------|-----------------|----------------|---------------------|--------------|
| 1 | `VICTORY_TABLE_MEMO` | `c45b43e`, `50bd954` | `draw.rs` | `draw.rs` bind path (`memory_ready`/`early_textures`); needs `EntryKey`/`Seen`/`EntryMemo` types + `entry_begin` equivalent | high CPU (81 % of ~54 k entries/frame), medium risk |
| 2 | `VICTORY_TABLE_NAMED_ONLY` | `bc264a4` | `draw.rs`, `shader_cache.rs`, `emit/mod.rs`, `valu_tests.rs` | `descriptor/host.rs` table building | ~27 k slots/frame |
| 3 | `VICTORY_LIST_ALIAS_EXACT` | `c45b43e` | `draw.rs` | `draw.rs`; needs `aliased_view`/`alias_of`/`placed_texture` helpers | picture fix (stale lighting-filter read) |
| 4 | `VICTORY_LIST_HOLD` | `50bd954` | `descriptor/host.rs` | `descriptor/host.rs` copy path | small CPU |
| 5 | `VICTORY_PLANE_NOW` | `50bd954`, `183ac80` | `draw.rs`, `host.rs` | `draw.rs`; depends on (3)'s infra + `copystep.rs` meters | picture fix (material-class depth plane) |
| 6 | `VICTORY_PROBE_RAYS` | `183ac80` | `emit/{mod,intersect,lower,shader}.rs`, `draw.rs`, `host.rs`, `pipeline/make.rs` | `emit/*` (probe activation program), `draw.rs` bind | picture fix (violet vegetation), highest visual value |
| 7 | `VICTORY_TARGET_VARIANTS` | `c45b43e` | `host.rs`, `pipeline/make.rs` | `pipeline/make.rs` | pipelines for bound RT formats |
| 8 | `VICTORY_HALF_VIEWS`, `VICTORY_D16_BITS` | `411a745` | `emit/lower.rs`, `emit/mod.rs`, `valu_tests.rs`, `draw.rs`, `host.rs` | `emit/lower.rs` export path | fixes frozen facial animation / blurred disc |
| 9 | `VICTORY_NAN_GUARD` | `411a745` | `emit/lower.rs` | `emit/lower.rs` colour-export path (ours already has NaN-correct comparisons; this adds export guarding) | NaN artefacts |
| 10 | `VICTORY_POOL_DISCARD`, `VICTORY_POOL_CAP`, `VICTORY_MAP_LOCK_FINE` | `411a745`, `c45b43e`, `7c8f019` | `pool.rs`, `xmem.rs`, `watch.rs` | `pool.rs` (ours is a different, smaller pool), `xmem.rs`, `watch.rs` | ~1-4 GB commit, fewer freezes |

## How to verify a port

scarlett's `tools/` contains the fixed 6-minute route and the comparison
harness (`run-auto-coolfox.sh`, `run-compare.sh`) used to validate every
switch against the cool_fox floor. A port is only done when:

- `cargo check -p victory` and the `diag` profile pass,
- the switch's own meter counters (each switch logs its line per frame)
  show the expected behaviour, and
- a run on the test route shows the documented gain with no stale-copy
  counters (`tools/docs/fox-merge-hooks.md` describes the verification
  composition for the hook switches).

## Not in scope

The title's game files are never committed (`cargo xtask policy` enforces
it). The hooks (`VICTORY_GPU_ONLY`, `VICTORY_KNOWN_WRITES`) stay off by
default even after wiring, per scarlett's own findings (4-11 stale small
textures per 6-minute run).
