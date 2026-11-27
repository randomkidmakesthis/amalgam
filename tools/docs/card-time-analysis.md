# Card time: where the title lists' 45-70 ms go (2026-11-26)

Read-only analysis. Sources: `run-67-skip-old/plain.txt` (current build, 57 frame-log windows over 300 draws),
`run-12-merged/plain.txt` (per-pipeline card timer, 9 scene tables; OLD build: no gather, clamps inside the
bindings), code in `victory-dev/crates/victory-gpu/src` and `victory-shader/src`, README. `run-72-pipelines` did not
exist when this was written (06:05): section 2 must be re-read against it. Tags: **[P]** proven (log line or
code), **[I]** inferred. Lines are `file:line` in `victory-gpu/src` unless a crate is named.

## 1. Verdict
- [P] Card time of the title's lists does NOT follow the draw count. Run-67, weighted fit over 57 windows, layer
  commands subtracted: by draws alone R2 0.31; by number of lists (= passes) R2 0.70; with both, the draw term is
  **2.3 us a draw**. Same draw count, different scene: 3,400-3,700 draws cost 33 ms (03:42:23-54, 185 lists) and
  55-72 ms (03:39:19-03:41:08, 220-247 lists); 1,100-1,800 draws cost 40-45 ms (03:41:28-03:42:13).
- [P, run-12] 22-34 pipelines of ~1,500 take 0.2-12 ms per binding, almost all compute bound once a frame:
  **21-36 ms a frame in every scene**, light or heavy (46 ms frame: 22.9 ms; 112 ms frame: 33.1 ms). That block
  alone is the console's whole frame.
- [I] So the factor 4-6 is mostly in ~30 full-screen / whole-buffer compute passes, not in per-draw state. The
  old "flat 25 us a binding" (frame-budget section 5) was the in-list clamps and waits of that build; now the
  layer's commands are metered at 3-5 ms [P] and the per-draw remainder is 2-6 us.
- [P] The card is also starved: idle 14-40 ms a frame, 12-31 ms of it while the submit thread prepares; 3-6 of
  10-17 sends find it empty. That is frame time, not card time (frame-pipeline-analysis.md covers it).
- [P] No debug layer, GPU validation or breadcrumbs slow the card (section 5).
- Plainly: the evidence says the card really is slow on a few dozen translated compute passes, for reasons that
  are the layer's (memory placement, memory-access code, unbuilt ray structures), not the 4080's. State filtering
  cannot reach 33 ms. The heavy passes must fall from ~25 to ~10 ms and the frame must become max(card, CPU).

## 2. Ledger of one heavy frame's title lists
| Part | ms | Basis |
|---|---|---|
| Layer commands in the lists: clamps 850-1,650 (2.0-4.0), card fills 240-3,360 (0.2-1.9), preludes ~77 (0.2-0.4) | 2.4-5.6 | [P] run-67 kinds meter |
| ~30 heavy passes (>= 200 us a binding): compute 19-33, graphics 0.4-3.5 | 21-36 | [P] run-12; [I] unchanged today (same shaders) |
| Thousands of small bindings, real shading included | 10-30 | [I] remainder; [P] 2.3 us marginal per draw |
| Total "all other lists" | 33-74 | [P] run-67 |

Heavy passes, run-12 mean over 9 scene tables (ms a frame; bindings; names not known except where said):
| Pipeline | ms | Bindings | us each | Note |
|---|---|---|---|---|
| compute cb6adcc6da2613ff | 5.3 (3.6-6.3) | 1.0 | 5,600 | every scene |
| compute 5cf00dc39042edaf | 5.3 (0.3-11.8) | 0.7-0.9 | 7,000-12,600 | varies 40x by scene: data-dependent loop [I] |
| compute 14264103296aa43a | 3.4 (2.7-3.6) | 1.0 | 3,570 | constant: size-bound |
| compute 2f03a2df10503cef | 1.3 | 1.0 | 1,370 | |
| compute 9ef9a9d8 / bad01f53 / 2dc364df / 66d8f8f6 / f22a76fc / 34fb51b8 / 17e73620 / ffdb4344 / df11dcd0 / aab1b3c7 | 0.2-0.8 each, 4.9 together | 1.0 each | 300-800 | in all 9 tables: fixed post / lighting chain |
| compute 0a5ecfee, e3730d3d, 2cc4c197, 3a79195f, 8f9eb68d | 0.3-0.6 each | 1-3 | 180-1,100 | |
| vertex 7f4b6212 + pixel 5ad3a82b; vertex 25ebc920 + pixel 48e74d16 / e3bcc531 / 1cd9bc63 | 0.2-0.7 each | 0.9 | 490-1,350 | full-screen triangles |
| compute 660814153af43185, f8f50ab8, caf96a32, 2561af95, c9da71da | 0.4-3.1 each | 50-100 | 5-31 | per-object compute (skinning-like) [I] |
| mesh 910b4be0 (`MSNC_VS_LinearDepthSkin`, translate-list.txt:545) + pixel bff8660b | 0.6-7.6 | 100-275 | 28 | skinned depth; the largest draw family |
| Listed small rows (20 us mean) + unlisted tail of ~1,450 pipelines (13-17 us a binding) | 23-79 | 1,600-4,100 | 13-22 | OLD build: includes its clamps and waits |

## 3. Why the heavy passes are slow (candidates, strongest first)
| # | Cause | Evidence | Tag |
|---|---|---|---|
| H1 | Shaders read and write title memory that lives in the PC's RAM, over the bus | "the PC's shared with the card: in use 5.3-7.7 GB" all run; copystat: 6,000 MB of host copies in PC memory, 5,028 MB of it windows; "windows on the card 1,872-2,244 MB ... promotions held back 2,060, held now" (card 13.1-14.4 of 15.2 GB); title-buffer card copies capped at 1,024 MB and full (119 copies, 1,031 MB); buffers under 1 MiB never get one (shadow.rs:351-371) and are `shared_buffer` = CUSTOM L0 system memory that shaders write (host.rs:10752). Code's own words: "every draw that takes vertices, indices or constants ... reads them over the bus" (shadow.rs:554). Size of the effect, proven once: run-20, one compute list 1 ms -> 42-72 ms when its 24 MB target lost its card copy (frame-budget section 7) | mechanism [P], share of the 25 ms [I] |
| H2 | Ray walkers run over unbuilt structures to the loop cap | builder off (`VICTORY_BVH_BUILDER`, victory-shader pipeline.rs:379); 64 walker programs still run on "whatever bytes lie where the structure would be"; cap 1,024 a lane per loop entry (emit/lower.rs:367-377: shadow mask 11.7 ms at 4,096, 2.4 ms at 1,024, on an RTX 5090). A built tree ends a walk in "some hundreds of nodes" | [P] code and README figure; ms on this card [I] 3-6 |
| H3 | Raw-address memory code: per access a page clamp, one extra directory load, URem + select, an UNCONDITIONAL store to the faults view, then a heap handle and the load | emit/lower.rs:2918-2981. A store per load makes every walker a writing shader and blocks the driver from merging loads | [P] code; 3-6x per access [I] |
| H4 | 64-lane console waves on a 32-lane card: paired form (one host thread = two lanes, "about 2.9 times their DXIL size") or halves exchanged through group-shared memory with two group barriers per branch | emit/mod.rs:437-711, README:294. Applies to tracing tiles, particles, decal trails, hair shadow masks | [P] code; 1.5-3x on those programs [I] |
| H5 | Views picked from the heap by index, per lane non-uniform, plus a stride-table load; stores through picked views add a heap-mark store | shader.rs:486-541, lower.rs:6440-6556 | [P]; 1.1-1.5x [I] |
| H6 | Loop caps (128 pixel, 256 mesh, 4,096 compute): one add + compare per iteration | lower.rs:358-377 | [P]; under 5 % [I] |
| - | NOT a cause: optimisation level. No DXC; DXIL is written directly, registers are SSA with phis, control flow stays structured (no dispatch loop), 16-byte loads are one load. Own clean-up passes are ON by default (`VICTORY_SHADER_OPT` unset = all; `=0` turns them off; README:307: 43 % fewer instructions, ~20 ms) | emit/bitcode/opt.rs:7-28, emit/mod.rs:185-237 | [P] |
| - | Costly but rare: `v_movrels` (register picked by M0) = compare/select chain over up to 256 registers | lower.rs:7009-7053 | [P]; whether a heavy pass uses it: unknown |

## 4. One title draw as the layer records it
The title's state calls are only noted; `State::prepare_for` + `State::bind` (draw.rs:1120, 1215) issue the host calls.
| Host call | When | Skipped if unchanged? | Line |
|---|---|---|---|
| SetDescriptorHeaps | bind | yes; once per host list (one view heap + one sampler heap per title heap, scratch inside it) | draw.rs:1225-1237 |
| Set*RootSignature | bind | yes, by pointer; one signature per host pipeline, so it changes with most pipeline changes | draw.rs:1249-1259 |
| SetPipelineState | bind | yes; forced again after each in-list clamp ("title pipeline bound again" 560-1,650 a frame) | draw.rs:1263, 4049 |
| Root 32-bit constants (table offsets, user data), per stage | every draw | **no** | draw.rs:1304-1327 |
| Root CBV / SRV, one-view tables, the layer's own root views (directory, strides, marks, faults) | every draw | **no** | draw.rs:1338-1443, 1577-1668 |
| IASetVertexBuffers, IASetIndexBuffer | every (indexed) draw | **no** | draw.rs:1162-1179 |
| Topology, OMSetRenderTargets, viewport, scissor, blend factor, stencil ref | on the title's call only | passed through | draw.rs:776-960 |
| Transitions for targets and textures | every draw | yes, only on a real change | draw.rs:1182-1204 |
| Begin/EndRenderPass, ClearState, markers, per-draw queries | never (markers only with `VICTORY_D3D12_DRED`) | - | - |
| In-list clamp: 2 barrier groups (each with a global wait), clamp signature + pipeline, 4 root views, constants + Dispatch per 64 records | 850-1,650 a frame | - | indirect.rs:541-585 |
| In-list card fill: global wait + CopyBufferRegion + 1-2 transitions | 240-3,360 a frame (`VICTORY_CARD_FILLS_BATCH=0` in run-67) | - | host.rs:8689-8701 |
- [P] No heap switches, no render-pass ends, no signature set per draw. What is redundant is 6-12 root-argument and
  buffer calls per draw: that is CPU record time (`State::bind` 11 ms of processor) and, on the card, part of the
  2.3 us a draw. Bound on the card: 3,500 draws x 2.3 us = 8 ms, real shading included.
- [P] What splits the card's batches: a global wait before and after every in-list clamp, one per in-list fill,
  and each title barrier on a buffer becomes a global wait (list.rs:863-869). Metered together: 3-5 ms.

## 5. Debug and build
| Item | State | Basis |
|---|---|---|
| `VICTORY_D3D12_DEBUG`, `_GBV`, `_DRED` (breadcrumbs, page-fault data) | off; each needs the diagnostics feature AND its switch; none is in run-67's env.txt | host.rs:588-670, 1516-1522 [P] |
| `diag` cargo profile | inherits release; host-side code only, the DXIL is the same | Cargo.toml:78 [P] |
| Card meter | 2 timestamps per list + 2,100-2,800 pairs per frame around layer runs; "an empty pair 0.1 us" | cardmeter.rs:184-197, 397-453 [P] |
| `VICTORY_CARD_METER_PIPELINES=1` | a pair per pipeline change; with the profiler run-22 was 14 % slower: tables give shares, not totals | frame-budget.md [P] |

## 6. Starvation (run-67, windows of 3,000-4,300 draws)
| Quantity | Per frame | Tag |
|---|---|---|
| Frame / title lists / our send-and-hash + compare lists | 79-120 / 49-74 / 10-13 ms | [P] |
| Card idle; of it while we prepare / while landing / title recording | 14-39; 12-31 / 2-8 / 0-6 ms | [P] |
| Sends that found the card empty | 3.0-6.2 of 16-17 | [P] |
| Lists / joined submissions | 185-247 / 16-17 | [P] |
| Cross-queue waits on the direct queue | 0.0 | [P] frame-pipeline-analysis section 6 |
The card is busy 60-85 % of the frame. Idle is the serial prepare-then-wait pipeline, already planned (change A).

## 7. Ranked changes (no change of picture)
| # | Change | Evidence | Expected card ms | Size | Symptom if wrong | Cheapest confirmation |
|---|---|---|---|---|---|---|
| 1 | Everything shaders read or write is in the card's memory: make room (card copies' "before" twins ~1 GB, 3.4 GB of render-target copies, free space inside texture heaps), then lift the 1,024 MB / 1 MiB limits and give every window a card side | H1 | 5-20 [I] | M-L | device lost on over-commit; late textures if windows crowd them | `VICTORY_CARD_WINDOWS_MB=0` against the default: the rise of "all other lists" per GB moved to RAM is the bus cost. Then `VICTORY_CARD_BUFFERS_MB=2048` (+ `VICTORY_CARD_BUFFERS_MIN_KB=64`). Timing only: one run with `VICTORY_TEXTURE_DROP_MIPS=1` so that no promotion is held back |
| 2 | Run the ray-structure builder so walks end early (also restores shadows, GI, reflections) | H2; render-audit item A | 3-6 saved in walkers, minus the builder's own 274-371 dispatches | L | known: crash ~80 s into play, card hangs | `VICTORY_BVH_HOLD=1` (drops walker dispatches; timing only): the fall in "all other lists" is what the walkers cost today |
| 3 | Raw-address code: store to the faults view only when the page is missing (branch) or once per program; keep the directory entry per block | H3 | 1-4 [I] | M | black or stale ray results if a missing page is no longer reported | per-pipeline table of run-72 for the walker hashes before and after; names by `VICTORY_DUMP_MATCH=<dir>;#<hash>` |
| 4 | Name and size the top 30 passes, then treat each by cause (H1-H5) | section 2 | decides 1-3 | S | - | run-72 table; `victory translate <pack> --validate --list`; `VICTORY_SHADER_LOOP_LIMIT=64` A/B and `VICTORY_SKIP_PIPELINES=5cf00dc39042edaf,cb6adcc6da2613ff,14264103296aa43a` (timing only; frame-budget run 2, still not done) |
| 5 | Remaining in-list layer commands: fills to the prelude (`VICTORY_CARD_FILLS_BATCH=1`, off in run-67), single dispatch records gathered (`VICTORY_GATHER_DISPATCH=1`, off in run-67), no global waits around clamps | kinds meter 3-5 ms | 2-4 | S-M | exploding or missing instanced geometry | the same meter line; `VICTORY_CLAMP_WAITS=0` (timing only) |
| 6 | Filter unchanged root arguments, vertex and index buffers per draw | section 4 | 1-3 card [I]; mostly CPU (recording 126-185 thread-ms) | M | wrong constants or geometry on single draws | recording ms and the draw term of the fit |
| 7 | Wave pairs: nothing cheap; a 32-lane card cannot run 64-lane waves | H4 | - | L | - | `VICTORY_WAVE_WHY=1` lists which heavy hashes are paired |
Not proposed (rejected or settled elsewhere): lower render size, dropping passes, `VICTORY_CARD_BUFFERS_MIN_KB=64`
alone under the 1,024 MB cap (run-20), in-flight fetch, shader optimisation level (already on).

## 8. What this means for 33 ms
- Card budget needed: title lists <= ~22 ms + our hash/compare lists (10-13 today) inside 33 ms, with the CPU side
  running beside it. Today: 33-74.
- Reachable by changes 5-6 alone: ~5 ms. The rest must come from the ~25 ms of heavy passes (changes 1-4), whose
  split between H1-H4 is NOT measured: run-72's table plus the three one-switch runs (`VICTORY_CARD_WINDOWS_MB=0`,
  `VICTORY_BVH_HOLD=1`, `VICTORY_SHADER_LOOP_LIMIT=64`) settle it.
- If those runs show the heavy passes do not move, the card is slow on the translated code itself (H3-H5), and 33 ms
  in heavy scenes needs translator work per pass, not layer work.

## 9. Proven / inferred
Proven: the fits and scene comparison; the heavy-pass block of run-12; the layer-command ms; memory placement
figures and limits; per-draw call list; debug state; idle split. Inferred: that run-12's heavy block holds today;
every share of H1-H5; all expected gains; pass identities other than 910b4be0.
