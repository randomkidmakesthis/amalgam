# Watches a game layer's process: samples its window caption, saves a picture of ITS window only
# (client area, nothing else on screen), and ends early if the process exits or stands still.
param(
    [string]$ProcessName = 'victory',
    [int]$Seconds = 300,
    [string]$OutDir = '.',
    [int]$StallSeconds = 90,
    [int]$ShotEvery = 30
)
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class WinShot {
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
    [DllImport("user32.dll")] public static extern bool IsHungAppWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
}
'@
# Each caption line ends with ` | fg=1|0 cpu=N% prio=<class>` (appended last so the older fields keep their places):
# fg = the foreground window is the game's (its main window, or any window of its process), cpu = the process's CPU
# time since the previous sample over the wall time between them, summed over all cores (100% = one core).
$captions = Join-Path $OutDir 'captions.txt'
$shots = Join-Path $OutDir 'shots'
New-Item -ItemType Directory -Force $shots | Out-Null
$start = Get-Date; $lastCpu = -1.0; $lastMove = Get-Date; $lastShot = $start.AddSeconds(-$ShotEvery); $seen = $false
$sampleCpu = -1.0; $sampleAt = $start; $samplePid = 0
while ($true) {
    $t = [int]((Get-Date) - $start).TotalSeconds
    if ($t -ge $Seconds) { "time limit reached at $t s"; break }
    $p = Get-Process $ProcessName -ErrorAction SilentlyContinue | Sort-Object WorkingSet64 -Descending | Select-Object -First 1
    if (-not $p) {
        if ($seen -or $t -gt 20) { "process gone at $t s"; break }
        Start-Sleep -Milliseconds 500; continue
    }
    $seen = $true
    $cpu = $p.TotalProcessorTime.TotalSeconds
    if ([math]::Abs($cpu - $lastCpu) -gt 0.05) { $lastMove = Get-Date; $lastCpu = $cpu }
    $idle = [int]((Get-Date) - $lastMove).TotalSeconds
    $hung = $false
    if ($p.MainWindowHandle -ne [IntPtr]::Zero) { $hung = [WinShot]::IsHungAppWindow($p.MainWindowHandle) }
    $ws = [int]($p.WorkingSet64 / 1MB); $pm = [int]($p.PrivateMemorySize64 / 1MB)
    $fgw = [WinShot]::GetForegroundWindow(); $fgPid = [uint32]0; [void][WinShot]::GetWindowThreadProcessId($fgw, [ref]$fgPid)
    $fg = if (($p.MainWindowHandle -ne [IntPtr]::Zero -and $fgw -eq $p.MainWindowHandle) -or $fgPid -eq [uint32]$p.Id) { 1 } else { 0 }
    $now = Get-Date
    $cpuPct = if ($sampleCpu -ge 0 -and $samplePid -eq $p.Id -and ($now - $sampleAt).TotalSeconds -gt 0.1) { [int](100 * ($cpu - $sampleCpu) / ($now - $sampleAt).TotalSeconds) } else { '-' }
    $sampleCpu = $cpu; $sampleAt = $now; $samplePid = $p.Id
    $prio = try { "$($p.PriorityClass)" } catch { '?' }
    Add-Content -Path $captions -Value ("{0,4}s | ws {1} MB, private {2} MB{3} | {4} | fg={5} cpu={6}% prio={7}" -f $t, $ws, $pm, $(if ($hung) { ', WINDOW NOT RESPONDING' } else { '' }), $p.MainWindowTitle, $fg, $cpuPct, $prio)
    if ($idle -ge $StallSeconds) { "process used no CPU for $idle s (stalled) at $t s"; break }
    if (((Get-Date) - $lastShot).TotalSeconds -ge $ShotEvery -and $p.MainWindowHandle -ne [IntPtr]::Zero -and -not [WinShot]::IsIconic($p.MainWindowHandle)) {
        $r = New-Object WinShot+RECT; $o = New-Object WinShot+POINT
        if ([WinShot]::GetClientRect($p.MainWindowHandle, [ref]$r) -and [WinShot]::ClientToScreen($p.MainWindowHandle, [ref]$o) -and $r.R -gt 16 -and $r.B -gt 16) {
            try {
                $bmp = New-Object System.Drawing.Bitmap $r.R, $r.B
                $g = [System.Drawing.Graphics]::FromImage($bmp)
                $g.CopyFromScreen($o.X, $o.Y, 0, 0, $bmp.Size)
                $bmp.Save((Join-Path $shots ("shot_{0:D4}.png" -f $t)), [System.Drawing.Imaging.ImageFormat]::Png)
                $g.Dispose(); $bmp.Dispose()
            } catch { }
        }
        $lastShot = Get-Date
    }
    Start-Sleep -Seconds 2
}
