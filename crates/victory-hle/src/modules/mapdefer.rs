//! (F10, `VICTORY_MAP_DEFER`; fox port: default OFF = every unmapping done
//! in the call, as the fox base does, until an A/B run; `=1`: on, our tree's
//! default) The title's two page-mapping calls on its frame thread
//! (`XMemMapPhysicalPagesScatter` for its CPU view, `CopyPageMappingsBatchX`
//! for its GPU view) return without doing their UNMAPPINGS: those go, in call
//! order, to one FIFO applied by one thread ("victory-map").
//!
//! What is deferred, and why nothing observable changes:
//! - Only unmappings (Scatter entries with page 0 / null pages, copied
//!   mappings of kind 2). The title is done with those pages (old level,
//!   freed after its frame+2 wait), so until the unmapping is applied they
//!   simply stay readable with their old bytes: nothing can fault on them,
//!   and a still-mapped page never disappears earlier than it would have.
//!   Unmappings report success whatever happens today, so the title's
//!   result is unchanged.
//! - Maps stay in the call. Before one (and before a copy reads what a
//!   source address shows, and before `XMemVirtualQuery`), every pending
//!   unmapping of those pages is applied first ([`before`]): operations on
//!   one page keep their call order. A page's final state depends only on
//!   the last operation on it (an unmapping maps again what stays of a view
//!   around it, the same pages at the same addresses), so operations on
//!   different pages may be applied in either order.
//! - The notes an unmapping makes (the hold over its bytes, views and
//!   guards forgotten, the exact copy release `VICTORY_UNMAP_EXACT`, window
//!   release) are made where it is applied, on the worker, in order: they
//!   only schedule release work, and until then the bytes are still there.
//!   Writes of DirectStorage into physical pages are not told at an address
//!   whose unmapping is pending ([`pending_at`]), as they would not be once
//!   it is applied.
//! - Back-pressure: past [`LIMIT_PAGES`] pending pages the calling thread
//!   applies operations itself until half of that is left.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::{Acquire, Relaxed, Release}};
use std::sync::{Condvar, Mutex, MutexGuard};
#[cfg(not(test))]
use std::sync::OnceLock;

const PAGE: usize = 64 << 10;

/// Pending pages past which the caller helps.
pub(crate) const LIMIT_PAGES: usize = 16_384;

/// An unmapping to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Op {
    /// `pages` pages of the CPU view from `at` on, page by page
    /// (`XMemMapPhysicalPages(at + k * 64 KiB, 1, null)`), as the call did.
    Cpu { at: usize, pages: usize },
    /// `[at, at + len)` of the GPU view (`map_alias(at, 0, len)`).
    Gpu { at: usize, len: usize },
}

impl Op {
    fn span(&self) -> (usize, usize) {
        match *self {
            Op::Cpu { at, pages } => (at, at.saturating_add(pages * PAGE)),
            Op::Gpu { at, len } => (at, at.saturating_add(len)),
        }
    }

    fn pages(&self) -> std::ops::Range<usize> {
        let (a, b) = self.span();
        a / PAGE..b.div_ceil(PAGE)
    }
}

struct Queue {
    ops: VecDeque<(u64, Op)>,
    next: u64,
    /// Pages pending, and each pending page's newest operation.
    pages: usize,
    pending: HashMap<usize, u64>,
}

/// A FIFO of operations applied in order by `apply`.
pub(crate) struct Deferrer {
    q: Mutex<Queue>,
    more: Condvar,
    /// Held while one operation is applied: the order of application.
    apply_lock: Mutex<()>,
    /// Every operation up to this number applied (numbers start at 1).
    applied: AtomicU64,
    /// Any page pending (the cheap check).
    any: AtomicBool,
    apply: fn(&Op),
    limit: usize,
    started: std::sync::Once,
}

/// Meter (since the last line): operations deferred, their pages, applied
/// by the worker, applied by a thread that needed them, by a caller helping;
/// waits for a pending operation, their ns, the most; depth most (pages).
static M: [AtomicU64; 9] = [const { AtomicU64::new(0) }; 9];
const DEFERRED: usize = 0;
const DEFERRED_PAGES: usize = 1;
const BY_WORKER: usize = 2;
const BY_READER: usize = 3;
const BY_HELPER: usize = 4;
const WAITS: usize = 5;
const WAIT_NS: usize = 6;
const WAIT_MAX_NS: usize = 7;
const DEPTH_MAX: usize = 8;

/// (C3, item 2b) `VICTORY_MAP_DEFER_BATCH` (default off: a waiter applies
/// every pending operation up to the newest on its pages, in one hold of the
/// apply lock, as before): `=N` at most N pages per hold, `=Nms` at most N
/// milliseconds per hold (`=1`: 64 pages); either way a waiter applies only
/// what its range needs ([`Deferrer::before_exact`]). Since the last line:
/// such waits, operations they applied, operations they left pending, the
/// longest hold (µs).
static M2: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];

fn batch_setting() -> Option<(usize, u64)> {
    #[cfg(test)]
    if TEST_BATCH.with(|t| t.get()) {
        return Some((2, u64::MAX));
    }
    static SET: std::sync::OnceLock<Option<(usize, u64)>> = std::sync::OnceLock::new();
    *SET.get_or_init(|| {
        let v = std::env::var("VICTORY_MAP_DEFER_BATCH").ok()?;
        let v = v.trim();
        if v.is_empty() || v == "0" {
            return None;
        }
        if v == "1" {
            return Some((64, u64::MAX));
        }
        if let Some(ms) = v.strip_suffix("ms") {
            return ms.trim().parse().ok().map(|ms: u64| (usize::MAX, ms.max(1)));
        }
        v.parse().ok().map(|n: usize| (n.max(1), u64::MAX))
    })
}

fn batch_on() -> bool {
    batch_setting().is_some()
}

fn batch() -> (usize, u64) {
    batch_setting().unwrap_or((usize::MAX, u64::MAX))
}

#[cfg(test)]
thread_local! {
    static TEST_BATCH: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

thread_local! {
    /// This thread applies operations (no deferral, no waiting on itself),
    /// and why: 1 the worker, 2 a thread that needed them, 3 a helper.
    static APPLYING: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
    #[cfg(test)]
    static TEST_ON: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether this thread is applying deferred operations now.
pub(crate) fn applying() -> bool {
    APPLYING.with(|a| a.get() != 0)
}

/// `VICTORY_MAP_DEFER` (fox port: default off; `=1` on; never on the applying thread).
pub(crate) fn on() -> bool {
    if applying() {
        return false;
    }
    #[cfg(test)]
    {
        TEST_ON.with(|t| t.get())
    }
    #[cfg(not(test))]
    {
        static ON: OnceLock<bool> = OnceLock::new();
        *ON.get_or_init(|| {
            let on = std::env::var_os("VICTORY_MAP_DEFER").is_some_and(|v| v == "1");
            tracing::info!(on, "VICTORY_MAP_DEFER: unmappings of the title's mapping calls applied on the victory-map thread");
            on
        })
    }
}

#[cfg(test)]
pub(crate) fn test_on(on: bool) {
    TEST_ON.with(|t| t.set(on));
}

impl Deferrer {
    pub(crate) fn new(apply: fn(&Op), limit: usize) -> Deferrer {
        Deferrer {
            q: Mutex::new(Queue { ops: VecDeque::new(), next: 1, pages: 0, pending: HashMap::new() }),
            more: Condvar::new(),
            apply_lock: Mutex::new(()),
            applied: AtomicU64::new(0),
            any: AtomicBool::new(false),
            apply,
            limit,
            started: std::sync::Once::new(),
        }
    }

    fn queue(&self) -> MutexGuard<'_, Queue> {
        self.q.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Defers `op`; past the limit, applies operations until half is left.
    pub(crate) fn defer(&'static self, op: Op) {
        self.started.call_once(|| {
            if std::thread::Builder::new().name("victory-map".into()).spawn(move || self.work()).is_err() {
                tracing::error!("VICTORY_MAP_DEFER: the victory-map thread did not start; callers apply what they defer");
            }
        });
        let depth = {
            let mut q = self.queue();
            let seq = q.next;
            q.next += 1;
            let pages = op.pages();
            q.pages += pages.len();
            for p in pages.clone() {
                q.pending.insert(p, seq);
            }
            q.ops.push_back((seq, op));
            self.any.store(true, Release);
            M[DEFERRED].fetch_add(1, Relaxed);
            M[DEFERRED_PAGES].fetch_add(pages.len() as u64, Relaxed);
            M[DEPTH_MAX].fetch_max(q.pages as u64, Relaxed);
            q.pages
        };
        self.more.notify_one();
        if depth > self.limit {
            let _g = self.apply_lock.lock().unwrap_or_else(|e| e.into_inner());
            while self.queue().pages > self.limit / 2 && self.apply_next(3) {}
        }
    }

    /// Applies the oldest operation (the caller holds `apply_lock`); false if
    /// none is left.
    fn apply_next(&self, why: u8) -> bool {
        let Some((seq, op)) = self.queue().ops.pop_front() else {
            return false;
        };
        let was = APPLYING.with(|a| a.replace(why));
        (self.apply)(&op);
        APPLYING.with(|a| a.set(was));
        M[[0, BY_WORKER, BY_READER, BY_HELPER][usize::from(why)]].fetch_add(1, Relaxed);
        let mut q = self.queue();
        let pages = op.pages();
        q.pages -= pages.len();
        for p in pages {
            if q.pending.get(&p) == Some(&seq) {
                q.pending.remove(&p);
            }
        }
        if q.ops.is_empty() {
            self.any.store(false, Release);
        }
        // Everything older than the oldest still pending is applied (with
        // `VICTORY_MAP_DEFER_BATCH` operations may go out of order).
        let below = q.ops.front().map_or(q.next - 1, |&(s, _)| s - 1);
        let _ = seq;
        self.applied.store(below, Release);
        true
    }

    /// (C3, item 2b) Applies the pending operation `seq` out of turn (the
    /// caller holds `apply_lock`); false if it is not pending any more.
    fn apply_one(&self, seq: u64, why: u8) -> bool {
        let op = {
            let mut q = self.queue();
            let Some(i) = q.ops.iter().position(|&(s, _)| s == seq) else {
                return false;
            };
            q.ops.remove(i).map(|(_, op)| op)
        };
        let Some(op) = op else {
            return false;
        };
        let was = APPLYING.with(|a| a.replace(why));
        (self.apply)(&op);
        APPLYING.with(|a| a.set(was));
        M[[0, BY_WORKER, BY_READER, BY_HELPER][usize::from(why)]].fetch_add(1, Relaxed);
        let mut q = self.queue();
        let pages = op.pages();
        q.pages -= pages.len();
        for p in pages {
            if q.pending.get(&p) == Some(&seq) {
                q.pending.remove(&p);
            }
        }
        if q.ops.is_empty() {
            self.any.store(false, Release);
        }
        let below = q.ops.front().map_or(q.next - 1, |&(s, _)| s - 1);
        self.applied.store(below, Release);
        true
    }

    /// (C3, item 2b) `VICTORY_MAP_DEFER_BATCH`: what `[at, at + len)` needs
    /// before it is mapped or read, and nothing else: the pending operations
    /// up to the newest on its pages that share a page with it or, going back,
    /// with one taken (so every operation on a page that comes before a taken
    /// one on that page is taken too: per page the call order stays; the
    /// ones left share no page with any taken, and their order against them
    /// does not matter, see the module's header). Applied oldest first, the
    /// apply lock let go after every [`batch`] pages or milliseconds so the
    /// worker and other waiters get their turn.
    fn before_exact(&self, at: usize, len: usize, newest: u64) {
        let t = std::time::Instant::now();
        let selected: Vec<u64> = {
            let q = self.queue();
            let mut spans: Vec<(usize, usize)> = vec![(at / PAGE, at.saturating_add(len).div_ceil(PAGE))];
            let mut taken = Vec::new();
            for &(seq, op) in q.ops.iter().rev() {
                if seq > newest {
                    continue;
                }
                let p = op.pages();
                if spans.iter().any(|&(a, b)| a < p.end && p.start < b) {
                    spans.push((p.start, p.end));
                    taken.push(seq);
                }
            }
            M2[2].fetch_add(q.ops.iter().filter(|&&(s, _)| s <= newest).count() as u64 - taken.len() as u64, Relaxed);
            taken.reverse();
            taken
        };
        let (most_pages, most_ms) = batch();
        let mut i = 0;
        while i < selected.len() {
            let held = std::time::Instant::now();
            let _site = victory_gpu::hangwatch::at(&"map deferral: a mapping or a reader applies the pending unmaps it needs (apply lock)");
            let _g = self.apply_lock.lock().unwrap_or_else(|e| e.into_inner());
            let mut pages = 0;
            while i < selected.len() {
                let seq = selected[i];
                i += 1;
                let n = self.queue().ops.iter().find(|&&(s, _)| s == seq).map_or(0, |(_, op)| op.pages().len());
                if self.apply_one(seq, 2) {
                    M2[1].fetch_add(1, Relaxed);
                    pages += n;
                }
                if pages >= most_pages || held.elapsed().as_millis() as u64 >= most_ms {
                    break;
                }
            }
            drop(_g);
            M2[3].fetch_max(held.elapsed().as_micros() as u64, Relaxed);
            std::thread::yield_now();
        }
        M2[0].fetch_add(1, Relaxed);
        let ns = t.elapsed().as_nanos() as u64;
        M[WAITS].fetch_add(1, Relaxed);
        M[WAIT_NS].fetch_add(ns, Relaxed);
        M[WAIT_MAX_NS].fetch_max(ns, Relaxed);
    }

    fn work(&self) {
        loop {
            {
                let _site = victory_gpu::hangwatch::at(&"map deferral worker: idle, waiting for an unmap to apply");
                let mut q = self.queue();
                while q.ops.is_empty() {
                    q = self.more.wait(q).unwrap_or_else(|e| e.into_inner());
                }
            }
            let _site = victory_gpu::hangwatch::at(&"map deferral worker: applying an unmap (apply lock, then the mapping call)");
            let _g = self.apply_lock.lock().unwrap_or_else(|e| e.into_inner());
            self.apply_next(1);
        }
    }

    /// The newest pending operation on any page of `[at, at + len)`.
    fn newest(&self, at: usize, len: usize) -> Option<u64> {
        if !self.any.load(Acquire) || len == 0 {
            return None;
        }
        let q = self.queue();
        let (lo, hi) = (at / PAGE, at.saturating_add(len).div_ceil(PAGE));
        if hi - lo > q.pending.len() {
            q.pending.iter().filter(|(p, _)| (lo..hi).contains(*p)).map(|(_, s)| *s).max()
        } else {
            (lo..hi).filter_map(|p| q.pending.get(&p).copied()).max()
        }
    }

    /// Applies every operation up to `seq` (in order, helping the worker).
    fn ensure(&self, seq: u64, why: u8) {
        if self.applied.load(Acquire) >= seq {
            return;
        }
        let t = std::time::Instant::now();
        {
            let _site = victory_gpu::hangwatch::at(&"map deferral: a mapping or a reader waits for pending unmaps (apply lock)");
            let _g = self.apply_lock.lock().unwrap_or_else(|e| e.into_inner());
            while self.applied.load(Acquire) < seq && self.apply_next(why) {}
        }
        let ns = t.elapsed().as_nanos() as u64;
        M[WAITS].fetch_add(1, Relaxed);
        M[WAIT_NS].fetch_add(ns, Relaxed);
        M[WAIT_MAX_NS].fetch_max(ns, Relaxed);
    }

    /// Before `[at, at + len)` is mapped or read for what it shows: its
    /// pending operations applied.
    pub(crate) fn before(&self, at: usize, len: usize) {
        if applying() {
            return;
        }
        if let Some(seq) = self.newest(at, len) {
            if batch_on() {
                self.before_exact(at, len, seq);
            } else {
                self.ensure(seq, 2);
            }
        }
    }

    /// Whether the page at `at` has an operation pending.
    pub(crate) fn pending_at(&self, at: usize) -> bool {
        self.any.load(Acquire) && self.queue().pending.contains_key(&(at / PAGE))
    }

    /// Everything deferred so far applied.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn drain(&self) {
        if applying() || !self.any.load(Acquire) {
            return;
        }
        let last = self.queue().next - 1;
        self.ensure(last, 2);
    }
}

/// (Fox port) The meter's line every five seconds while unmappings are
/// deferred; the fox base has no "frame thread streaming" line to append
/// it to.
pub(crate) fn line_now() {
    static LAST: Mutex<Option<std::time::Instant>> = Mutex::new(None);
    let Ok(mut last) = LAST.try_lock() else {
        return;
    };
    let now = std::time::Instant::now();
    let then = *last.get_or_insert(now);
    if now.duration_since(then).as_secs_f64() < 5.0 {
        return;
    }
    *last = Some(now);
    tracing::info!(
        "map deferral over the last {:.1} s (VICTORY_MAP_DEFER=1){}",
        now.duration_since(then).as_secs_f64(),
        take_line()
    );
}

/// The meter's part of the "frame thread streaming" line, the counts since
/// the last call.
pub(crate) fn take_line() -> String {
    let m: Vec<u64> = M.iter().map(|a| a.swap(0, Relaxed)).collect();
    format!(
        "; mapping ops deferred {} (pages {}), applied by worker {}, drained by a waiting reader {} (waits {}: mean {:.2} ms, max {:.2} ms), helped by the caller {}, title faults on pending pages 0 (maps not deferred); queue depth max {} pages",
        m[DEFERRED],
        m[DEFERRED_PAGES],
        m[BY_WORKER],
        m[BY_READER],
        m[WAITS],
        if m[WAITS] == 0 { 0.0 } else { m[WAIT_NS] as f64 / m[WAITS] as f64 / 1e6 },
        m[WAIT_MAX_NS] as f64 / 1e6,
        m[BY_HELPER],
        m[DEPTH_MAX],
    ) + &{
        let n: Vec<u64> = M2.iter().map(|a| a.swap(0, Relaxed)).collect();
        if batch_on() {
            format!(
                "; exact waits (VICTORY_MAP_DEFER_BATCH) {} applied {} ops, left {} ops pending that they did not need; longest hold of the apply lock {:.2} ms",
                n[0],
                n[1],
                n[2],
                n[3] as f64 / 1e3
            )
        } else {
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    /// A page table of the test: page -> what it shows (0 nothing), and the
    /// log of what was applied.
    static TABLE: Mutex<Vec<(usize, Op)>> = Mutex::new(Vec::new());
    static SLOW_MS: AtomicUsize = AtomicUsize::new(0);
    static ONE: Mutex<()> = Mutex::new(());

    fn apply(op: &Op) {
        std::thread::sleep(std::time::Duration::from_millis(SLOW_MS.load(Relaxed) as u64));
        TABLE.lock().unwrap().push((APPLYING.with(|a| a.get()) as usize, *op));
    }

    fn leak(limit: usize) -> &'static Deferrer {
        Box::leak(Box::new(Deferrer::new(apply, limit)))
    }

    fn cpu(page: usize, pages: usize) -> Op {
        Op::Cpu { at: page * PAGE, pages }
    }

    #[test]
    fn an_unmap_then_a_map_of_the_same_page_keep_their_order() {
        let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
        TABLE.lock().unwrap().clear();
        SLOW_MS.store(20, Relaxed);
        let d = leak(LIMIT_PAGES);
        d.defer(cpu(100, 2));
        d.defer(Op::Gpu { at: 300 * PAGE, len: PAGE });
        // A map of page 101 (sync): its pending unmapping goes first.
        d.before(101 * PAGE, PAGE);
        assert!(d.applied.load(Acquire) >= 1);
        assert!(!d.pending_at(100 * PAGE) && !d.pending_at(101 * PAGE));
        TABLE.lock().unwrap().push((9, Op::Cpu { at: 101 * PAGE, pages: 0 })); // "the map"
        d.drain();
        let log = TABLE.lock().unwrap().clone();
        let unmap = log.iter().position(|e| e.1 == cpu(100, 2)).unwrap();
        let map = log.iter().position(|e| e.0 == 9).unwrap();
        assert!(unmap < map, "{log:?}");
        assert!(!d.pending_at(300 * PAGE));
        SLOW_MS.store(0, Relaxed);
    }

    #[test]
    fn a_reader_of_a_pending_page_waits_until_it_is_applied() {
        let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
        TABLE.lock().unwrap().clear();
        SLOW_MS.store(30, Relaxed);
        let d = leak(LIMIT_PAGES);
        for k in 0..4 {
            d.defer(cpu(1000 + k, 1));
        }
        let reader = std::thread::spawn(move || {
            d.before(1003 * PAGE + 5, 1);
            d.pending_at(1003 * PAGE)
        });
        assert!(!reader.join().unwrap(), "the page is no longer pending once the reader goes on");
        // In call order.
        let log: Vec<Op> = TABLE.lock().unwrap().iter().map(|e| e.1).collect();
        assert_eq!(log, (0..4).map(|k| cpu(1000 + k, 1)).collect::<Vec<_>>());
        // A page never deferred: no wait.
        let t = std::time::Instant::now();
        d.before(5 * PAGE, PAGE);
        assert!(t.elapsed() < std::time::Duration::from_millis(20));
        SLOW_MS.store(0, Relaxed);
    }

    #[test]
    fn past_the_limit_the_caller_helps() {
        let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
        TABLE.lock().unwrap().clear();
        SLOW_MS.store(2, Relaxed);
        let d = leak(8);
        for k in 0..40 {
            d.defer(cpu(2000 + 2 * k, 2));
            assert!(d.queue().pages <= 8 + 2, "bounded");
        }
        d.drain();
        let log = TABLE.lock().unwrap().clone();
        assert_eq!(log.len(), 40);
        assert!(log.iter().any(|e| e.0 == 3), "the caller applied some: {log:?}");
        // Still in call order, whoever applied them.
        let order: Vec<Op> = log.iter().map(|e| e.1).collect();
        assert_eq!(order, (0..40).map(|k| cpu(2000 + 2 * k, 2)).collect::<Vec<_>>());
        SLOW_MS.store(0, Relaxed);
    }

    /// (C3) `VICTORY_MAP_DEFER_BATCH`: a waiter applies what its page needs,
    /// with every earlier operation sharing a page with those, in call order;
    /// its page is then not pending.
    #[test]
    fn an_exact_wait_keeps_each_pages_order() {
        let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
        TABLE.lock().unwrap().clear();
        SLOW_MS.store(5, Relaxed);
        TEST_BATCH.with(|t| t.set(true));
        let d = leak(LIMIT_PAGES);
        d.defer(cpu(500, 2)); // pages 500, 501
        d.defer(cpu(600, 1));
        d.defer(cpu(400, 1));
        d.defer(cpu(501, 1)); // page 501 again
        d.before(501 * PAGE, PAGE);
        assert!(!d.pending_at(500 * PAGE) && !d.pending_at(501 * PAGE));
        let log: Vec<Op> = TABLE.lock().unwrap().iter().map(|e| e.1).collect();
        let first = log.iter().position(|&o| o == cpu(500, 2)).unwrap();
        let second = log.iter().position(|&o| o == cpu(501, 1)).unwrap();
        assert!(first < second, "{log:?}");
        d.drain();
        assert_eq!(TABLE.lock().unwrap().len(), 4, "each applied once");
        TEST_BATCH.with(|t| t.set(false));
        SLOW_MS.store(0, Relaxed);
    }
}
