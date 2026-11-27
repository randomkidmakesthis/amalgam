# Why the game runs faster when its window is NOT focused (2026-11-26)

User report: clicking away from the game window makes it faster. Older runs logged no focus state, so this
ranking comes from reading the code and settings, not from a measured A/B. The new caption fields (below) settle it.

## What the logs show (run-102, -105, -106, scene, 5-s `usual frame` means)
- The frame time follows the submit thread: `submit thread: at work` is roughly the frame time
  (e.g. 45 of 47 ms). The card's idle time is small (2-20 ms), and the render thread's `in the present call`
  is 0-4 ms. So a focus effect has to slow a CPU thread, or the host Present inside the submit thread
  (not metered separately).
- run-106 08:59:07-43: `in the present call` was 140-150 ms and frames took 215 ms. That is Windows throttling
  presents (the display was off or the window hidden; that run had no keep-display-awake). It slows the
  *unfocused/hidden* case, so it is not this effect.
- run-102 input.txt shows Shift+S presses at 120 s and 283 s while the game had focus (Win+Shift+S, the
  snipping tool), so the user does interact during runs. No run recorded when focus changed.

## Ranked causes
1. **Xbox Game Bar treats our exe as a game and records it in the background (applies, strongest).**
   `HKCU\System\GameConfigStore\Children` has `C:\GTAVI\Tools\victory-run\target\diag\victory.exe` (Flags=17).
   `HKCU\...\GameDVR` has AppCaptureEnabled=1 and **HistoricalCaptureEnabled=1** ("Record what happened").
   Windows captures and encodes only the *foreground* game, and Game Mode also applies only while it has focus.
   So focused = capture copies, an encoder, and extra DWM work on top of our frame. Unfocused = none.
   Fix (no code, user setting, the picture does not change): Settings > Gaming > Captures > turn off
   "Record what happened", or Win+G on the game > untick "Remember this is a game" (each build's exe is its own entry).
2. **Foreground quantum stretching (applies, plausible).** Win32PrioritySeparation=2 (client default: short,
   variable, foreground 3:1). Every thread of the foreground process gets ~6 clock ticks (~94 ms) per quantum
   instead of ~2 (~31 ms). The title's threads keep console affinity masks (affinity.rs, default `console`)
   and set their own priorities (kernel.rs:429 passes them through, runtime.rs:288 makes the process
   AboveNormal). When a spinning title thread and a working thread share a logical processor at equal
   priority, the working thread waits up to 3x longer when focused. Test: set Win32PrioritySeparation to 0x24
   (36: short, variable, no foreground boost; works at once, no reboot; put back 2 afterwards). This is a
   system setting, so the user decides. A layer-side fix would mean changing the title's spin-waits (large).
3. **Presentation mode can change with focus (applies, likely small).** host.rs:9405-9419: FLIP_DISCARD,
   BUFFERS=2 (host.rs:8731), no ALLOW_TEARING swap-chain flag, no waitable object, default frame latency.
   host.rs:9375 calls `Present(0, 0)`: sync interval 0, no tearing flag. It runs on the submit lane
   (queue/behind.rs:584, show_planes). A focused window can be promoted to MPO/independent flip, or to
   G-SYNC windowed mode (NVIDIA applies that only to the focused window). Then flips wait for vblank.
   Unfocused windows are DWM-composed and Present returns at once. At 15-50 fps on 144 Hz this costs at most
   about 7 ms a frame. The caption FPS counts host presents (meter after Present, host.rs:9379), the same way
   in both cases. Proposed switch: `VICTORY_PRESENT_TEARING=1` (CheckFeatureSupport, then the swap-chain flag
   ALLOW_TEARING plus `DXGI_PRESENT_ALLOW_TEARING` on sync-0 presents) and `VICTORY_PRESENT_BUFFERS=3`,
   ~30 lines in host.rs, default = today's behaviour. Not built yet: the logs show no present cost to remove.
   First measure the host Present with `VICTORY_TIMELINE` (the span "the host's Present", host.rs:9373),
   once focused and once not.
4. **Our input recorder polled keys only while the game had focus (applied, small; FIXED).**
   record-input.ps1 ran 22 `Note` calls every 8 ms when the game was foreground. Timed: 1.3 ms a loop, about
   14 % of a core. Measured during today's run: 32 CPU-s in 298 s on average, 1 % while unfocused. Now one C#
   call reads all keys and `Note` runs only when the mask changes. The output format is unchanged. It is not
   a hook; no low-level hook is installed anywhere.
5. **The layer's keyboard poll (applies, negligible).** gameinput.rs:1619-1626: while focused, the 8-ms hub
   thread (gameinput.rs:820) calls GetAsyncKeyState 244 times, about 30k calls/s (<1 % of a core).
   host/panel.rs:52 checks F3 once a frame. No fix needed.

## Checked, does not apply
- The title is not told about focus. lifecycle.rs:243-278 handles only SC_CLOSE and SC_KEYMENU. WM_ACTIVATE,
  WM_ACTIVATEAPP and the focus messages go to the title's own procedure, unchanged. The title imports only
  CreateWindowExA, DefWindowProcA, RegisterClassExA, ShowWindow and Peek/Translate/DispatchMessage: no raw
  input, ClipCursor, ShowCursor, SetCapture or foreground calls. No constrained/suspend routine runs on focus
  changes; they run only from end_title (lifecycle.rs:282). That is called on a user close (lifecycle.rs:252)
  or by XLaunchNewGame (xgame.rs:86). run-99 ("title suspended; ending the title", lifecycle.rs:310) began
  with "window closed", i.e. the window was closed (close button or Alt+F4), not a focus change. No picture
  loss is involved.
- Mouse: GameInput reports no mouse (gameinput.rs:1496). WM_MOUSEMOVE does not depend on focus and is
  coalesced.
- Our scripts: watch-window.ps1 never calls SetForegroundWindow or SendInput, and takes one window shot
  every 30 s either way. keep-display-awake.ps1 calls SetThreadExecutionState every 20 s and moves the mouse
  by 1 px once at the start. builds-yield.ps1 runs Get-Process every 2 s and keeps builds Idle on LPs 12-15.
  None of them depends on focus.
- Core classes: the 9800X3D has none, and the background state does not move threads between cores. Power
  throttling (EcoQoS) would slow the *background* case.

## Confirming in the next run
watch-window.ps1 now appends ` | fg=1|0 cpu=N% prio=<class>` to every caption sample (2-s timing unchanged;
the older fields keep their places for run-compare, input-check, run-freezes and live-status). Then:
`bash run-focus.sh <label>` prints the median ms per draw band (1-2.5k, 2.5-4k, >=4k), sample counts and
mean CPU for fg=1 and fg=0. Spend about a minute in the scene each way. Expect, if cause 1 is right:
fg=1 slower at equal draws, process CPU about the same (the capture runs in other processes), and the gap
gone after "Record what happened" is off. If cause 2 is right: the gap stays with recording off and closes
with Win32PrioritySeparation=0x24. prio should read AboveNormal in both states.
