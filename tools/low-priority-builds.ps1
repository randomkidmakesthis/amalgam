# While the game runs, keep every build/test process (cargo, rustc, linkers, test binaries under par\ or victory-dev)
# at the lowest CPU priority so they do not disturb the game or its measurements. Ends when the game ends.
param([int]$MaxSeconds = 900)
$names = 'cargo','rustc','rust-lld','lld-link','link','clang','clang-cl','rustdoc','build-script-build','llvm-lib'
$start = Get-Date; $seen = 0
while (((Get-Date) - $start).TotalSeconds -lt $MaxSeconds) {
    $game = Get-Process victory -ErrorAction SilentlyContinue | Where-Object { $_.Path -like '*\victory-run\*' }
    if (-not $game -and ((Get-Date) - $start).TotalSeconds -gt 150) { break }
    $procs = Get-Process -ErrorAction SilentlyContinue | Where-Object {
        ($names -contains $_.ProcessName) -or
        ($_.Path -and ($_.Path -like 'C:\GTAVI\Tools\par\*' -or $_.Path -like 'C:\GTAVI\Tools\victory-dev\target\*' -or $_.Path -like 'C:\GTAVI\Tools\victory-faces\target\*'))
    }
    foreach ($p in $procs) { try { if ($p.PriorityClass -ne 'Idle') { $p.PriorityClass = 'Idle'; $seen++ } } catch {} }
    Start-Sleep -Seconds 2
}
"lowered $seen build processes to Idle priority"
