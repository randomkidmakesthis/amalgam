# While victory.exe runs: keeps every build process (cargo, rustc, linkers, test binaries of the build) at Idle
# priority and confined to logical processors 12-15 (two physical cores the game then rarely needs), so agents can
# compile during a measured run. Writes how much build activity overlapped the run. Ends when the game exits.
param([string]$Log = 'C:\GTAVI\Tools\victory-data\build-activity.txt', [int]$WaitForGameS = 120)
$names = 'cargo','rustc','rust-lld','lld-link','link','clang-cl','clang','llvm-lib','build-script-build'
$mask = [IntPtr]0xF000
$deadline = (Get-Date).AddSeconds($WaitForGameS); $game = $null
while (-not $game -and (Get-Date) -lt $deadline) { $game = Get-Process victory -ErrorAction SilentlyContinue | Select-Object -First 1; if (-not $game) { Start-Sleep -Milliseconds 500 } }
if (-not $game) { exit 1 }
$samples = 0; $busy = 0; $cpu0 = @{}; $cpu = 0.0
while (-not $game.HasExited) {
  $samples++
  $b = Get-Process -Name $names -ErrorAction SilentlyContinue
  if ($b) {
    $busy++
    foreach ($p in $b) {
      try { if ($p.PriorityClass -ne 'Idle') { $p.PriorityClass = 'Idle' }; if ($p.ProcessorAffinity -ne $mask) { $p.ProcessorAffinity = $mask } } catch {}
      try { $t = $p.TotalProcessorTime.TotalSeconds; if ($cpu0.ContainsKey($p.Id)) { $cpu += [math]::Max(0, $t - $cpu0[$p.Id]) }; $cpu0[$p.Id] = $t } catch {}
    }
  }
  Start-Sleep -Seconds 2
}
"build activity during the run: build processes seen in $busy of $samples samples (2 s apart); about $([math]::Round($cpu)) CPU-seconds, confined to logical processors 12-15 at Idle priority" | Out-File $Log -Encoding utf8
