//! (F21 meter, with `VICTORY_CHUNK_METER=1` on the hle side) The title's
//! arena segment C: chunks of per-draw data, each written once by one
//! recording context and final when its list is closed (0x1424749d0 hands
//! the retired chunks with the list's fence token), reused only once that
//! token has passed. Measured here: the bytes of the chunks retired against
//! the bytes our refreshes take from the segment, and the hazard the analyst
//! names: a chunk retired again (so reused) with no refresh of ours over it
//! since it was last retired ("chunk reused before read").

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// The segment: start, bytes, chunk bytes (0: not known yet).
static SEGMENT: [AtomicU64; 3] = [const { AtomicU64::new(0) }; 3];
/// Per chunk (by index in the segment): the number of its last retirement,
/// and of the last refresh of ours over it.
static CHUNKS: Mutex<Vec<(u64, u64)>> = Mutex::new(Vec::new());
static SEQ: AtomicU64 = AtomicU64::new(1);
/// Since the last line: chunks retired, their bytes, retired chunks outside
/// the segment, chunks retired again with no refresh over them since their
/// last retirement, bytes our refreshes took from the segment, refreshes
/// that touched it.
static COUNTS: [AtomicU64; 6] = [const { AtomicU64::new(0) }; 6];

/// The segment as the title's globals give it now.
pub fn segment(start: u64, len: u64, chunk: u64) {
    SEGMENT[0].store(start, Ordering::Relaxed);
    SEGMENT[1].store(len, Ordering::Relaxed);
    SEGMENT[2].store(chunk, Ordering::Relaxed);
}

fn index(address: u64) -> Option<usize> {
    let (start, len, chunk) = (SEGMENT[0].load(Ordering::Relaxed), SEGMENT[1].load(Ordering::Relaxed), SEGMENT[2].load(Ordering::Relaxed));
    (chunk != 0 && address >= start && address < start + len).then(|| ((address - start) / chunk) as usize)
}

/// A list was closed: `chunks` (their addresses) are retired with its token.
pub fn retired(chunks: &[u64]) {
    if final_on() {
        retired_final(chunks);
    }
    let chunk = SEGMENT[2].load(Ordering::Relaxed);
    let mut all = CHUNKS.lock().unwrap_or_else(|e| e.into_inner());
    for &address in chunks {
        let Some(i) = index(address) else {
            COUNTS[2].fetch_add(1, Ordering::Relaxed);
            continue;
        };
        if all.len() <= i {
            all.resize(i + 1, (0, 0));
        }
        let (last_retired, last_read) = all[i];
        if last_retired != 0 && last_read < last_retired {
            COUNTS[3].fetch_add(1, Ordering::Relaxed);
        }
        all[i].0 = SEQ.fetch_add(1, Ordering::Relaxed);
        COUNTS[0].fetch_add(1, Ordering::Relaxed);
        COUNTS[1].fetch_add(chunk, Ordering::Relaxed);
    }
}

/// A refresh of ours took title memory `[from, to)` into a host copy.
pub(crate) fn refreshed(from: usize, to: usize) {
    let (start, len) = (SEGMENT[0].load(Ordering::Relaxed), SEGMENT[1].load(Ordering::Relaxed));
    if len == 0 {
        return;
    }
    let (a, b) = ((from as u64).max(start), (to as u64).min(start + len));
    if a >= b {
        return;
    }
    COUNTS[4].fetch_add(b - a, Ordering::Relaxed);
    COUNTS[5].fetch_add(1, Ordering::Relaxed);
    let (Some(first), Some(last)) = (index(a), index(b - 1)) else {
        return;
    };
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let mut all = CHUNKS.lock().unwrap_or_else(|e| e.into_inner());
    if all.len() <= last {
        all.resize(last + 1, (0, 0));
    }
    for c in &mut all[first..=last] {
        c.1 = seq;
    }
}

/// The counts since the last call: chunks retired, their bytes, outside the
/// segment, reused before read, bytes refreshed from the segment, refreshes.
pub fn take() -> [u64; 6] {
    [0, 1, 2, 3, 4, 5].map(|i| COUNTS[i].swap(0, Ordering::Relaxed))
}

/// (Fox port C2, F21; `VICTORY_CHUNK_FINAL=1`, default off = cool_fox) Bytes of
/// the arena's chunk segment are final from the close of the list that used
/// them (0x1424749d0 retires the chunk with the list's fence token) until the
/// title reuses the chunk, which it does only after that token passed and
/// which ends at a later close that names the chunk again (open or retired).
/// So a host copy's refresh of a range that lies in retired chunks, and that
/// this copy already took whole since their last retirement, finds nothing
/// new: it is dropped before the write-watch is asked and before any compare
/// (`host::Batch`). A chunk named still open at a close is left to the
/// usual path until it is retired. Every list we submit was closed before,
/// so the chunks it reads were named at its close.
pub fn final_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_CHUNK_FINAL").is_some_and(|v| v == "1"))
}

/// Per chunk (by index): its retirement number while final (0: not final).
static FINAL: Mutex<Vec<u64>> = Mutex::new(Vec::new());

/// Per host copy (its id) and chunk: the retirement number it took the chunk
/// at, and the part of the chunk it took then (from, to; title addresses).
static TAKEN: Mutex<Option<std::collections::HashMap<(u64, usize), (u64, u64, u64)>>> = Mutex::new(None);

/// Since the line: refreshes dropped as known, their bytes; refreshes noted
/// taken; chunks named open; chunks retired.
static SPARED: [AtomicU64; 5] = [const { AtomicU64::new(0) }; 5];

/// The chunks a closing list names still open: not final.
pub fn still_open(chunks: &[u64]) {
    if !final_on() {
        return;
    }
    let mut f = FINAL.lock().unwrap_or_else(|e| e.into_inner());
    for &address in chunks {
        if let Some(i) = index(address) {
            if f.len() <= i {
                f.resize(i + 1, 0);
            }
            f[i] = 0;
            SPARED[3].fetch_add(1, Ordering::Relaxed);
        }
    }
    drop(f);
    final_line();
}

/// (`VICTORY_CHUNK_FINAL`) `chunks` were retired: final, with a new number.
fn retired_final(chunks: &[u64]) {
    let mut f = FINAL.lock().unwrap_or_else(|e| e.into_inner());
    for &address in chunks {
        if let Some(i) = index(address) {
            if f.len() <= i {
                f.resize(i + 1, 0);
            }
            f[i] = SEQ.fetch_add(1, Ordering::Relaxed);
            SPARED[4].fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// The chunks `[from, to)` lies in (first, last), if all of it lies in the
/// segment.
fn chunks_of(from: u64, to: u64) -> Option<(usize, usize)> {
    if to <= from {
        return None;
    }
    let (first, last) = (index(from)?, index(to - 1)?);
    Some((first, last))
}

/// The retirement numbers of the chunks under `[from, to)` now (taken before
/// a refresh, for [`taken`]); `None`: not all in final chunks.
pub(crate) fn before(from: usize, to: usize) -> Option<Vec<u64>> {
    if !final_on() {
        return None;
    }
    let (first, last) = chunks_of(from as u64, to as u64)?;
    let f = FINAL.lock().unwrap_or_else(|e| e.into_inner());
    let n: Vec<u64> = (first..=last).map(|i| f.get(i).copied().unwrap_or(0)).collect();
    n.iter().all(|&g| g != 0).then_some(n)
}

/// Host copy `copy` brought `[from, to)` up to date, its chunks having had the
/// numbers `numbers` ([`before`]) when it began.
pub(crate) fn taken(copy: u64, from: usize, to: usize, numbers: Option<Vec<u64>>) {
    let Some(numbers) = numbers else { return };
    let Some((first, _)) = chunks_of(from as u64, to as u64) else { return };
    let (start, chunk) = (SEGMENT[0].load(Ordering::Relaxed), SEGMENT[2].load(Ordering::Relaxed));
    let mut t = TAKEN.lock().unwrap_or_else(|e| e.into_inner());
    let t = t.get_or_insert_with(Default::default);
    if t.len() > 1 << 20 {
        t.clear();
    }
    for (k, &number) in numbers.iter().enumerate() {
        let i = first + k;
        let (lo, hi) = (start + i as u64 * chunk, start + (i as u64 + 1) * chunk);
        let (a, b) = ((from as u64).max(lo), (to as u64).min(hi));
        let e = t.entry((copy, i)).or_insert((number, a, b));
        if e.0 != number {
            *e = (number, a, b);
        } else if a <= e.2 && e.1 <= b {
            (e.1, e.2) = (e.1.min(a), e.2.max(b));
        } else if b - a > e.2 - e.1 {
            (e.1, e.2) = (a, b);
        }
    }
    SPARED[2].fetch_add(1, Ordering::Relaxed);
}

/// Whether host copy `copy` holds `[from, to)` as final: every chunk under it
/// final, and taken by this copy at its present number over that part.
pub(crate) fn known(copy: u64, from: usize, to: usize) -> bool {
    if !final_on() {
        return false;
    }
    let Some((first, last)) = chunks_of(from as u64, to as u64) else { return false };
    let (start, chunk) = (SEGMENT[0].load(Ordering::Relaxed), SEGMENT[2].load(Ordering::Relaxed));
    let f = FINAL.lock().unwrap_or_else(|e| e.into_inner());
    let t = TAKEN.lock().unwrap_or_else(|e| e.into_inner());
    let Some(t) = t.as_ref() else { return false };
    let all = (first..=last).all(|i| {
        let number = f.get(i).copied().unwrap_or(0);
        let (lo, hi) = (start + i as u64 * chunk, start + (i as u64 + 1) * chunk);
        let (a, b) = ((from as u64).max(lo), (to as u64).min(hi));
        number != 0 && t.get(&(copy, i)).is_some_and(|&(n, x, y)| n == number && x <= a && b <= y)
    });
    if all {
        SPARED[0].fetch_add(1, Ordering::Relaxed);
        SPARED[1].fetch_add((to - from) as u64, Ordering::Relaxed);
    }
    all
}

/// Every five seconds: "chunk final: ..." (`VICTORY_CHUNK_FINAL=1`).
fn final_line() {
    static LAST: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    let last = LAST.load(Ordering::Relaxed);
    if now < last + 5_000 || LAST.compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed).is_err() {
        return;
    }
    let s: Vec<u64> = SPARED.iter().map(|c| c.swap(0, Ordering::Relaxed)).collect();
    tracing::info!(
        "chunk final (VICTORY_CHUNK_FINAL=1), last 5 s: refreshes found final and dropped {} ({:.1} MB), refreshes noted as taking final chunks {}, \
         chunks named open {}, chunks retired {}; segment {:#x} + {} MB",
        s[0],
        s[1] as f64 / 1048576.0,
        s[2],
        s[3],
        s[4],
        SEGMENT[0].load(Ordering::Relaxed),
        SEGMENT[1].load(Ordering::Relaxed) >> 20
    );
}

/// (`VICTORY_CHUNK_FINAL`) Title memory was written by a step of ours at a
/// place not known: no copy holds any chunk as final any more.
pub(crate) fn forget() {
    if !final_on() {
        return;
    }
    if let Some(t) = TAKEN.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        t.clear();
    }
}

/// (`VICTORY_CHUNK_FINAL`) A step of ours wrote title memory `[at, at + len)`:
/// the chunks under it are not final until they are retired again.
pub(crate) fn wrote(at: usize, len: usize) {
    if !final_on() || len == 0 {
        return;
    }
    let (start, size) = (SEGMENT[0].load(Ordering::Relaxed), SEGMENT[1].load(Ordering::Relaxed));
    let (a, b) = ((at as u64).max(start), ((at + len) as u64).min(start + size));
    let Some((first, last)) = (a < b).then(|| chunks_of(a, b)).flatten() else { return };
    let mut f = FINAL.lock().unwrap_or_else(|e| e.into_inner());
    for g in f.iter_mut().take(last + 1).skip(first) {
        *g = 0;
    }
}
