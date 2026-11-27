# Records gamepad (XInput) and keyboard presses while victory.exe runs, with times in seconds since the
# game process started. Writes one line per press/release. Stops when the game exits.
param([string]$Out = 'C:\GTAVI\Tools\victory-data\input-recording.txt', [int]$WaitForGameS = 120)

Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class Rec {
  [StructLayout(LayoutKind.Sequential)] public struct Pad { public ushort Buttons; public byte LT; public byte RT; public short LX; public short LY; public short RX; public short RY; }
  [StructLayout(LayoutKind.Sequential)] public struct State { public uint Packet; public Pad Pad; }
  [DllImport("xinput1_4.dll")] public static extern uint XInputGetState(uint index, out State state);
  [DllImport("user32.dll")] public static extern short GetAsyncKeyState(int vk);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  // All watched keys in one call: bit i set = vks[i] held. (Polling them one by one from PowerShell, 22 calls
  // every 8 ms, cost tens of percent of a core, and only while the game had the focus.)
  public static ulong KeyMask(int[] vks) { ulong m = 0; for (int i = 0; i < vks.Length; i++) if ((GetAsyncKeyState(vks[i]) & 0x8000) != 0) m |= 1UL << i; return m; }
}
'@

$deadline = (Get-Date).AddSeconds($WaitForGameS)
$game = $null
while (-not $game -and (Get-Date) -lt $deadline) { $game = Get-Process victory -ErrorAction SilentlyContinue | Select-Object -First 1; if (-not $game) { Start-Sleep -Milliseconds 200 } }
if (-not $game) { 'no game process found' | Out-File $Out -Encoding utf8; exit 1 }
$start = $game.StartTime
"# recording started; times are seconds since the game process started ($($start.ToString('HH:mm:ss')))" | Out-File $Out -Encoding utf8

$padBits = [ordered]@{ 0x0001='Up'; 0x0002='Down'; 0x0004='Left'; 0x0008='Right'; 0x0010='Start'; 0x0020='Back'; 0x0040='LS'; 0x0080='RS';
  0x0100='LB'; 0x0200='RB'; 0x1000='A'; 0x2000='B'; 0x4000='X'; 0x8000='Y' }
$keys = [ordered]@{ 0x0D='Enter'; 0x1B='Esc'; 0x20='Space'; 0x08='Backspace'; 0x09='Tab'; 0x25='ArrowLeft'; 0x26='ArrowUp'; 0x27='ArrowRight'; 0x28='ArrowDown';
  0x57='W'; 0x41='A-key'; 0x53='S'; 0x44='D'; 0x45='E'; 0x51='Q'; 0x46='F'; 0x52='R'; 0x58='X-key'; 0x5A='Z'; 0x43='C'; 0x10='Shift'; 0x11='Ctrl' }

$vks = [int[]]@($keys.Keys); $keyNames = @($keys.Values); $lastMask = [uint64]0
$held = @{}
function Note([string]$name, [bool]$down, [bool]$focus) {
  $t = ((Get-Date) - $start).TotalSeconds
  $was = $held.ContainsKey($name)
  if ($down -and -not $was) { $held[$name] = $t; ('{0,8:N2}  down  {1}{2}' -f $t, $name, $(if ($focus) { '' } else { '   (game window not focused)' })) | Add-Content $Out -Encoding utf8 }
  elseif (-not $down -and $was) { ('{0,8:N2}  up    {1}   (held {2:N2} s)' -f $t, $name, ($t - $held[$name])) | Add-Content $Out -Encoding utf8; $held.Remove($name) }
}

while (-not $game.HasExited) {
  $fgPid = 0; [void][Rec]::GetWindowThreadProcessId([Rec]::GetForegroundWindow(), [ref]$fgPid)
  $focus = ($fgPid -eq $game.Id)
  for ($i = 0; $i -lt 4; $i++) {
    $s = New-Object Rec+State
    if ([Rec]::XInputGetState([uint32]$i, [ref]$s) -eq 0) {
      foreach ($e in $padBits.GetEnumerator()) { Note "pad$i $($e.Value)" (($s.Pad.Buttons -band $e.Key) -ne 0) $focus }
      Note "pad$i LT" ($s.Pad.LT -gt 100) $focus; Note "pad$i RT" ($s.Pad.RT -gt 100) $focus
      Note "pad$i stick Up" ($s.Pad.LY -gt 20000) $focus; Note "pad$i stick Down" ($s.Pad.LY -lt -20000) $focus
      Note "pad$i stick Left" ($s.Pad.LX -lt -20000) $focus; Note "pad$i stick Right" ($s.Pad.LX -gt 20000) $focus
    }
  }
  if ($focus) {
    $mask = [Rec]::KeyMask($vks)
    if ($mask -ne $lastMask) { for ($k = 0; $k -lt $vks.Length; $k++) { Note "key $($keyNames[$k])" (($mask -band ([uint64]1 -shl $k)) -ne 0) $true }; $lastMask = $mask }
  }
  Start-Sleep -Milliseconds 8
}
'# game exited' | Add-Content $Out -Encoding utf8
