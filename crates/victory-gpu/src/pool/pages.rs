//! (Fox V; `VICTORY_PAGE_POOL=1`, default off; `=verify` counts only) Host
//! copies of textures the title fills on shared 64 KiB pages, as the title
//! keeps its own texture memory: any free page serves any texture, so no hole
//! is ever too small or in the wrong block (the placed pool: 2.3-4.6 GB of
//! holes in 64 MB blocks pinned by a few old pieces, runs 130/159).
//!
//! A copy of 64 KiB or more is a reserved texture
//! (`D3D12_TEXTURE_LAYOUT_64KB_UNDEFINED_SWIZZLE`) with ALL its tiles (packed
//! levels included) mapped when it is made, by a mapping thread on a queue of
//! its own (`UpdateTileMappings`, one call per heap it takes pages of; the
//! host's queue waits on the card for it, ahead of every list that can name
//! the copy; V7: on the host's queue itself the calls stalled the submitting
//! thread, run-169); the copy queue of uploads made ahead waits for the
//! mapping ([`ready_on`]). It is filled whole before anything reads it, as a
//! placed copy is. The rules of the placed pool carry over:
//! - pages come from heaps that every submission begun before their making has
//!   left behind (ripe: n08/n11), lowest heap first; heaps are made ahead,
//!   with the not-resident / resident / mapped waits of the placed blocks;
//! - a page given back rests [`super::REST`] before it is used again;
//! - a new heap only while the card's raw use with it stays under the raw cap
//!   (`VICTORY_POOL_RAW_PCT`, 95 %); past it, a copy that would need one goes to
//!   the placed pool (counted), which has its own rule;
//! - free pages count as free room ([`room`], `pool::texture_room`).
//! Each copy that does not go on pages goes to the placed pool as before,
//! counted by reason: not 2D, an array or cube, multisampled, under 64 KiB,
//! rounding waste over a quarter (tiles vs the placed size), a tiling the card
//! gives that does not fit, the card refused the reserved texture, more than
//! 64 MiB, no pages under the cap, the per-window mapping budget spent.

use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use windows::Win32::Graphics::Direct3D12::*;
use windows::core::{GUID, HRESULT, IUnknown, Interface};

use super::{Cap, REST, Ripe, SPARES};

/// The card's tile.
const PAGE: u64 = 64 << 10;
/// Pages of a heap (64 MiB).
const HEAP_PAGES: u32 = 1024;
const HEAP_BYTES: u64 = PAGE * HEAP_PAGES as u64;
/// The most pages one copy takes (64 MiB): larger ones are placed copies
/// (none such is pooled either: `pool::texture` takes up to a block).
const MOST_PAGES: u32 = HEAP_PAGES;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Mode {
    Off,
    Verify,
    On,
}

pub(crate) fn mode() -> Mode {
    static MODE: std::sync::OnceLock<Mode> = std::sync::OnceLock::new();
    *MODE.get_or_init(|| {
        let mode = match std::env::var("VICTORY_PAGE_POOL").as_deref().map(str::trim) {
            Ok("1") => Mode::On,
            Ok("verify") => Mode::Verify,
            _ => Mode::Off,
        };
        if mode != Mode::Off {
            tracing::info!(
                verify = mode == Mode::Verify,
                budget_us = budget_us(),
                "texture copies of 64 KiB or more on shared 64 KiB pages (VICTORY_PAGE_POOL={})",
                if mode == Mode::On { "1" } else { "verify" }
            );
        }
        mode
    })
}

/// Microseconds of mapping calls per [`WINDOW_MS`] past which copies go to
/// the placed pool until the window ends (`VICTORY_PAGE_POOL_US`, 2000: some
/// 6 % of the submitting thread at most).
fn budget_us() -> u64 {
    static US: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *US.get_or_init(|| std::env::var("VICTORY_PAGE_POOL_US").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(2000))
}

/// The mapping budget's window, about a frame at 30 a second.
const WINDOW_MS: u64 = 33;

/// Why a copy is not on pages ([`Pages::texture`]).
#[derive(Clone, Copy)]
pub(crate) enum Why {
    NotTwoD,
    Array,
    Samples,
    Small,
    Rounding,
    Tiling,
    Refused,
    Large,
    NoPages,
    Budget,
    NoHeap,
    NoQueue,
}

const WHY: usize = 12;
const WHY_NAMES: [&str; WHY] = [
    "not 2D",
    "array or cube",
    "multisampled",
    "under 64 KiB",
    "rounding over 25 %",
    "tiling does not fit",
    "refused as reserved",
    "over 64 MiB",
    "no pages under the cap",
    "mapping budget spent",
    "no heap could be made",
    "no mapping thread",
];

/// Since the start: copies sent to the placed pool by [`Why`], and bytes.
static FALLBACK: [[AtomicU64; WHY]; 2] = [const { [const { AtomicU64::new(0) }; WHY] }; 2];

fn fell(why: Why, bytes: u64) {
    FALLBACK[0][why as usize].fetch_add(1, Ordering::Relaxed);
    FALLBACK[1][why as usize].fetch_add(bytes, Ordering::Relaxed);
}

/// Since the start: copies made on pages (or, verifying, that would have
/// been), their pages, the placed bytes they would have taken; copies on
/// pages alive; heaps made, given back; pieces from unripe heaps; heaps made
/// ahead the cap held back; heaps past the cap for want of pages (none: the
/// placed pool takes those).
static MADE: [AtomicU64; 9] = [const { AtomicU64::new(0) }; 9];
const M_COPIES: usize = 0;
const M_PAGES: usize = 1;
const M_PLACED: usize = 2;
const M_ALIVE: usize = 3;
const M_HEAPS: usize = 4;
const M_GIVEN_BACK: usize = 5;
const M_UNRIPE: usize = 6;
const M_AHEAD_HELD: usize = 7;
const M_ALIVE_PAGES: usize = 8;

/// Mapping calls and microseconds since the last line; the most
/// microseconds in one window; the window's start and its microseconds.
static MAPPING: [AtomicU64; 7] = [const { AtomicU64::new(0) }; 7];

struct Heap {
    heap: ID3D12Heap,
    free: Free,
    /// The newest submission begun when it was made ([`Ripe`]).
    made: u64,
    /// (`VICTORY_PAGE_MIGRATE`) Its pages are being moved out: new copies
    /// take pages of it only when no other heap has them.
    emptying: bool,
}

/// Free runs of a heap (first page, pages), in order, no two touching.
struct Free {
    runs: Vec<(u32, u32)>,
    pages: u32,
}

impl Free {
    fn whole() -> Free {
        Free { runs: vec![(0, HEAP_PAGES)], pages: HEAP_PAGES }
    }

    /// Up to `want` pages, lowest first, into `out`: how many.
    fn take(&mut self, mut want: u32, out: &mut Vec<(u32, u32)>) -> u32 {
        let mut got = 0;
        while want > 0 && !self.runs.is_empty() {
            let (start, n) = self.runs[0];
            let part = n.min(want);
            out.push((start, part));
            if part == n {
                self.runs.remove(0);
            } else {
                self.runs[0] = (start + part, n - part);
            }
            want -= part;
            got += part;
        }
        self.pages -= got;
        got
    }

    fn give(&mut self, given: &[(u32, u32)]) {
        self.runs.extend_from_slice(given);
        self.runs.sort_unstable();
        let mut joined: Vec<(u32, u32)> = Vec::with_capacity(self.runs.len());
        for &(from, n) in &self.runs {
            match joined.last_mut() {
                Some(last) if last.0 + last.1 == from => last.1 += n,
                _ => joined.push((from, n)),
            }
        }
        self.runs = joined;
        self.pages = self.runs.iter().map(|r| r.1).sum();
    }
}

/// Pages of a copy: by heap place, the runs.
type Runs = Vec<(usize, Vec<(u32, u32)>)>;

struct Pages {
    heaps: Vec<Option<Heap>>,
    resting: VecDeque<(u64, Runs)>,
    retry_at: u64,
}

static PAGES: Mutex<Pages> = Mutex::new(Pages { heaps: Vec::new(), resting: VecDeque::new(), retry_at: 0 });

fn lock() -> std::sync::MutexGuard<'static, Pages> {
    PAGES.lock().unwrap_or_else(|e| e.into_inner())
}

impl Pages {
    fn count(&self) -> usize {
        self.heaps.iter().flatten().count()
    }

    fn empty(&self) -> usize {
        self.heaps.iter().flatten().filter(|h| h.free.pages == HEAP_PAGES).count()
    }

    /// Pages that rested long enough are free; empty heaps past the kept
    /// ones go (as empty blocks: [`SPARES`]).
    fn rested(&mut self, now: u64, keep: usize) {
        while self.resting.front().is_some_and(|r| r.0 <= now) {
            let Some((_, runs)) = self.resting.pop_front() else { break };
            for (i, runs) in runs {
                if let Some(Some(h)) = self.heaps.get_mut(i) {
                    h.free.give(&runs);
                }
            }
        }
        let mut kept = 0;
        for h in &mut self.heaps {
            if h.as_ref().is_some_and(|h| h.free.pages == HEAP_PAGES) {
                kept += 1;
                if kept > keep {
                    *h = None;
                    MADE[M_GIVEN_BACK].fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    fn make(&mut self, device: &ID3D12Device, now: u64, ripe: Ripe) -> Option<usize> {
        if now < self.retry_at {
            return None;
        }
        let desc = D3D12_HEAP_DESC {
            SizeInBytes: HEAP_BYTES,
            Properties: D3D12_HEAP_PROPERTIES { Type: D3D12_HEAP_TYPE_DEFAULT, ..Default::default() },
            Alignment: 0,
            Flags: D3D12_HEAP_FLAG_ALLOW_ONLY_NON_RT_DS_TEXTURES | crate::host::not_resident(),
        };
        let mut heap: Option<ID3D12Heap> = None;
        // SAFETY: plain creation.
        if let Err(e) = unsafe { device.CreateHeap(&desc, &mut heap) } {
            crate::residency::allocation_error(&e);
            tracing::warn!(error = %e, "no heap of texture pages");
        }
        let heap = heap.filter(|h| {
            crate::host::resident(device, h).and_then(|()| crate::host::mapped(device)).is_ok()
        });
        let Some(heap) = heap else {
            self.retry_at = now + 500;
            return None;
        };
        crate::host::name(&heap, || "texture pages (a heap of 64 MiB)".into());
        MADE[M_HEAPS].fetch_add(1, Ordering::Relaxed);
        let fresh = Heap { heap, free: Free::whole(), made: ripe.started, emptying: false };
        Some(match self.heaps.iter().position(Option::is_none) {
            Some(i) => {
                self.heaps[i] = Some(fresh);
                i
            }
            None => {
                self.heaps.push(Some(fresh));
                self.heaps.len() - 1
            }
        })
    }

    /// `want` pages: from ripe heaps, lowest first, then unripe ones (the
    /// oldest first), then a new heap under the cap. `None`: not that many
    /// to be had under the cap (nothing is taken then).
    fn take(&mut self, device: &ID3D12Device, want: u32, now: u64, ripe: Ripe, cap: Cap) -> Option<Runs> {
        let ripe_is = |h: &Heap| ripe.ahead == 0 || h.made <= ripe.finished;
        let free = |pages: &Pages, ripe_only: bool| -> u32 {
            pages.heaps.iter().flatten().filter(|h| !ripe_only || ripe_is(h)).map(|h| h.free.pages).sum()
        };
        while free(self, false) < want {
            if cap.over(blocks_since(self.count())) {
                return None;
            }
            self.make(device, now, ripe)?;
        }
        // (V7) The fullest ripe heap that holds all of them; else ripe heaps
        // fullest first (sparse heaps are left to empty); then unripe ones,
        // the oldest first. Empty heaps come last among the ripe.
        let whole = self
            .heaps
            .iter()
            .enumerate()
            .filter_map(|(i, h)| h.as_ref().filter(|h| ripe_is(h) && !h.emptying && h.free.pages >= want).map(|h| (h.free.pages, i)))
            .min();
        let mut order: Vec<(u8, u64, usize)> = match whole {
            Some((_, i)) => vec![(0, 0, i)],
            None => self
                .heaps
                .iter()
                .enumerate()
                .filter_map(|(i, h)| {
                    let h = h.as_ref().filter(|h| h.free.pages > 0)?;
                    Some(if h.emptying { (2, u64::from(h.free.pages), i) } else if ripe_is(h) { (0, u64::from(h.free.pages), i) } else { (1, h.made, i) })
                })
                .collect(),
        };
        order.sort_unstable();
        let mut runs: Runs = Vec::new();
        let mut left = want;
        for (unripe, _, i) in order {
            if left == 0 {
                break;
            }
            let Some(Some(h)) = self.heaps.get_mut(i) else { continue };
            let mut got = Vec::new();
            left -= h.free.take(left, &mut got);
            if unripe == 1 {
                MADE[M_UNRIPE].fetch_add(1, Ordering::Relaxed);
            }
            runs.push((i, got));
        }
        // Made ahead, under the cap only.
        if self.empty() < ripe.ahead {
            if cap.over(blocks_since(self.count()) + 1) {
                MADE[M_AHEAD_HELD].fetch_add(1, Ordering::Relaxed);
            } else {
                self.make(device, now, ripe);
            }
        }
        Some(runs)
    }
}

/// Page heaps when the card's figures were last taken ([`tick`]).
static HEAPS_THEN: AtomicU64 = AtomicU64::new(0);

/// For [`Cap::over`]: texture blocks alive now (as of the last second if
/// their pool is busy: never waited for under this lock) plus the page heaps
/// made since the card's raw use was last taken.
fn blocks_since(heaps: usize) -> usize {
    let blocks = super::TEXTURES.try_lock().map_or(super::CARD[2].load(Ordering::Relaxed) as usize, |p| p.blocks());
    blocks + heaps.saturating_sub(HEAPS_THEN.load(Ordering::Relaxed) as usize)
}

/// Bytes of the page heaps no copy holds (free and resting pages, empty
/// heaps): free room for the room rule.
pub(crate) fn room() -> u64 {
    if mode() != Mode::On {
        return 0;
    }
    let heaps = lock().count() as u64;
    (heaps * HEAP_BYTES).saturating_sub(MADE[M_ALIVE_PAGES].load(Ordering::Relaxed) * PAGE)
}

/// Makes `queue` wait (on the card) for every mapping asked for so far, if
/// one is not done yet. For the copy queue of uploads made ahead.
pub(crate) fn ready_on(queue: &ID3D12CommandQueue) {
    let n = ENQUEUED.load(Ordering::Acquire);
    if n == 0 {
        return;
    }
    let Some(device) = crate::host::host().map(|h| h.device.clone()) else { return };
    let Some(worker) = worker(&device) else { return };
    // SAFETY: a live fence of ours.
    if unsafe { worker.fence.GetCompletedValue() } < n {
        // SAFETY: as above.
        let _ = unsafe { queue.Wait(&worker.fence, n) };
    }
}

/// The card's tiling of a reserved texture: tiles of each standard level
/// (width in tiles, tiles) and of the packed ones (first packed level,
/// tiles), and all of them.
#[derive(Clone)]
struct Tiling {
    standard: Vec<(u32, u32)>,
    packed: Option<(u32, u32)>,
    total: u32,
}

fn tiling(device: &ID3D12Device, texture: &ID3D12Resource, levels: u32) -> Option<Tiling> {
    if levels == 0 || levels > 16 {
        return None;
    }
    let mut total = 0u32;
    let mut packed = D3D12_PACKED_MIP_INFO::default();
    let mut shape = D3D12_TILE_SHAPE::default();
    let mut count = levels;
    let mut tilings = vec![D3D12_SUBRESOURCE_TILING::default(); levels as usize];
    // SAFETY: outputs of our own, `count` tilings of them.
    unsafe {
        device.GetResourceTiling(
            texture,
            Some(&raw mut total),
            Some(&raw mut packed),
            Some(&raw mut shape),
            Some(&raw mut count),
            0,
            tilings.as_mut_ptr(),
        );
    }
    let standard_levels = u32::from(packed.NumStandardMips);
    if total == 0 || standard_levels + u32::from(packed.NumPackedMips) != levels || standard_levels > count {
        return None;
    }
    let mut standard = Vec::with_capacity(standard_levels as usize);
    let mut sum = 0u32;
    for t in &tilings[..standard_levels as usize] {
        if t.DepthInTiles != 1 || t.StartTileIndexInOverallResource == D3D12_PACKED_TILE {
            return None;
        }
        let tiles = t.WidthInTiles.checked_mul(u32::from(t.HeightInTiles))?;
        if tiles == 0 {
            return None;
        }
        standard.push((t.WidthInTiles, tiles));
        sum += tiles;
    }
    let packed = (packed.NumPackedMips > 0).then_some((standard_levels, packed.NumTilesForPackedMips));
    if packed.is_some_and(|p| p.1 == 0) {
        return None;
    }
    sum += packed.map_or(0, |p| p.1);
    (sum == total).then_some(Tiling { standard, packed, total })
}

/// The regions of tiles `[from, to)` of the texture in its tiles' order
/// (each standard level in rows, then the packed ones).
fn regions(t: &Tiling, from: u32, to: u32) -> (Vec<D3D12_TILED_RESOURCE_COORDINATE>, Vec<D3D12_TILE_REGION_SIZE>) {
    let (mut at, mut size) = (Vec::new(), Vec::new());
    let mut start = 0u32;
    let parts = t.standard.iter().enumerate().map(|(level, &(width, tiles))| (level as u32, width, tiles)).chain(
        t.packed.map(|(level, tiles)| (level, 0, tiles)),
    );
    for (level, width, tiles) in parts {
        let (a, b) = (from.max(start), to.min(start + tiles));
        if a < b {
            let local = a - start;
            let (x, y) = if width == 0 { (local, 0) } else { (local % width, local / width) };
            at.push(D3D12_TILED_RESOURCE_COORDINATE { X: x, Y: y, Z: 0, Subresource: level });
            size.push(D3D12_TILE_REGION_SIZE { NumTiles: b - a, ..Default::default() });
        }
        start += tiles;
    }
    (at, size)
}

/// Copies on pages alive by interface pointer (`VICTORY_HEAP_PACK=1`'s
/// order: such a copy is in no block, and is not to be taken first for it).
static ON_PAGES: Mutex<Option<std::collections::HashSet<usize>>> = Mutex::new(None);

pub(crate) fn holds(texture: usize) -> bool {
    ON_PAGES.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|s| s.contains(&texture))
}

/// A copy of `desc` (the host's description, placed `placed` bytes) on
/// pages, or why not ([`Why`]; verifying: always a reason, counted as made
/// when it would have gone).
///
/// # Safety
/// `desc` is a valid description.
pub(crate) unsafe fn texture(
    device: &ID3D12Device,
    desc: &D3D12_RESOURCE_DESC,
    state: D3D12_RESOURCE_STATES,
    placed: u64,
) -> Result<ID3D12Resource, Option<Why>> {
    let out = |why: Why| {
        fell(why, placed);
        Err(Some(why))
    };
    if desc.Dimension != D3D12_RESOURCE_DIMENSION_TEXTURE2D {
        return out(Why::NotTwoD);
    }
    if desc.DepthOrArraySize != 1 {
        return out(Why::Array);
    }
    if desc.SampleDesc.Count > 1 {
        return out(Why::Samples);
    }
    if placed < PAGE {
        return out(Why::Small);
    }
    let mut d = *desc;
    d.Layout = D3D12_TEXTURE_LAYOUT_64KB_UNDEFINED_SWIZZLE;
    d.Alignment = 0;
    // SAFETY: plain query.
    let tiled_bytes = unsafe { device.GetResourceAllocationInfo(0, &[d]) }.SizeInBytes;
    if tiled_bytes == u64::MAX || tiled_bytes == 0 {
        return out(Why::Refused);
    }
    if tiled_bytes > u64::from(MOST_PAGES) * PAGE {
        return out(Why::Large);
    }
    if tiled_bytes > placed + placed / 4 {
        return out(Why::Rounding);
    }
    if mode() == Mode::Verify {
        MADE[M_COPIES].fetch_add(1, Ordering::Relaxed);
        MADE[M_PAGES].fetch_add(tiled_bytes.div_ceil(PAGE), Ordering::Relaxed);
        MADE[M_PLACED].fetch_add(placed, Ordering::Relaxed);
        return Err(None);
    }
    // (V7) The mapping budget: what is queued for the mapping thread may not
    // pass it (reserved before anything is made; one call a heap, at the
    // cost per call measured so far).
    let Some(worker) = worker(device) else { return out(Why::NoQueue) };
    // (V8) A copy is always taken while nothing is queued; else only while
    // what is queued, and what the thread did in this window, stay under the
    // budget. The cost of a call is the call's alone (not the queue's wait),
    // a moving mean that every mapped call refreshes.
    let per_call = PER_CALL_US.load(Ordering::Relaxed).max(1);
    let queued = BACKLOG_US.fetch_add(per_call, Ordering::AcqRel);
    let window_busy = if crate::host::now_ms() < WINDOW_AT.load(Ordering::Relaxed) + WINDOW_MS {
        WINDOW_WORK.load(Ordering::Relaxed)
    } else {
        0
    };
    if queued > 0 && (queued + per_call > budget_us() || window_busy + queued + per_call > budget_us()) {
        BACKLOG_US.fetch_sub(per_call, Ordering::AcqRel);
        return out(Why::Budget);
    }
    let unreserve = |us: u64| {
        BACKLOG_US.fetch_sub(us, Ordering::AcqRel);
    };
    let Some(host) = crate::host::host() else {
        unreserve(per_call);
        return out(Why::Refused);
    };
    let mut made: Option<ID3D12Resource> = None;
    // SAFETY: a valid description in the tiles' layout; the out pointer is ours.
    if unsafe { device.CreateReservedResource(&d, state, None, &mut made) }.is_err() {
        unreserve(per_call);
        return out(Why::Refused);
    }
    let Some(texture) = made else {
        unreserve(per_call);
        return out(Why::Refused);
    };
    let Some(t) = tiling(device, &texture, u32::from(d.MipLevels)) else {
        unreserve(per_call);
        return out(Why::Tiling);
    };
    let now = crate::host::now_ms();
    let ripe = Ripe::now();
    let cap = super::cap().or(super::classes()).unwrap_or(Cap { pct: 95 });
    let mut pages = lock();
    pages.rested(now, SPARES.load(Ordering::Relaxed));
    let Some(runs) = pages.take(device, t.total, now, ripe, cap) else {
        let why = if pages.retry_at > now { Why::NoHeap } else { Why::NoPages };
        drop(pages);
        unreserve(per_call);
        return out(why);
    };
    let maps: Vec<(ID3D12Heap, Vec<(u32, u32)>)> = runs
        .iter()
        .filter_map(|(i, r)| Some((pages.heaps.get(*i)?.as_ref()?.heap.clone(), r.clone())))
        .collect();
    MADE[M_ALIVE_PAGES].fetch_add(u64::from(t.total), Ordering::Relaxed);
    drop(pages);
    let estimate = per_call * maps.len().max(1) as u64;
    BACKLOG_US.fetch_add(estimate - per_call, Ordering::AcqRel);
    // The tiles are mapped by the mapping thread on its queue; the host's
    // queue waits (on the card) for that before anything sent after this,
    // which is all that can name the copy; so does the copy queue of uploads
    // made ahead ([`ready_on`]).
    let total = t.total;
    let kept_tiling = t.clone();
    let sent = {
        let send = worker.send.lock().unwrap_or_else(|e| e.into_inner());
        let n = ENQUEUED.load(Ordering::Acquire) + 1;
        let job = Job { texture: texture.clone(), tiling: t, maps, n, estimate, queued: std::time::Instant::now() };
        match send.send(job) {
            Ok(()) => {
                ENQUEUED.store(n, Ordering::Release);
                // SAFETY: a live fence of ours, on the host's queue.
                let _ = unsafe { host.queue().Wait(&worker.fence, n) };
                true
            }
            Err(_) => false,
        }
    };
    if !sent {
        // The mapping thread is gone: nothing was asked of the host's queue;
        // the pages go back (resting) with the texture, which is not used.
        unreserve(estimate);
        give_back_with(&texture, runs, total);
        return out(Why::NoQueue);
    }
    MADE[M_COPIES].fetch_add(1, Ordering::Relaxed);
    MADE[M_PAGES].fetch_add(u64::from(total), Ordering::Relaxed);
    MADE[M_PLACED].fetch_add(placed, Ordering::Relaxed);
    MADE[M_ALIVE].fetch_add(1, Ordering::Relaxed);
    copies().get_or_insert_with(Default::default).insert(
        texture.as_raw() as usize,
        Copy { segs: flat(&runs), tiling: kept_tiling, desc: d, made: now, host: None },
    );
    give_back_with(&texture, runs, total);
    Ok(texture)
}

/// (V7) Estimated microseconds queued for the mapping thread, and the
/// measured cost of a mapping call (moving mean; 100 before any).
static BACKLOG_US: AtomicU64 = AtomicU64::new(0);
static PER_CALL_US: AtomicU64 = AtomicU64::new(80);
/// The mapping thread's window ([`WINDOW_MS`]): its start (ms) and the
/// microseconds of calls in it.
static WINDOW_AT: AtomicU64 = AtomicU64::new(0);
static WINDOW_WORK: AtomicU64 = AtomicU64::new(0);
/// Since the last line: call microseconds, microseconds jobs waited in the
/// queue, microseconds of the fence signal.
static SAMPLES: Mutex<[Vec<u32>; 3]> = Mutex::new([Vec::new(), Vec::new(), Vec::new()]);
/// Jobs given to the mapping thread so far (the fence value the last one
/// signals).
static ENQUEUED: AtomicU64 = AtomicU64::new(0);

/// One copy's tiles to map: the heaps and their runs, in the tiles' order.
struct Job {
    texture: ID3D12Resource,
    tiling: Tiling,
    maps: Vec<(ID3D12Heap, Vec<(u32, u32)>)>,
    n: u64,
    estimate: u64,
    queued: std::time::Instant,
}

struct Worker {
    send: Mutex<std::sync::mpsc::Sender<Job>>,
    fence: ID3D12Fence,
}

/// The mapping thread and its queue, made at the first copy on pages.
fn worker(device: &ID3D12Device) -> Option<&'static Worker> {
    static WORKER: std::sync::OnceLock<Option<Worker>> = std::sync::OnceLock::new();
    WORKER
        .get_or_init(|| {
            // SAFETY: plain creation.
            let (queue, fence) = unsafe {
                let queue: ID3D12CommandQueue = device
                    .CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC { Type: D3D12_COMMAND_LIST_TYPE_DIRECT, ..Default::default() })
                    .ok()?;
                let fence: ID3D12Fence = device.CreateFence(0, D3D12_FENCE_FLAG_NONE).ok()?;
                (queue, fence)
            };
            crate::host::name(&queue, || "texture page mappings".into());
            let (send, receive) = std::sync::mpsc::channel::<Job>();
            let signal = fence.clone();
            std::thread::Builder::new()
                .name("victory texture pages".into())
                .spawn(move || map_jobs(&queue, &signal, &receive))
                .ok()?;
            Some(Worker { send: Mutex::new(send), fence })
        })
        .as_ref()
}

/// The mapping thread: each job's tiles, one call a heap, then its fence
/// value, always (a job that could not be mapped still signals: the host's
/// queue waits for it).
fn map_jobs(queue: &ID3D12CommandQueue, fence: &ID3D12Fence, jobs: &std::sync::mpsc::Receiver<Job>) {
    while let Ok(job) = jobs.recv() {
        let waited = job.queued.elapsed().as_micros() as u64;
        let mut call_us = 0u64;
        let mapped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut next = 0u32;
            for (heap, runs) in &job.maps {
                let n: u32 = runs.iter().map(|r| r.1).sum();
                let (at, size) = regions(&job.tiling, next, next + n);
                let flags = vec![D3D12_TILE_RANGE_FLAG_NONE; runs.len()];
                let starts: Vec<u32> = runs.iter().map(|r| r.0).collect();
                let counts: Vec<u32> = runs.iter().map(|r| r.1).collect();
                let began = std::time::Instant::now();
                // SAFETY: a live reserved texture and a live heap of ours;
                // regions covering `n` tiles and ranges covering `n` pages,
                // in arrays that live through the call.
                unsafe {
                    queue.UpdateTileMappings(
                        &job.texture,
                        at.len() as u32,
                        Some(at.as_ptr()),
                        Some(size.as_ptr()),
                        heap,
                        runs.len() as u32,
                        Some(flags.as_ptr()),
                        Some(starts.as_ptr()),
                        Some(counts.as_ptr()),
                        D3D12_TILE_MAPPING_FLAG_NONE,
                    );
                }
                call_us += began.elapsed().as_micros() as u64;
                next += n;
            }
        }));
        if mapped.is_err() {
            MAPPING[5].fetch_add(1, Ordering::Relaxed);
        }
        let signal_began = std::time::Instant::now();
        // SAFETY: a live fence of ours, on our queue.
        let _ = unsafe { queue.Signal(fence, job.n) };
        let signal_us = signal_began.elapsed().as_micros() as u64;
        let calls = job.maps.len().max(1) as u64;
        let pages: u64 = job.maps.iter().map(|m| m.1.iter().map(|r| u64::from(r.1)).sum::<u64>()).sum();
        MAPPING[0].fetch_add(calls, Ordering::Relaxed);
        MAPPING[1].fetch_add(call_us, Ordering::Relaxed);
        MAPPING[3].fetch_add(1, Ordering::Relaxed);
        MAPPING[6].fetch_add(pages, Ordering::Relaxed);
        let old = PER_CALL_US.load(Ordering::Relaxed);
        PER_CALL_US.store(((old * 7 + call_us / calls) / 8).max(1), Ordering::Relaxed);
        let _ = BACKLOG_US.fetch_update(Ordering::AcqRel, Ordering::Relaxed, |b| Some(b.saturating_sub(job.estimate)));
        {
            let mut samples = SAMPLES.lock().unwrap_or_else(|e| e.into_inner());
            if samples[0].len() < 100_000 {
                samples[0].push((call_us / calls).min(u64::from(u32::MAX)) as u32);
                samples[1].push(waited.min(u64::from(u32::MAX)) as u32);
                samples[2].push(signal_us.min(u64::from(u32::MAX)) as u32);
            }
        }
        // The window's work, and the most in a window.
        let now = crate::host::now_ms();
        if now >= WINDOW_AT.load(Ordering::Relaxed) + WINDOW_MS {
            MAPPING[2].fetch_max(WINDOW_WORK.swap(0, Ordering::Relaxed), Ordering::Relaxed);
            WINDOW_AT.store(now, Ordering::Relaxed);
        }
        WINDOW_WORK.fetch_add(call_us, Ordering::Relaxed);
        drop(job);
    }
}

/// (`VICTORY_PAGE_MIGRATE`) Every copy on pages by interface pointer: its pages
/// in its tiles' order (heap, first page, pages), its tiling and description,
/// when it was made, and its host record once that is made ([`adopt`]). The
/// pages here are the copy's own (a move swaps them), given back when it goes.
struct Copy {
    segs: Vec<(usize, u32, u32)>,
    tiling: Tiling,
    desc: D3D12_RESOURCE_DESC,
    made: u64,
    host: Option<std::sync::Weak<crate::resource::HostTexture>>,
}

// SAFETY: plain data and a weak reference to a shared record.
unsafe impl Send for Copy {}

static COPIES: Mutex<Option<std::collections::HashMap<usize, Copy>>> = Mutex::new(None);

fn copies() -> std::sync::MutexGuard<'static, Option<std::collections::HashMap<usize, Copy>>> {
    COPIES.lock().unwrap_or_else(|e| e.into_inner())
}

fn flat(runs: &Runs) -> Vec<(usize, u32, u32)> {
    runs.iter().flat_map(|(i, rs)| rs.iter().map(move |&(start, n)| (*i, start, n))).collect()
}

fn grouped(segs: &[(usize, u32, u32)]) -> Runs {
    let mut out: Runs = Vec::new();
    for &(i, start, n) in segs {
        match out.iter_mut().find(|g| g.0 == i) {
            Some(g) => g.1.push((start, n)),
            None => out.push((i, vec![(start, n)])),
        }
    }
    out
}

/// The host record of a copy, for moving its pages (`resource.rs`, where
/// the record is made); nothing for a copy not on pages.
pub(crate) fn adopt(host: &std::sync::Arc<crate::resource::HostTexture>) {
    if mode() != Mode::On {
        return;
    }
    if let Some(copy) = copies().as_mut().and_then(|c| c.get_mut(&(host.texture.as_raw() as usize))) {
        copy.host = Some(std::sync::Arc::downgrade(host));
    }
}

mod migrate;


/// An object the copy holds and releases when it goes: its pages rest then.
#[repr(C)]
struct Reclaim {
    methods: &'static [*const (); 3],
    references: AtomicU32,
    runs: Runs,
    pages: u32,
    texture: usize,
}

struct Methods([*const (); 3]);
// SAFETY: function addresses only.
unsafe impl Sync for Methods {}

static RECLAIM: Methods = Methods([query as *const (), add_ref as *const (), release as *const ()]);

unsafe extern "system" fn query(this: *mut c_void, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    // SAFETY: COM's arguments.
    unsafe {
        if out.is_null() {
            return HRESULT(0x8000_4003_u32 as i32);
        }
        if iid.as_ref().is_some_and(|iid| *iid == IUnknown::IID) {
            add_ref(this);
            *out = this;
            return HRESULT(0);
        }
        *out = std::ptr::null_mut();
    }
    HRESULT(0x8000_4002_u32 as i32)
}

unsafe extern "system" fn add_ref(this: *mut c_void) -> u32 {
    // SAFETY: one of ours, alive while a reference to it is.
    unsafe { (*this.cast::<Reclaim>()).references.fetch_add(1, Ordering::Relaxed) + 1 }
}

unsafe extern "system" fn release(this: *mut c_void) -> u32 {
    // SAFETY: as above; the last reference frees the box it was made as.
    unsafe {
        let left = (*this.cast::<Reclaim>()).references.fetch_sub(1, Ordering::AcqRel) - 1;
        if left == 0 {
            let gone = Box::from_raw(this.cast::<Reclaim>());
            MADE[M_ALIVE].fetch_sub(1, Ordering::Relaxed);
            MADE[M_ALIVE_PAGES].fetch_sub(u64::from(gone.pages), Ordering::Relaxed);
            if let Some(set) = ON_PAGES.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
                set.remove(&gone.texture);
            }
            // Its pages as of now (a move may have swapped them).
            let runs = copies().as_mut().and_then(|c| c.remove(&gone.texture)).map_or(gone.runs, |c| grouped(&c.segs));
            lock().resting.push_back((crate::host::now_ms() + REST, runs));
        }
        left
    }
}

fn give_back_with(texture: &ID3D12Resource, runs: Runs, pages: u32) {
    const KEY: GUID = GUID::from_u128(0x3c1b7a52_8e0d_4f6a_b2d9_5a7e01c4f9a3);
    let raw = texture.as_raw() as usize;
    let made = Box::into_raw(Box::new(Reclaim { methods: &RECLAIM.0, references: AtomicU32::new(1), runs, pages, texture: raw }));
    if super::heap_pack().1 {
        ON_PAGES.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Default::default).insert(raw);
    }
    // SAFETY: a COM object of ours with one reference, which this takes and
    // drops at the end; the texture takes its own.
    unsafe {
        let object = IUnknown::from_raw(made.cast());
        if texture.SetPrivateDataInterface(&KEY, &object).is_err() {
            // Never given back: the pages stay taken under a texture that
            // may be alive.
            std::mem::forget(object);
        }
    }
}

/// Once a second ([`super::room`]): rested pages free, empty heaps past the
/// kept ones go.
pub(crate) fn tick(now: u64, keep: usize) {
    if mode() != Mode::On {
        return;
    }
    if let Ok(mut pages) = PAGES.try_lock() {
        pages.rested(now, keep);
        HEAPS_THEN.store(pages.count() as u64, Ordering::Relaxed);
    }
    migrate::start();
}

/// The line of every 5 s (printed at zero).
#[cfg(feature = "diagnostics")]
pub(crate) fn line(budget: u64, raw: u64, net: u64) -> String {
    let mb = |b: u64| b >> 20;
    let (heaps, empty, free_pages, resting) = {
        let p = lock();
        let resting: u64 = p.resting.iter().map(|r| r.1.iter().map(|h| h.1.iter().map(|x| u64::from(x.1)).sum::<u64>()).sum::<u64>()).sum();
        (p.count() as u64, p.empty() as u64, p.heaps.iter().flatten().map(|h| u64::from(h.free.pages)).sum::<u64>(), resting)
    };
    let m: [u64; 9] = std::array::from_fn(|i| MADE[i].load(Ordering::Relaxed));
    let calls = MAPPING[0].swap(0, Ordering::Relaxed);
    let us = MAPPING[1].swap(0, Ordering::Relaxed);
    let most = MAPPING[2].swap(0, Ordering::Relaxed);
    let mapped_pages = MAPPING[6].swap(0, Ordering::Relaxed);
    let jobs_done = MAPPING[3].swap(0, Ordering::Relaxed);
    static JOBS_BEFORE: AtomicU64 = AtomicU64::new(0);
    let enqueued = ENQUEUED.load(Ordering::Acquire);
    let jobs_queued = enqueued - JOBS_BEFORE.swap(enqueued, Ordering::Relaxed).min(enqueued);
    let mut samples = std::mem::take(&mut *SAMPLES.lock().unwrap_or_else(|e| e.into_inner()));
    let p50_max = |v: &mut Vec<u32>| {
        v.sort_unstable();
        (v.get(v.len() / 2).copied().unwrap_or(0), v.last().copied().unwrap_or(0))
    };
    let call = p50_max(&mut samples[0]);
    let wait = p50_max(&mut samples[1]);
    let signal = p50_max(&mut samples[2]);
    let mut out = format!(
        "texture page mappings (mapping thread) in the last 5 s: jobs queued {jobs_queued}, done {jobs_done}; {calls} calls of {mapped_pages} pages, \
         {us} us in the calls ({:.0} us a call, {:.2} us a page); a call p50 {} us max {} us; waiting in the queue p50 {} us max {} us; \
         fence signal p50 {} us max {} us; cost a call now {} us, queued {} us, failed jobs {}; ",
        us as f64 / calls.max(1) as f64,
        us as f64 / mapped_pages.max(1) as f64,
        call.0,
        call.1,
        wait.0,
        wait.1,
        signal.0,
        signal.1,
        PER_CALL_US.load(Ordering::Relaxed),
        BACKLOG_US.load(Ordering::Relaxed),
        MAPPING[5].load(Ordering::Relaxed),
    );
    out += &format!(
        "texture pages (VICTORY_PAGE_POOL={}): copies on pages {} alive ({} made{}), {} pages alive ({} MB; placed they would take {} MB of {} MB made), \
         heaps {} ({} MB; {} empty), free pages {} MB, resting {} MB, heaps made {} / given back {}, pieces from unripe heaps {}, heaps ahead held by the cap {}; \
         card raw {} MB, net {} MB, budget {} MB; mapping in the last 5 s: {} calls, {} us, the most in a {} ms window {} us (queued at most {} us); placed instead:",
        match mode() {
            Mode::On => "1",
            Mode::Verify => "verify",
            Mode::Off => "0",
        },
        m[M_ALIVE],
        m[M_COPIES],
        if mode() == Mode::Verify { " would have gone" } else { "" },
        m[M_ALIVE_PAGES],
        mb(m[M_ALIVE_PAGES] * PAGE),
        mb(m[M_PLACED]),
        mb(m[M_PAGES] * PAGE),
        heaps,
        mb(heaps * HEAP_BYTES),
        empty,
        mb(free_pages * PAGE),
        mb(resting * PAGE),
        m[M_HEAPS],
        m[M_GIVEN_BACK],
        m[M_UNRIPE],
        m[M_AHEAD_HELD],
        mb(raw),
        mb(net),
        mb(budget),
        calls,
        us,
        WINDOW_MS,
        most,
        budget_us(),
    );
    out.push_str(&migrate::line());
    for (i, name) in WHY_NAMES.iter().enumerate() {
        out.push_str(&format!(
            " {name} {} ({} MB);",
            FALLBACK[0][i].load(Ordering::Relaxed),
            mb(FALLBACK[1][i].load(Ordering::Relaxed))
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_follow_the_tiles_order_across_levels_and_the_packed_ones() {
        // Level 0: 4 x 2 tiles, level 1: 2 x 1, packed 2 tiles: 12 tiles.
        let t = Tiling { standard: vec![(4, 8), (2, 2)], packed: Some((2, 2)), total: 12 };
        let (at, size) = regions(&t, 0, 12);
        assert_eq!(at.len(), 3);
        assert_eq!(size.iter().map(|s| s.NumTiles).sum::<u32>(), 12);
        // Tiles 5..11 begin in level 0's second row, second column.
        let (at, size) = regions(&t, 5, 11);
        assert_eq!((at[0].Subresource, at[0].X, at[0].Y, size[0].NumTiles), (0, 1, 1, 3));
        assert_eq!((at[1].Subresource, size[1].NumTiles), (1, 2));
        assert_eq!((at[2].Subresource, at[2].X, size[2].NumTiles), (2, 0, 1));
    }

    #[test]
    fn free_runs_are_taken_lowest_first_and_join_again() {
        let mut f = Free { runs: vec![(0, 16)], pages: 16 };
        let mut got = Vec::new();
        assert_eq!(f.take(5, &mut got), 5);
        let mut more = Vec::new();
        assert_eq!(f.take(20, &mut more), 11);
        assert_eq!(f.pages, 0);
        f.give(&more);
        f.give(&got);
        assert_eq!(f.runs, vec![(0, 16)]);
        assert_eq!(f.pages, 16);
        let mut a = Vec::new();
        f.take(4, &mut a);
        let mut b = Vec::new();
        f.take(4, &mut b);
        f.give(&a);
        let mut c = Vec::new();
        assert_eq!(f.take(6, &mut c), 6);
        assert_eq!(c, vec![(0, 4), (8, 2)], "the lowest free pages, in two runs");
    }

    /// A copy on pages on the card: made reserved with all its tiles, its
    /// pages rest when it goes, the device stays.
    #[test]
    fn a_copy_on_pages_is_mapped_and_gives_its_pages_back() {
        let _card = crate::pool::tests::CARD_TEST.lock().unwrap_or_else(|e| e.into_inner());
        let Some(host) = crate::host::host() else { return };
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Width: 1024,
            Height: 512,
            DepthOrArraySize: 1,
            MipLevels: 11,
            Format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_BC1_UNORM,
            SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            ..Default::default()
        };
        // SAFETY: plain query.
        let placed = unsafe { host.device.GetResourceAllocationInfo(0, &[desc]) }.SizeInBytes;
        let alive = MADE[M_ALIVE].load(Ordering::Relaxed);
        let resting = || lock().resting.len();
        let before = resting();
        // SAFETY: a valid description.
        let made = unsafe { texture(&host.device, &desc, D3D12_RESOURCE_STATE_COMMON, placed) };
        let t = match made {
            Ok(t) => t,
            Err(why) => panic!("not on pages: {:?}", why.map(|w| WHY_NAMES[w as usize])),
        };
        assert_eq!(MADE[M_ALIVE].load(Ordering::Relaxed), alive + 1);
        // The mapping thread holds the copy until it is mapped and signalled.
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while crate::host::references(&t) > 1 && std::time::Instant::now() < until {
            std::thread::yield_now();
        }
        assert_eq!(crate::host::references(&t), 1, "what the copy holds is no reference to it");
        let worker = worker(&host.device).unwrap();
        // SAFETY: a live fence of ours.
        let done = || unsafe { worker.fence.GetCompletedValue() } >= ENQUEUED.load(Ordering::Acquire);
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !done() && std::time::Instant::now() < until {
            std::thread::yield_now();
        }
        assert!(done(), "the mapping was signalled");
        drop(t);
        // The mapping thread lets its reference go once it has mapped it.
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while resting() == before && std::time::Instant::now() < until {
            std::thread::yield_now();
        }
        assert_eq!(resting(), before + 1);
        // SAFETY: plain query.
        assert!(unsafe { host.device.GetDeviceRemovedReason() }.is_ok());
    }
}
