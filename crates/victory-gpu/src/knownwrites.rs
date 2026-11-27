//! (Coder J, ledger F21/F23/F24) `VICTORY_KNOWN_WRITES=verify|1` (default
//! off): the layer's own DIRTY LEDGER for buffer copies and windows, used as
//! a FILTER in front of the old tracking. A page the ledger has no mark on
//! since its take is "certainly not written": no write-watch ask, no guard
//! arm, no compare. A page it has marked still goes through the old exact
//! tracking. So copy volume stays as today, and a coarse mark costs an ask,
//! never stale data.
//!
//! Marks (one per 64 KiB page: the highest asking number noted, the source,
//! the frame), lock-free in a two-level table over the address space:
//! (a) our landings of shader writes / copy-back, (b) our list/placed copies
//! and other copies into title memory, (c) kernel / DirectStorage writes we
//! issue, (d) mapping changes and new writable views of the same physical
//! bytes (`watch::views_mapped`) - through `lockmeter::note`, and again once
//! the bytes are in where `watch` is told before (landings, our copies);
//! (e) the engine's buffer lock windows (hooks in `victory-hle/src/lockhook.rs`):
//! dirty while open, marked whole at the unlock. Other title addresses that
//! show the same physical pages (`inplace::shown_elsewhere`) get the same
//! marks, lazily, at the next look.
//!
//! A page is dirty for a copy that took it at asking `t` if its mark has a
//! number >= `t`, or a lock window over it is open, or (mapping changes
//! only) the mark is from the last [`RECENT`] frames.
//!
//! Which pages the filter may spare ("event-covered"): not in the graphics
//! heap (arena C is left to its own path, `VICTORY_CHUNK_FINAL`; the rest has
//! no event known); shown at no other title address (a page of a CPU-visible
//! physical chunk is written through its other view by writers no event
//! names: run-141's changed misses, all in streaming GPU-view windows); seen
//! in refreshes for `VICTORY_KNOWN_WRITES_PROBATION` frames (default 120)
//! with no write the filter would have missed. Demoted for good: a page with
//! such a write that the write-watch saw (a real CPU write, whatever the
//! bytes), or with changed bytes; and all of a window or copy of at most
//! 16 MiB with such a write in it.
//!
//! `=verify`: the old tracking decides; the ledger counts beside it in
//! `Shadow::copy_stale`. `=1`: the filter (`Shadow::update_filtered`):
//! spared pages are not asked about; the rest go the usual way and are
//! counted as in verify. Every refresh the filter does not filter is counted
//! by its reason (line "known writes FILTER ON"); the mode and the gates are
//! said once at the first line.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicPtr, AtomicU8, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::{Mutex, OnceLock};

const PAGE: usize = crate::watch::PAGE;
const WHOLE: u16 = u16::MAX;
/// Frames a mapping change keeps its pages dirty whatever was taken since.
const RECENT: u32 = 2;
/// Copies up to this size are demoted whole on a real miss.
const SMALL_COPY: u64 = 16 << 20;

/// Sources, as `lockmeter::Source` numbers them (0-5), then the ledger's own.
pub(crate) const SRC_LANDING: u8 = 0;
pub(crate) const SRC_OUR_COPY: u8 = 2;
pub(crate) const SRC_MAPPED: u8 = 5;
const SRC_ALIAS: u8 = 6;
const SRC_WIN_CLOSED: u8 = 7;
const SRC_WIN_OPEN: u8 = 8;
const SOURCES: usize = 9;
const SOURCE_NAMES: [&str; SOURCES] = [
    "landing/copy-back",
    "list/placed copy",
    "other copy of ours",
    "kernel read",
    "kernel pages",
    "mapping change / new view",
    "other view of the same pages",
    "lock window closed after the take",
    "lock window open",
];

/// 0 off, 1 verify, 2 the filter (`=1`). Any other value is off (said at
/// the first line).
pub fn mode() -> u8 {
    let m = MODE.load(Relaxed);
    if m != u8::MAX {
        return m;
    }
    let m = parse_mode(std::env::var("VICTORY_KNOWN_WRITES").ok().as_deref());
    // (A test's mode set meanwhile stays.)
    match MODE.compare_exchange(u8::MAX, m, Relaxed, Relaxed) {
        Ok(_) => m,
        Err(set) => set,
    }
}
static MODE: AtomicU8 = AtomicU8::new(u8::MAX);

fn parse_mode(value: Option<&str>) -> u8 {
    match value.map(str::trim) {
        Some("verify") => 1,
        Some("1") => 2,
        _ => 0,
    }
}

fn mode_name(m: u8) -> &'static str {
    match m {
        1 => "verify (the old tracking decides; the filter only counted)",
        2 => "1 (the FILTER: spared pages are not asked about)",
        _ => "off",
    }
}

/// Spared pages between asked runs of up to this many pages are asked with
/// them (`VICTORY_KNOWN_WRITES_GAP`, default 8): one ask instead of two.
pub(crate) fn gap() -> usize {
    static N: OnceLock<usize> = OnceLock::new();
    *N.get_or_init(|| std::env::var("VICTORY_KNOWN_WRITES_GAP").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(8))
}

/// `VICTORY_KNOWN_WRITES=verify` or `=1`.
pub fn on() -> bool {
    mode() != 0
}

fn probation() -> u32 {
    static N: OnceLock<u32> = OnceLock::new();
    *N.get_or_init(|| std::env::var("VICTORY_KNOWN_WRITES_PROBATION").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(120))
}

/// The frame as kept in the table (0: none).
fn frame_mark() -> u32 {
    (crate::frame::current() as u32).wrapping_add(1).max(1)
}

// --- The table ---

const CHUNK_PAGES: usize = 1 << 14;
const TOP: usize = (1usize << 47) / PAGE / CHUNK_PAGES;

/// 1 GiB of title addresses. `state`: bits 0-31 first seen (frame mark),
/// bit 32 demoted, bit 33 shown at another address, bits 34-63 the frame
/// mark (low 30 bits) of that look.
struct Chunk {
    stamp: [AtomicU64; CHUNK_PAGES],
    frame: [AtomicU32; CHUNK_PAGES],
    src: [AtomicU8; CHUNK_PAGES],
    state: [AtomicU64; CHUNK_PAGES],
    /// The frame mark of the last demotion, and how many there were (saturating).
    demoted_at: [AtomicU32; CHUNK_PAGES],
    demotions: [AtomicU8; CHUNK_PAGES],
    /// Landings in flight on the page (`landing` .. `landed`): dirty for
    /// every take meanwhile, whatever the numbers. One with no end keeps it
    /// dirty for good (asked: never "clean").
    inflight: [AtomicU8; CHUNK_PAGES],
}

/// Chunks of the table at most (`VICTORY_KNOWN_WRITES_TABLE_GIB`, default
/// 192 GiB of title addresses, ~0.37 MB each). Past it a page has no state
/// and no marks: it is watched (today's tracking), counted.
fn table_most() -> u64 {
    static N: OnceLock<u64> = OnceLock::new();
    *N.get_or_init(|| std::env::var("VICTORY_KNOWN_WRITES_TABLE_GIB").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(192))
}
/// Since the line: pages watched because the table is full.
static TABLE_FULL: AtomicU64 = AtomicU64::new(0);

/// Frames after a demotion before the page is tried again (in probation,
/// watched meanwhile), doubled for each further demotion up to 64 times
/// (`VICTORY_KNOWN_WRITES_RETRY`, default 1800).
fn retry() -> u32 {
    static N: OnceLock<u32> = OnceLock::new();
    *N.get_or_init(|| std::env::var("VICTORY_KNOWN_WRITES_RETRY").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(1800))
}

static TABLE: [AtomicPtr<Chunk>; TOP] = [const { AtomicPtr::new(std::ptr::null_mut()) }; TOP];
static CHUNKS_MADE: AtomicU64 = AtomicU64::new(0);

fn chunk(page: usize, make: bool) -> Option<&'static Chunk> {
    let slot = TABLE.get(page / CHUNK_PAGES)?;
    let p = slot.load(std::sync::atomic::Ordering::Acquire);
    if !p.is_null() {
        // SAFETY: chunks are made once and never freed.
        return Some(unsafe { &*p });
    }
    if !make || CHUNKS_MADE.load(Relaxed) >= table_most() {
        TABLE_FULL.fetch_add(u64::from(make), Relaxed);
        return None;
    }
    let layout = std::alloc::Layout::new::<Chunk>();
    // SAFETY: all atomics: zero is a valid value of each.
    let new = unsafe { std::alloc::alloc_zeroed(layout) }.cast::<Chunk>();
    if new.is_null() {
        return None;
    }
    match slot.compare_exchange(std::ptr::null_mut(), new, std::sync::atomic::Ordering::AcqRel, std::sync::atomic::Ordering::Acquire) {
        Ok(_) => {
            CHUNKS_MADE.fetch_add(1, Relaxed);
            // SAFETY: ours, now in the table for good.
            Some(unsafe { &*new })
        }
        Err(other) => {
            // SAFETY: ours, never shared.
            unsafe { std::alloc::dealloc(new.cast(), layout) };
            // SAFETY: as above.
            Some(unsafe { &*other })
        }
    }
}

fn mark(at: usize, len: usize, stamp: u64, frame: u32, src: u8) {
    let (first, last) = (at / PAGE, at.saturating_add(len).div_ceil(PAGE));
    let mut page = first;
    while page < last {
        let Some(c) = chunk(page, true) else { return };
        let end = last.min((page / CHUNK_PAGES + 1) * CHUNK_PAGES);
        for p in page..end {
            let i = p % CHUNK_PAGES;
            c.stamp[i].fetch_max(stamp, Relaxed);
            c.src[i].store(src, Relaxed);
            match src {
                // A new mapping: whether the page is shown elsewhere is looked at again at once.
                SRC_MAPPED => {
                    c.frame[i].store(frame, Relaxed);
                    c.state[i].fetch_and(!LOOK_BITS, Relaxed);
                }
                // Marked through another view: shown elsewhere, so watched from now on (until a look says otherwise).
                SRC_ALIAS => {
                    c.state[i].fetch_or(MULTI, Relaxed);
                }
                _ => {}
            }
        }
        page = end;
    }
}

/// Since the line: bytes noted, notes, alias notes dropped (queue full),
/// other-view marks, time in notes (ns), time resolving other views (ns).
static NOTED: [AtomicU64; 6] = [const { AtomicU64::new(0) }; 6];

static ALIASES: Mutex<Vec<(usize, usize, u64, u32)>> = Mutex::new(Vec::new());

/// (`lockmeter::note` and the after-notes) Title memory `[at, at + len)` is
/// written by us, the kernel for the title, or changed its mapping.
pub(crate) fn note(source: u8, at: usize, len: usize) {
    if !on() || len == 0 {
        return;
    }
    let began = std::time::Instant::now();
    let stamp = crate::watch::asking_now().max(1);
    let frame = frame_mark();
    mark(at, len, stamp, frame, source.min(SRC_MAPPED));
    // Other views matter only for a mapping change: the page there is then
    // shown elsewhere (watched); any other write on a page shown twice lands
    // on pages watched already.
    if source == SRC_MAPPED {
        let mut q = ALIASES.lock().unwrap_or_else(|e| e.into_inner());
        if q.len() < 1 << 16 {
            q.push((at, len, stamp, frame));
        } else {
            NOTED[2].fetch_add(1, Relaxed);
        }
    }
    NOTED[0].fetch_add(len as u64, Relaxed);
    NOTED[1].fetch_add(1, Relaxed);
    NOTED[4].fetch_add(began.elapsed().as_nanos() as u64, Relaxed);
}

/// `watch::landing`: shader writes are about to go into `[at, at + len)`.
pub(crate) fn landing(at: usize, len: usize) {
    if !on() || len == 0 {
        return;
    }
    for page in at / PAGE..at.saturating_add(len).div_ceil(PAGE) {
        if let Some(c) = chunk(page, true) {
            let _ = c.inflight[page % CHUNK_PAGES].fetch_update(Relaxed, Relaxed, |n| n.checked_add(1));
        }
    }
}

/// The landing's bytes are in (after `landing` of the same range): noted
/// now, with the asking current now, and no longer in flight.
pub(crate) fn landed(at: usize, len: usize) {
    if !on() || len == 0 {
        return;
    }
    note(SRC_LANDING, at, len);
    for page in at / PAGE..at.saturating_add(len).div_ceil(PAGE) {
        if let Some(c) = chunk(page, false) {
            // (Saturated at 255: stays in flight, dirty, for good.)
            let _ = c.inflight[page % CHUNK_PAGES].fetch_update(Relaxed, Relaxed, |n| (n != 0 && n != u8::MAX).then(|| n - 1));
        }
    }
}

/// Marks the other views of the notes queued (outside every lock of ours:
/// `shown_elsewhere` takes the views' lock).
fn resolve_aliases() {
    let queued = {
        let Ok(mut q) = ALIASES.try_lock() else { return };
        if q.is_empty() {
            return;
        }
        std::mem::take(&mut *q)
    };
    let began = std::time::Instant::now();
    // Notes that meet are looked up as one (the highest number and frame:
    // dirtier, never cleaner).
    let mut queued = queued;
    queued.sort_unstable_by_key(|q| q.0);
    let mut merged: Vec<(usize, usize, u64, u32)> = Vec::with_capacity(queued.len());
    for (at, len, stamp, frame) in queued {
        match merged.last_mut() {
            Some(m) if at <= m.0 + m.1 => {
                m.1 = m.1.max(at + len - m.0);
                m.2 = m.2.max(stamp);
                m.3 = m.3.max(frame);
            }
            _ => merged.push((at, len, stamp, frame)),
        }
    }
    let mut n = 0;
    for (at, len, stamp, frame) in merged {
        let end = at + len;
        for (page, others) in crate::inplace::shown_elsewhere(at & !(PAGE - 1), end) {
            let (a, b) = (at.max(page), end.min(page + PAGE));
            if a >= b {
                continue;
            }
            for other in others {
                // Watched already: nothing to add.
                if chunk(other / PAGE, false).is_some_and(|c| c.state[(other / PAGE) % CHUNK_PAGES].load(Relaxed) & MULTI != 0) {
                    continue;
                }
                mark(other + (a - page), b - a, stamp, frame, SRC_ALIAS);
                n += 1;
            }
        }
    }
    NOTED[3].fetch_add(n, Relaxed);
    NOTED[5].fetch_add(began.elapsed().as_nanos() as u64, Relaxed);
}

// --- (e) Lock windows ---

#[derive(Default)]
struct Windows {
    /// Open write windows: start -> (end, how many).
    open: BTreeMap<u64, (u64, u32)>,
    longest: u64,
    /// Windows open by object: pointer, bytes, for write.
    objects: HashMap<u64, Vec<(u64, u64, bool)>>,
}

fn windows() -> std::sync::MutexGuard<'static, Windows> {
    static W: OnceLock<Mutex<Windows>> = OnceLock::new();
    W.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner())
}

/// Windows forgotten for want of room (they stay open = dirty).
static WIN_DROPPED: AtomicU64 = AtomicU64::new(0);
/// The open windows overflowed: every page counts as marked.
static WIN_FULL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The ledger cost more than `VICTORY_KNOWN_WRITES_MAX_MS` (default 3) a
/// frame in the last second: with `=1` the filter stands aside for the next
/// second (today's tracking for all); counted in verify too.
static OVER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static OVER_SECONDS: AtomicU64 = AtomicU64::new(0);
fn max_ms() -> f64 {
    static N: OnceLock<f64> = OnceLock::new();
    *N.get_or_init(|| std::env::var("VICTORY_KNOWN_WRITES_MAX_MS").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(3.0))
}

/// Write windows open now (the lookup takes the lock only then).
static OPEN: AtomicU64 = AtomicU64::new(0);
/// Since the line: write windows opened, closed, unlocks with no window.
static LOCKS: [AtomicU64; 3] = [const { AtomicU64::new(0) }; 3];

/// (`lockmeter::locked`) The engine's inner lock returned.
pub(crate) fn locked(object: u64, mode: u8, ok: bool, ptr: u64, bytes: u64) {
    if !on() || !ok {
        return;
    }
    let write = (2..=5).contains(&mode) && ptr != 0 && bytes != 0;
    let mut w = windows();
    // Bounded: an object that never unlocks (0x14173d3c0) keeps 256 windows;
    // one forgotten here stays open below (dirty: never "clean").
    if w.objects.len() >= 1 << 16 && !w.objects.contains_key(&object) {
        w.objects.clear();
        WIN_DROPPED.fetch_add(1, Relaxed);
    }
    let list = w.objects.entry(object).or_default();
    if list.len() >= 256 {
        list.remove(0);
        WIN_DROPPED.fetch_add(1, Relaxed);
    }
    list.push((ptr, bytes, write));
    if !write {
        return;
    }
    if w.open.len() >= 1 << 16 {
        // More open buffers than kept: every page counts as marked from now
        // on (today's tracking for all), counted.
        WIN_FULL.store(true, Relaxed);
        return;
    }
    LOCKS[0].fetch_add(1, Relaxed);
    lock_buffer(ptr as usize, bytes as usize);
    w.longest = w.longest.max(bytes);
    let e = w.open.entry(ptr).or_insert((ptr + bytes, 0));
    e.0 = e.0.max(ptr + bytes);
    e.1 += 1;
    OPEN.fetch_add(1, Relaxed);
}

/// (`lockmeter::unlocked`) The unlock was entered.
pub(crate) fn unlocked(object: u64, ptr: u64) {
    if !on() {
        return;
    }
    let mut w = windows();
    let Some(list) = w.objects.get_mut(&object) else {
        LOCKS[2].fetch_add(1, Relaxed);
        return;
    };
    let at = list.iter().position(|x| x.0 == ptr).unwrap_or(list.len() - 1);
    let (p, bytes, write) = list.swap_remove(at);
    if list.is_empty() {
        w.objects.remove(&object);
    }
    if !write {
        return;
    }
    LOCKS[1].fetch_add(1, Relaxed);
    let mut end = p + bytes;
    if let Some(e) = w.open.get_mut(&p) {
        end = end.max(e.0);
        e.1 -= 1;
        if e.1 == 0 {
            w.open.remove(&p);
        }
        OPEN.fetch_sub(1, Relaxed);
    }
    drop(w);
    // The buffer is final now: marked whole, after the title's writes.
    mark(p as usize, (end - p) as usize, crate::watch::asking_now().max(1), frame_mark(), SRC_WIN_CLOSED);
}

/// The pages of a buffer locked for write: watched for good.
fn lock_buffer(at: usize, len: usize) {
    let (first, last) = (at / PAGE, at.saturating_add(len).div_ceil(PAGE));
    for page in first..last.min(first + (1 << 16)) {
        if let Some(c) = chunk(page, true) {
            c.state[page % CHUNK_PAGES].fetch_or(LOCKBUF, Relaxed);
        }
    }
}

#[cfg(test)]
fn window_open(page: usize) -> bool {
    if WIN_FULL.load(Relaxed) {
        return true;
    }
    if OPEN.load(Relaxed) == 0 {
        return false;
    }
    let (a, b) = ((page * PAGE) as u64, ((page + 1) * PAGE) as u64);
    let w = windows();
    w.open.range(a.saturating_sub(w.longest)..b).any(|(_, &(end, _))| end > a)
}

// --- Verify ---

/// One page a refresh looked at: taken before at `taken`; last found written
/// by the old tracking at `write`; the 4 KiB parts written since the take
/// (`u16::MAX`: the whole page, or not known finer: a guard's answer).
pub(crate) struct Seen {
    pub page: usize,
    pub taken: u64,
    pub write: u64,
    pub parts: u16,
}

const DEMOTED: u64 = 1 << 32;
const MULTI: u64 = 1 << 33;
/// A buffer the engine's lock named for write lies on the page: watched
/// (F23 names callers that keep the pointer past the unlock; run-143 saw a
/// write after the window closed).
const LOCKBUF: u64 = 1 << 34;
const LOOK_SHIFT: u32 = 35;
const LOOK_MASK: u32 = (1 << 29) - 1;
const LOOK_BITS: u64 = (LOOK_MASK as u64) << LOOK_SHIFT;
/// Frames between looks whether a page is shown at another address.
const LOOK_EVERY: u32 = 300;

/// Since the line, bytes unless said.
static C: [AtomicU64; 33] = [const { AtomicU64::new(0) }; 33];
const LOOKED: usize = 0;
const COVERED: usize = 1;
const W_HEAP: usize = 2;
const W_MULTI: usize = 3;
const W_DEMOTED: usize = 4;
const W_PROBATION: usize = 5;
const FOUND: usize = 6;
const FOUND_MARKED: usize = 7;
const FOUND_COVERED: usize = 8;
const SKIP: usize = 9;
const ASK: usize = 10;
const SKIP_PAGES: usize = 11;
const SKIP_WHOLE: usize = 12;
const SKIP_PART: usize = 13;
/// Filter misses (pages): all, unchanged, changed, not compared; of them seen by the write-watch.
const MISS: usize = 14;
const MISS_SAME: usize = 15;
const MISS_CHANGED: usize = 16;
const MISS_UNCOMPARED: usize = 17;
const MISS_WATCHED: usize = 18;
/// Unexplained on pages in probation: unchanged, changed or not compared.
const PROB_SAME: usize = 19;
const PROB_CHANGED: usize = 20;
const EXTRA: usize = 21;
const DEMOTE_PAGES: usize = 22;
const DEMOTE_COPIES: usize = 23;
const NANOS: usize = 24;
const COMPARE_NS: usize = 25;
const REFRESHES: usize = 26;
const LOOK_NS: usize = 27;
const LOOKS: usize = 28;
const RETRIED: usize = 29;
const NO_STATE: usize = 30;
const LOOP_NS: usize = 31;
const W_LOCKBUF: usize = 32;
/// Since the line: bytes of marked pages found unwritten, by the source of their mark.
static EXTRA_BY: [AtomicU64; SOURCES] = [const { AtomicU64::new(0) }; SOURCES];
/// Since the start: filter misses (pages), of them changed or not compared, demoted pages.
static TOTAL: [AtomicU64; 3] = [const { AtomicU64::new(0) }; 3];

/// Compare budget left in this second.
static BUDGET: AtomicU64 = AtomicU64::new(BUDGET_BYTES);
const BUDGET_BYTES: u64 = 2 << 30;

struct Miss {
    address: usize,
    copy: u64,
    kind: &'static str,
    class: &'static str,
    differed: Option<bool>,
    writer: u64,
    frame: u32,
    taken: u64,
    write: u64,
    parts: u16,
    /// The page's mark: stamp, source, frames since (mapping changes only).
    stamp: u64,
    src: u8,
    /// Not compared because title memory could not be read there.
    unreadable: bool,
}

#[derive(Default)]
struct Book {
    /// First 32 changed filter misses, first 8 unchanged.
    changed: Vec<Miss>,
    same: Vec<Miss>,
    /// Changed filter misses by (copy kind, class, source of the page's
    /// mark or none, writer): pages, first address, copy.
    groups: HashMap<(&'static str, &'static str, u8, u64), (u64, usize, u64)>,
    /// Demotions by (copy, class, writer): pages, first address, copy kind.
    demotions: HashMap<(u64, &'static str, u64), (u64, usize, &'static str)>,
}

fn book() -> std::sync::MutexGuard<'static, Book> {
    static B: OnceLock<Mutex<Book>> = OnceLock::new();
    B.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner())
}

/// The page's state, made at its first look; whether it is shown at another
/// address looked at again every [`LOOK_EVERY`] frames.
fn state_of(c: &Chunk, i: usize, page: usize, now: u32, sums: &mut [u64; 33]) -> u64 {
    let mut st = c.state[i].load(Relaxed);
    // A demoted page is tried again after a while (watched meanwhile, and in
    // probation after): a writer may be gone; one that is still there demotes
    // it again, for longer.
    if st & DEMOTED != 0 {
        let times = u32::from(c.demotions[i].load(Relaxed)).clamp(1, 7);
        if now.wrapping_sub(c.demoted_at[i].load(Relaxed)) >= retry().saturating_mul(1 << (times - 1)) {
            let again = (st & !DEMOTED & !0xffff_ffff) | u64::from(now);
            if c.state[i].compare_exchange(st, again, Relaxed, Relaxed).is_ok() {
                sums[RETRIED] += 1;
                st = again;
            }
        }
    }
    let looked = (st >> LOOK_SHIFT) as u32;
    let fresh = st & 0xffff_ffff == 0;
    let due = fresh || looked == 0 || now.wrapping_sub(looked) & LOOK_MASK >= LOOK_EVERY;
    if !due {
        return st;
    }
    let began = std::time::Instant::now();
    let multi = crate::inplace::pages_of(page * PAGE).len() > 1;
    sums[LOOK_NS] += began.elapsed().as_nanos() as u64;
    sums[LOOKS] += 1;
    let first = if fresh { u64::from(now) } else { st & 0xffff_ffff };
    // (A look frame of 0 means "look again": kept off 0.)
    let look = u64::from((now & LOOK_MASK).max(1)) << LOOK_SHIFT;
    let new = first | (st & (DEMOTED | LOCKBUF)) | if multi { MULTI } else { 0 } | look;
    let _ = c.state[i].compare_exchange(st, new, Relaxed, Relaxed);
    new
}

/// `=verify`: a refresh of `[from, to)` of copy `copy` (`[start, start +
/// len)`) looked at `pages` (taken before) with the old tracking's answer;
/// the filter's answer is set beside it and counted. `compare(a, b)`:
/// whether title memory `[a, b)` differs from the copy's bytes from its last
/// take (`None`: not compared). Called before the take.
#[allow(clippy::too_many_arguments)]
pub(crate) fn verify(
    copy: u64,
    kind: &'static str,
    start: u64,
    len: u64,
    from: usize,
    to: usize,
    pages: &[Seen],
    compare: &dyn Fn(usize, usize) -> Option<bool>,
) {
    if !on() || pages.is_empty() {
        return;
    }
    let began = std::time::Instant::now();
    resolve_aliases();
    let now = frame_mark();
    let probation = probation();
    let clip = |page: usize| ((page * PAGE).max(from), ((page + 1) * PAGE).min(to));
    let mut sums = [0u64; 33];
    let mut extra_by = [0u64; SOURCES];
    sums[REFRESHES] = 1;
    // Written pages the filter would skip: (page, a, b, event-covered, seen, stamp, src).
    let mut missed: Vec<(usize, usize, usize, bool, &Seen, u64, u8)> = Vec::new();
    let (mut any_skip, mut all_skip) = (false, true);
    // Once per refresh: the heap's ranges, the write windows open over it.
    let ((c0, c1), (h0, h1)) = crate::lockmeter::heap_ranges();
    let win_full = WIN_FULL.load(Relaxed);
    let open_windows: Vec<(u64, u64)> = if OPEN.load(Relaxed) == 0 || win_full {
        Vec::new()
    } else {
        let (a, b) = (from as u64, to as u64);
        let w = windows();
        w.open.range(a.saturating_sub(w.longest)..b).filter(|(_, v)| v.0 > a).map(|(&k, v)| (k, v.0)).collect()
    };
    let looping = std::time::Instant::now();
    for s in pages {
        let (a, b) = clip(s.page);
        if a >= b {
            continue;
        }
        let bytes = (b - a) as u64;
        sums[LOOKED] += bytes;
        let Some(c) = chunk(s.page, true) else {
            sums[NO_STATE] += bytes;
            all_skip = false;
            continue;
        };
        let i = s.page % CHUNK_PAGES;
        let address = (s.page * PAGE) as u64;
        let heap = c1 != 0 && address >= c0 && address < c0 + c1 || address >= h0 && address < h1;
        let st = if heap { 0 } else { state_of(c, i, s.page, now, &mut sums) };
        let candidate = !heap && st & (DEMOTED | MULTI | LOCKBUF) == 0;
        let covered = candidate && now.wrapping_sub(st as u32) >= probation;
        sums[if covered {
            COVERED
        } else if heap {
            W_HEAP
        } else if st & MULTI != 0 {
            W_MULTI
        } else if st & LOCKBUF != 0 {
            W_LOCKBUF
        } else if st & DEMOTED != 0 {
            W_DEMOTED
        } else {
            W_PROBATION
        }] += bytes;
        let stamp = c.stamp[i].load(Relaxed);
        let src = c.src[i].load(Relaxed);
        let open = win_full || open_windows.iter().any(|&(x, y)| x < address + PAGE as u64 && y > address);
        let marked = open
            || c.inflight[i].load(Relaxed) != 0
            || stamp != 0 && stamp >= s.taken
            || src == SRC_MAPPED && {
                let f = c.frame[i].load(Relaxed);
                f != 0 && now.wrapping_sub(f) <= RECENT
            };
        if covered {
            if marked {
                sums[ASK] += bytes;
                all_skip = false;
            } else {
                sums[SKIP] += bytes;
                sums[SKIP_PAGES] += 1;
                any_skip = true;
            }
        } else {
            all_skip = false;
        }
        if s.write > s.taken {
            sums[FOUND] += bytes;
            if covered {
                sums[FOUND_COVERED] += bytes;
            }
            if marked {
                sums[FOUND_MARKED] += bytes;
            } else if candidate {
                missed.push((s.page, a, b, covered, s, stamp, src));
            }
        } else if marked {
            sums[EXTRA] += bytes;
            extra_by[if open { usize::from(SRC_WIN_OPEN) } else { usize::from(src).min(SOURCES - 1) }] += bytes;
        }
    }
    sums[LOOP_NS] = looping.elapsed().as_nanos() as u64;
    if any_skip {
        sums[if all_skip { SKIP_WHOLE } else { SKIP_PART }] += 1;
    }
    let comparing = std::time::Instant::now();
    let mut demote: Vec<(usize, u64)> = Vec::new();
    let mut whole_copy = false;
    for &(page, a, b, covered, s, stamp, src) in &missed {
        let bytes = (b - a) as u64;
        let budget = BUDGET.fetch_update(Relaxed, Relaxed, |l| l.checked_sub(bytes)).is_ok();
        let differed = if budget { compare(a, b) } else { None };
        // The write-watch's answer (4 KiB parts) is a CPU write, whatever its bytes.
        let watched = s.parts != 0 && s.parts != WHOLE;
        let real = watched || differed != Some(false);
        let writer = if real || covered { crate::lockmeter::writer_of(page, s.taken) } else { 0 };
        if covered {
            sums[MISS] += 1;
            sums[match differed {
                Some(false) => MISS_SAME,
                Some(true) => MISS_CHANGED,
                None => MISS_UNCOMPARED,
            }] += 1;
            if watched {
                sums[MISS_WATCHED] += 1;
            }
            TOTAL[0].fetch_add(1, Relaxed);
            if differed != Some(false) {
                TOTAL[1].fetch_add(1, Relaxed);
            }
            let class = crate::lockmeter::region_of((page * PAGE) as u64).1;
            let mut bk = book();
            let m = Miss { address: page * PAGE, copy, kind, class, differed, writer, frame: now, taken: s.taken, write: s.write, parts: s.parts, stamp, src, unreadable: budget && differed.is_none() };
            if differed != Some(false) {
                let key = (kind, class, if stamp == 0 { u8::MAX } else { src }, writer);
                if bk.groups.len() < 4096 || bk.groups.contains_key(&key) {
                    bk.groups.entry(key).or_insert((0, page * PAGE, copy)).0 += 1;
                }
                if bk.changed.len() < 32 {
                    tracing::warn!("known writes FILTER MISS, changed, #{} (VICTORY_KNOWN_WRITES): {}", bk.changed.len() + 1, describe(&m));
                    bk.changed.push(m);
                }
            } else if bk.same.len() < 8 {
                tracing::warn!("known writes FILTER MISS, unchanged, #{} (VICTORY_KNOWN_WRITES): {}", bk.same.len() + 1, describe(&m));
                bk.same.push(m);
            }
        } else {
            sums[if differed == Some(false) { PROB_SAME } else { PROB_CHANGED }] += 1;
        }
        if real {
            demote.push((page, writer));
            whole_copy |= len <= SMALL_COPY;
        }
    }
    sums[COMPARE_NS] = comparing.elapsed().as_nanos() as u64;
    if !demote.is_empty() {
        let set = |page: usize| -> bool {
            chunk(page, true).is_some_and(|c| {
                let i = page % CHUNK_PAGES;
                let first = c.state[i].fetch_or(DEMOTED, Relaxed) & DEMOTED == 0;
                if first {
                    c.demoted_at[i].store(now, Relaxed);
                    let _ = c.demotions[i].fetch_update(Relaxed, Relaxed, |n| n.checked_add(1));
                }
                first
            })
        };
        let mut bk = book();
        for &(page, writer) in &demote {
            if set(page) {
                sums[DEMOTE_PAGES] += 1;
                let class = crate::lockmeter::region_of((page * PAGE) as u64).1;
                if bk.demotions.len() < 1 << 16 {
                    bk.demotions.entry((copy, class, writer)).or_insert((0, page * PAGE, kind)).0 += 1;
                }
            }
        }
        if whole_copy {
            sums[DEMOTE_COPIES] += 1;
            for page in start as usize / PAGE..(start + len).div_ceil(PAGE as u64) as usize {
                if set(page) {
                    sums[DEMOTE_PAGES] += 1;
                }
            }
        }
    }
    TOTAL[2].fetch_add(sums[DEMOTE_PAGES], Relaxed);
    sums[NANOS] = began.elapsed().as_nanos() as u64;
    for (i, v) in sums.into_iter().enumerate() {
        if v != 0 {
            C[i].fetch_add(v, Relaxed);
        }
    }
    for (i, v) in extra_by.into_iter().enumerate() {
        if v != 0 {
            EXTRA_BY[i].fetch_add(v, Relaxed);
        }
    }
}

// --- `=1`: the filter ---

/// `VICTORY_KNOWN_WRITES=1` (whether the filter may act; [`gate`] then says
/// whether it does for this refresh).
pub(crate) fn filter_on() -> bool {
    mode() == 2
}

/// `=1`, since the line: refreshes the filter did NOT filter, by reason.
static NOT: [AtomicU64; 6] = [const { AtomicU64::new(0) }; 6];
pub(crate) const NOT_ASIDE: usize = 0;
pub(crate) const NOT_WIN_FULL: usize = 1;
pub(crate) const NOT_WATCHED: usize = 2;
const NOT_NONE_TAKEN: usize = 3;
const NOT_NONE_COVERED: usize = 4;
const NOT_ALL_MARKED: usize = 5;
const NOT_NAMES: [&str; 6] = [
    "stood aside (cost limit)",
    "windows overflowed",
    "copy not watched (taken whole)",
    "no page taken whole before",
    "no event-covered page",
    "every event-covered page marked",
];
/// `=1`, since the line: pages of the refreshes the filter looked at that it
/// did not spare, by the first reason that holds.
static PAGES_NOT: [AtomicU64; 13] = [const { AtomicU64::new(0) }; 13];
const P_NOT_TAKEN: usize = 0;
const P_TABLE_FULL: usize = 1;
const P_HEAP: usize = 2;
const P_MULTI: usize = 3;
const P_LOCKBUF: usize = 4;
const P_DEMOTED: usize = 5;
const P_PROBATION: usize = 6;
const P_MARKED: usize = 7;
const P_INFLIGHT: usize = 8;
const P_MAPPED_RECENT: usize = 9;
const P_WINDOW: usize = 10;
/// (Coder W) In the streaming regions (the title's variables; all pages while unknown): remapped
/// through the GPU view with no event of the filter's (run-231's misses).
const P_STREAMING: usize = 11;
/// (Coder W) Last marked by a landing or a mapping change, however long ago.
const P_MARK_KIND: usize = 12;
const P_NAMES: [&str; 13] = [
    "never taken or in part",
    "table full",
    "graphics heap",
    "shown at another address",
    "engine lock buffer",
    "demoted",
    "in probation",
    "marked since the take",
    "landing in flight",
    "mapping change in the last frames",
    "lock window open",
    "in the streaming regions",
    "last marked by a landing or a mapping change",
];

/// (`Shadow::update_free`, `=1`) A refresh the filter will not look at, for `reason`.
pub(crate) fn not_filtered(reason: usize) {
    NOT[reason].fetch_add(1, Relaxed);
}

/// `=1`: whether the filter acts for this refresh; when not (standing aside
/// this second for the cost limit, or the open windows overflowed) the
/// reason is counted and today's tracking takes the refresh.
pub(crate) fn gate() -> bool {
    if mode() != 2 {
        return false;
    }
    if OVER.load(Relaxed) {
        NOT[NOT_ASIDE].fetch_add(1, Relaxed);
        F[F_ASIDE].fetch_add(1, Relaxed);
        return false;
    }
    if WIN_FULL.load(Relaxed) {
        NOT[NOT_WIN_FULL].fetch_add(1, Relaxed);
        return false;
    }
    true
}

/// `=1`, since the line: refreshes filtered, pages spared, bytes spared,
/// asks made for the rest, refreshes with no ask at all, time in those asks
/// (ns), safety compare bytes, stale pages found, stale bytes, filter time
/// (ns), spared pages asked anyway (in a gap between asked runs), refreshes
/// stood aside, stale copies demoted whole.
/// Then: refreshes the filter was offered (=1, watched copies), asks on the
/// usual path (whole refresh) and their time (ns).
static F: [AtomicU64; 16] = [const { AtomicU64::new(0) }; 16];
pub(crate) const F_REFRESHES: usize = 0;
pub(crate) const F_PAGES: usize = 1;
pub(crate) const F_BYTES: usize = 2;
pub(crate) const F_ASKS: usize = 3;
pub(crate) const F_NO_ASK: usize = 4;
pub(crate) const F_ASK_NS: usize = 5;
const F_SAFETY: usize = 6;
const F_STALE_PAGES: usize = 7;
const F_STALE_BYTES: usize = 8;
const F_NS: usize = 9;
pub(crate) const F_GAP: usize = 10;
pub(crate) const F_ASIDE: usize = 11;
const F_STALE_COPIES: usize = 12;
pub(crate) const F_OFFERED: usize = 13;
pub(crate) const F_BASE_ASKS: usize = 14;
pub(crate) const F_BASE_NS: usize = 15;

/// (`Shadow::update_free` under `=1`) Counts.
pub(crate) fn count(what: usize, n: u64) {
    F[what].fetch_add(n, Relaxed);
}

/// Safety compare budget left in this second (16 MB).
static SAFETY: AtomicU64 = AtomicU64::new(16 << 20);
/// Bytes of spared pages whose stale-check is due, in turn (so that every
/// spared page is looked at now and then, not only the first ones).
static SAFETY_TURN: AtomicU64 = AtomicU64::new(0);

/// `=1`: whether a spared page of `bytes` is compared now (the diag safety
/// compare, at most 16 MB a second).
pub(crate) fn safety_due(bytes: u64) -> bool {
    // One page in 8 offered, in turn, while the budget lasts.
    if SAFETY_TURN.fetch_add(1, Relaxed) % 8 != 0 {
        return false;
    }
    let ok = SAFETY.fetch_update(Relaxed, Relaxed, |l| l.checked_sub(bytes)).is_ok();
    if ok {
        F[F_SAFETY].fetch_add(bytes, Relaxed);
    }
    ok
}

/// `=1`: the safety compare found `page` of copy `copy` (`[start, start +
/// len)`) differing from the copy although the filter spared it: the page is
/// taken by the caller; all of the copy's pages are watched from now on.
pub(crate) fn stale(copy: u64, kind: &'static str, start: u64, len: u64, page: usize, bytes: u64) {
    F[F_STALE_PAGES].fetch_add(1, Relaxed);
    F[F_STALE_BYTES].fetch_add(bytes, Relaxed);
    F[F_STALE_COPIES].fetch_add(1, Relaxed);
    let now = frame_mark();
    let mut pages = 0u64;
    for p in start as usize / PAGE..(start + len).div_ceil(PAGE as u64) as usize {
        if let Some(c) = chunk(p, true) {
            let i = p % CHUNK_PAGES;
            if c.state[i].fetch_or(DEMOTED, Relaxed) & DEMOTED == 0 {
                c.demoted_at[i].store(now, Relaxed);
                let _ = c.demotions[i].fetch_update(Relaxed, Relaxed, |n| n.checked_add(1));
                pages += 1;
            }
        }
    }
    TOTAL[2].fetch_add(pages, Relaxed);
    let (stamp, src) = chunk(page, false).map_or((0, 0), |c| (c.stamp[page % CHUNK_PAGES].load(Relaxed), c.src[page % CHUNK_PAGES].load(Relaxed)));
    tracing::warn!(
        copy,
        kind,
        page = format_args!("{:#x}", page * PAGE),
        class = crate::lockmeter::region_of((page * PAGE) as u64).1,
        mark = if stamp == 0 { "none ever".to_string() } else { format!("{} at asking {stamp}", source_name(src)) },
        asking_now = crate::watch::asking_now(),
        demoted_pages = pages,
        "known writes: STALE BYTES FOUND ON A SPARED PAGE (VICTORY_KNOWN_WRITES=1 safety compare): the page is taken now and the whole copy is watched from now on"
    );
}

/// `=1`: for the pages of one refresh (page, taken at; 0: never taken, or
/// held only in part), whether each may be spared: event-covered and no mark
/// since its take, no landing in flight, no recent mapping change, no write
/// window open over it. The caller has passed [`gate`]. Pages not spared are
/// counted by reason; a refresh with none spared by its reason.
pub(crate) fn spared(pages: &[(usize, u64)], from: usize, to: usize) -> Vec<bool> {
    let began = std::time::Instant::now();
    resolve_aliases();
    let now = frame_mark();
    let probation = probation();
    let ((c0, c1), (h0, h1)) = crate::lockmeter::heap_ranges();
    let open_windows: Vec<(u64, u64)> = if OPEN.load(Relaxed) == 0 {
        Vec::new()
    } else {
        let (a, b) = (from as u64, to as u64);
        let w = windows();
        w.open.range(a.saturating_sub(w.longest)..b).filter(|(_, v)| v.0 > a).map(|(&k, v)| (k, v.0)).collect()
    };
    let mut sums = [0u64; 33];
    let mut not = [0u64; 13];
    let streaming = crate::pagekinds::streaming();
    let out: Vec<bool> = pages
        .iter()
        .map(|&(page, taken)| {
            let why = (|| {
                if taken == 0 {
                    return Some(P_NOT_TAKEN);
                }
                let address = (page * PAGE) as u64;
                if c1 != 0 && address >= c0 && address < c0 + c1 || address >= h0 && address < h1 {
                    return Some(P_HEAP);
                }
                match streaming {
                    // (The unit tests have no runtime to tell the regions.)
                    None if cfg!(test) => {}
                    None => return Some(P_STREAMING),
                    Some(((a0, a1), (b0, b1))) => {
                        let at = page * PAGE;
                        if (a0..a1).contains(&at) || (b0..b1).contains(&at) {
                            return Some(P_STREAMING);
                        }
                    }
                }
                let Some(c) = chunk(page, true) else { return Some(P_TABLE_FULL) };
                let i = page % CHUNK_PAGES;
                let st = state_of(c, i, page, now, &mut sums);
                if st & MULTI != 0 {
                    return Some(P_MULTI);
                }
                if st & LOCKBUF != 0 {
                    return Some(P_LOCKBUF);
                }
                if st & DEMOTED != 0 {
                    return Some(P_DEMOTED);
                }
                if now.wrapping_sub(st as u32) < probation {
                    return Some(P_PROBATION);
                }
                let stamp = c.stamp[i].load(Relaxed);
                if stamp != 0 && stamp >= taken {
                    return Some(P_MARKED);
                }
                if c.inflight[i].load(Relaxed) != 0 {
                    return Some(P_INFLIGHT);
                }
                if stamp != 0 && matches!(c.src[i].load(Relaxed), SRC_LANDING | SRC_MAPPED) {
                    return Some(P_MARK_KIND);
                }
                if c.src[i].load(Relaxed) == SRC_MAPPED {
                    let f = c.frame[i].load(Relaxed);
                    if f != 0 && now.wrapping_sub(f) <= RECENT {
                        return Some(P_MAPPED_RECENT);
                    }
                }
                if open_windows.iter().any(|&(x, y)| x < address + PAGE as u64 && y > address) {
                    return Some(P_WINDOW);
                }
                None
            })();
            if let Some(why) = why {
                not[why] += 1;
            }
            why.is_none()
        })
        .collect();
    for (i, v) in sums.into_iter().enumerate() {
        if v != 0 {
            C[i].fetch_add(v, Relaxed);
        }
    }
    for (i, v) in not.into_iter().enumerate() {
        if v != 0 {
            PAGES_NOT[i].fetch_add(v, Relaxed);
        }
    }
    if !out.iter().any(|&s| s) {
        let n = pages.len() as u64;
        NOT[if not[P_NOT_TAKEN] == n {
            NOT_NONE_TAKEN
        } else if not[P_MARKED] + not[P_INFLIGHT] + not[P_MAPPED_RECENT] + not[P_WINDOW] == 0 {
            NOT_NONE_COVERED
        } else {
            NOT_ALL_MARKED
        }]
        .fetch_add(1, Relaxed);
    }
    F[F_NS].fetch_add(began.elapsed().as_nanos() as u64, Relaxed);
    out
}

fn source_name(src: u8) -> &'static str {
    SOURCE_NAMES.get(usize::from(src)).copied().unwrap_or("no mark ever")
}

fn describe(m: &Miss) -> String {
    format!(
        "page {:#x} of {} #{} ({}), bytes {}, writer {}, frame {}, taken at asking {}, found written at {} by {}, the page's mark: {}",
        m.address,
        m.kind,
        m.copy,
        m.class,
        match m.differed {
            Some(true) => "DIFFERED from the copy's last take",
            Some(false) => "the same as the copy's last take",
            None if m.unreadable => "not compared: title memory not readable there (unmapped, or being remapped)",
            None => "not compared: compare budget spent",
        },
        crate::lockmeter::writer_name(m.writer),
        m.frame,
        m.taken,
        m.write,
        if m.parts == 0 || m.parts == WHOLE { "a guard (whole page)".to_string() } else { format!("the write-watch, parts {:#06x}", m.parts) },
        if m.stamp == 0 { "none ever".to_string() } else { format!("{} at asking {}", source_name(m.src), m.stamp) }
    )
}

/// Once, at the first line: the mode in force and the state of each gate.
fn say_mode() {
    let raw = std::env::var("VICTORY_KNOWN_WRITES").ok();
    let m = mode();
    if m == 0 {
        if let Some(raw) = raw.as_deref().filter(|v| !v.trim().is_empty() && v.trim() != "0") {
            tracing::warn!("known writes: VICTORY_KNOWN_WRITES={raw:?} is not a mode (verify or 1): OFF");
        }
        return;
    }
    tracing::info!(
        "known writes: mode in force {} (VICTORY_KNOWN_WRITES={:?}); gates: the filter acts {}; probation {} frames (VICTORY_KNOWN_WRITES_PROBATION); cost limit {} ms/frame, over it the filter stands aside for the next second (VICTORY_KNOWN_WRITES_MAX_MS), standing aside now {}; open write windows overflowed {}; table {} of {} GiB made (VICTORY_KNOWN_WRITES_TABLE_GIB; past it pages are watched); demoted pages tried again after {} frames (VICTORY_KNOWN_WRITES_RETRY); spared gaps of up to {} pages asked with their runs (VICTORY_KNOWN_WRITES_GAP); safety compare of spared pages {}",
        mode_name(m),
        raw.unwrap_or_default(),
        if m == 2 { "yes (refreshes of watched copies)" } else { "no (verify: counts only)" },
        probation(),
        max_ms(),
        OVER.load(Relaxed),
        WIN_FULL.load(Relaxed),
        CHUNKS_MADE.load(Relaxed),
        table_most(),
        retry(),
        gap(),
        if cfg!(feature = "diagnostics") { "16 MB/s (diagnostics)" } else { "off (no diagnostics)" },
    );
}

/// Once a second (the lock hooks' line thread): "known writes ..." lines.
pub fn line(frames: f64) {
    static SAID_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !SAID_MODE.swap(true, Relaxed) {
        say_mode();
    }
    if !on() {
        return;
    }
    static SECONDS: AtomicU64 = AtomicU64::new(0);
    let second = SECONDS.fetch_add(1, Relaxed) + 1;
    let frames = frames.max(1.0);
    let k: Vec<u64> = C.iter().map(|v| v.swap(0, Relaxed)).collect();
    let e: Vec<u64> = EXTRA_BY.iter().map(|v| v.swap(0, Relaxed)).collect();
    let n: Vec<u64> = NOTED.iter().map(|v| v.swap(0, Relaxed)).collect();
    let locks: Vec<u64> = LOCKS.iter().map(|v| v.swap(0, Relaxed)).collect();
    BUDGET.store(BUDGET_BYTES, Relaxed);
    let mb = |v: u64| v as f64 / 1048576.0 / frames;
    let ms = |v: u64| v as f64 / 1e6 / frames;
    let f: Vec<u64> = F.iter().map(|v| v.swap(0, Relaxed)).collect();
    SAFETY.store(16 << 20, Relaxed);
    let over = ms(k[NANOS]) + ms(n[4]) + ms(f[F_NS]) > max_ms();
    OVER.store(over, Relaxed);
    if over {
        OVER_SECONDS.fetch_add(1, Relaxed);
    }
    tracing::info!(
        "known writes FILTER per frame over {frames:.0} frames (VICTORY_KNOWN_WRITES={}): spared (event-covered, no mark since the take: no ask, \
         arm or compare) {:.2} MB in {:.1} pages, refreshes spared wholly {:.1}, in part {:.1}; event-covered with a mark (still asked) {:.2} MB; \
         FILTER MISSES {:.2} pages/frame: CHANGED {}, not compared {}, unchanged {} (seen by the write-watch {}); since the start {} pages, {} \
         changed or not compared; in probation: unexplained writes unchanged {}, changed {}; demoted {} pages, {} whole copies (since the start {} \
         pages); COST per frame: ledger on refresh threads {:.3} ms (of it the page loop {:.3} ms, other-view marks {:.3} ms, view looks {:.3} ms in {:.0}, compares {:.3} \
         ms), notes on the noting threads {:.3} ms in {:.0} notes; seconds over the cost limit ({} ms; =1 stands aside then) {}; pages with no state (table full: watched) {:.2} MB, demoted pages tried again {}, windows forgotten {}, windows overflowed {}",
        if mode() == 1 { "verify" } else { "1: counts of the pages still asked" },
        mb(k[SKIP]),
        k[SKIP_PAGES] as f64 / frames,
        k[SKIP_WHOLE] as f64 / frames,
        k[SKIP_PART] as f64 / frames,
        mb(k[ASK]),
        k[MISS] as f64 / frames,
        k[MISS_CHANGED],
        k[MISS_UNCOMPARED],
        k[MISS_SAME],
        k[MISS_WATCHED],
        TOTAL[0].load(Relaxed),
        TOTAL[1].load(Relaxed),
        k[PROB_SAME],
        k[PROB_CHANGED],
        k[DEMOTE_PAGES],
        k[DEMOTE_COPIES],
        TOTAL[2].load(Relaxed),
        ms(k[NANOS]),
        ms(k[LOOP_NS]),
        ms(n[5]),
        ms(k[LOOK_NS]),
        k[LOOKS] as f64 / frames,
        ms(k[COMPARE_NS]),
        ms(n[4]),
        n[1] as f64 / frames,
        max_ms(),
        OVER_SECONDS.load(Relaxed),
        mb(k[NO_STATE]),
        k[RETRIED],
        WIN_DROPPED.load(Relaxed),
        WIN_FULL.load(Relaxed),
    );
    let _ = TABLE_FULL.swap(0, Relaxed);
    let not: Vec<u64> = NOT.iter().map(|v| v.swap(0, Relaxed)).collect();
    let pages_not: Vec<u64> = PAGES_NOT.iter().map(|v| v.swap(0, Relaxed)).collect();
    if mode() == 2 {
        let reasons: Vec<String> = (0..NOT.len()).map(|i| format!("{} {:.1}", NOT_NAMES[i], not[i] as f64 / frames)).collect();
        let page_reasons: Vec<String> =
            (0..PAGES_NOT.len()).filter(|&i| pages_not[i] != 0).map(|i| format!("{} {:.1}", P_NAMES[i], pages_not[i] as f64 / frames)).collect();
        tracing::info!(
            "known writes FILTER ON per frame (VICTORY_KNOWN_WRITES=1): refreshes offered {:.1}, filtered {:.1}, NOT filtered by reason: {}; pages not spared in the refreshes looked at, by reason: {}; gates now: standing aside {}, windows overflowed {}; usual-path asks {:.1} taking {:.3} ms",
            f[F_OFFERED] as f64 / frames,
            f[F_REFRESHES] as f64 / frames,
            reasons.join(", "),
            if page_reasons.is_empty() { "none".to_string() } else { page_reasons.join(", ") },
            OVER.load(Relaxed),
            WIN_FULL.load(Relaxed),
            f[F_BASE_ASKS] as f64 / frames,
            ms(f[F_BASE_NS]),
        );
        tracing::info!(
            "known writes FILTER ON per frame (VICTORY_KNOWN_WRITES=1): refreshes filtered {:.1}; spared ACTUALLY (no write-watch ask, no guard arm, no compare) {:.1} pages = {:.2} MB, of them asked anyway in a gap between asked runs {:.1} pages; refreshes with no ask at all {:.1}; asks made for the rest {:.1} (one per filtered refresh before), taking {:.3} ms; filter time {:.3} ms; stood aside (cost limit) {:.1} refreshes, seconds over the limit since the start {}; SAFETY compare {:.2} MB of spared pages: STALE pages {} ({} bytes differing pages, {} copies watched whole) - must be 0",
            f[F_REFRESHES] as f64 / frames,
            f[F_PAGES] as f64 / frames,
            mb(f[F_BYTES]),
            f[F_GAP] as f64 / frames,
            f[F_NO_ASK] as f64 / frames,
            f[F_ASKS] as f64 / frames,
            ms(f[F_ASK_NS]),
            ms(f[F_NS]),
            f[F_ASIDE] as f64 / frames,
            OVER_SECONDS.load(Relaxed),
            f[F_SAFETY] as f64 / 1048576.0,
            f[F_STALE_PAGES],
            f[F_STALE_BYTES],
            f[F_STALE_COPIES],
        );
    }
    let by: Vec<String> = (0..SOURCES).filter(|&i| e[i] != 0).map(|i| format!("{} {:.2}", SOURCE_NAMES[i], mb(e[i]))).collect();
    let (open, objects) = {
        let w = windows();
        (w.open.len(), w.objects.len())
    };
    tracing::info!(
        "known writes per frame (VICTORY_KNOWN_WRITES): pages taken before and looked at {:.2} MB in {:.1} refreshes = event-covered {:.2} MB / \
         watched: graphics heap {:.2} MB, shown at another address {:.2} MB, engine lock buffers {:.2} MB, demoted {:.2} MB, in probation {:.2} MB; found written by the old \
         tracking {:.2} MB (on event-covered pages {:.2} MB), on marked pages {:.2} MB; MARKED BUT FOUND UNWRITTEN (asks kept for nothing) {:.2} MB, by \
         the source of the mark: {}; noted {:.2} MB, other-view marks {:.0}, alias notes dropped {}; write windows opened {:.1} closed {:.1}, unlocks \
         with no window {}, open now {open} (objects with windows {objects}); table {} GiB",
        mb(k[LOOKED]),
        k[REFRESHES] as f64 / frames,
        mb(k[COVERED]),
        mb(k[W_HEAP]),
        mb(k[W_MULTI]),
        mb(k[W_LOCKBUF]),
        mb(k[W_DEMOTED]),
        mb(k[W_PROBATION]),
        mb(k[FOUND]),
        mb(k[FOUND_COVERED]),
        mb(k[FOUND_MARKED]),
        mb(k[EXTRA]),
        by.join(", "),
        mb(n[0]),
        n[3] as f64 / frames,
        n[2],
        locks[0] as f64 / frames,
        locks[1] as f64 / frames,
        locks[2],
        CHUNKS_MADE.load(Relaxed),
    );
    if second % 10 == 0 {
        let bk = book();
        for (list, what) in [(&bk.changed, "CHANGED"), (&bk.same, "unchanged")] {
            if !list.is_empty() {
                let l: Vec<String> = list.iter().map(describe).collect();
                tracing::info!("known writes: the first {} {what} filter misses since the start (VICTORY_KNOWN_WRITES): {}", l.len(), l.join(" | "));
            }
        }
        if !bk.groups.is_empty() {
            let mut top: Vec<_> = bk.groups.iter().collect();
            top.sort_unstable_by(|a, b| b.1.0.cmp(&a.1.0));
            let list: Vec<String> = top
                .iter()
                .take(16)
                .map(|((kind, class, src, writer), (pages, first, copy))| {
                    format!(
                        "{pages} pages of {kind} ({class}), page's mark: {}, writer {}, first {first:#x} in #{copy}",
                        source_name(*src),
                        crate::lockmeter::writer_name(*writer)
                    )
                })
                .collect();
            tracing::info!("known writes: CHANGED filter misses since the start by kind, class, mark, writer ({} groups): {}", bk.groups.len(), list.join("; "));
        }
        if !bk.demotions.is_empty() {
            let mut top: Vec<_> = bk.demotions.iter().collect();
            top.sort_unstable_by(|a, b| b.1.0.cmp(&a.1.0));
            let list: Vec<String> = top
                .iter()
                .take(16)
                .map(|((copy, class, writer), (pages, first, kind))| {
                    format!("{pages} pages of {kind} #{copy} ({class}), first {first:#x}, writer {}", crate::lockmeter::writer_name(*writer))
                })
                .collect();
            tracing::info!("known writes: top demotions since the start, of {} groups (VICTORY_KNOWN_WRITES): {}", bk.demotions.len(), list.join("; "));
        }
    }
}

/// For tests of the filter: the mode, the stand-aside gate, the safety
/// compare off, pages made event-covered and unmarked. Tests that use them
/// hold [`TEST_LOCK`] and put the mode back to off.
#[cfg(test)]
pub(crate) mod test_hooks {
    use super::*;

    pub(crate) static TEST_LOCK: Mutex<()> = Mutex::new(());

    pub(crate) fn set_mode(m: u8) {
        MODE.store(m, Relaxed);
    }

    pub(crate) fn stand_aside(on: bool) {
        OVER.store(on, Relaxed);
    }

    pub(crate) fn no_safety_compare() {
        SAFETY.store(0, Relaxed);
    }

    /// `[at, at + len)`: past probation, looked at now (shown nowhere else),
    /// no mark, nothing in flight, not demoted.
    pub(crate) fn cover(at: usize, len: usize) {
        let now = frame_mark();
        for page in at / PAGE..(at + len).div_ceil(PAGE) {
            let c = chunk(page, true).expect("table");
            let i = page % CHUNK_PAGES;
            let first = u64::from(now.wrapping_sub(probation() + 1).max(1));
            let look = u64::from((now & LOOK_MASK).max(1)) << LOOK_SHIFT;
            c.state[i].store(first | look, Relaxed);
            c.stamp[i].store(0, Relaxed);
            c.src[i].store(SRC_LANDING, Relaxed);
            c.frame[i].store(0, Relaxed);
            c.inflight[i].store(0, Relaxed);
        }
    }

    pub(crate) fn not_count(reason: usize) -> u64 {
        NOT[reason].load(Relaxed)
    }

    pub(crate) fn f_count(what: usize) -> u64 {
        F[what].load(Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The switch's values: only `verify` and `1` turn it on.
    #[test]
    fn the_mode_is_parsed_strictly() {
        assert_eq!(parse_mode(Some("verify")), 1);
        assert_eq!(parse_mode(Some(" 1 ")), 2);
        assert_eq!(parse_mode(Some("on")), 0);
        assert_eq!(parse_mode(Some("2")), 0);
        assert_eq!(parse_mode(None), 0);
    }

    /// Event-covered unmarked pages are spared; a marked one, one in flight,
    /// one never taken are not, each counted by its reason; a refresh with
    /// none spared is counted by its reason.
    #[test]
    fn spared_pages_and_the_reasons_for_the_rest() {
        let _l = test_hooks::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let at = 0xb_4000_0000usize;
        test_hooks::cover(at, 6 * PAGE);
        let taken = crate::watch::asking_now().max(1);
        let base = at / PAGE;
        mark(at + PAGE, 10, taken, frame_mark(), SRC_OUR_COPY);
        chunk(base + 2, false).expect("made").inflight[(base + 2) % CHUNK_PAGES].store(1, Relaxed);
        // A mark from before the take does not count.
        mark(at + 3 * PAGE, 10, taken - 1, frame_mark(), SRC_OUR_COPY);
        let pages: Vec<(usize, u64)> = (0..6).map(|k| (base + k, if k == 5 { 0 } else { taken })).collect();
        let marked = PAGES_NOT[P_MARKED].load(Relaxed);
        let got = spared(&pages, at, at + 6 * PAGE);
        assert_eq!(got, [true, false, false, true, true, false]);
        assert!(PAGES_NOT[P_MARKED].load(Relaxed) > marked);
        // None spared: by the reason.
        let before = NOT[NOT_NONE_TAKEN].load(Relaxed);
        assert!(spared(&[(base, 0), (base + 1, 0)], at, at + 2 * PAGE).iter().all(|&s| !s));
        assert_eq!(NOT[NOT_NONE_TAKEN].load(Relaxed), before + 1);
        let before = NOT[NOT_ALL_MARKED].load(Relaxed);
        assert!(spared(&[(base + 1, taken), (base + 2, taken)], at, at + 3 * PAGE).iter().all(|&s| !s));
        assert_eq!(NOT[NOT_ALL_MARKED].load(Relaxed), before + 1);
        chunk(base + 2, false).expect("made").inflight[(base + 2) % CHUNK_PAGES].store(0, Relaxed);
    }

    /// Standing aside (cost limit) closes the gate, counted; only `=1` opens it.
    #[test]
    fn the_gate_stands_aside_counted() {
        let _l = test_hooks::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        test_hooks::set_mode(2);
        assert!(gate());
        let before = NOT[NOT_ASIDE].load(Relaxed);
        test_hooks::stand_aside(true);
        assert!(!gate());
        assert_eq!(NOT[NOT_ASIDE].load(Relaxed), before + 1);
        test_hooks::stand_aside(false);
        test_hooks::set_mode(1);
        assert!(!gate(), "verify never filters");
        test_hooks::set_mode(0);
    }

    #[test]
    fn marks_keep_the_highest_number_per_page() {
        let at = 0x7_0000_0000usize + 3 * PAGE + 100;
        mark(at, 2 * PAGE, 7, 10, SRC_LANDING);
        mark(at, 10, 5, 10, SRC_ALIAS);
        let page = at / PAGE;
        let c = chunk(page, false).expect("made by the marks");
        let i = page % CHUNK_PAGES;
        assert_eq!(c.stamp[i].load(Relaxed), 7);
        assert_eq!(c.stamp[i + 2].load(Relaxed), 7);
        assert_eq!(c.stamp[i + 3].load(Relaxed), 0);
        assert_eq!(c.stamp[i - 1].load(Relaxed), 0);
        // Landing marks keep no frame (only mapping changes do).
        assert_eq!(c.frame[i].load(Relaxed), 0);
    }

    #[test]
    fn a_landing_is_in_flight_until_its_bytes_are_in() {
        // (`landing`/`landed` act only with the switch on; the counter itself.)
        let page = 0xa_0000_0000usize / PAGE + 5;
        let c = chunk(page, true).expect("made");
        let i = page % CHUNK_PAGES;
        c.inflight[i].fetch_add(1, Relaxed);
        assert_ne!(c.inflight[i].load(Relaxed), 0);
        let _ = c.inflight[i].fetch_update(Relaxed, Relaxed, |n| (n != 0 && n != u8::MAX).then(|| n - 1));
        assert_eq!(c.inflight[i].load(Relaxed), 0);
        c.inflight[i].store(u8::MAX, Relaxed);
        assert!(c.inflight[i].fetch_update(Relaxed, Relaxed, |n| (n != 0 && n != u8::MAX).then(|| n - 1)).is_err(), "saturated stays");
    }

    #[test]
    fn open_windows_are_found_by_page() {
        let mut w = windows();
        let a = (0x9_0000_0000usize + 4 * PAGE + 4096) as u64;
        w.open.insert(a, (a + 8192, 1));
        w.longest = w.longest.max(8192);
        drop(w);
        OPEN.fetch_add(1, Relaxed);
        assert!(window_open(a as usize / PAGE));
        assert!(!window_open(a as usize / PAGE + 1));
        OPEN.fetch_sub(1, Relaxed);
        windows().open.remove(&a);
    }
}
