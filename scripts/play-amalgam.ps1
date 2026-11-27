# Amalgam: the foxvic-scarlett build (foxhound + scarlett merged) with the settings
# that measured best across four forks:
#
#   foxhound            DLSS/1440p/ambient, WC_NO_BACK, soft views, levels-on-demand=0
#   scarlett            levels-on-demand=0, soft views, bake/pipeline waits, shader precompile
#   Victory-Royale      port constants/journal, 8 translators, card snapshots, 1080p output,
#                       Series S + FSR 4.1.2 profile (not in this core), CCD-neutral thread counts
#   hle-d3d12-7900xtx   SKIP_WC_BACK (= our WC_NO_BACK), ASK_EARLY, texture stage, CCD
#   Dark-Victory        Series S / 8 GB-VRAM caps (card-buffer sizes), fullscreen
#
#   .\scripts\play-amalgam.ps1 -Exe 'D:\MyDump\game_scarlett_master.exe'
#   ... -SeriesS                     the 1280x720-inside profile (faster, softer)
#   ... -OneHome small|buffers|both  the single-home buffer experiment (see below)
#   ... -Log run.log                 also write the console to a file (run-summary reads it)
#   ... -More @{VICTORY_X='1'}       add or override any switch
#
# -OneHome: one "real" location per buffer instead of a RAM copy + a card copy synced
# every frame.
#   small   (VICTORY_INPLACE=1, WC_BAR=1, WC_NO_BACK=1): title memory outside buffers that
#           lies in private committed memory is opened as a host heap and read by the card
#           in place; write-combined title buffers get one copy in card memory the CPU maps
#           (D3D12_HEAP_TYPE_GPU_UPLOAD); shader writes into big write-combined copies stay
#           on the card. Nothing else changes: unplaced memory does the full round trip.
#   buffers (VICTORY_INPLACE=2 as well): title buffers too, except the big ones in card
#           memory. Do not combine with VICTORY_LATE_WAIT.
#   both    small + buffers + VICTORY_WINDOWS_BAR_MB=2048 (windows in BAR memory; foxhound
#           measured this a wash on a 4070 SUPER, kept off by default).
#
# Every VICTORY_... switch is described in a comment where the code reads it.
param(
    [Parameter(Mandatory)][string]$Exe,
    [ValidateSet('diag', 'release')][string]$Build = 'diag',
    [switch]$SeriesS,
    [ValidateSet('1440', '1080to1440', '4k')][string]$Res = '1440',
    [ValidateSet('off', 'dlaa', 'quality', 'balanced', 'performance')][string]$Dlss = 'off',
    [switch]$FrameGen,
    [ValidateRange(1, 1000)][int]$Fps = 0,
    [ValidateSet('off', 'small', 'buffers', 'both')][string]$OneHome = 'off',
    # With -OneHome: place windows that descriptor slots show as texture memory
    # too. Their placed copies are made due again after the card writes the
    # home (the reference's after-wait rule), two frames on.
    [switch]$OneHomeTextures,
    [ValidateRange(1, 64)][int]$JoinParts = 8,
    [ValidateRange(0, 1000)][int]$StageMs = 16,
    [ValidateRange(0, 64)][int]$InPlaceMb = 4096,
    [switch]$Safe,
    [string]$Cache = '',
    [string]$Data = '',
    [string]$Log = '',
    [string]$Keys = '',
    [hashtable]$More = @{}
)
$repo = Split-Path $PSScriptRoot -Parent

# The foxhound set (the core's tested defaults), then the cross-fork additions.
$settings = @{
    VICTORY_THIN = '1'; VICTORY_JOIN_PARTS = "$JoinParts"; VICTORY_RELAY = '2'; VICTORY_RELAY_COPY = '1'
    VICTORY_WIDE_BACK = '1'; VICTORY_CARD_REHASH_RUNS = '1'; VICTORY_JOIN_COPIES = '1'
    VICTORY_CARD_STEPS = '2'; VICTORY_CARD_AWAKE = '1'; VICTORY_CARD_BUFFERS_MIN_KB = '64'
    VICTORY_INDIRECT_WINDOW = '1'; VICTORY_INDIRECT_BATCH = '2'
    VICTORY_UPLOAD_AHEAD = '1'; VICTORY_SHOW_AFTER = '1'; VICTORY_MAP_THREADS = '1'
    # Big write-combined copies stay on the card (foxhound / hle SKIP_WC_BACK, -5 ms a frame)
    VICTORY_WC_NO_BACK = '1'; VICTORY_WC_NO_BACK_MIN_KB = '1024'
    # Page-mapping copies kept without Windows views (no freezes at scene cuts)
    VICTORY_SOFT_VIEWS = 'copies'
    VICTORY_TEXTURE_POOL = '1'; VICTORY_RIPE_BLOCKS = '0'; VICTORY_TEXTURE_NO_WAIT = '1'; VICTORY_TEXTURE_CATCHUP = '0'
    VICTORY_TEXTURE_STAGE_MS = "$StageMs"; VICTORY_LEVELS_ON_DEMAND = '0'
    VICTORY_PRELUDE_UPLOADS = '1'; VICTORY_SHOW_NEW_LATER = '1'; VICTORY_POOL_SMALL = '0'; VICTORY_TABLED_REREAD = '1'
    VICTORY_QUEUE_HOST_TEXTURES = '0'
    VICTORY_HEAP_SYNC_FIRST = '1'; VICTORY_SYNC_IN_PLACE = '1'; VICTORY_SPANS_QUICK = '1'; VICTORY_SYNC_FINE = '1'
    VICTORY_HEAP_SYNC_AT_SUBMIT = '1'
    VICTORY_AMBIENT = '0.8'; VICTORY_AMBIENT_AIR = '1.5'; VICTORY_AMBIENT_SPECULAR = '1'
    VICTORY_EARLY_DEPTH = '1'
    VICTORY_LEAVE_OUT = '02384a60fda6f2d0,731a39171b568298'
    VICTORY_PIPELINE_BOOK = '1'
    # From hle-d3d12-7900xtx: ask once for the next submission's images, not twice
    VICTORY_ASK_EARLY = '2'
    # Renderer port #6's fault half, our fork's form: a write fault takes no
    # guards' lock (the block's bit and the protection change; the next holder
    # of the lock settles it). 510 faults a frame waited 48 ms for that lock
    # without it on the test PC.
    VICTORY_FAULT_UNLOCKED = '1'
    # Victory-Royale: 1 MiB+ buffer copies compared on the card; no address-space walk per memory line
    VICTORY_CARD_BUFFERS_MIN_MB = '1'; VICTORY_MEMORY_MAP = '0'
}
# Victory-Royale's 8-core tuning: thread counts the pools are sized from.
if ($env:NUMBER_OF_PROCESSORS -and [int]$env:NUMBER_OF_PROCESSORS -ge 8) {
    $settings.VICTORY_THIN_THREADS = '8'; $settings.VICTORY_LAND_THREADS = '4'
    $settings.VICTORY_UNDER_THREADS = '4'; $settings.VICTORY_VIEW_THREADS = '4'
}
if ($Safe) {
    # The first thing to try when something is wrong: no thin lists, no relay, no on-card steps.
    foreach ($k in @('VICTORY_THIN', 'VICTORY_RELAY', 'VICTORY_RELAY_COPY', 'VICTORY_CARD_STEPS',
                     'VICTORY_JOIN_COPIES', 'VICTORY_INDIRECT_WINDOW', 'VICTORY_INDIRECT_BATCH',
                     'VICTORY_ASK_EARLY', 'VICTORY_WC_NO_BACK', 'VICTORY_FAULT_UNLOCKED')) {
        $settings.Remove($k)
    }
}
if (-not $SeriesS) {
    if ($Res -eq '1440') { $settings.VICTORY_NATIVE_1440P = '1' }
    elseif ($Res -eq '1080to1440') { $settings.VICTORY_RENDER_1080P = '1' }
}
if ($Dlss -ne 'off') {
    $sl = if ($env:VICTORY_STREAMLINE_DIR) { $env:VICTORY_STREAMLINE_DIR } else { 'C:\dev\sdk\streamline-sdk-v2.14.16\bin\x64' }
    if (Test-Path (Join-Path $sl 'sl.interposer.dll')) {
        $settings.VICTORY_DLSS = '1'; $settings.VICTORY_DLSS_MODE = $Dlss; $settings.VICTORY_STREAMLINE_DIR = $sl
        if ($FrameGen) { $settings.VICTORY_FRAMEGEN = '1' }
    } else {
        Write-Host "No NVIDIA Streamline in $sl (set VICTORY_STREAMLINE_DIR, see README): running without DLSS."
    }
}
if ($Fps -gt 0) { $settings.VICTORY_FRAME_INTERVAL_US = [string][int][math]::Round(1000000 / $Fps) }

# The single-home buffer experiment. It is a direct-memory mode: the relay and
# the late waits are turned off too (VICTORY_RELAY, VICTORY_RELAY_COPY removed),
# because with flights in the air a home the card read before a landing holds
# older bytes -- the reference memory core refuses the same combination. Pass
# -More @{VICTORY_RELAY='2';VICTORY_INPLACE_RELAY='1'} to override knowingly.
switch ($OneHome) {
    'small' {
        $settings.VICTORY_INPLACE = '1'; $settings.VICTORY_INPLACE_MB = "$InPlaceMb"
        $settings.VICTORY_WC_BAR = '1'
    }
    'buffers' {
        $settings.VICTORY_INPLACE = '2'; $settings.VICTORY_INPLACE_MB = "$InPlaceMb"
        $settings.VICTORY_WC_BAR = '1'
    }
    'both' {
        $settings.VICTORY_INPLACE = '2'; $settings.VICTORY_INPLACE_MB = "$InPlaceMb"
        $settings.VICTORY_WC_BAR = '1'; $settings.VICTORY_WINDOWS_BAR_MB = '2048'
    }
}
if ($OneHomeTextures -and $OneHome -ne 'off') {
    $settings.VICTORY_INPLACE_TEXTURES = '1'
    Write-Host 'one-home: texture windows placed too (placed copies re-uploaded after card writes)'
}
if ($OneHome -ne 'off' -and -not $More.ContainsKey('VICTORY_RELAY')) {
    # shadow::relay() is on by default: 0 turns it off, removing it would not.
    $settings.VICTORY_RELAY = '0'
    $settings.Remove('VICTORY_RELAY_COPY')
}
foreach ($k in $More.Keys) { $settings[$k] = $More[$k] }
# Unattended runs: <seconds>:<key> presses (0.2 s each), <from>-<to>:<key> holds.
#   e.g. "5:Enter,7:Enter,80-300:w"
if ($Keys) { $settings.VICTORY_KEYS = $Keys }
$console = if ($SeriesS) { 'series-s' } else { 'series-x' }

$victory = Join-Path $repo "target\$Build\victory.exe"
if (-not (Test-Path $victory)) { Write-Host "No $victory. Build it first (README)."; return }
if (-not (Test-Path -LiteralPath $Exe)) { Write-Host "No $Exe."; return }

# Paths: env vars for this run win; victory.toml gives them otherwise.
if ($Cache) { $env:VICTORY_CACHE_DIR = $Cache }
if ($Data)  { $env:VICTORY_DATA_DIR  = $Data }
$paths = 'VICTORY_CONFIG', 'VICTORY_GAME_DIR', 'VICTORY_CACHE_DIR', 'VICTORY_DATA_DIR', 'VICTORY_STREAMLINE_DIR'
Get-ChildItem Env:VICTORY_* -ErrorAction SilentlyContinue | Where-Object { $_.Name -notin $paths } | ForEach-Object { Remove-Item "Env:$($_.Name)" }
foreach ($k in $settings.Keys) { Set-Item "Env:$k" $settings[$k] }

$lines = New-Object System.Collections.Generic.List[string]
$lines.Add("# console=$console build=$Build onehome=$OneHome switches: " + (($settings.GetEnumerator() | Sort-Object Name | ForEach-Object { "$($_.Name)=$($_.Value)" }) -join ' '))
Write-Host ("amalgam: console={0} build={1} onehome={2}" -f $console, $Build, $OneHome)

Push-Location $repo   # victory.toml is read from here
try {
    & $victory run $Exe --console $console --execute 2>&1 | ForEach-Object {
        $line = "$_"
        $lines.Add($line)
        $line
    }
}
finally {
    Pop-Location
    if ($Log) {
        $lines | Set-Content -LiteralPath $Log -Encoding UTF8
        Write-Host "Log: $Log"
    }
}
