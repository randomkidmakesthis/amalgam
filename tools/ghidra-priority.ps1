# Keeps Ghidra's analysis process at High priority while no game runs and at Idle while victory.exe runs.
# Ends when no Ghidra process is left.
param([int]$EverySeconds = 3)
$last = ''
while ($true) {
  $ghidra = Get-CimInstance Win32_Process -Filter "Name='java.exe'" | Where-Object { $_.CommandLine -match 'ghidra' }
  if (-not $ghidra) { break }
  $want = if (Get-Process victory -ErrorAction SilentlyContinue) { 'Idle' } else { 'High' }
  foreach ($g in $ghidra) {
    try { $p = Get-Process -Id $g.ProcessId -ErrorAction Stop; if ($p.PriorityClass -ne $want) { $p.PriorityClass = $want } } catch {}
  }
  if ($want -ne $last) { "$(Get-Date -Format HH:mm:ss) ghidra priority -> $want" | Add-Content 'C:\GTAVI\Tools\ghidra-proj\priority.log'; $last = $want }
  Start-Sleep -Seconds $EverySeconds
}
"$(Get-Date -Format HH:mm:ss) ghidra gone; watcher ends" | Add-Content 'C:\GTAVI\Tools\ghidra-proj\priority.log'
