# cool_fox baseline (the floor) — kept 2026-11-26

User rule: cool_fox as it ran is the floor for speed AND picture; nothing we add may make either worse.

## What is kept here
- `exe\victory.coolfox-pure-f4b48ad.exe` — cool_fox built from its source at f4b48ad, untouched, no diagnostics
  (git tag `coolfox-base` in `Tools\victory-fox`; full history also in `Tools\patches\cool_fox-f4b48ad-full.bundle`).
- `exe\victory.fox-floor-diagnostics.exe` — the same base + our tooling and picture fixes, with diagnostics (runs 102/103).
- `runs\run-100-coolfox` — the pure base run the user liked (no diagnostics, so no frame log: caption figures only).
- `runs\run-103-fox-floor`, `runs\run-109-fox-floor2` — the floor with diagnostics, twice (same settings).

## Floor settings (run-auto standard set plus cool_fox's launcher set, no THIN, no AMBIENT)
`VICTORY_RELAY=2 VICTORY_WIDE_BACK=1 VICTORY_RELAY_COPY=1 VICTORY_CARD_STEPS=2 VICTORY_INDIRECT_BATCH=2 VICTORY_MAP_THREADS=1
VICTORY_CARD_AWAKE=1 VICTORY_SHOW_AFTER=1 VICTORY_JOIN_PARTS=8 VICTORY_UPLOAD_AHEAD=1 VICTORY_CARD_BUFFERS_MIN_KB=64`
Run with `Tools\run-auto-coolfox.sh <label> 360 <settings>` (own data/cache folder `victory-data\coolfox`).

## Floor figures (6-minute runs, `run-compare.sh`)
| | run-103 | run-109 |
|---|---|---|
| median ms, 1-2.5k draws | 50 | 55 |
| median ms, 2.5-4k draws | 50 | 53 |
| median ms, >4k draws | 59 | 66 |
| long frames | 7.7 % | 6.9 % |
| 5-s periods with a frame over 500 ms | 18 | 21 |
| worst frame | 3.9 s | 4.3 s |
| frames in the run | 4,377 | 5,872 |
| textures due (mean) | 39,121 | 53,722 |
| peak commit | 37.0 GB | 36.9 GB |

Spread between the two: ~3 ms and ~3 bad periods. A change counts as a regression only beyond that; as a win only
beyond that too. Known picture defects OF THE FLOOR: bushes and some trees glow; heavy texture churn (pop-in).

## How to check a run against the floor
`bash run-compare.sh run-103-fox-floor <label>` and compare with the table; re-run the floor (same settings, the
`fox-floor-diagnostics` exe) whenever the machine state may have changed.

## Best configuration so far (2026-11-26, run-118)
Floor settings + `VICTORY_POOL_DISCARD=1 VICTORY_FILL_TITLE=1 VICTORY_PARTIAL_DELTA=1` (exe `victory-run\target\diag\victory.fox10.exe`):
41 ms at 1-2.5k draws, 50 ms at 2.5-4k, 61 ms >4k; long frames 4.3 %; 11 bad 5-s periods; worst 3.4 s; 7,256 frames
in the run (floor: 4,377-5,872); commit 36.5 GB. Rejected on this base: VRAM_RAW_PCT, UNMAP_EXACT, TEXTURE_RAM_CAPS=0,
first REMAP_MARKS. No measurable gain alone: REFRESH_ONCE, CHUNK_FINAL, CHUNK_COUNT (game uses 216 chunks), BIND_FILTER.
