# Move to the cool_fox base: common brief for the three porting coders (2026-11-26)

Decision (user): cool_fox is the new base. It measured ~64 ms/frame at 2,500-4,000 draws on this PC (run-100-coolfox)
against ~88 ms for our tree (run-98). "Implement all our fixes there."

## Trees
- OLD tree, FROZEN, read-only: `C:\GTAVI\Tools\victory-dev` (base soldsawn/victory-nvidia-8gb + all our work, uncommitted;
  `git -C victory-dev diff HEAD` + untracked files = our work). Newer translator/host fixes not yet in it: `Tools\par\D1`
  against `Tools\par\D1base` (diff them).
- NEW base: `C:\GTAVI\Tools\victory-fox` (clone of ref-forks\cool_fox @f4b48ad, branch local/scarlett-fox). Do not edit it;
  main merges patches into it.
- YOUR copy: `C:\GTAVI\Tools\par\foxA|foxB|foxC` (a git clone of victory-fox). Edit only there and only the files you own.

## Rules
- cargo only through `bash C:/GTAVI/Tools/cargo-bg.sh ...` with `CARGO_TARGET_DIR` set to `<your copy>\target`,
  `--offline --locked`, profile `diag`. Never launch, kill or touch the game or victory.exe. Never use `start`.
  No git push; commit locally in your copy as you go (small commits, clear messages) so main can cherry-pick.
- `C:\GTAVI\GTA6` is read-only. Nothing from `ref-forks\*` is built or run.
- cool_fox is FASTER than our tree. So: port what it LACKS; where it has its own mechanism for the same job, keep ITS
  mechanism and do not add ours on top unless ours is provably additive. Never replace a cool_fox path.
- Every ported behaviour: a `VICTORY_*` switch with the same name as in victory-dev, the old cool_fox path kept, a
  counter/log line. Defaults: correctness and picture fixes ON; meters OFF; speed mechanisms that overlap something
  cool_fox already does OFF until an A/B run; tooling (unattended input, caption, frame log, hang watch) ON as in ours.
- No change may trade picture for speed (no skipped passes, no lower resolution, no Series S, no stand-in light).
- Read first: `docs\ghidra-ledger.md` (what was measured, adopted, REJECTED - do not port rejected things),
  `docs\cool-fox-parts\*.md` (what cool_fox has; `base-speed.md`, `base-memory-visual.md`), `docs\silent-defects-audit.md`,
  `docs\frame-pipeline-analysis.md`, `docs\vram-plan.md`.

## Method (each coder)
1. Inventory: list every feature/switch of ours in your area (grep `VICTORY_[A-Z0-9_]+` in the diff of the old tree) and
   classify against the new base: `present` (same switch or same mechanism - name it), `lacking`, `conflicts`
   (cool_fox does it differently - describe), `rejected/obsolete` (ledger). Write the table to your port log first.
2. Port the `lacking` rows in priority order (below per coder); build after each group; commit.
3. Port our unit tests for what you port; run the touched crates' tests at the end.
4. Port log: `docs\fox-port-<A|B|C>.md`: the inventory table with final status per row, switches and defaults, log lines,
   what was NOT ported and why, what needs a game run to judge.
5. SendMessage to "main" (under 120 words) after the inventory, after each built group, and at the end.

## File ownership (strict; ask main before touching anything else)
- A: `crates/victory-hle/**`, `crates/victory-loader/**`, `crates/victory-core/**`, `crates/victory-pe/**`, `apps/**`,
  and in victory-gpu ONLY `src/frame.rs` (+ a new file of your own if needed, with a one-line `mod` in lib.rs: tell main).
- B: `crates/victory-shader/**`, `crates/victory-formats/**`, and in victory-gpu ONLY `src/shader_cache.rs`.
- C: `crates/victory-gpu/**` except `src/frame.rs` and `src/shader_cache.rs`; `crates/victory-pipeline/**`.
