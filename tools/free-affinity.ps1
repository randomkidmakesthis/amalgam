# Test aid for finding F2: lifts the console affinity masks the game puts on its threads.
# While victory.exe runs, every thread whose affinity is narrower than the process's is reset to the full mask.
# Mode 'free' = all logical processors. Mode 'spread' = the five recorder threads (mask 0x1f00) get one hardware
# thread on each of five physical cores (LPs 6,8,10,12,14 = 0x5540); every other narrowed thread is freed.
param([ValidateSet('free','spread')][string]$Mode = 'free', [int]$WaitForGameS = 120, [string]$Log = 'C:\GTAVI\Tools\victory-data\affinity.log')

Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class Aff {
  [DllImport("kernel32.dll")] public static extern IntPtr OpenThread(uint access, bool inherit, uint id);
  [DllImport("kernel32.dll")] public static extern UIntPtr SetThreadAffinityMask(IntPtr h, UIntPtr mask);
  [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr h);
}
'@

$deadline = (Get-Date).AddSeconds($WaitForGameS); $game = $null
while (-not $game -and (Get-Date) -lt $deadline) { $game = Get-Process victory -ErrorAction SilentlyContinue | Select-Object -First 1; if (-not $game) { Start-Sleep -Milliseconds 300 } }
if (-not $game) { exit 1 }
$full = [UInt64]$game.ProcessorAffinity.ToInt64()
$seen = @{}; $changed = 0; $recorders = 0
"$(Get-Date -Format HH:mm:ss) affinity mode $Mode, process mask 0x$($full.ToString('x'))" | Out-File $Log -Encoding utf8
while (-not $game.HasExited) {
  $game.Refresh()
  foreach ($t in $game.Threads) {
    if ($seen.ContainsKey($t.Id)) { continue }
    $h = [Aff]::OpenThread(0x0260, $false, [uint32]$t.Id)   # SET_INFORMATION | QUERY_INFORMATION | SET_LIMITED
    if ($h -eq [IntPtr]::Zero) { continue }
    # Setting the full mask returns the previous one: that is how the old mask is read.
    $old = [UInt64][Aff]::SetThreadAffinityMask($h, [UIntPtr]$full).ToUInt64()
    if ($old -ne 0 -and $old -ne $full) {
      $changed++
      if ($Mode -eq 'spread' -and $old -eq 0x1f00) { [void][Aff]::SetThreadAffinityMask($h, [UIntPtr][UInt64]0x5540); $recorders++ }
      "$(Get-Date -Format HH:mm:ss) thread $($t.Id): 0x$($old.ToString('x')) lifted" | Add-Content $Log -Encoding utf8
    }
    [void][Aff]::CloseHandle($h); $seen[$t.Id] = 1
  }
  Start-Sleep -Milliseconds 500
}
"$(Get-Date -Format HH:mm:ss) done: $changed threads lifted, $recorders recorder threads spread" | Add-Content $Log -Encoding utf8
