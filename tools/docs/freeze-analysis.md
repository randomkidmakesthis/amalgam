# Freeze analysis (coder V, 2026-11-26)

Sources:
- run-191-fox-held-levels-6min, run-216-fox-prison-memo-alias, run-196-fox-open-world-save: the `framelog: long frame`
  lines, which print each frame's own figures.
- Times "at X s" are run time; frame lines are written in 5 s batches.
- "remap" = time the title's own map/unmap calls spent inside our change of mapping: from asking the mapping lock
  until it is let go (`watch.rs` `Remapping`; split as "waiting for readers" and "our notes").
- "faults" = page faults of the process in the frame. "kernel" = the process's kernel CPU time in the frame.

## The freezes (frames >= 400 ms)

| run | at s | ms | remap ms (calls) | faults | kernel ms | free MB | card idle ms | copies MB | class |
|---|---|---|---|---|---|---|---|---|---|
| 191 | 4.7 | 998 | 292 (54,860) | 16k | 297 | 15,674 | 65 | 0 | S start-up mapping |
| 191 | 60.7 | 2,929 | 117 (1,077) | 1.53M | 1,484 | 5,432 | 2,077 | 2,254 | A load burst |
| 191 | 61.6 | 899 | 112 (2,969) | 451k | 656 | 5,268 | 1,433 | 631 | A load burst |
| 191 | 63.6 | 448 | 419 (791) | 11k | 94 | 4,184 | 34 | 70 | **B mapping call** |
| 191 | 67.6 | 727 | 719 (943) | 4k | 141 | 4,255 | 16 | 119 | **B** |
| 191 | 70.3 | 444 | 408 (397) | 4k | 125 | 4,124 | 28 | 177 | **B** |
| 191 | 72.1 | 445 | 401 (578) | 7k | 156 | 4,524 | 11 | 112 | **B** |
| 191 | 330.8 | 832 | 13 (902) | 16k | 859 | 2,091 | 825 | 128 | K kernel, RAM short |
| 191 | 335.7 | 709 | 597 (4,013) | 143k | 547 | 3,896 | 132 | 201 | **B** |
| 191 | 336.2 | 415 | 170 (1,419) | 167k | 219 | 3,960 | 810 | 116 | F faults |
| 191 | 338.5 | 561 | 539 (2,842) | 62k | 312 | 4,024 | 92 | 161 | **B** |
| 191 | 344.1 | 1,205 | 1,160 (1,341) | 8k | 125 | 4,128 | 8 | 119 | **B** |
| 191 | 348.6 | 525 | 500 (5,628) | 102k | 312 | 4,094 | 9 | 61 | **B** |
| 191 | 349.2 | 511 | 371 (2,851) | 66k | 391 | 4,629 | 49 | 98 | **B** |
| 191 | 355.2 | 1,146 | 1,138 (562) | 12k | 94 | 4,807 | 15 | 57 | **B** |
| 191 | 356.6 | 494 | 474 (4,759) | 73k | 266 | 4,482 | 19 | 56 | **B** |
| 216 | 4.4 | 834 | 208 (54,860) | 14k | 188 | 15,939 | 64 | 0 | S |
| 216 | 57.4 | 1,600 | 1,577 (7,816) | 42k | 62 | 10,260 | 53 | 12 | **B** |
| 216 | 65.2 | 3,136 | 1,280 (3,094) | 1.58M | 1,938 | 4,300 | 2,238 | 2,136 | A (+B) |
| 216 | 66.4 | 1,236 | 323 (3,136) | 476k | 953 | 3,760 | 1,758 | 891 | A |
| 216 | 68.3 | 489 | 476 (2,579) | 28k | 188 | 2,836 | 27 | 62 | **B** |
| 216 | 80.3 | 685 | 671 (3,558) | 505k | 188 | 1,599 | 429 | 254 | **B** (+F) |
| 216 | 80.7 | 456 | 429 (882) | 225k | 516 | 1,469 | 473 | 1,703 | **B** |
| 216 | 88.2 | 481 | 460 (349) | 3k | 94 | 2,489 | 64 | 4 | **B** |
| 216 | 90.2 | 504 | 485 (708) | 4k | 0 | 3,077 | 62 | 4 | **B** |
| 216 | 95.3 | 549 | 11 (1,713) | 208k | 234 | 1,802 | 396 | 7 | F, RAM short |
| 216 | 98.8 | 870 | 137 (5,514) | 533k | 719 | **409** | 19 | 331 | F, RAM exhausted |
| 216 | 101.2 | 2,406 | 409 (1,853) | 257k | **3,547** | 4,403 | 2,720 | 1,580 | A (submit calls 1,778 ms) |
| 216 | 108.5 | 434 | 359 (538) | 13k | 188 | 4,534 | 200 | 155 | **B** |
| 216 | 157.2 | 462 | 424 (754) | 11k | 94 | 5,591 | 9 | 150 | **B** |
| 216 | 158.7 | 655 | 631 (536) | 11k | 156 | 5,518 | 10 | 127 | **B** |
| 196 | 4.2 | 870 | 220 (54,860) | 15k | 250 | 17,030 | 65 | 0 | S |
| 196 | 52.9 | 4,281 | 346 (5,327) | **2.33M** | 3,406 | 2,700 | 3,897 | 2,738 | A (user 15.3 s on all threads) |
| 196 | 53.4 | 533 | 60 (1,317) | 299k | 1,328 | 2,680 | 419 | 527 | A |

In every one of them:
- pipelines waited 0 ms;
- DirectStorage/disk reads 0 ("disk: 0 reads");
- guard faults 0;
- "waiting for readers" at most 5 ms;
- "our notes" at most 63 ms;
- the render thread ran 0-94 ms.

So in class B the remap time is neither our page-table lock nor our bookkeeping. It is time spent inside the title's
map/unmap calls themselves: system calls and what they wait on.

## Totals by class

| class | what it is | run-191 | run-216 | run-196 |
|---|---|---|---|---|
| **B mapping call** | One of the title's map/unmap calls takes 0.4-1.6 s: nearly the whole frame, few faults, card busy and waiting on the title. | 11 frames, 7.2 s (54 %) | 10 frames, 6.3 s | 0 |
| A load burst | Scene load: 0.45-2.3 M faults, 0.6-3.5 s kernel, 0.6-2.7 GB copied up, card idle 1.4-3.9 s. | 2 frames, 3.8 s (29 %) | 3 frames, 6.8 s | 2 frames, 4.8 s |
| F fault storm / RAM short | 0.2-0.5 M faults, free RAM 0.4-2 GB. | 1 frame, 0.4 s | 3 frames, 2.0 s | 0 |
| K kernel time, RAM short | 859 ms kernel, few faults, free 2.1 GB. | 1 frame, 0.8 s | 0 | 0 |
| S start-up | The first 54,860 mappings at 4-5 s: once per run, before play. | 1 frame, 1.0 s | 1 frame, 0.8 s | 1 frame, 0.9 s |

## Best-supported causes

1. **B: single slow map/unmap calls of the title (half of all freeze time in play).**
   - Unknown: which system call (`UnmapViewOfFile2`, the placeholder split, `MapViewOfFile3`, the commit) and why it
     takes up to 1.1 s.
   - The runtime changes a mapping in several steps: to unmap a page of a view it unmaps the whole view and maps what
     stays (`xmem.rs` `unmap`). So a call's cost grows with the view, not with the page.
   - With `VICTORY_POOL_DISCARD=1`, pages given back are `MEM_RESET` and trimmed while free RAM is short. An unmap or
     map of such a view may wait on that work.
   - Title threads that touch memory being remapped wait for it (`watch.rs` `came_back`), so the title's frame waits
     for the whole call.
2. **A: scene loads.**
   - The title touches newly mapped pool memory (first-touch / demand-zero faults: 1.5-2.3 M faults ≈ 6-9 GB).
   - We copy 2.1-2.7 GB of new title memory up in one frame; the card stands idle meanwhile.
   - Part of this is the title's own loading and cannot go. The faults on pages `POOL_DISCARD` reset are ours.
3. **F/K: RAM short.** Free RAM 0.4-2 GB. Faults and kernel time on working-set trims and resets. run-216 98.8 s had
   409 MB free.

## What is unknown, and the one meter that settles it

The split of B's time into system calls. **No build is needed:** the meter exists, `VICTORY_REMAP_METER=1`
(`xmem.rs` `remap_meter`). Every 5 s it prints:
- the mapping calls and their pages;
- per system call kind (unmap, split, map, commit): calls, ms, mean, longest, number over 5 ms;
- the mapping lock's wait, our notes, and free RAM.

The 5 s line lines up with the frame log's 5 s batches. It has never been on in a fox run.

**Run:** the standing set + `VICTORY_REMAP_METER=1`, 6 min (the same route as run-191). Optional second run:
+ `VICTORY_POOL_DISCARD=0`, to see A/F/K faults and whether B's long calls disappear when no pages are reset.

## Ranked fixes (after that run)

1. If one call kind dominates B:
   - If the unmap or re-map of whole views dominates: unmap only the pages asked, by splitting a placeholder around
     them (`UnmapViewOfFile2` + `VirtualFree(MEM_PRESERVE_PLACEHOLDER)` on the part). That makes the cost follow the
     size asked.
   - If it is the commit: commit ahead, off the title's thread.
   - If it waits on `MEM_RESET` work: no reset of pages in views being changed, and resets in a thread of their own,
     paced.
2. `VICTORY_POOL_DISCARD`: keep it only if A/F/K do not grow without it. It saves RAM, but every reset page is
   faulted again, and RAM-short runs freeze anyway.
3. Load bursts (A): send the up-copies of a load in slices of at most ~256 MB a frame, with the card working, instead
   of 2+ GB in one frame. Our part of A is the copies and the card standing idle.
4. RAM: the commit cuts already under way (held levels −4 GB) lower F/K. Watch free RAM ≥ 3 GB in the prison scene.
