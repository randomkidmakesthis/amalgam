# Faces by run: picture audit of run-190 .. run-231

Read-only audit, 2026-11-26. Nothing was run, built or changed in the project.

## How it was done

- 42 run folders (`C:\GTAVI\Tools\victory-data\bench\run-190*` .. `run-231*`); all have a `shots` directory.
- Every screenshot from 90 s on was looked at (175 shots). To fit them, the PNGs were tiled four to a sheet at about 40 % size, plus a 1:1 crop of the last shot for the 3-minute runs, and 1:1 crops of every shot that looked suspicious. Sheets are temporary, in the session scratchpad; the originals were not touched. One original (run-190 shot_0154) was opened directly as a reference.
- Limits: one frame every 30 s; night scenes with coloured light; many faces are in shadow or small. "OK" means "no defect visible in these frames", not "proven correct". Fine wet/matte differences on dark faces cannot be judged at this sampling.
- The scripted route does NOT show the same scene at the same second in every run. There are two timings: in some runs the 124 s shot is a credits card and the 154 s shot is the man in the red cap; in others the 124 s and 154 s shots are already the later scene with the police officer and the woman at the car. This matters for "wet": it was only judgeable where the officer/car scene was captured.
- Settings are the final value per variable in `env.txt`. The exe is not recorded per run (`VBIN=diag` in all 42).
- Log numbers come from `plain.txt`: fps lines "dropped N without a host pipeline (M being made)" with M > 0, and "uploads by cause ... bake N (" lines with N > 0 in the first 100 s of the log.

## Settings identical in all 42 runs (final values)

`VICTORY_LEVELS_ON_DEMAND=0`, `VICTORY_BAKE_DELTA=1`, `VICTORY_PARTIAL_DELTA=1`, and ASYNC_SUBMIT=1, BACK_OWN_VIEW=1, CARD_AWAKE=0, CARD_BUFFERS_MIN_KB=64, CARD_FILLS_BATCH=0, CARD_METER=1, CARD_REHASH_RUNS=1, CARD_STEPS=2, COPY_PLAIN=1, FILL_TITLE=1, FRAME_LOG=1, GATHER_DISPATCH=0, INDIRECT_BATCH=2, INDIRECT_GATHER=1, INDIRECT_WINDOW=1, JOIN_COPIES=1, JOIN_PARTS=8, MAP_LOCK_FINE=1, MAP_THREADS=1, MIRROR_FAST=1, POOL_DISCARD=1, QUEUE2=1, REFRESH_ONCE=1, RELAY=2, RELAY_COPY=1, SHOW_AFTER=1, TEXTURE_NO_WAIT=1, UPLOAD_AHEAD=1, WIDE_BACK=1, WIDE_MERGE=1, WIDE_TEXTURES=1.

Never set in any of the 42 runs: `VICTORY_PAGE_POOL`, `VICTORY_PAGE_MIGRATE`, `VICTORY_BVH_BUILDER`.

Baseline for the table below (the most common values): `GPU_ONLY=1, KNOWN_WRITES=1, POOL_CAP=1, PIPELINE_WAIT_MS=20, LATE_WAIT_LISTS=1024`, and unset: TABLE_MEMO, TABLE_MEMO_SLOTS, LIST_HOLD, LIST_ALIAS_EXACT, LIST_BATCH, LIST_CALLS, REFRESH_PAGES, NARROW_WRITES, THIN, WINDOWS_BAR_MB, SHADER_LOOP_LIMIT, CACHE_DIR, DISPLAY_MODE. The "differs from baseline" column lists only what is different. AUTO_INPUT is the same script in all runs except run-196 (`w1`, hand-played).

## Table

Columns: shots = seconds looked at. Cold-cache evidence = fps lines with "being made" > 0 (count / sum of M / seconds of first-last such line); first fps line time; bake-upload lines in first 100 s (count / uploads).

| Run | Shots (s) | Verdict | What is seen | Differs from baseline | Cold-cache evidence |
|---|---|---|---|---|---|
| 190-fox-held-levels | 93,123,154 | faces OK | 93: woman in profile, face in shadow. 123: credits card. 154: man in red cap, face with skin and beard, matte | POOL_CAP unset | made 1 / 2 / 52 s; first fps 6 s; bake 8 / 12736 |
| 191-fox-held-levels-6min | 93..337 | **skin wet-glossy** (faces OK) | 124 and 154 (officer + woman at car): bright specular streaks on the woman's arms, chest and midriff and on the officer's forearm, skin looks oiled. 184 woman close-up, 215 man, 245/276 woman indoors: faces complete. 306, 337 credits | none (pure baseline) | made 1 / 2 / 47 s; first fps 7 s; bake 8 / 12106 |
| 192-fox-held-waits-meter | 93,124,154 | faces OK | 154: man in cap, face mostly in shadow, beard visible; nothing missing | none | made 0; first fps 6 s; bake 8 / 11879 |
| 193-fox-rtoff-tablesteps | 93,124,154 | faces OK | 154: man in red cap, face complete, matte | GPU_ONLY unset, KNOWN_WRITES unset, POOL_CAP unset, PASS_TIMES=1 | made 0; first fps 6 s; bake 7 / 10932 |
| 194-fox-fill-from-placed-verify | 93,124,154 | faces OK; other: speckles | 154: woman at car, face complete, matte. A few small magenta streaks/speckles on her top and arm | FILL_FROM_PLACED=verify | made 1 / 3 / 49 s; first fps 8 s; bake 7 / 10985 |
| 195-fox-pass-times-serial | 93,123,154 | faces OK; other: speckles | 123: man with raised hands and woman, faces complete, hands matte. 154: officer and woman close; faces complete; a white sparkling speckled band at the woman's neck | PASS_TIMES=2 | made 6 / 472 / 7-64 s (partly cold); first fps 7 s; bake 8 / 11135 |
| 196-fox-open-world-save (hand-played) | 93..489 | faces OK (hand-played) | Daylight open world, player mostly from behind; 276 and 459 side views of the face: complete; arm skin matte. Unrelated: magenta pole at 245; 398 is a file-manager window, not the game | FILL_FROM_PLACED=verify, AUTO_INPUT=w1, KEEP_SAVE=1 | made 5 / 14 / 37-271 s; first fps 6 s; bake 2 / 2020 |
| 197-fox-signature-local | 93,124,154 | faces OK | 154: man in cap from the front, face complete, matte | SIGNATURE_LOCAL=1 | made 0; first fps 6 s; bake 8 / 1909 |
| 198-fox-audit-set | 93,124,154 | **skin wet-glossy** (faces OK) | 124 and 154 (officer + woman): strong glossy streaks on the woman's arm, shoulder, chest and on the officer's forearm, same look as run-191. Faces complete | POOL_CAP unset, LATE_WAIT_LISTS=256, MEMORY_MAP=0, METER_THREAD=1 | made 1 / 1 / 53 s; first fps 7 s; bake 8 / 13892 |
| 199-fox-thin | 93,124,154 | faces OK | 154: man in cap from the front, eyes, skin and beard visible | THIN=1, POOL_CAP unset | made 2 / 5 / 47-52 s; first fps 6 s; bake 8 / 14078 |
| 200-fox-release-build | 93,124,154 | faces OK | 154: woman at car, face complete, skin matte | POOL_CAP unset | made 3 / 259 / 6-52 s (partly cold); first fps 6 s; bake 0 / 0 (no bake-upload lines in the whole log) |
| 201-fox-display-1440p | 93,124,154 | faces OK | 124 and 154 (officer + woman): faces complete, skin matte, no streaks | DISPLAY_MODE=2560x1440, REFRESH_ACCOUNT=1, POOL_CAP unset | made 0; first fps 7 s; bake 8 / 10179 |
| 202-fox-bind-same-verify | 93,123,154 | faces OK | 154: man in cap from the front, face complete | BIND_SAME=verify, LIST_CALLS=1, POOL_CAP unset | made 2 / 21 / 47-52 s; first fps 7 s; bake 8 / 13547 |
| 203-fox-display-1440p-sizes | 93 only | **cannot judge** | Only one shot after 90 s: woman from behind, no face | DISPLAY_MODE=2560x1440, GPU_ONLY unset, KNOWN_WRITES unset, POOL_CAP unset, PASS_TIMES=1 | made 1 / 18 / 47 s; first fps 6 s; bake 7 / 2842 |
| 204-fox-windows-bar | 93,124,154 | faces OK | 154: woman and officer, her face complete, skin matte | WINDOWS_BAR_MB=2048 | made 1 / 1 / 47 s; first fps 6 s; bake 8 / 9763 |
| 205-fox-loop-limit-1024 | 93..336 | **faces wrong: black, skin missing** | 154, 184, 215: the man's head is black with only eyeballs and teeth drawn; cap, hair, beard, arms and clothes are drawn. 276: man on the sofa, face black with only eyes. 93: no police car, a row of blocky coloured rectangles on the road where it stands in other runs. 306, 336 are a file-manager window, not the game | SHADER_LOOP_LIMIT=1024, CACHE_DIR=...\coolfox\cache-loop1024, PIPELINE_WAIT_MS=500 | made 15 / 5039 / 66-327 s (cold cache, shaders translated through the scenes); first fps 6 s; bake 7 / 8412 |
| 206-fox-table-memo-verify | 93,124,154 | faces OK | 154: woman close, face complete, skin matte | TABLE_MEMO=verify, LIST_CALLS=1 | made 0; first fps 7 s; bake 6 / 13862 |
| 207-fox-loop-limit-1024-warm | 93 only | **cannot judge** | Only one shot after 90 s: woman from behind. Not a face, but visible: the centres of the street lamp, the red light and the police light are black holes | SHADER_LOOP_LIMIT=1024, CACHE_DIR=...\coolfox\cache-loop1024 | made 2 / 32 / 48-53 s; first fps 7 s; bake 7 / 6840 |
| 208-fox-ask-kept-early | 93,123,154 | faces OK | 93: woman in profile, face in shadow. 154: man in red cap, face complete | ASK_KEPT_EARLY=1 | made 0; first fps 6 s; bake 8 / 14216 |
| 209-fox-table-memo-verify2 | 93,123,154 | faces OK | 123: woman and man with raised hands. 154: man from the front. Faces complete | TABLE_MEMO=verify, LIST_CALLS=1 | made 0; first fps 6 s; bake 8 / 9781 |
| 210-fox-table-memo-on | 93,124,154 | faces OK (little face visible) | 154: woman covering her face with her arm; skin of arm and body matte. Task Manager covers part of the frames | TABLE_MEMO=1, LIST_CALLS=1 | made 2 / 37 / 48-53 s; first fps 7 s; bake 8 / 9809 |
| 211-fox-prison-latest (hand-played from ~184 s) | 93..276 | faces OK (hand-played) | 154: man in red cap, face complete. 215: woman in profile at window, complete. 245: her face in shadow with a patch of orange light, nothing missing. Forearms look pale grey (could be undershirt sleeves; cannot tell) | none | made 0; first fps 6 s; bake 7 / 13117 |
| 212-fox-table-split | 93,124,154 | faces OK | 124: woman and officer, small, faces complete, skin matte. 154: woman from behind | TABLE_MEMO=verify, LIST_CALLS=1 | made 0; first fps 6 s; bake 7 / 11167 |
| 213-fox-table-memo-on2 | 93,123,154 | faces OK | 93: dim fade frame. 123: woman and officer, faces complete, skin matte. 154: legs only | TABLE_MEMO=1, LIST_CALLS=1 | made 0; first fps 6 s; bake 8 / 13359 |
| 214-fox-listbatch-verify | 93,124,154 | faces OK | 154: woman and officer; skin matte. A dark diagonal band across her eyes; the same band is in run-220 at the same moment and hair strands cross the face there in 198 and 226, so it looks like hair in that animation frame | TABLE_MEMO=verify, LIST_CALLS=1, LIST_ALIAS_EXACT=1, LIST_BATCH=1 | made 1 / 1 / 47 s; first fps 6 s; bake 8 / 9076 |
| 215-fox-listbatch-on | 93,123,154 | faces OK | 154: man in red cap in red light, skin and beard visible. Task Manager covers part of the frames | TABLE_MEMO=1, LIST_CALLS=1, LIST_ALIAS_EXACT=1, LIST_BATCH=1 | made 3 / 83 / 48-58 s; first fps 7 s; bake 6 / 12285 |
| 216-fox-prison-memo-alias (hand-played) | 93..276 | faces OK (hand-played) | 93: black frame. 123: woman's face large in sunlight, complete, matte. 184: guard from behind. Others: backs and silhouettes. Forearms pale grey as in 211 | TABLE_MEMO=1, LIST_ALIAS_EXACT=1 | made 0; first fps 6 s; bake 8 / 29060 |
| 217-fox-clean-stamp2-verify | 93,124,154 | faces OK; other: **blotchy skin on back** | 124: woman and officer small, faces complete. 154: woman from behind: bright pink/white blocky blotches on the skin of her back and on the top | CLEAN_STAMP=verify | made 1 / 3 / 52 s; first fps 7 s; bake 8 / 13425 |
| 218-fox-card-split | 93,124,154 | faces OK | 124: woman and officer, faces complete, matte. 154: man in cap at the car, face in shadow | none | made 0; first fps 7 s; bake 8 / 13726 |
| 219-fox-prepare-shape | 93,124,154 | faces OK | 154: man in red cap, face complete | PREPARE_SHAPE=1 | made 1 / 7 / 52 s; first fps 6 s; bake 7 / 10028 |
| 220-fox-narrow-verify | 93,124,154 | faces OK | 124, 154: woman and officer, skin matte. Same dark band across her eyes as in 214 | NARROW_WRITES=verify | made 1 / 1 / 53 s; first fps 7 s; bake 8 / 12017 |
| 221-fox-narrow-on | 93,123,154 | faces OK | 154: man in red cap, red light, face complete | NARROW_WRITES=1 | made 1 / 2 / 52 s; first fps 6 s; bake 8 / 10982 |
| 222-fox-prepare-shape2 | 93,123,154 | **face partly wrong: dark patch over the eyes** | 154: man in cap from the front; a dark blot covers the eye area (worst over one eye, like an eye patch); rest of the face, beard and skin are drawn. In 197, 199, 202, 209, 224 the same shot shows both eyes | PREPARE_SHAPE=1 | made 1 / 2 / 47 s; first fps 6 s; bake 7 / 3436 |
| 223-fox-flush-split | 93,123,154 | faces OK | 123: woman and officer, faces complete, matte. 154: man at left edge | none | made 0; first fps 6 s; bake 8 / 10677 |
| 224-fox-remap-meter-6min | 93..337 | faces OK | 154 man front, 185 man in car (profile), 215 woman in red light, 276 man on sofa: all faces complete, matte | REMAP_METER=1 | made 1 / 1 / 349 s; first fps 7 s; bake 8 / 11496 |
| 225-fox-refresh-kinds | 93,124,154 | faces OK | 154: woman at car, face complete, matte | none | made 0; first fps 7 s; bake 8 / 9123 |
| 226-fox-refresh-pages-verify | 93,124,154 | faces OK | 154: woman and officer; hair strands across her face; a soft highlight on her arm, not the streaks of 191/198 | REFRESH_PAGES=verify | made 1 / 3 / 47 s; first fps 7 s; bake 8 / 11878 |
| 227-fox-refresh-pages-on | 91,122,152 | faces OK | 152: woman at car, face complete, matte | REFRESH_PAGES=1 | made 1 / 1 / 53 s; first fps 7 s; bake 8 / 8610 |
| 228-fox-recorder1-verify | 93,124,154 | faces OK; other: speckles | 154: woman at car, face complete, matte; magenta streaks/speckles on her top and arm (as in 194). 124 is another program's window, not the game | TABLE_MEMO=verify, TABLE_MEMO_SLOTS=1, LIST_HOLD=1, LIST_CALLS=1 | made 1 / 5 / 52 s; first fps 6 s; bake 8 / 7513 |
| 229-fox-recorder1-on | 93,124,154 | faces OK | 93: woman in profile. 154: man in red cap. Faces complete | TABLE_MEMO=1, TABLE_MEMO_SLOTS=1, LIST_HOLD=1, LIST_CALLS=1 | made 0; first fps 7 s; bake 7 / 5608 |
| 230-fox-remap-meter2-6min | 93..337 | faces OK | 124 man with raised hands and woman, 154 officer, 215 man and woman in car, 276 man on sofa: faces complete, matte. 245 is a file-manager window | REMAP_METER=1 | made 3 / 81 / 47-57 s; first fps 7 s; bake 8 / 7509 |
| 231-fox-journal-growth-6min | 93..337 | faces OK | 124, 154 officer + woman: skin matte, no streaks. 185 woman close-up, 215 man in red light, 245/276 woman indoors: faces complete | none (pure baseline) | made 2 / 2 / 48-352 s; first fps 7 s; bake 7 / 11043 |

## Totals

- Faces wrong: 2 runs: run-205 (black head, only eyes and teeth) and run-222 (dark patch over the eyes, much milder).
- Skin wet-glossy: 2 runs: run-191 and run-198.
- Cannot judge: 2 runs: run-203 and run-207 (stopped after the 93 s shot, no face on it).
- Faces OK: 36 runs (33 scripted + 3 hand-played: 196, 211, 216). Four of them show small skin/clothing speckles that are not on the face: 194, 228 (magenta speckles on the torso), 195 (white speckle band at the neck), 217 (blotchy blocks on the back).

## What the bad runs have in common

### Black face (run-205)

Present in run-205 and in no run judged good:

- `VICTORY_SHADER_LOOP_LIMIT=1024` together with `VICTORY_CACHE_DIR=...\cache-loop1024`. The only other run with these two, run-207, has no face on its single late shot, but shows black holes in the centres of lights, which no baseline run shows. So every run with the loop limit of 1024 shows a black-pixel defect, and no run without it shows a black face.
- `VICTORY_PIPELINE_WAIT_MS=500` (only run-205).
- A cold shader cache through the scenes: 15 fps lines with pipelines being made, 5039 in total, from 66 s to 327 s. No other run comes close (next: run-195 with 472, all before 64 s, faces OK; run-200 with 259, faces OK).

These three are tangled in the one run and cannot be separated from the pictures. Run-207 (warm, wait 20 ms, same loop limit and cache) still showing black light centres points at the loop limit rather than the cold cache or the wait, but that is one frame without a face, not proof.

### Dark patch over the eyes (run-222)

Nothing separates it. Its only non-baseline setting is `VICTORY_PREPARE_SHAPE=1`, which run-219 also has with a complete face. Its bake uploads in the first 100 s are low (3436 against roughly 7500-14000 in most runs), but run-197 (1909) and run-229 (5608) are also low with complete faces, and run-203 (2842) cannot be judged. No shaders were being made at that time (2 in total, at 47 s). With nothing in the settings or the log setting it apart, this looks like run-to-run timing (for example the face bake racing texture loading), not a setting. That is an inference from the absence of a difference, not something the files show.

### Nothing is common to both face-wrong runs

No setting or log condition is present in both run-205 and run-222 and absent from all good runs. They also look different (whole head black against one dark patch), so they are probably two separate problems.

### Wet-glossy skin (run-191, run-198)

- No setting separates them. Run-191 is pure baseline; run-231 has exactly the same final settings, the same 6-minute route and the same officer/car scene at 124 s and 154 s, and its skin is matte. Run-198 differs from run-191 in four settings (POOL_CAP unset, LATE_WAIT_LISTS=256, MEMORY_MAP=0, METER_THREAD=1); none of them is in run-191.
- No log condition separates them: almost no shaders being made (2 and 1), ordinary bake uploads (12106 and 13892), ordinary warning counts.
- What they do share is time: they are the two earliest runs in which the officer/car scene was captured (started 17:34 and 18:11). Every later run that captured that scene is matte: 201, 204, 206, 210, 212, 213, 214, 217, 218, 220, 223, 226, 231 (18:22 onwards). Run-195 (18:03) lies between the two wet runs and shows no streaks, but its shots are of other moments (raised hands, a close-up), so it does not contradict or confirm.
- So the wet look is either tied to something that changed between about 18:11 and 18:22 that env.txt does not record (the exe is not recorded per run; run-200, started 18:18, is labelled "release-build", which is only a label), or it is run-to-run. The pictures cannot tell these apart. Only 2 wet runs against 13 matte ones after the gap, and none matte before it in the same scene, fits a change better than chance, but that is a correlation in time, not a demonstrated cause.
- Wetness could not be judged in runs that never reached the officer/car scene within their shots (the "man in red cap at 154 s" runs); a matte verdict there only covers the faces that were on screen.

### Partial correlations worth knowing

- Small speckles on skin or clothing (194, 195, 217, 228) have no common setting: FILL_FROM_PLACED=verify, PASS_TIMES=2, CLEAN_STAMP=verify, and the TABLE_MEMO/LIST_HOLD verify set respectively; three of the four are "verify" runs, but other verify runs (202, 206, 209, 212, 214, 220, 226) show none in their shots.
- TABLE_MEMO, TABLE_MEMO_SLOTS, LIST_HOLD, LIST_ALIAS_EXACT, LIST_BATCH, LIST_CALLS, NARROW_WRITES, REFRESH_PAGES, THIN, WINDOWS_BAR_MB, DISPLAY_MODE, GPU_ONLY/KNOWN_WRITES unset, POOL_CAP on or off: each appears in at least one run with complete matte faces and in no face-wrong run, so none of them is implicated by these pictures.
