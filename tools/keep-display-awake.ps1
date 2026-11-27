# While victory.exe runs: keeps the display and the system awake (a sleeping display makes Windows throttle
# presentation to ~6 frames a second, which ruins a measured run). Wakes the display once at the start with a
# one-pixel mouse move and back. Ends when the game exits.
param([int]$WaitForGameS = 120)
Add-Type @'
using System; using System.Runtime.InteropServices;
public static class Awake {
  [DllImport("kernel32.dll")] public static extern uint SetThreadExecutionState(uint f);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, UIntPtr e);
}
'@
[void][Awake]::SetThreadExecutionState(0x80000003)   # CONTINUOUS | SYSTEM_REQUIRED | DISPLAY_REQUIRED
[Awake]::mouse_event(1, 1, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 50; [Awake]::mouse_event(1, -1, 0, 0, [UIntPtr]::Zero)
$deadline = (Get-Date).AddSeconds($WaitForGameS); $game = $null
while (-not $game -and (Get-Date) -lt $deadline) { $game = Get-Process victory -ErrorAction SilentlyContinue | Select-Object -First 1; if (-not $game) { Start-Sleep -Milliseconds 500 } }
while ($game -and -not $game.HasExited) { [void][Awake]::SetThreadExecutionState(0x80000003); Start-Sleep -Seconds 20 }
[void][Awake]::SetThreadExecutionState(0x80000000)
