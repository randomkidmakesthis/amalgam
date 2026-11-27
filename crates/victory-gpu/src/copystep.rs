//! (Coder W) A per-copy refresh account, **`VICTORY_REFRESH_ACCOUNT=1`** (diagnostics meter,
//! default off): per buffer copy, refreshes, bytes asked, bytes taken, time, and the 64 KiB
//! pages found written split by what came before: a shader-write landing in the last two
//! frames, or neither (title CPU, DirectStorage, mapping changes; the "copy step" column of
//! the first build stays 0: `VICTORY_COPY_STEP_BESIDE` was dropped after run-176, its steps
//! were ~17 a frame and none was taken again). Line every 5 s, top 10 by time.

use std::collections::HashMap;
use std::sync::Mutex;

const BLOCK: usize = 1 << 16;

fn maybe_line() {
    static LAST: Mutex<Option<(std::time::Instant, u64)>> = Mutex::new(None);
    let now = std::time::Instant::now();
    let frame = crate::frame::current();
    let frames = {
        let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        let (at, f) = *last.get_or_insert((now, frame));
        if now.duration_since(at).as_secs() < 5 {
            return;
        }
        *last = Some((now, frame));
        (frame.saturating_sub(f)).max(1) as f64
    };
    account_line(frames);
}

// ---------------------------------------------------------------------------------------------
// Refresh account (`VICTORY_REFRESH_ACCOUNT=1`).

pub(crate) fn account_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_REFRESH_ACCOUNT").is_some_and(|v| v == "1"))
}

/// 64 KiB block -> the asking number when a copy step last wrote it.
static STEPS: Mutex<Option<HashMap<usize, u64>>> = Mutex::new(None);

#[derive(Default, Clone, Copy)]
struct Account {
    refreshes: u64,
    asked: u64,
    taken: u64,
    ns: u64,
    again: u64,
    first: u64,
    by_step: u64,
    landed: u64,
}

static ACCOUNTS: Mutex<Option<HashMap<(u64, u64), Account>>> = Mutex::new(None);

thread_local! {
    /// What the last `copy_stale` on this thread found: pages taken again, of them after a copy
    /// step, after a landing; pages taken for the first time.
    static FOUND: std::cell::Cell<[u64; 4]> = const { std::cell::Cell::new([0; 4]) };
}

/// From `Shadow::copy_stale`: the pages taken again (page, taken at) and the pages taken for the
/// first time.
pub(crate) fn found(again: &[(usize, u64)], first: u64) {
    if !account_on() {
        return;
    }
    let (mut by_step, mut landed) = (0, 0);
    {
        let s = STEPS.lock().unwrap_or_else(|e| e.into_inner());
        for &(page, taken) in again {
            if s.as_ref().and_then(|s| s.get(&page)).is_some_and(|&w| w >= taken) {
                by_step += 1;
            } else if crate::watch::landed_lately(page * BLOCK, BLOCK, 2) {
                landed += 1;
            }
        }
    }
    FOUND.set([again.len() as u64, by_step, landed, first]);
}

/// From `Shadow::update_free`: a refresh of the copy of `(start, len)` asked `asked` bytes and
/// took `taken`, in `ns`.
pub(crate) fn refreshed(start: u64, len: u64, asked: u64, taken: u64, ns: u64) {
    let f = FOUND.replace([0; 4]);
    let mut all = ACCOUNTS.lock().unwrap_or_else(|e| e.into_inner());
    let a = all.get_or_insert_with(HashMap::new).entry((start, len)).or_default();
    a.refreshes += 1;
    a.asked += asked;
    a.taken += taken;
    a.ns += ns;
    a.again += f[0];
    a.by_step += f[1];
    a.landed += f[2];
    a.first += f[3];
    drop(all);
    maybe_line();
}

fn account_line(frames: f64) {
    if !account_on() {
        return;
    }
    let all = ACCOUNTS.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_default();
    let mut all: Vec<_> = all.into_iter().collect();
    let total_ns: u64 = all.iter().map(|a| a.1.ns).sum();
    let total_taken: u64 = all.iter().map(|a| a.1.taken).sum();
    all.sort_by_key(|a| std::cmp::Reverse(a.1.ns));
    tracing::info!(
        "refresh account (VICTORY_REFRESH_ACCOUNT=1) per frame: {} copies refreshed, {:.2} ms, {:.1} MB taken; the most time:",
        all.len(),
        total_ns as f64 / 1e6 / frames,
        total_taken as f64 / 1e6 / frames
    );
    for ((start, len), a) in all.into_iter().take(10) {
        tracing::info!(
            "  refresh account {start:#x}+{len:#x}: {:.1}x/frame, asked {:.1} MB, taken {:.2} MB, {:.2} ms; 64 KiB pages taken again {:.0} \
             (after a copy step of ours {:.0}, after a landing {:.0}, neither: title CPU / dstorage / mapping {:.0}), taken first {:.0}",
            a.refreshes as f64 / frames,
            a.asked as f64 / 1e6 / frames,
            a.taken as f64 / 1e6 / frames,
            a.ns as f64 / 1e6 / frames,
            a.again as f64 / frames,
            a.by_step as f64 / frames,
            a.landed as f64 / frames,
            a.again.saturating_sub(a.by_step + a.landed) as f64 / frames,
            a.first as f64 / frames,
        );
    }
}

// ---------------------------------------------------------------------------------------------
// (Coder W) Refreshes at a flush by what they came to, and `VICTORY_CLEAN_STAMP`.

/// `VICTORY_CLEAN_STAMP`: 0 off, 1 on, 2 verify. With it a copy keeps the last range
/// `Shadow::nothing_new` found clean and the change count (`watch::write_gen`) read before
/// it looked; asked again for a range inside it with the count unchanged, the answer is
/// "clean" at once (no pending, copied or journal lock, no walk over the range's pages and
/// the journal's). Sound: the answer reads only the journal, the published and beside bits
/// (each change of which moves the count), the shaders' ranges in flight (a new one clears
/// the stamp) and the copy's own notes (which only grow more current). `verify`: looks
/// anyway and counts stamped answers the look did not confirm (must be 0).
pub(crate) fn clean_stamp() -> u8 {
    static MODE: std::sync::OnceLock<u8> = std::sync::OnceLock::new();
    *MODE.get_or_init(|| {
        let m = match std::env::var("VICTORY_CLEAN_STAMP").as_deref().map(str::trim) {
            Ok("1") => 1,
            Ok("verify") => 2,
            _ => 0,
        };
        tracing::info!(mode = m, "clean stamps of copies (VICTORY_CLEAN_STAMP: 0 off, 1 on, 2 verify)");
        m
    })
}

pub(crate) const S_HIT: usize = 0;
const S_LOOKS: usize = 1;
const S_LOOK_NS: usize = 2;
const S_STAMPED_LOOKS: usize = 3;
const S_MISS: usize = 4;
const S_CLEAN: usize = 5;
static STAMPS: [std::sync::atomic::AtomicU64; 6] = [const { std::sync::atomic::AtomicU64::new(0) }; 6];
static MISSES_ALL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub(crate) fn stamp_count(i: usize, n: u64) {
    STAMPS[i].fetch_add(n, std::sync::atomic::Ordering::Relaxed);
}

/// A look of `nothing_new` that began at `at`: whether a stamp said clean, what it found.
pub(crate) fn stamp_looked(stamped: bool, same: bool, at: std::time::Instant) {
    stamp_count(S_LOOKS, 1);
    stamp_count(S_LOOK_NS, at.elapsed().as_nanos() as u64);
    if same {
        stamp_count(S_CLEAN, 1);
    }
    if stamped {
        stamp_count(S_STAMPED_LOOKS, 1);
        if !same {
            stamp_count(S_MISS, 1);
            MISSES_ALL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

/// Per kind (0 known clean, 1 known by final chunks, 2 refreshed): count, nanoseconds.
static FLUSHED: [std::sync::atomic::AtomicU64; 6] = [const { std::sync::atomic::AtomicU64::new(0) }; 6];

/// A refresh at a flush (`host::Batch::flush_one`) that began at `at` came to `kind`.
pub(crate) fn flushed(kind: usize, at: std::time::Instant) {
    use std::sync::atomic::Ordering::Relaxed;
    if clean_stamp() == 0 && !account_on() {
        return;
    }
    FLUSHED[kind * 2].fetch_add(1, Relaxed);
    FLUSHED[kind * 2 + 1].fetch_add(at.elapsed().as_nanos() as u64, Relaxed);
    static LAST: Mutex<Option<(std::time::Instant, u64)>> = Mutex::new(None);
    let now = std::time::Instant::now();
    let frame = crate::frame::current();
    let frames = {
        let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        let (was, f) = *last.get_or_insert((now, frame));
        if now.duration_since(was).as_secs() < 5 {
            return;
        }
        *last = Some((now, frame));
        (frame.saturating_sub(f)).max(1) as f64
    };
    let f: [u64; 6] = std::array::from_fn(|i| FLUSHED[i].swap(0, Relaxed));
    let s: [u64; 6] = std::array::from_fn(|i| STAMPS[i].swap(0, Relaxed));
    let per = |n: u64| n as f64 / frames;
    let us_each = |ns: u64, n: u64| if n == 0 { 0.0 } else { ns as f64 / 1e3 / n as f64 };
    tracing::info!(
        "refreshes at flushes per frame (submitting thread): known clean {:.0} in {:.2} ms ({:.1} us each), known by final chunks \
         {:.0} in {:.2} ms, refreshed {:.0} in {:.2} ms ({:.1} us each); clean looks (VICTORY_CLEAN_STAMP={}): answered by the \
         stamp {:.0}, looked {:.0} in {:.2} ms ({:.1} us each, {:.0} found clean), of them stamped {:.0}; MUST BE 0 since the \
         start: stamped clean but the look found otherwise {}",
        per(f[0]),
        f[1] as f64 / 1e6 / frames,
        us_each(f[1], f[0]),
        per(f[2]),
        f[3] as f64 / 1e6 / frames,
        per(f[4]),
        f[5] as f64 / 1e6 / frames,
        us_each(f[5], f[4]),
        ["0", "1", "verify"][clean_stamp() as usize],
        per(s[S_HIT]),
        per(s[S_LOOKS]),
        s[S_LOOK_NS] as f64 / 1e6 / frames,
        us_each(s[S_LOOK_NS], s[S_LOOKS]),
        per(s[S_CLEAN]),
        per(s[S_STAMPED_LOOKS]),
        MISSES_ALL.load(Relaxed),
    );
}

// ---------------------------------------------------------------------------------------------
// (Coder W, diagnostics) Waits of the submitting thread for flights before a submission, by
// why and, for "title memory read" by a refresh of another copy (a copy done on the card not
// landed yet), by which copy wrote and which reads.

static WAITS: Mutex<Option<HashMap<(usize, String), (u64, u64)>>> = Mutex::new(None);

pub(crate) fn waited_for(why: usize, blame: Option<String>, at: std::time::Instant) {
    let ns = at.elapsed().as_nanos() as u64;
    let frames = {
        static LAST: Mutex<Option<(std::time::Instant, u64)>> = Mutex::new(None);
        let now = std::time::Instant::now();
        let frame = crate::frame::current();
        let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        let (was, f) = *last.get_or_insert((now, frame));
        if now.duration_since(was).as_secs() >= 5 {
            *last = Some((now, frame));
            Some((frame.saturating_sub(f)).max(1) as f64)
        } else {
            None
        }
    };
    let mut all = WAITS.lock().unwrap_or_else(|e| e.into_inner());
    let e = all.get_or_insert_with(HashMap::new).entry((why, blame.unwrap_or_default())).or_default();
    e.0 += 1;
    e.1 += ns;
    let Some(frames) = frames else { return };
    let mut rows: Vec<_> = all.take().unwrap_or_default().into_iter().collect();
    drop(all);
    rows.sort_by_key(|r| std::cmp::Reverse(r.1.1));
    let total: u64 = rows.iter().map(|r| r.1.1).sum();
    tracing::info!(
        "waits for flights before submissions per frame (submitting thread): {:.2} ms in {:.1} waits; the longest by why and who:",
        total as f64 / 1e6 / frames,
        rows.iter().map(|r| r.1.0).sum::<u64>() as f64 / frames
    );
    for ((why, who), (n, ns)) in rows.into_iter().take(8) {
        tracing::info!("  waited for flights: why {why}, {:.2} ms in {:.2} waits a frame: {who}", ns as f64 / 1e6 / frames, n as f64 / frames);
    }
}

// ---------------------------------------------------------------------------------------------
// (Coder W, diagnostics) Per thread: ms in the write-fault handler, waiting for the guards'
// lock, waiting for the trackers' other locks (journal, ranges held). Line every 5 s: the
// threads held up most (thread ids as on copystat's "thread N submitted" and "list calls by
// thread" lines).

pub(crate) const T_FAULTS: usize = 0;
pub(crate) const T_GUARDS: usize = 1;
pub(crate) const T_OTHER_LOCKS: usize = 2;
const T_KINDS: usize = 3;

type ThreadNs = std::sync::Arc<[std::sync::atomic::AtomicU64; T_KINDS + 1]>;
static THREADS: Mutex<Vec<(u32, ThreadNs)>> = Mutex::new(Vec::new());

thread_local! {
    static MINE: ThreadNs = {
        let mine: ThreadNs = std::sync::Arc::new(std::array::from_fn(|_| std::sync::atomic::AtomicU64::new(0)));
        // SAFETY: no arguments.
        let id = unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };
        THREADS.lock().unwrap_or_else(|e| e.into_inner()).push((id, mine.clone()));
        mine
    };
}

pub(crate) fn thread_ns(kind: usize, ns: u64) {
    use std::sync::atomic::Ordering::Relaxed;
    // (In the fault handler too: a thread-local made once, then atomics.)
    let _ = MINE.try_with(|m| {
        m[kind].fetch_add(ns, Relaxed);
        m[T_KINDS].fetch_add(1, Relaxed);
    });
    // (Never from the fault handler: the line is written by a lock wait's thread.)
    static DUE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    if kind == T_FAULTS || !line_due(&DUE) {
        return;
    }
    static LAST: Mutex<Option<(std::time::Instant, u64)>> = Mutex::new(None);
    let Ok(mut last) = LAST.try_lock() else { return };
    let now = std::time::Instant::now();
    let frame = crate::frame::current();
    let (was, f) = *last.get_or_insert((now, frame));
    if now.duration_since(was).as_secs() < 5 {
        return;
    }
    *last = Some((now, frame));
    drop(last);
    let frames = (frame.saturating_sub(f)).max(1) as f64;
    let mut rows: Vec<(u32, [u64; T_KINDS])> = THREADS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .map(|(id, m)| (*id, std::array::from_fn(|k| m[k].swap(0, Relaxed))))
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1.iter().sum::<u64>()));
    let ms = |ns: u64| ns as f64 / 1e6 / frames;
    let top: Vec<String> = rows
        .iter()
        .take(10)
        .map(|(id, k)| format!("thread {id} {:.1} (faults {:.1}, guards' lock {:.1}, other locks {:.1})", ms(k.iter().sum()), ms(k[0]), ms(k[1]), ms(k[2])))
        .collect();
    tracing::info!("write trackers holding up threads, ms per frame by thread: {}", top.join("; "));
    held_line(frames);
}

// ---------------------------------------------------------------------------------------------
// (Coder W, diagnostics) Who holds the guards' lock (`watch::guards`), by the code that took
// it: count and ms held per frame. Printed with the per-thread line.

const HELD_SLOTS: usize = 128;
static HELD_AT: [std::sync::atomic::AtomicPtr<std::panic::Location<'static>>; HELD_SLOTS] =
    [const { std::sync::atomic::AtomicPtr::new(std::ptr::null_mut()) }; HELD_SLOTS];
static HELD: [[std::sync::atomic::AtomicU64; 2]; HELD_SLOTS + 1] =
    [const { [std::sync::atomic::AtomicU64::new(0), std::sync::atomic::AtomicU64::new(0)] }; HELD_SLOTS + 1];

/// (Coder W) `VICTORY_LOCK_HELD=1` (diagnostics, default off): the guards' lock is timed from
/// take to release by the code that took it. Off: no clock read at each take.
pub(crate) fn held_meter() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_LOCK_HELD").is_some_and(|v| v == "1"))
}

pub(crate) fn guards_held(at: &'static std::panic::Location<'static>, ns: u64) {
    use std::sync::atomic::Ordering::{AcqRel, Acquire, Relaxed};
    let p = std::ptr::from_ref(at).cast_mut();
    let first = (p as usize >> 4) % HELD_SLOTS;
    let mut slot = HELD_SLOTS;
    for k in 0..8 {
        let i = (first + k) % HELD_SLOTS;
        let was = HELD_AT[i].load(Acquire);
        if was == p || (was.is_null() && HELD_AT[i].compare_exchange(std::ptr::null_mut(), p, AcqRel, Acquire).map_or_else(|o| o == p, |_| true)) {
            slot = i;
            break;
        }
    }
    HELD[slot][0].fetch_add(1, Relaxed);
    HELD[slot][1].fetch_add(ns, Relaxed);
}

fn held_line(frames: f64) {
    use std::sync::atomic::Ordering::{Acquire, Relaxed};
    let mut rows: Vec<(String, u64, u64)> = (0..=HELD_SLOTS)
        .map(|i| {
            let name = if i == HELD_SLOTS {
                "other".to_string()
            } else {
                // SAFETY: null or a `Location` of the program, which lives for good.
                unsafe { HELD_AT[i].load(Acquire).as_ref() }.map_or_else(String::new, |l| format!("{}:{}", l.file().rsplit(['/', char::from(92)]).next().unwrap_or(""), l.line()))
            };
            (name, HELD[i][0].swap(0, Relaxed), HELD[i][1].swap(0, Relaxed))
        })
        .filter(|r| r.1 > 0)
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.2));
    let total: u64 = rows.iter().map(|r| r.2).sum();
    let top: Vec<String> =
        rows.iter().take(12).map(|(n, c, ns)| format!("{n} {:.2} ms in {:.0}", *ns as f64 / 1e6 / frames, *c as f64 / frames)).collect();
    tracing::info!("guards' lock held per frame, all threads: {:.2} ms; by who took it: {}", total as f64 / 1e6 / frames, top.join(", "));
}

// ---------------------------------------------------------------------------------------------
// (Coder W, diagnostics) The title's list calls by the call (`list::with_state`'s caller): count
// and ms per frame, all recording threads. Line every 5 s.

static CALL_AT: [std::sync::atomic::AtomicPtr<std::panic::Location<'static>>; HELD_SLOTS] =
    [const { std::sync::atomic::AtomicPtr::new(std::ptr::null_mut()) }; HELD_SLOTS];
static CALLS: [[std::sync::atomic::AtomicU64; 2]; HELD_SLOTS + 1] =
    [const { [std::sync::atomic::AtomicU64::new(0), std::sync::atomic::AtomicU64::new(0)] }; HELD_SLOTS + 1];

/// (Coder W) `VICTORY_LIST_CALLS=1` (diagnostics, default off): list calls are timed (by
/// call, by thread, copystat's recording lines) and `State::execute_indirect` / `State::bind`
/// split in parts. Off: none of it reads a clock or a shared counter.
pub(crate) fn list_meter() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_LIST_CALLS").is_some_and(|v| v == "1"))
}

/// Whether a 5-second line is due (one relaxed load mostly; the clock read once in 4,096 calls).
fn line_due(next: &std::sync::atomic::AtomicU64) -> bool {
    use std::sync::atomic::Ordering::Relaxed;
    static TICK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    if TICK.fetch_add(1, Relaxed) % 4096 != 0 {
        return false;
    }
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let ms = START.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64;
    let due = next.load(Relaxed);
    ms >= due && next.compare_exchange(due, ms + 5000, Relaxed, Relaxed).is_ok()
}

static CALLS_DUE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub(crate) fn list_call(at: &'static std::panic::Location<'static>, ns: u64) {
    use std::sync::atomic::Ordering::{AcqRel, Acquire, Relaxed};
    let p = std::ptr::from_ref(at).cast_mut();
    let first = (p as usize >> 4) % HELD_SLOTS;
    let mut slot = HELD_SLOTS;
    for k in 0..8 {
        let i = (first + k) % HELD_SLOTS;
        let was = CALL_AT[i].load(Acquire);
        if was == p || (was.is_null() && CALL_AT[i].compare_exchange(std::ptr::null_mut(), p, AcqRel, Acquire).map_or_else(|o| o == p, |_| true)) {
            slot = i;
            break;
        }
    }
    CALLS[slot][0].fetch_add(1, Relaxed);
    CALLS[slot][1].fetch_add(ns, Relaxed);
    if !line_due(&CALLS_DUE) {
        return;
    }
    static LAST: Mutex<Option<(std::time::Instant, u64)>> = Mutex::new(None);
    let Ok(mut last) = LAST.try_lock() else { return };
    let now = std::time::Instant::now();
    let frame = crate::frame::current();
    let (was, f) = *last.get_or_insert((now, frame));
    if now.duration_since(was).as_secs() < 5 {
        return;
    }
    *last = Some((now, frame));
    drop(last);
    let frames = (frame.saturating_sub(f)).max(1) as f64;
    let mut rows: Vec<(String, u64, u64)> = (0..=HELD_SLOTS)
        .map(|i| {
            let name = if i == HELD_SLOTS {
                "other".to_string()
            } else {
                // SAFETY: null or a `Location` of the program, which lives for good.
                unsafe { CALL_AT[i].load(Acquire).as_ref() }
                    .map_or_else(String::new, |l| format!("{}:{}", l.file().rsplit(['/', char::from(92)]).next().unwrap_or(""), l.line()))
            };
            (name, CALLS[i][0].swap(0, Relaxed), CALLS[i][1].swap(0, Relaxed))
        })
        .filter(|r| r.1 > 0)
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.2));
    let total: u64 = rows.iter().map(|r| r.2).sum();
    let top: Vec<String> = rows
        .iter()
        .take(16)
        .map(|(n, c, ns)| format!("{n} {:.2} ms in {:.0} ({:.1} us each)", *ns as f64 / 1e6 / frames, *c as f64 / frames, *ns as f64 / 1e3 / (*c).max(1) as f64))
        .collect();
    tracing::info!("list calls by call per frame, all recording threads: {:.2} ms; {}", total as f64 / 1e6 / frames, top.join(", "));
    ei_line(frames);
    bind_line(frames);
    #[cfg(feature = "diagnostics")]
    crate::draw::tables_line(frames);
}

// ---------------------------------------------------------------------------------------------
// (Coder W, diagnostics) `State::execute_indirect` (the title's ExecuteIndirect, list.rs:1965)
// in parts: 0 checks and the host pipeline, 1 finding the arguments' (and count's) host copies,
// 2 the host command signature, 3 refreshes, batch, window and clamp, 4 prepare (state, root
// arguments, tables), 5 ExecuteIndirect, 6 after (drew / written). Calls that return early are
// counted in the parts they reached.

const EI_PARTS: usize = 7;
static EI: [std::sync::atomic::AtomicU64; EI_PARTS + 1] = [const { std::sync::atomic::AtomicU64::new(0) }; EI_PARTS + 1];

pub(crate) struct EiSplit(Option<std::time::Instant>);

impl EiSplit {
    pub(crate) fn begin() -> Self {
        if !list_meter() {
            return EiSplit(None);
        }
        EI[EI_PARTS].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        EiSplit(Some(std::time::Instant::now()))
    }
    pub(crate) fn part(&mut self, i: usize) {
        let Some(was) = self.0 else { return };
        let now = std::time::Instant::now();
        EI[i].fetch_add((now - was).as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
        self.0 = Some(now);
    }
}

fn ei_line(frames: f64) {
    use std::sync::atomic::Ordering::Relaxed;
    let v: [u64; EI_PARTS + 1] = std::array::from_fn(|i| EI[i].swap(0, Relaxed));
    let calls = v[EI_PARTS].max(1) as f64;
    const NAMES: [&str; EI_PARTS] = ["pipeline and checks", "arguments' host copies", "signature", "refresh/batch/window/clamp", "prepare", "ExecuteIndirect", "after"];
    let parts: Vec<String> =
        (0..EI_PARTS).map(|i| format!("{} {:.2} ms ({:.1} us a call)", NAMES[i], v[i] as f64 / 1e6 / frames, v[i] as f64 / 1e3 / calls)).collect();
    tracing::info!("execute_indirect per frame, all recording threads: {:.0} calls; {}", calls / frames, parts.join(", "));
}

// (Coder W, diagnostics) `host::Batch::flush_all` in parts, all flushes of the submitting thread:
// 0 refresh-once notes, 1 final chunks, 2 known clean (few), 3 the ask ahead, 4 refreshes on the
// workers (waited for), 5 refreshes one by one here, 6 the asking's end. Line every 5 s.

const FL_PARTS: usize = 7;
/// Per part ns; then flushes, refreshes in, refreshes left for 4/5, flushes that went wide.
static FL: [std::sync::atomic::AtomicU64; FL_PARTS + 4] = [const { std::sync::atomic::AtomicU64::new(0) }; FL_PARTS + 4];

pub(crate) struct FlushSplit(std::time::Instant);

impl FlushSplit {
    pub(crate) fn begin(refreshes: usize) -> Self {
        use std::sync::atomic::Ordering::Relaxed;
        FL[FL_PARTS].fetch_add(1, Relaxed);
        FL[FL_PARTS + 1].fetch_add(refreshes as u64, Relaxed);
        FlushSplit(std::time::Instant::now())
    }
    pub(crate) fn left(&mut self, n: usize) {
        FL[FL_PARTS + 2].fetch_add(n as u64, std::sync::atomic::Ordering::Relaxed);
    }
    pub(crate) fn part(&mut self, i: usize) {
        use std::sync::atomic::Ordering::Relaxed;
        let now = std::time::Instant::now();
        FL[i].fetch_add((now - self.0).as_nanos() as u64, Relaxed);
        if i == 4 {
            FL[FL_PARTS + 3].fetch_add(1, Relaxed);
        }
        self.0 = now;
        if i != 6 {
            return;
        }
        static DUE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        static FRAME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        let ms = START.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64;
        let due = DUE.load(Relaxed);
        if ms < due || DUE.compare_exchange(due, ms + 5000, Relaxed, Relaxed).is_err() {
            return;
        }
        let frame = crate::frame::current();
        let frames = frame.saturating_sub(FRAME.swap(frame, Relaxed)).max(1) as f64;
        let v: [u64; FL_PARTS + 4] = std::array::from_fn(|k| FL[k].swap(0, Relaxed));
        const NAMES: [&str; FL_PARTS] =
            ["refresh-once notes", "final chunks", "known clean (few)", "ask ahead", "on the workers", "one by one here", "asking's end"];
        let parts: Vec<String> = (0..FL_PARTS).map(|k| format!("{} {:.2}", NAMES[k], v[k] as f64 / 1e6 / frames)).collect();
        tracing::info!(
            "flushes per frame (submitting thread): {:.1} flushes of {:.0} refreshes, {:.0} left after the known-clean look, {:.1} went to the workers; ms: {}",
            v[FL_PARTS] as f64 / frames,
            v[FL_PARTS + 1] as f64 / frames,
            v[FL_PARTS + 2] as f64 / frames,
            v[FL_PARTS + 3] as f64 / frames,
            parts.join(", ")
        );
    }
}

// (Coder W, diagnostics) `State::bind` in parts: 0 textures picked by index (`early_textures`),
// 1 heaps, depth, root signature, 2 pipeline and the stages' root arguments, tables and views.
// (Fox T) Part 2 split further: 2 pipeline, half places, picks; 3 tables; 4 memory by
// address and user data; 5 root views; 6 twins written. The count is the last.
const BIND_PARTS: usize = 7;
static BIND: [std::sync::atomic::AtomicU64; BIND_PARTS + 1] = [const { std::sync::atomic::AtomicU64::new(0) }; BIND_PARTS + 1];

pub(crate) struct BindSplit(Option<std::time::Instant>);

impl BindSplit {
    pub(crate) fn begin() -> Self {
        if !list_meter() {
            return BindSplit(None);
        }
        BIND[BIND_PARTS].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        BindSplit(Some(std::time::Instant::now()))
    }
    pub(crate) fn part(&mut self, i: usize) {
        let Some(was) = self.0 else { return };
        let now = std::time::Instant::now();
        BIND[i].fetch_add((now - was).as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
        self.0 = Some(now);
    }
}

fn bind_line(frames: f64) {
    use std::sync::atomic::Ordering::Relaxed;
    let v: [u64; BIND_PARTS + 1] = std::array::from_fn(|i| BIND[i].swap(0, Relaxed));
    let calls = v[BIND_PARTS].max(1) as f64;
    const NAMES: [&str; BIND_PARTS] = [
        "textures picked by index",
        "heaps, depth, root signature",
        "pipeline, half places, picks",
        "descriptor tables",
        "memory by address, user data",
        "root views",
        "twins written",
    ];
    let parts: Vec<String> =
        (0..BIND_PARTS).map(|i| format!("{} {:.2} ms ({:.1} us a call)", NAMES[i], v[i] as f64 / 1e6 / frames, v[i] as f64 / 1e3 / calls)).collect();
    tracing::info!("bind per frame, all recording threads: {:.0} calls; {}", calls / frames, parts.join(", "));
}

// (Coder W) Where a refresh's time goes, summed over the threads that ran it (diagnostics; on
// with `VICTORY_REFRESH_SPLIT=1`): the stale path whole (its take inside), today's path's ask,
// walk (its take inside) and take, texture refreshes done in front; count, ms, MB. Line every 5 s.
pub(crate) const R_FAST: usize = 0;
pub(crate) const R_FAST_TAKE: usize = 1;
pub(crate) const R_OLD_ASK: usize = 2;
pub(crate) const R_OLD_WALK: usize = 3;
pub(crate) const R_OLD_TAKE: usize = 4;
pub(crate) const R_TEXTURES: usize = 5;
/// Every read of title memory into ours (`host::read`: the guarded copy, `safecopy`), any caller.
pub(crate) const R_READ: usize = 6;
/// Inside an ask (`watch::writes` when it must ask): the kernel's write-watch question (its pages),
/// the guards' walk and re-arming (blocks found written), the journal note (pages noted).
pub(crate) const R_ASK_KERNEL: usize = 7;
pub(crate) const R_ASK_GUARD: usize = 8;
pub(crate) const R_ASK_NOTE: usize = 9;
const R_KINDS: usize = 10;
static RS: [[std::sync::atomic::AtomicU64; 3]; R_KINDS] =
    [const { [std::sync::atomic::AtomicU64::new(0), std::sync::atomic::AtomicU64::new(0), std::sync::atomic::AtomicU64::new(0)] }; R_KINDS];

pub(crate) fn split_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_REFRESH_SPLIT").is_some_and(|v| v == "1"))
}

pub(crate) fn rsplit(kind: usize, at: std::time::Instant, bytes: usize) {
    use std::sync::atomic::Ordering::Relaxed;
    if !split_on() {
        return;
    }
    RS[kind][0].fetch_add(1, Relaxed);
    RS[kind][1].fetch_add(at.elapsed().as_nanos() as u64, Relaxed);
    RS[kind][2].fetch_add(bytes as u64, Relaxed);
    static DUE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    if !line_due(&DUE) {
        return;
    }
    static FRAME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let frame = crate::frame::current();
    let frames = frame.saturating_sub(FRAME.swap(frame, Relaxed)).max(1) as f64;
    let v: Vec<[u64; 3]> = (0..R_KINDS).map(|k| std::array::from_fn(|i| RS[k][i].swap(0, Relaxed))).collect();
    let f = |k: usize| format!("{:.0} in {:.2} ms", v[k][0] as f64 / frames, v[k][1] as f64 / 1e6 / frames);
    tracing::info!(
        "refresh time split per frame (summed over the threads that ran them; VICTORY_REFRESH_SPLIT=1): stale path {} (its takes {}, {:.1} MB); \
         today's path: asks {}, walks {} (their takes {}, {:.1} MB); texture refreshes in front {}; all reads of title memory (guarded copy) {} ({:.1} MB); asks inside: kernel {} over {:.0} pages, guards {} finding {:.0} blocks written, notes {} of {:.0} pages",
        f(R_FAST),
        f(R_FAST_TAKE),
        v[R_FAST_TAKE][2] as f64 / 1e6 / frames,
        f(R_OLD_ASK),
        f(R_OLD_WALK),
        f(R_OLD_TAKE),
        v[R_OLD_TAKE][2] as f64 / 1e6 / frames,
        f(R_TEXTURES),
        f(R_READ),
        v[R_READ][2] as f64 / 1e6 / frames,
        f(R_ASK_KERNEL),
        v[R_ASK_KERNEL][2] as f64 / frames,
        f(R_ASK_GUARD),
        v[R_ASK_GUARD][2] as f64 / frames,
        f(R_ASK_NOTE),
        v[R_ASK_NOTE][2] as f64 / frames
    );
}

// (Coder W, diagnostics, `VICTORY_REFRESH_STALE=1`) Refreshes the stale path answered: pages
// in their ranges, stale pages looked at, pages taken. On the walks line.
static STALE_WALKED: [std::sync::atomic::AtomicU64; 4] = [const { std::sync::atomic::AtomicU64::new(0) }; 4];

pub(crate) fn stale_walked(range_pages: usize, looked: usize, taken: usize) {
    use std::sync::atomic::Ordering::Relaxed;
    STALE_WALKED[0].fetch_add(1, Relaxed);
    STALE_WALKED[1].fetch_add(range_pages as u64, Relaxed);
    STALE_WALKED[2].fetch_add(looked as u64, Relaxed);
    STALE_WALKED[3].fetch_add(taken as u64, Relaxed);
}

// ---------------------------------------------------------------------------------------------
// (Coder W, diagnostics) `Shadow::copy_stale` (REFRESH_STALE step 1): 64 KiB pages walked per
// refresh, journal entries visited (written pages in the range), pages taken. Line every 5 s.

static WALKED: [std::sync::atomic::AtomicU64; 4] = [const { std::sync::atomic::AtomicU64::new(0) }; 4];

pub(crate) fn walked(pages: usize, taken: usize, entries: usize) {
    use std::sync::atomic::Ordering::Relaxed;
    WALKED[0].fetch_add(1, Relaxed);
    WALKED[1].fetch_add(pages as u64, Relaxed);
    WALKED[2].fetch_add(taken as u64, Relaxed);
    WALKED[3].fetch_add(entries as u64, Relaxed);
    static DUE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    if !line_due(&DUE) {
        return;
    }
    static FRAME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let frame = crate::frame::current();
    let frames = frame.saturating_sub(FRAME.swap(frame, Relaxed)).max(1) as f64;
    let v: [u64; 4] = std::array::from_fn(|i| WALKED[i].swap(0, Relaxed));
    let st: [u64; 4] = std::array::from_fn(|i| STALE_WALKED[i].swap(0, Relaxed));
    tracing::info!(
        "refresh walks per frame (copy_stale): {:.0} walks over {:.0} pages ({:.0} journal entries visited), {:.0} pages taken ({:.2} % of those walked); \
         by the stale pages (VICTORY_REFRESH_STALE=1): {:.0} refreshes over {:.0} pages, {:.0} stale pages looked at, {:.0} taken",
        v[0] as f64 / frames,
        v[1] as f64 / frames,
        v[3] as f64 / frames,
        v[2] as f64 / frames,
        v[2] as f64 * 100.0 / v[1].max(1) as f64,
        st[0] as f64 / frames,
        st[1] as f64 / frames,
        st[2] as f64 / frames,
        st[3] as f64 / frames
    );
}
