# Picture defects by run: vegetation glow against ray tracing, and other defects (runs 166-179, 190-231)

Read-only audit, 2026-11-26. Nothing was run in the game, built or changed in the project. Companion to `faces-by-run.md`.

## How it was done

- Shots were viewed as reduced contact sheets (four shots per sheet, about 40 % size) plus 1:1 crops of the vegetation areas, built in the session scratchpad; the originals were not touched.
- Ray tracing setting read from each `env.txt` (final value of `VICTORY_BVH_BUILDER`).
- Limits: one frame per 30 s, night scenes. The route does not hit the same moment at the same second in every run, so comparisons below are only made between shots that show the same camera view.
- Of runs 190-231, 36 scripted runs were looked at again for this report. Not looked at again for vegetation: 191, 224 (first sheet), 230 (scripted 6-minute runs) and hand-played 216; for those only the notes from the faces audit exist, and they did not record vegetation.

## Ray tracing setting per run (from env.txt)

| Runs | VICTORY_BVH_BUILDER |
|---|---|
| 166, 167, 168, 169, 170, 171, 172, 173, 174, 176, 177 | 1 (on) |
| 175, 178, 179 | unset (off) |
| 190 - 231 | unset (off) |

Runs 168 and 169 were not in the list given but also have it on. Run 178 has no shot after 63 s. Run 176 is the only RT-on run that also has `VICTORY_GPU_ONLY=1`.

## 1. Is the violet vegetation glow gone with ray tracing on?

**Yes, in every like-for-like pair available.**

### View A: street with the police car (lamp, palms and a bush on the left; shot at about 93 s)

| Run | RT | Shot | Vegetation |
|---|---|---|---|
| 168 | on | shot_0093 | palms green where the lamp lights them, otherwise dark; no violet |
| 169 | on | shot_0093 | same; no violet |
| 171 | on | shot_0093 | same; bush by the red light is dark; no violet |
| 173 | on | shot_0093 | same; no violet |
| 176 | on (GPU_ONLY=1) | shot_0093 | same; no violet |
| 177 | on | shot_0093 | same view from a slightly different moment; palms green; no violet |
| 175 | off | shot_0093 | palms right of the lamp violet-pink; bush by the red light pink-violet; low plants behind the car violet-white |
| 179 | off | shot_0093 | same violet palms, pink bush, violet-white low plants |
| 203, 207, 229 and others in 190-231 | off | shot_0093 | same violet palms and plants (see section 2) |

1:1 crops of the same image area were compared side by side for 171 and 176 (on) against 175 and 229 (off): the same palms are green-and-dark with RT on and violet with RT off.

### View B: lit hotel behind the man in the cap, row of palms in the distance on the right (shot at about 154 s)

| Run | RT | Shot | Vegetation |
|---|---|---|---|
| 166 | on | shot_0152 | distant palms are dark silhouettes; no violet |
| 168 | on | shot_0154 | distant palms dark; no violet |
| 174 | on | shot_0154 | distant palms dark; no violet |
| 179 | off | shot_0154 | distant palms glow violet-white |
| 193, 197, 199, 202, 208, 209, 219, 221, 222, 229 | off | shot_0154 | distant palms glow violet-white or pink |

### Other RT-on shots with vegetation

- 170 shot_0093, 174 shot_0093 (man with the bat): palm crown behind the woman is dark, no violet.
- 170 shot_0154, 171 shot_0154, 173 shot_0154, 176 shot_0154 (woman at the car): palm trunk lit warm, no violet anywhere. The matching RT-off shot 175 shot_0154 has violet-pink palms at the left edge and a violet patch on the woman's chest.
- No daytime vegetation exists in any RT-on run, so daylight cannot be compared.

### What this does and does not show

- Runs 175 and 179 are from the same period as the RT-on runs, have RT off, and show the glow. So the difference follows the ray tracing setting, not the age of the build.
- The logs of the RT-off runs contain a line saying the program `CS_RaytracingResolve` is left out while the ray-tracing structure builder is off. That fits, but the pictures only show the correlation; they do not show which pass produces the violet.
- Sample: 11 RT-on runs, 9 of them with a usable vegetation shot; none shows violet vegetation.

## 2. Other visible defects in RT-off shots of runs 190-231

Counts are "runs in which it was seen in at least one shot". A defect can only be counted where the scene that shows it was captured.

| # | Defect | Runs | Shots |
|---|---|---|---|
| 1 | Violet / pink / violet-white vegetation (palms, bushes, low plants, hedge, a plume-like plant by the fence) | 36 of 36 scripted runs looked at | shot_0093 (0091 in 227) in 190, 192, 193, 194, 195, 197-210, 212, 214, 215, 217-223, 225-229, 231; shot_0154 in 193, 197, 199, 202, 205, 208, 209, 212, 213, 217, 218, 219, 221, 222, 223, 229; also 205 shot_0184 and 231 shot_0185 (violet foliage through the car window) |
| 2 | Hotel awnings bright saturated blue, hotel wall emblem tinted violet (with RT on the same awnings are dark green and the emblem is plain) | 16 (every run whose shots show the hotel) | shot_0154 of 190, 192, 193, 197, 199, 202, 205, 208, 209, 215, 218, 219, 221, 222, 223, 229 |
| 3 | White or violet dot clusters (speckle) on ground-cover plants, or around distant palms | 6 | 212, 213, 217 shot_0154 (hedge); 218, 223, 229 shot_0154 (dots around the distant palms) |
| 4 | Magenta or blue streaks / speckles on a character's skin or clothing | 5 | 194 shot_0154, 228 shot_0154 (magenta streaks on the top and arm); 200 shot_0154 (small blue speck on the arm); 195 shot_0154 (white speckle band at the neck, violet patch on the officer's forearm); 217 shot_0154 (pink-white blotches on the back) |
| 5 | Glossy "wet" streaks on skin | 2 | 191 shot_0124, shot_0154; 198 shot_0124, shot_0154 |
| 6 | Dark diagonal band across the woman's eyes (may be hair in that animation frame) | 2 | 214 shot_0154, 220 shot_0154 |
| 7 | Black hole in the centre of lights (street lamp, red light, police light) | 1 | 207 shot_0093 |
| 8 | Head drawn black with only eyes and teeth | 1 | 205 shot_0154, 0184, 0215, 0276 |
| 9 | Missing object and blocky rectangles: no police car, a row of coloured blocks on the road in its place | 1 | 205 shot_0093 |
| 10 | Dark patch over the eye area | 1 | 222 shot_0154 |
| 11 | Magenta on objects in daylight (hand-played): a pole streaked magenta, magenta on the player's hand | 1 | 196 shot_0245 |
| 12 | Frame almost uniformly dark grey with the scene barely visible (could be a fade between scenes; cannot tell) | 1 | 213 shot_0093 |

Not seen in any shot: stretched geometry, white holes. In run 205 shot_0093 the palms by the lamp are green and not violet although RT is off; that frame is also the one with the missing car, so it is not a normal frame.

Not game defects, but in the shots: Task Manager or other windows cover part of the frame in 190, 192, 209, 210, 215 and 228 shot_0124 (a driver page), and file-manager windows in 196, 205, 230.

## 3. Systematic differences between RT on and RT off besides lighting

- Violet vegetation: only with RT off (section 1).
- Blue awnings and violet emblem on the hotel: only with RT off. RT on: 166 shot_0152, 168 shot_0154, 174 shot_0154 show dark green awnings.
- Low plants behind the police car: violet-white with RT off, dark with RT on.
- Magenta or blue streaks on characters: seen only with RT off (175 shot_0154 also has a violet patch on the woman's chest). Not seen in the RT-on shots, but only about 20 RT-on shots show a character, so absence there is weak evidence.
- Dot clusters in the distance are in both: RT on 166 shot_0152, 168 shot_0154, 174 shot_0154, 177 shot_0093 (white dots near the palms or in the sky); RT off 218, 223, 229. So that speckle is not explained by the ray tracing setting.
- Broken faces in RT-on run 172 (shot_0124, shot_0155: faces dark with only eyes and mouth) were expected and are left out of the comparison.
- Wet-glossy skin: not seen in RT-on shots of the officer scene (167 shot_0154, 169 shot_0154, 172 shot_0155), and seen in only 2 of the RT-off runs, so it does not follow the ray tracing setting.
