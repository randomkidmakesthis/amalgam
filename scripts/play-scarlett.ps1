# Starts the title in Series X mode with this branch's current settings.
#
#   .\scripts\play-scarlett.ps1 -Game 'D:\Games\MyDump'
#
# -Game        your own dump: the folder with game_scarlett_master.exe (or that file itself).
#              Remembered in game-folder.txt next to the repository's README, so later starts need no -Game.
# -Build       diag (default: the build all measurements are of) or release
# -Victory     another victory.exe than target\<Build>\victory.exe
# -Cache       shader and pipeline caches (default: %LOCALAPPDATA%\victory-scarlett\cache). A first run fills them
#              and is slow; faces can be missing in that first run and are right from the second.
# -Data        saves (default: %LOCALAPPDATA%\victory-scarlett\data)
# -RayTracing  the title's own ray tracing (VICTORY_BVH_BUILDER=1): better light, much slower. Off by default.
# -AutoStart   presses through the start screens by itself, for a fresh start with no save
# -NoProbeRays the base's picture with ray tracing off: bushes, trees and palms glow violet. By default the
#              title's light probes are activated and every ray misses, as with an empty ray-tracing
#              structure (no cost measured); the first start after a change of this re-translates shaders.
# -Hooks       with the memory hooks: faster, but they can show a few small textures with old contents for a
#              moment. Off by default until that is fixed.
# -NoPrecompile  skip the step below
# -OnlyPrecompile  do the step below and stop, without starting the game
# -More        other settings, last word: -More @{ VICTORY_LOG = 'warn' }
#
# Before the game starts, the game's shaders are translated ahead (victory.exe shaders <shaders.rpf>) whenever
# that is needed: on the first start, after a new build of victory.exe, and after a change of a setting that
# changes the translation (-RayTracing, -NoProbeRays). It takes a few minutes and is remembered in the cache
# folder, so later starts go straight to the game. Without it the first session translates while playing:
# slow, and faces can be drawn wrong for that session.
#
# Run it from a normal window, not as administrator.
param(
    [string]$Game = '',
    [ValidateSet('diag', 'release')][string]$Build = 'diag',
    [string]$Victory = '',
    [string]$Cache = '',
    [string]$Data = '',
    [switch]$RayTracing,
    [switch]$AutoStart,
    [switch]$NoProbeRays,
    [switch]$ProbeRays,
    [switch]$Hooks,
    [switch]$NoHooks,
    [switch]$NoPrecompile,
    [switch]$OnlyPrecompile,
    [hashtable]$More = @{}
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$remembered = Join-Path $root 'game-folder.txt'

if (-not $Game -and (Test-Path -LiteralPath $remembered)) { $Game = (Get-Content -LiteralPath $remembered -TotalCount 1) }
if (-not $Game) { throw "Say where your dump is: .\scripts\play-scarlett.ps1 -Game 'D:\Games\MyDump'" }
$Game = $Game.Trim().Trim('"')
if ((Test-Path -LiteralPath $Game -PathType Leaf) -and $Game.EndsWith('.exe')) { $Game = Split-Path -Parent $Game }
$exe = Join-Path $Game 'game_scarlett_master.exe'
if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw "No game_scarlett_master.exe in $Game. -Game is the folder of your dump." }
Set-Content -LiteralPath $remembered -Value $Game -Encoding utf8

if (-not $Victory) { $Victory = Join-Path $root "target\$Build\victory.exe" }
if (-not (Test-Path -LiteralPath $Victory -PathType Leaf)) {
    throw "No $Victory. Build it first: cargo build --profile diag -p victory --features diagnostics --locked"
}
if (-not $Cache) { $Cache = Join-Path $env:LOCALAPPDATA 'victory-scarlett\cache' }
if (-not $Data) { $Data = Join-Path $env:LOCALAPPDATA 'victory-scarlett\data' }
New-Item -ItemType Directory -Force -Path $Cache, $Data | Out-Null

# Settings left in this window by an earlier start go: only what is below counts.
Get-ChildItem Env: | Where-Object { $_.Name -like 'VICTORY_*' } | ForEach-Object { Remove-Item "Env:$($_.Name)" }

$set = [ordered]@{
    VICTORY_GAME_DIR  = $Game
    VICTORY_CACHE_DIR = $Cache
    VICTORY_DATA_DIR  = $Data

    # The standing set of this fork's runs (some names come from its older tree and are ignored here)
    VICTORY_JOIN_COPIES = '1'; VICTORY_REFRESH_ONCE = '1'; VICTORY_CARD_REHASH_RUNS = '1'; VICTORY_BACK_OWN_VIEW = '1'
    VICTORY_WIDE_MERGE = '1'; VICTORY_MIRROR_FAST = '1'; VICTORY_FRAME_LOG = '1'; VICTORY_CARD_METER = '1'
    VICTORY_PIPELINE_WAIT_MS = '500'; VICTORY_ASYNC_SUBMIT = '1'; VICTORY_QUEUE2 = '1'; VICTORY_INDIRECT_WINDOW = '1'
    VICTORY_TEXTURE_NO_WAIT = '1'; VICTORY_COPY_PLAIN = '1'; VICTORY_WIDE_TEXTURES = '1'; VICTORY_INDIRECT_GATHER = '1'
    VICTORY_LATE_WAIT_LISTS = '1024'; VICTORY_CARD_FILLS_BATCH = '0'; VICTORY_GATHER_DISPATCH = '0'

    # cool_fox's launcher set, without Series S, VICTORY_THIN and the stand-in ambient light; the keep-busy loop off
    VICTORY_RELAY = '2'; VICTORY_WIDE_BACK = '1'; VICTORY_RELAY_COPY = '1'; VICTORY_CARD_STEPS = '2'
    VICTORY_INDIRECT_BATCH = '2'; VICTORY_MAP_THREADS = '1'; VICTORY_CARD_AWAKE = '0'; VICTORY_SHOW_AFTER = '1'
    VICTORY_JOIN_PARTS = '8'; VICTORY_UPLOAD_AHEAD = '1'; VICTORY_CARD_BUFFERS_MIN_KB = '64'

    # Textures keep only the levels their views show (the base's default; cool_fox's launcher turned it off)
    VICTORY_LEVELS_ON_DEMAND = '0'

    # This fork's memory and upload switches
    VICTORY_POOL_DISCARD = '1'; VICTORY_FILL_TITLE = '1'; VICTORY_PARTIAL_DELTA = '1'; VICTORY_BAKE_DELTA = '1'
    VICTORY_MAP_LOCK_FINE = '1'; VICTORY_POOL_CAP = '1'

    # Draw set-up: table entries done earlier in a list are not done again; an aliased texture's copy follows its
    # source's writes within a list (a stale read in the base)
    VICTORY_TABLE_MEMO = '1'; VICTORY_LIST_ALIAS_EXACT = '1'; VICTORY_LIST_HOLD = '1'

    # Picture: the title's character bake waits up to 10 s for its textures (1.5 s by default), and a draw
    # waits up to 500 ms for a pipeline still being made (above) instead of being dropped after 20 ms
    VICTORY_BAKE_TEXTURES_MS = '10000'

    # Picture: the material-class depth plane is current before the title's tiling pass reads it (the base
    # gave it last frame's)
    VICTORY_PLANE_NOW = '1'

    # Freezes: the views of the title's batch page-mapping copies are mapped only when touched
    VICTORY_SOFT_VIEWS = 'copies'

    # Picture: pipelines with static samplers are made each run, not loaded from the saved library
    VICTORY_SAMPLERS_MADE = '1'

    # Draw set-up: only the table slots a program can read; texture-queue notes that change nothing skip its lock
    VICTORY_TABLE_NAMED_ONLY = '1'; VICTORY_QUEUE_MARKS = '1'
}
if ($Hooks -and -not $NoHooks) {
    # The memory hooks
    $set.VICTORY_GPU_ONLY = '1'; $set.VICTORY_KNOWN_WRITES = '1'
}
if (-not $NoProbeRays -and -not $RayTracing) { $set.VICTORY_PROBE_RAYS = '1' }
if ($RayTracing) { $set.VICTORY_BVH_BUILDER = '1' }
if ($AutoStart) {
    $set.VICTORY_AUTO_INPUT = 'd>=15,rep(A,1,d<10),w3,d>=20,w1,rep(A,1,d>=35),w1.5,Down,w7,Down,w0.6,A,w1.5,A,w0.7,Down,w0.5,A,w0.7,Down,w0.5,A'
}
foreach ($k in $More.Keys) { $set[$k] = [string]$More[$k] }
foreach ($k in $set.Keys) { Set-Item -Path "Env:$k" -Value $set[$k] }

# Shaders translated ahead, when needed: the mark holds what the translations depend on that this script knows of
# (the build and the settings that change the translation).
$rpf = Join-Path $Game 'shaders.rpf'
$mark = Join-Path $Cache 'scarlett-shaders-ahead.txt'
$built = Get-Item -LiteralPath $Victory
$stamp = '{0}|{1}|{2}|probe={3}|rays={4}' -f $built.Length, $built.LastWriteTimeUtc.Ticks, $Build, $set['VICTORY_PROBE_RAYS'], $set['VICTORY_BVH_BUILDER']
$have = if (Test-Path -LiteralPath $mark) { Get-Content -LiteralPath $mark -TotalCount 1 } else { '' }
if (-not $NoPrecompile -and $have -ne $stamp) {
    if (Test-Path -LiteralPath $rpf -PathType Leaf) {
        Write-Host 'Translating the game''s shaders ahead (needed once per build and setting; a few minutes)...'
        & $Victory shaders $rpf
        if ($LASTEXITCODE -eq 0) {
            Set-Content -LiteralPath $mark -Value $stamp -Encoding utf8
        } else {
            Write-Warning 'Translating ahead failed; the shaders will be translated while playing instead.'
        }
    } else {
        Write-Warning "No shaders.rpf in ${Game}: the shaders will be translated while playing."
    }
}
if ($OnlyPrecompile) { exit 0 }

Write-Host ("Series X mode, {0} build, ray tracing {1}, memory hooks {2}" -f $Build, $(if ($RayTracing) { 'on' } else { 'off' }), $(if ($Hooks -and -not $NoHooks) { 'on' } else { 'off' }))
Write-Host "Cache: $Cache"
Write-Host "Saves: $Data"
& $Victory run $exe --execute
exit $LASTEXITCODE
