//! (coder G, F24/F25) `VICTORY_PAGE_KINDS=1` (diagnostics builds; default off): the GPU side's half of the page
//! kinds meter. The runtime (`victory-hle` xmem `pagekinds`) knows for each title address whether the pool page
//! there is shown in the CPU view too or in the GPU view only, and registers [`set_query`]. Here: the layer's
//! work over title memory split by that (write-watch asks, write-guard arms and faults, copies read from title
//! memory: pages and ns), title-code writes found on GPU-only pages (must be 0), and the host texture copies
//! alive whose whole source is GPU-only. The runtime prints it all on its 5 s line ([`line`]).

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering::Relaxed};

/// Whether the meter runs.
pub fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| cfg!(feature = "diagnostics") && std::env::var("VICTORY_PAGE_KINDS").is_ok_and(|v| v.trim() == "1"))
}

/// (B) `VICTORY_GPU_ONLY`: 0 off (default), 1 on (coder K: pages the title keeps GPU-only are not tracked;
/// their copies change on known events only), 2 `verify` (today's tracking kept; what the GPU-only path would
/// have missed counted).
pub fn gpu_only_mode() -> u8 {
    static MODE: OnceLock<u8> = OnceLock::new();
    *MODE.get_or_init(|| match std::env::var("VICTORY_GPU_ONLY").as_deref().map(str::trim) {
        Ok("verify") => 2,
        Ok("1") => {
            tracing::info!(
                "VICTORY_GPU_ONLY=1: pages the title keeps GPU-only (F24 table) are not write-watched, guarded or compared; their copies change on our dstorage writes, mapping changes, new levels (F25 alloc) and shader writes; copies live as without the rule unless VICTORY_GPU_ONLY_LIFETIME=free|dead"
            );
            1
        }
        _ => 0,
    })
}

/// Whether the runtime keeps the page kinds (the meter or B).
pub fn tracked() -> bool {
    on() || gpu_only_mode() != 0
}

/// The runtime's count of legitimate changes of the pool page at a title address: our DirectStorage writes into
/// it and mapping changes of the address.
pub type Legit = fn(usize) -> u32;
static LEGIT: AtomicUsize = AtomicUsize::new(0);

pub fn set_legit(f: Legit) {
    LEGIT.store(f as usize, Relaxed);
}

fn legit(at: usize) -> u32 {
    match LEGIT.load(Relaxed) {
        0 => 0,
        f => {
            // SAFETY: set from a `Legit`.
            let q: Legit = unsafe { std::mem::transmute::<usize, Legit>(f) };
            q(at)
        }
    }
}

/// (B verify) Our own writers of title memory, as coder H's lockmeter credits them (the same note points in
/// `watch`): buffer copies at submission, mapping changes, shader writes landing, our copies (and dstorage's
/// `kernel_will_write`), kernel reads for the title, dstorage pages reported at their addresses.
#[derive(Clone, Copy)]
pub(crate) enum Ours {
    ListCopy = 0,
    Mapped = 1,
    Landing = 2,
    OurCopy = 3,
    KernelRead = 4,
    KernelPages = 5,
}
const OURS: usize = 6;
const OURS_NAMES: [&str; OURS] =
    ["buffer copies at submission", "mapping changes", "shader writes landing", "our copies / dstorage", "kernel reads", "dstorage pages"];

/// Per 64 KiB page: a count per writer of ours.
static OUR_WRITES: std::sync::Mutex<Option<std::collections::HashMap<usize, [u32; OURS]>>> = std::sync::Mutex::new(None);

/// One of our writers wrote, or is about to write, `[at, at + len)`.
pub(crate) fn ours(source: Ours, at: usize, len: usize) {
    event(source as u8, at, len);
    if gpu_only_mode() != 2 || len == 0 {
        return;
    }
    let mut w = OUR_WRITES.lock().unwrap_or_else(|e| e.into_inner());
    let w = w.get_or_insert_with(Default::default);
    for p in at / (64 << 10)..at.saturating_add(len).div_ceil(64 << 10) {
        let e = w.entry(p).or_default();
        e[source as usize] = e[source as usize].wrapping_add(1);
    }
}

/// (B verify) Written pages the tracking found on GPU-only pool pages: all; explained by the runtime (our
/// dstorage write into the pool page, a mapping change of the address); explained by each writer of ours;
/// first findings (nothing to compare with); missed (no explanation: must be 0).
static VERIFY: [AtomicU64; 4 + OURS] = [const { AtomicU64::new(0) }; 4 + OURS];
static LAST_LEGIT: std::sync::Mutex<Option<std::collections::HashMap<usize, (u32, [u32; OURS])>>> =
    std::sync::Mutex::new(None);

/// (B verify) The write tracking found these 64 KiB pages (`watch` page numbers) written by CPU code.
pub(crate) fn found_written(pages: impl Iterator<Item = usize>) {
    if gpu_only_mode() != 2 {
        return;
    }
    for p in pages {
        let at = p * (64 << 10);
        if query(at, 1)[0] == 0 {
            continue;
        }
        VERIFY[0].fetch_add(1, Relaxed);
        let l = legit(at);
        let o = OUR_WRITES
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .and_then(|w| w.get(&p).copied())
            .unwrap_or_default();
        let was = {
            let mut last = LAST_LEGIT.lock().unwrap_or_else(|e| e.into_inner());
            last.get_or_insert_with(Default::default).insert(p, (l, o))
        };
        match was {
            None => {
                VERIFY[2].fetch_add(1, Relaxed);
            }
            Some((wl, _)) if wl != l => {
                VERIFY[1].fetch_add(1, Relaxed);
            }
            Some((_, wo)) => match (0..OURS).find(|&i| wo[i] != o[i]) {
                Some(i) => {
                    VERIFY[4 + i].fetch_add(1, Relaxed);
                }
                None => {
                    let n = VERIFY[3].fetch_add(1, Relaxed);
                    stale_case("verify: written with nothing of ours to explain it", at, 64 << 10, 0);
                    if n < 16 {
                        tracing::warn!(
                            at = format_args!("{at:#x}"),
                            copy = ?copy_over(at, 64 << 10),
                            "VICTORY_GPU_ONLY=verify: a GPU-only page found written with no write of ours, dstorage write or mapping change since the last finding"
                        );
                    }
                }
            },
        }
    }
}

/// Bytes of `[at, at + len)` on GPU-only pool pages, on pool pages the CPU view shows, elsewhere.
pub type Query = fn(usize, usize) -> [u64; 3];

static QUERY: AtomicUsize = AtomicUsize::new(0);

/// The runtime's page kinds (once).
pub fn set_query(f: Query) {
    QUERY.store(f as usize, Relaxed);
}

/// See [`Query`]; all "elsewhere" without the runtime.
pub fn query(at: usize, len: usize) -> [u64; 3] {
    match QUERY.load(Relaxed) {
        0 => [0, 0, len as u64],
        f => {
            // SAFETY: set from a `Query`.
            let q: Query = unsafe { std::mem::transmute::<usize, Query>(f) };
            q(at, len)
        }
    }
}

/// The layer's work over title memory.
#[derive(Clone, Copy)]
pub(crate) enum Work {
    WriteWatchAsk = 0,
    GuardArm = 1,
    GuardFault = 2,
    CopyFromTitle = 3,
}
const WORKS: usize = 4;
const WORK_NAMES: [&str; WORKS] = ["write-watch asks", "write-guard arms", "write-guard faults", "copies read from title memory"];

/// Per work: bytes on GPU-only pages, bytes on CPU-visible pool pages, calls touching GPU-only pages, their ns,
/// all calls, their ns.
static WORK: [[AtomicU64; 6]; WORKS] = [const { [const { AtomicU64::new(0) }; 6] }; WORKS];

/// Counts one piece of the layer's work over `[at, at + len)` that took `ns`.
pub(crate) fn work(kind: Work, at: usize, len: usize, ns: u64) {
    if !on() || len == 0 {
        return;
    }
    let [gpu, cpu, _] = query(at, len);
    let w = &WORK[kind as usize];
    w[0].fetch_add(gpu, Relaxed);
    w[1].fetch_add(cpu, Relaxed);
    if gpu != 0 {
        w[2].fetch_add(1, Relaxed);
        w[3].fetch_add(ns, Relaxed);
    }
    w[4].fetch_add(1, Relaxed);
    w[5].fetch_add(ns, Relaxed);
}

/// A timer for [`work`] (a no-op when the meter is off).
pub(crate) struct Timed(Option<(std::time::Instant, Work, usize, usize)>);

impl Timed {
    pub(crate) fn new(kind: Work, at: usize, len: usize) -> Timed {
        Timed(on().then(|| (std::time::Instant::now(), kind, at, len)))
    }
}

impl Drop for Timed {
    fn drop(&mut self) {
        if let Some((t, kind, at, len)) = self.0 {
            work(kind, at, len, t.elapsed().as_nanos() as u64);
        }
    }
}

/// Title-code writes that faulted on a GPU-only page (write guards): count, and the first few logged.
static TITLE_WRITES: AtomicU64 = AtomicU64::new(0);

/// A write fault at `at` by the instruction at `rip` (from `watch`'s handler).
pub(crate) fn write_fault(at: usize, rip: usize) {
    if !tracked() {
        return;
    }
    if query(at, 1)[0] == 0 {
        return;
    }
    let base = crate::loadmeter::title_range();
    if !(base.0..base.1).contains(&rip) {
        return;
    }
    let n = TITLE_WRITES.fetch_add(1, Relaxed);
    if n < 32 {
        tracing::warn!(
            at = format_args!("{at:#x}"),
            writer = format_args!("title+{:#x}", rip - base.0),
            "page kinds (VICTORY_PAGE_KINDS=1): title code wrote a GPU-only pool page (F24 says it never does)"
        );
    }
}

/// First seen (by [`line`]'s census) per host copy serial, ms since the start.
static SEEN: std::sync::Mutex<Option<std::collections::HashMap<u64, u64>>> = std::sync::Mutex::new(None);

fn ms() -> u64 {
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64
}

/// The host copy of a placed texture over `[at, at + len)` if one is alive: how long (ms) it has been seen
/// alive (at the 5 s census grain; 0: younger than the last census).
pub fn copy_over(at: usize, len: usize) -> Option<u64> {
    #[cfg(windows)]
    {
        let serial = crate::resource::placed_copy_in(at as u64, len as u64)?;
        let seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
        Some(seen.as_ref().and_then(|s| s.get(&serial)).map_or(0, |&t| ms().saturating_sub(t)))
    }
    #[cfg(not(windows))]
    {
        let _ = (at, len);
        None
    }
}

/// The GPU side's part of the 5 s line; `seconds` since the last one.
pub fn line(seconds: f64, frames: f64, view: fn(usize) -> u8) -> String {
    let mut out = String::new();
    #[cfg(windows)]
    {
        let copies = crate::resource::placed_copies();
        let now = ms();
        let (mut n, mut bytes, mut gpu_n, mut gpu_bytes, mut cpu_n) = (0u64, 0u64, 0u64, 0u64, 0u64);
        let mut in_view = [0u64; 3];
        {
            let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
            let seen = seen.get_or_insert_with(Default::default);
            let alive: std::collections::HashSet<u64> = copies.iter().map(|c| c.2).collect();
            seen.retain(|s, _| alive.contains(s));
            for &(at, len, serial, host) in &copies {
                seen.entry(serial).or_insert(now);
                in_view[usize::from(view(at as usize).min(2))] += 1;
                n += 1;
                bytes += host;
                let [g, c, _] = query(at as usize, len as usize);
                if g == len && len != 0 {
                    gpu_n += 1;
                    gpu_bytes += host;
                } else if c != 0 {
                    cpu_n += 1;
                }
            }
        }
        out += &format!(
            "host copies of placed textures alive {n} ({:.0} MB): source all GPU-only {gpu_n} ({:.0} MB), some CPU-visible {cpu_n}; their addresses in the CPU region {}, GPU region {}, elsewhere {}",
            bytes as f64 / 1048576.0,
            gpu_bytes as f64 / 1048576.0,
            in_view[0],
            in_view[1],
            in_view[2]
        );
    }
    let frames = frames.max(1.0);
    for (k, name) in WORK_NAMES.iter().enumerate() {
        let v: Vec<u64> = WORK[k].iter().map(|c| c.swap(0, Relaxed)).collect();
        out += &format!(
            "; {name}: GPU-only {:.1} MB / CPU-visible {:.1} MB, calls on GPU-only pages {:.1}/frame ({:.3} ms/frame) of {:.1}/frame ({:.3} ms/frame)",
            v[0] as f64 / 1048576.0,
            v[1] as f64 / 1048576.0,
            v[2] as f64 / frames,
            v[3] as f64 / 1e6 / frames,
            v[4] as f64 / frames,
            v[5] as f64 / 1e6 / frames,
        );
    }
    out += &format!(
        "; title-code writes on GPU-only pages {} (must be 0) over {seconds:.0} s",
        TITLE_WRITES.load(Relaxed)
    );
    if gpu_only_mode() == 1 {
        out += &untracked_line(frames);
    }
    if gpu_only_mode() == 2 {
        let v: Vec<u64> = VERIFY.iter().map(|c| c.load(Relaxed)).collect();
        let by: Vec<String> = (0..OURS).map(|i| format!("{} {}", OURS_NAMES[i], v[4 + i])).collect();
        out += &format!(
            "; VICTORY_GPU_ONLY=verify since the start: written 64 KiB pages found by the write tracking on GPU-only pages {}, explained by a dstorage write or mapping change (runtime) {}, by our writers: {}; first findings {}; missed {} (must be 0)",
            v[0],
            v[1],
            by.join(", "),
            v[2],
            v[3]
        );
    }
    out
}

// ---- (coder K) `VICTORY_GPU_ONLY=1`: no tracking on pages the title keeps GPU-only ----

const BLOCK: usize = 64 << 10;

/// The runtime's answer: whether every 64 KiB page of `[at, at + len)` is GPU-only by the title's table
/// [0x1471bfb08] (bit 63 set, bit 62 clear).
pub type GpuOnly = fn(usize, usize) -> bool;
static GPU_ONLY: AtomicUsize = AtomicUsize::new(0);

pub fn set_gpu_only(f: GpuOnly) {
    GPU_ONLY.store(f as usize, Relaxed);
}

fn table_gpu_only(at: usize, len: usize) -> bool {
    match GPU_ONLY.load(Relaxed) {
        0 => false,
        f => {
            // SAFETY: set from a `GpuOnly`.
            let q: GpuOnly = unsafe { std::mem::transmute::<usize, GpuOnly>(f) };
            q(at, len)
        }
    }
}

/// Blocks answered without tracking since they were last taken whole (set on entry, cleared when the block
/// returns to today's tracking): the known events note these in the journal.
static UNTRACKED: crate::watch::Bits = crate::watch::Bits::new();
/// Blocks given back to today's tracking for good (the safety net found one stale).
static KEPT: crate::watch::Bits = crate::watch::Bits::new();

/// Whether `[start, end)` lies wholly on pages the title keeps GPU-only, each shown at this one address
/// (no other view of the page, `crate::inplace::shown_twice`), none given back for good.
pub(crate) fn exempt(start: usize, end: usize) -> bool {
    matches!(verdict(start, end), Verdict::Under)
}

/// Why a range is or is not under the rule.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Verdict {
    Under,
    /// Mode off, or not every page GPU-only by the title's table.
    NotGpuOnly,
    /// Given back for good by the safety net.
    Kept,
    /// The page is shown at another address that really shows it: in the CPU region where the title's table
    /// says CPU-mapped (0), in the GPU region (1), outside the region (2); the address.
    Twice(u8, usize),
}

/// The runtime's answer for an address of the region's CPU view: 0 the title's table says CPU-unmapped
/// there, 1 CPU-mapped, 2 in the GPU view, 3 outside the region.
pub type CpuView = fn(usize) -> u8;
static CPU_VIEW: AtomicUsize = AtomicUsize::new(0);

pub fn set_cpu_view(f: CpuView) {
    CPU_VIEW.store(f as usize, Relaxed);
}

fn cpu_view(at: usize) -> u8 {
    match CPU_VIEW.load(Relaxed) {
        0 => 3,
        f => {
            // SAFETY: set from a `CpuView`.
            let q: CpuView = unsafe { std::mem::transmute::<usize, CpuView>(f) };
            q(at)
        }
    }
}

/// Our own views of the pool's sections (address, bytes), told by the runtime every 5 s: not the title's, an
/// address there never disqualifies a copy (one made since the last word does, until then: safe).
static OUR_VIEWS: std::sync::RwLock<Vec<(usize, usize)>> = std::sync::RwLock::new(Vec::new());

pub fn set_our_views(views: Vec<(usize, usize)>) {
    *OUR_VIEWS.write().unwrap_or_else(|e| e.into_inner()) = views;
}

/// The title's linear views of its pool sections (address, bytes, section), told every 5 s: a known class
/// (decision run-144: the title does not write texture bytes through it; checked by [`linear_round`]), not a
/// reason to refuse.
static LINEAR: std::sync::RwLock<Vec<(usize, usize, usize)>> = std::sync::RwLock::new(Vec::new());

pub fn set_linear_views(views: Vec<(usize, usize, usize)>) {
    let lo = views.iter().map(|v| v.0).min().unwrap_or(0);
    let hi = views.iter().map(|v| v.0 + v.1).max().unwrap_or(0);
    *LINEAR.write().unwrap_or_else(|e| e.into_inner()) = views;
    LINEAR_SPAN[0].store(lo, Relaxed);
    LINEAR_SPAN[1].store(hi, Relaxed);
}

/// The lowest start and highest end of the linear views (a quick "not there").
static LINEAR_SPAN: [AtomicUsize; 2] = [const { AtomicUsize::new(0) }; 2];

fn near_linear(at: usize, len: usize) -> bool {
    let (lo, hi) = (LINEAR_SPAN[0].load(Relaxed), LINEAR_SPAN[1].load(Relaxed));
    lo < hi && at < hi && at.saturating_add(len) > lo
}

/// Blocks of the linear view our own layer read (`crate::host::read`) since the watch last looked: a touch
/// found there may be ours.
static OUR_LINEAR_READS: crate::watch::Bits = crate::watch::Bits::new();
/// Blocks of the linear view a known event wrote since the watch last looked.
static LINEAR_KNOWN: crate::watch::Bits = crate::watch::Bits::new();
/// (coder K2) Blocks under the rule (GPU-view address) whose mapping changed since the watch last looked: the
/// title's mapping executor works on the pool through the linear view (analyst 10, T13).
static REMAPPED: crate::watch::Bits = crate::watch::Bits::new();

/// Our layer reads title memory `[at, at + len)` (copies, digests).
pub(crate) fn our_read(at: usize, len: usize) {
    if gpu_only_mode() != 1 || len == 0 || !near_linear(at, len) {
        return;
    }
    for b in at / BLOCK..at.saturating_add(len).div_ceil(BLOCK) {
        OUR_LINEAR_READS.set(b);
    }
}

fn linear_at(at: usize) -> bool {
    LINEAR.read().unwrap_or_else(|e| e.into_inner()).iter().any(|&(a, n, _)| (a..a + n).contains(&at))
}

/// The linear-view address of the page shown at `at`, if the title has a linear view of its section.
fn linear_of(at: usize) -> Option<usize> {
    let (section, offset) = crate::inplace::key_of(at)?;
    let l = LINEAR.read().unwrap_or_else(|e| e.into_inner());
    l.iter().find(|v| v.2 == section && (offset as usize) < v.1).map(|v| v.0 + offset as usize)
}

fn ours_at(at: usize) -> bool {
    OUR_VIEWS.read().unwrap_or_else(|e| e.into_inner()).iter().any(|&(a, n)| (a..a + n).contains(&at))
}

/// The runtime's description of an address (for the refusal log): which of its views holds it.
pub type Describe = fn(usize) -> String;
static DESCRIBE: AtomicUsize = AtomicUsize::new(0);

pub fn set_describe(f: Describe) {
    DESCRIBE.store(f as usize, Relaxed);
}

pub(crate) fn describe(at: usize) -> String {
    match DESCRIBE.load(Relaxed) {
        0 => String::new(),
        f => {
            // SAFETY: set from a `Describe`.
            let q: Describe = unsafe { std::mem::transmute::<usize, Describe>(f) };
            q(at)
        }
    }
}

/// See [`Verdict`]. Another view of a page counts only if our record's view there really shows that page
/// and it is not a CPU-view address of the region the title's table calls CPU-unmapped (the title does not
/// write through an address it unmapped; our runtime may keep it mapped, run-140: no copy ever entered).
pub(crate) fn verdict(start: usize, end: usize) -> Verdict {
    if gpu_only_mode() != 1 || start >= end {
        return Verdict::NotGpuOnly;
    }
    let (first, last) = (start / BLOCK, end.div_ceil(BLOCK));
    if KEPT.any(first, last) {
        return Verdict::Kept;
    }
    if !table_gpu_only(start, end - start) {
        return Verdict::NotGpuOnly;
    }
    if !crate::inplace::shown_twice(first, last) {
        return Verdict::Under;
    }
    for b in first..last {
        if !crate::inplace::shown_twice(b, b + 1) {
            continue;
        }
        for (other, real) in crate::inplace::other_views(b * BLOCK) {
            if !real || ours_at(other) || linear_at(other) {
                continue;
            }
            match cpu_view(other) {
                0 => {}
                1 => return Verdict::Twice(0, other),
                2 => return Verdict::Twice(1, other),
                _ => return Verdict::Twice(2, other),
            }
        }
    }
    Verdict::Under
}

/// Since the start (=1): asks answered by the rule (count, bytes, of them guarded, ns of the rule itself),
/// blocks entering (noted whole once), blocks returned to tracking (noted whole once), known events noted by
/// source (ours: dstorage / kernel / shader writes; mapping changes; new levels; safety net), texture
/// refreshes answered unchanged (count, bytes, of them hash turns the guard path would have read), compares
/// of a due texture with what its copy holds answered without reading (count, bytes).
static UNTRACKED_N: [AtomicU64; 15] = [const { AtomicU64::new(0) }; 15];

/// [`exempt`], counted, from `watch::writes`: the blocks of the range not untracked yet enter (noted in the
/// journal whole: the copy takes them once more, from the asking that follows).
pub(crate) fn exempt_asked(start: usize, end: usize, guarded: bool) -> bool {
    let t = std::time::Instant::now();
    if !exempt(start, end) {
        return false;
    }
    start_net();
    let (first, last) = (start / BLOCK, end.div_ceil(BLOCK));
    let entering: Vec<usize> = (first..last).filter(|&b| !UNTRACKED.any(b, b + 1)).collect();
    if !entering.is_empty() {
        for &b in &entering {
            UNTRACKED.set(b);
        }
        crate::watch::note_known(&entering);
        UNTRACKED_N[4].fetch_add(entering.len() as u64, Relaxed);
        linear_watch(&entering);
    }
    UNTRACKED_N[0].fetch_add(1, Relaxed);
    UNTRACKED_N[1].fetch_add((end - start) as u64, Relaxed);
    if guarded {
        UNTRACKED_N[2].fetch_add(1, Relaxed);
    }
    UNTRACKED_N[3].fetch_add(t.elapsed().as_nanos() as u64, Relaxed);
    true
}

/// `watch::writes` goes today's way for `[first, last)` (blocks): those of its blocks that were untracked and
/// are no longer GPU-only (or shown elsewhere now, or given back) return, noted whole once.
pub(crate) fn returned(first: usize, last: usize) {
    if !UNTRACKED.any(first, last) {
        return;
    }
    let back: Vec<usize> =
        UNTRACKED.set_in(first, last).into_iter().filter(|&b| !exempt(b * BLOCK, (b + 1) * BLOCK)).collect();
    if back.is_empty() {
        return;
    }
    for &b in &back {
        UNTRACKED.clear(b);
    }
    crate::watch::note_known(&back);
    UNTRACKED_N[5].fetch_add(back.len() as u64, Relaxed);
}

/// What wrote, or changed what an address shows.
#[derive(Clone, Copy)]
pub(crate) enum Known {
    /// Our dstorage write, a kernel read for the title, a shader write landing.
    Ours = 0,
    Mapped = 1,
    /// The title's level allocator handed the address out (F25): a new texture there.
    Allocated = 2,
    /// The safety net found the copy stale.
    Stale = 3,
}

/// A known event on `[at, at + len)`: its untracked blocks are noted written in the journal.
pub(crate) fn known(kind: Known, at: usize, len: usize) {
    event(10 + kind as u8, at, len);
    if gpu_only_mode() != 1 || len == 0 {
        return;
    }
    let (first, last) = (at / BLOCK, at.saturating_add(len).div_ceil(BLOCK));
    let mut blocks: Vec<usize> = UNTRACKED.set_in(first, last);
    if matches!(kind, Known::Mapped) {
        for &b in &blocks {
            REMAPPED.set(b);
        }
    }
    // An event at the linear view writes the same pages as their GPU-view addresses show.
    if near_linear(at, len) && linear_at(at) {
        for b in first..last {
            LINEAR_KNOWN.set(b);
            for (other, real) in crate::inplace::other_views(b * BLOCK) {
                if real && UNTRACKED.any(other / BLOCK, other / BLOCK + 1) {
                    blocks.push(other / BLOCK);
                }
            }
        }
        if blocks.len() > UNTRACKED.set_in(first, last).len() {
            LINEAR_N[3].fetch_add(1, Relaxed);
        }
    }
    if blocks.is_empty() {
        return;
    }
    crate::watch::note_known(&blocks);
    UNTRACKED_N[6 + kind as usize].fetch_add(blocks.len() as u64, Relaxed);
}

/// A texture refresh answered "unchanged" by the rule; `hot_until` as the guard path keeps it.
pub(crate) fn refresh_spared(start: usize, size: usize, hot_until: u64) {
    UNTRACKED_N[10].fetch_add(1, Relaxed);
    UNTRACKED_N[11].fetch_add(size as u64, Relaxed);
    let frame = crate::frame::current();
    if frame.wrapping_add((start >> 16) as u64).is_multiple_of(8) || frame < hot_until || hot_until == 0 {
        UNTRACKED_N[12].fetch_add(1, Relaxed);
    }
}

/// `crate::texture::unchanged` for `[at, at + len)`, whose copy `serial` holds bytes read at the asking noted
/// by [`held`]: `Some(true)` if the range is under the rule and nothing known wrote it since; `None`: compare.
pub(crate) fn unchanged_by_rule(at: usize, len: usize, serial: u64) -> Option<bool> {
    if gpu_only_mode() != 1 || !exempt(at, at + len) {
        return None;
    }
    let read = HELD_AT.lock().unwrap_or_else(|e| e.into_inner()).as_ref()?.get(&serial).copied()?;
    let (first, last) = (at / BLOCK, (at + len).div_ceil(BLOCK));
    // Blocks not untracked yet enter (noted whole once): compared this time.
    let entering: Vec<usize> = (first..last).filter(|&b| !UNTRACKED.any(b, b + 1)).collect();
    if !entering.is_empty() {
        for &b in &entering {
            UNTRACKED.set(b);
        }
        crate::watch::note_known(&entering);
        UNTRACKED_N[4].fetch_add(entering.len() as u64, Relaxed);
        linear_watch(&entering);
        start_net();
        return None;
    }
    if crate::watch::last_noted(first, last) > read {
        return None;
    }
    UNTRACKED_N[13].fetch_add(1, Relaxed);
    UNTRACKED_N[14].fetch_add(len as u64, Relaxed);
    Some(true)
}

// ---- Lifetime (F25): a copy all on GPU-only pages lives until the title frees its level ----

/// Levels the title freed whose pages were GPU-only, for the next sweep (`crate::resource::let_go_idle`).
static FREED_LEVELS: std::sync::Mutex<Vec<(u64, u64, u64)>> = std::sync::Mutex::new(Vec::new());

/// Since the start: GPU-only level frees noted, frees that released a copy, copies released and their bytes,
/// copies still held at a sweep (looked at again), copies kept as used again after the free (the address
/// handed out again to a level of the same description),
/// held from the idle sweep or for the room rule's last pass (`VICTORY_GPU_ONLY_HOLD=1`; once per copy until
/// it is used again; count, bytes), copies under the rule the room rule let go (HOLD=1: in its last pass;
/// count, bytes), held copies used again.
/// (K2) [11] copies marked dead at a free (not released: waiting for a new level at the address), [12] bytes
/// of the copies re-filled in place ([5]), released dead on the timeout [13] count [14] bytes, released dead under
/// room pressure (before any live copy) [15] count [16] bytes.
pub(crate) static LIFE: [AtomicU64; 17] = [const { AtomicU64::new(0) }; 17];

/// The title's free 0x1403716a0 of a level at `[at, at + len)` (under the title's lock: short).
pub fn level_freed(at: usize, len: usize) {
    if gpu_only_mode() != 1 || len == 0 || !table_gpu_only(at, len) {
        return;
    }
    LIFE[0].fetch_add(1, Relaxed);
    if lifetime_mode() == 0 {
        return;
    }
    let mut f = FREED_LEVELS.lock().unwrap_or_else(|e| e.into_inner());
    if f.len() < 1 << 20 {
        f.push((at as u64, len as u64, crate::host::moment()));
    } else {
        // No sweep took them (no submissions): the lifetime rule stands down for good, the room rule decides
        // for every copy as today. Counted.
        LIFETIME_OFF.store(true, Relaxed);
        FALLBACK[0].fetch_add(1, Relaxed);
    }
}

/// The lifetime rule stood down (a queue overflowed): the room rule decides for all copies as today.
static LIFETIME_OFF: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Since the start, fallbacks to today's way: level frees not queued (queue full: lifetime rule off for
/// good), frees whose copy stayed held for 60 s (that copy is left to the room rule), safety-net hits
/// (blocks tracked for good), net look-again notes dropped at a wrap.
pub(crate) static FALLBACK: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];

/// Whether the lifetime half stands (`VICTORY_GPU_ONLY_LIFETIME` not 0, no queue overflowed).
pub(crate) fn lifetime_on() -> bool {
    gpu_only_mode() == 1 && lifetime_mode() != 0 && !LIFETIME_OFF.load(Relaxed)
}

/// (coder K2, decision after run-154) `VICTORY_GPU_ONLY_LIFETIME`: the copy-lifetime half of `VICTORY_GPU_ONLY=1`.
/// `0` (default): copies live exactly as without the rule (room rule and idle sweep as today; the title's free
/// only counted); `free`: copies released at the title's free (f7c9cac's way); `dead`: marked dead at the free,
/// re-filled in place by a new level of the same description, released after `VICTORY_GPU_ONLY_DEAD_S` or first
/// under room pressure (4f548e5). run-154 (dead, 180 s): let go for room 19.6 GB / made again 3.9 GB against
/// 9.5-10.5 / 0.8-1.1 GB without the rule. `VICTORY_GPU_ONLY_HOLD=1` needs a lifetime mode.
pub(crate) fn lifetime_mode() -> u8 {
    static MODE: OnceLock<u8> = OnceLock::new();
    *MODE.get_or_init(|| match std::env::var("VICTORY_GPU_ONLY_LIFETIME").as_deref().map(str::trim) {
        Ok("free") => 1,
        Ok("dead") => 2,
        _ => 0,
    })
}

/// The title's alloc 0x14034bc70 returned `[at, at + len)`: a new level there, never "unchanged": the copies
/// over it take its pages again (its bytes arrive with our dstorage write, noted again then).
pub fn level_allocated(at: usize, len: usize) {
    if gpu_only_mode() != 1 || len == 0 {
        return;
    }
    known(Known::Allocated, at, len);
}

/// Copies released at a free, by title address: when (ms), what the copy held (digest of the title bytes).
static RELEASED_AT: std::sync::Mutex<Option<std::collections::HashMap<u64, (u64, Option<crate::texture::Digest>)>>> =
    std::sync::Mutex::new(None);
/// Since the start: copies released at a free and uploaded again at the same address within 5 s (count, MB),
/// of them with the same bytes as before (count, MB).
static REMADE: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];

/// The sweep released the copy of the texture at `at` at the title's free.
pub(crate) fn released_at_free(at: u64, held: Option<crate::texture::Digest>) {
    let mut r = RELEASED_AT.lock().unwrap_or_else(|e| e.into_inner());
    let r = r.get_or_insert_with(Default::default);
    if r.len() > 1 << 18 {
        let now = ms();
        r.retain(|_, v| now.saturating_sub(v.0) < 5_000);
    }
    r.insert(at, (ms(), held));
}

/// A host copy of the texture at `at` was filled and uploaded with bytes of digest `hash` (`bytes` of title
/// memory).
pub(crate) fn uploaded_at(at: u64, bytes: u64, hash: crate::texture::Digest) {
    if gpu_only_mode() != 1 {
        return;
    }
    let was = RELEASED_AT.lock().unwrap_or_else(|e| e.into_inner()).as_mut().and_then(|r| r.remove(&at));
    if let Some((when, held)) = was
        && ms().saturating_sub(when) < 5_000
    {
        REMADE[0].fetch_add(1, Relaxed);
        REMADE[1].fetch_add(bytes, Relaxed);
        if held == Some(hash) {
            REMADE[2].fetch_add(1, Relaxed);
            REMADE[3].fetch_add(bytes, Relaxed);
        }
    }
}

/// For the sweep: the frees since the last one (address, bytes, `crate::host::moment` of the free).
pub(crate) fn take_freed() -> Vec<(u64, u64, u64)> {
    std::mem::take(&mut *FREED_LEVELS.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Frees whose copy was still held at a sweep, for the next.
pub(crate) fn keep_freed(again: Vec<(u64, u64, u64)>) {
    if again.is_empty() {
        return;
    }
    if LIFETIME_OFF.load(Relaxed) {
        return;
    }
    FREED_LEVELS.lock().unwrap_or_else(|e| e.into_inner()).extend(again);
}

// ---- Safety net (diagnostics): untracked texture copies checked against what was uploaded ----

/// Per host copy serial: the journal's asking count at or before which its held bytes were read.
static HELD_AT: std::sync::Mutex<Option<std::collections::HashMap<u64, u64>>> = std::sync::Mutex::new(None);

/// The host copy `serial` now holds bytes read at or after asking `at`.
pub(crate) fn held(serial: u64, at: u64) {
    if gpu_only_mode() != 1 {
        return;
    }
    HELD_AT.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Default::default).insert(serial, at);
}

/// A host copy went: its note goes.
pub(crate) fn copy_gone(serial: u64) {
    if gpu_only_mode() != 1 {
        return;
    }
    if let Some(h) = HELD_AT.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        h.remove(&serial);
    }
}

/// Since the start: bytes compared, copies compared, copies skipped (a known write since the read, not
/// uploaded yet), mismatches seen once (looked at again), stale copies found (must be 0).
static NET: [AtomicU64; 5] = [const { AtomicU64::new(0) }; 5];
/// (coder K2) Mismatches seen once, as the next look settled them: a known write had been noted since the bytes
/// were read (a write in flight when the net read: our dstorage write, a landing), the bytes were what was
/// uploaded (a torn read), the copy had been uploaded anew (its held digest changed).
static NET_AGAIN: [AtomicU64; 3] = [const { AtomicU64::new(0) }; 3];

/// Starts the net's thread (once, =1, diagnostics builds).
pub(crate) fn start_net() {
    static ONCE: OnceLock<()> = OnceLock::new();
    if gpu_only_mode() != 1 || !cfg!(feature = "diagnostics") {
        return;
    }
    ONCE.get_or_init(|| {
        let _ = std::thread::Builder::new().name("victory-gpuonly-net".into()).spawn(|| {
            #[cfg(windows)]
            {
                let mut from = 0u64;
                let mut seen: std::collections::HashMap<u64, crate::texture::Digest> = Default::default();
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    from = net_round(from, &mut seen);
                    linear_round();
                    // A wrap: notes of copies gone since are dropped (bounded over hours).
                    if from == 0 && seen.len() > 4096 {
                        FALLBACK[3].fetch_add(seen.len() as u64, Relaxed);
                        seen.clear();
                    }
                }
            }
        });
    });
}

/// One second's look: up to 16 MB of copies under the rule, from address `from` on; returns where the
/// next look starts (0: from the lowest again).
#[cfg(windows)]
fn net_round(from: u64, seen: &mut std::collections::HashMap<u64, crate::texture::Digest>) -> u64 {
    const MOST: u64 = 16 << 20;
    let copies = crate::resource::rule_copies(from, MOST);
    let mut next = 0;
    let mut buf: Vec<u8> = Vec::new();
    for (start, len, serial, hold, resource) in copies {
        next = start + 1;
        let Some(held) = hold else {
            continue;
        };
        let Some(at) = HELD_AT.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|h| h.get(&serial).copied())
        else {
            continue;
        };
        let (s, e) = (start as usize, (start + len) as usize);
        if len == 0 || !exempt(s, e) {
            continue;
        }
        let (first, last) = (s / BLOCK, e.div_ceil(BLOCK));
        let pending = || crate::watch::last_noted(first, last) > at;
        if pending() {
            NET[2].fetch_add(1, Relaxed);
            if seen.remove(&serial).is_some() {
                NET_AGAIN[0].fetch_add(1, Relaxed);
            }
            continue;
        }
        buf.resize(len as usize, 0);
        crate::host::read_there(s as *const u8, &mut buf);
        let now = crate::texture::digest(&buf);
        NET[0].fetch_add(len, Relaxed);
        NET[1].fetch_add(1, Relaxed);
        // A write may have landed before its note: the next look decides.
        if now == held || pending() {
            if seen.remove(&serial).is_some() {
                NET_AGAIN[usize::from(now == held)].fetch_add(1, Relaxed);
            }
            continue;
        }
        if seen.get(&serial) != Some(&held) {
            if seen.insert(serial, held).is_some() {
                NET_AGAIN[2].fetch_add(1, Relaxed);
            }
            NET[3].fetch_add(1, Relaxed);
            continue;
        }
        seen.remove(&serial);
        stale_case("safety net: a stale copy", start as usize, len as usize, resource as usize);
        let n = NET[4].fetch_add(1, Relaxed);
        if n < 32 {
            tracing::warn!(
                at = format_args!("{start:#x}"),
                bytes = len,
                texture = format_args!("{resource:#x}"),
                serial,
                "VICTORY_GPU_ONLY=1 safety net: a stale GPU-only copy found (title memory differs from what was uploaded, no known write since): uploaded again, its pages tracked for good"
            );
        }
        known(Known::Stale, s, e - s);
        FALLBACK[2].fetch_add(1, Relaxed);
        for b in first..last {
            KEPT.set(b);
            UNTRACKED.clear(b);
        }
        crate::descriptor::host::remapped(s, e - s);
    }
    next
}

/// The `VICTORY_GPU_ONLY=1` part of the 5 s line.
fn untracked_line(frames: f64) -> String {
    static LAST: [AtomicU64; 15] = [const { AtomicU64::new(0) }; 15];
    let v: Vec<u64> = UNTRACKED_N
        .iter()
        .zip(LAST.iter())
        .map(|(c, l)| {
            let now = c.load(Relaxed);
            now - l.swap(now, Relaxed)
        })
        .collect();
    let l: Vec<u64> = LIFE.iter().map(|c| c.load(Relaxed)).collect();
    let n: Vec<u64> = NET.iter().map(|c| c.load(Relaxed)).collect();
    let f = frames.max(1.0);
    let mb = |b: u64| b as f64 / 1048576.0;
    #[cfg(windows)]
    let (rule_n, rule_b, refused) = crate::resource::rule_census();
    #[cfg(not(windows))]
    let (rule_n, rule_b, refused) = (0u64, 0u64, [0u64; 4]);
    format!(
        "; VICTORY_GPU_ONLY=1 (per frame over the line): asks answered by the rule (no write-watch ask, guard or compare) {:.1}/frame ({:.1} MB/frame; guarded asks = guard arms spared {:.1}/frame), the rule's own cost {:.3} ms/frame; \
         texture refreshes answered unchanged {:.1}/frame ({:.1} MB/frame; hash reads spared {:.1}/frame), due-texture compares answered without reading {:.1}/frame ({:.1} MB/frame); blocks entering {} / returned to tracking {} (each taken whole once); \
         known events noted (blocks): ours (dstorage, kernel, shader landing) {}, mapping changes {}, new levels (alloc) {}, safety net {}; \
         copies under the rule {rule_n} ({:.0} MB), copies all on GPU-only pages refused: given back {}, also shown at a CPU-mapped CPU-view address {}, at another GPU-view address {}, outside the region {}; since the start: GPU-only level frees {}, frees that released a copy {}, copies marked dead at the free {}, re-filled in place by a new level of the same description (no destroy, no create) {} ({:.0} MB), dead released on the timeout (VICTORY_GPU_ONLY_DEAD_S) {} ({:.0} MB) / under room pressure, before any live copy {} ({:.0} MB), copies released in all {} ({:.0} MB), frees looked at again at a sweep (dead copies waiting, or held) {}, \
         held from the idle sweep or for the room rule's last pass (VICTORY_GPU_ONLY_HOLD=1; copies) {} ({:.0} MB), let go by the room rule (HOLD=1: its last pass) {} ({:.0} MB), held and used again {}; \
         released at a free and uploaded again at the same address within 5 s {} ({:.0} MB), of them with the same bytes {} ({:.0} MB); \
         fallbacks to today's way since the start: level frees not queued (lifetime rule off) {}, freed copies held 60 s (left to the room rule) {}, safety-net hits {}, net notes dropped {}; \
         linear view of the pool: blocks watched {}, unexplained touches (must be 0; the page leaves the rule for good) {}, blocks whose section has no linear view (nothing to watch; looked for again each second) {}, blocks with no record of their section (left the rule) {}, \
         known writes there translated to the GPU view {}; touches (each noted written: the copy takes the page again) explained by our own reads {} / by a known write there {} / by a mapping change of the page {} / at a linear address the page no longer has (watched at its new one) {}; \
         safety net: compared {:.0} MB in {} copies, skipped (known write not uploaded yet) {}, mismatches seen once {} (on the next look: a known write had been noted {}, the bytes were what was uploaded {}, the copy was uploaded anew {}), stale GPU-only copies found {} (must be 0)",
        v[0] as f64 / f,
        mb(v[1]) / f,
        v[2] as f64 / f,
        v[3] as f64 / 1e6 / f,
        v[10] as f64 / f,
        mb(v[11]) / f,
        v[12] as f64 / f,
        v[13] as f64 / f,
        mb(v[14]) / f,
        v[4],
        v[5],
        v[6],
        v[7],
        v[8],
        v[9],
        mb(rule_b),
        refused[0],
        refused[1],
        refused[2],
        refused[3],
        l[0],
        l[1],
        l[11],
        l[5],
        mb(l[12]),
        l[13],
        mb(l[14]),
        l[15],
        mb(l[16]),
        l[2],
        mb(l[3]),
        l[4],
        l[6],
        mb(l[7]),
        l[8],
        mb(l[9]),
        l[10],
        REMADE[0].load(Relaxed),
        mb(REMADE[1].load(Relaxed)),
        REMADE[2].load(Relaxed),
        mb(REMADE[3].load(Relaxed)),
        FALLBACK[0].load(Relaxed),
        FALLBACK[1].load(Relaxed),
        FALLBACK[2].load(Relaxed),
        FALLBACK[3].load(Relaxed),
        LINEAR_N[0].load(Relaxed),
        LINEAR_N[1].load(Relaxed),
        LINEAR_N[2].load(Relaxed),
        LINEAR_N[6].load(Relaxed),
        LINEAR_N[3].load(Relaxed),
        LINEAR_N[4].load(Relaxed),
        LINEAR_N[5].load(Relaxed),
        LINEAR_N[7].load(Relaxed),
        LINEAR_N[8].load(Relaxed),
        mb(n[0]),
        n[1],
        n[2],
        n[3],
        NET_AGAIN[0].load(Relaxed),
        NET_AGAIN[1].load(Relaxed),
        NET_AGAIN[2].load(Relaxed),
        n[4],
    )
}

// ---- (coder K, step 5) `VICTORY_PAGE_KINDS=1`: which "both" (kind 0) pages the title's CPU side touches ----

/// (Coder W) The streaming regions by the title's own variables: (CPU view from, to), (GPU view
/// from, to); `None` until the runtime has told them.
pub(crate) fn streaming() -> Option<((usize, usize), (usize, usize))> {
    let (cpu, bytes, gpu, _) = region();
    (bytes != 0).then_some(((cpu, cpu + bytes), (gpu, gpu + bytes)))
}

// ---- (Coder W) GPU_ONLY stale-case meter: the last events on each 64 KiB block of the streaming
// regions (ours: 0-5 as `Ours`; known: 10 + `Known`), and for each block a stale case names: what
// wrote it last, how long ago. No behaviour change. ----

const EVENT_NAMES: [&str; 14] = [
    "buffer copy at submission (CPU copy step)",
    "mapping change",
    "shader writes landing",
    "our copy / dstorage",
    "kernel read for the title",
    "dstorage pages",
    "?",
    "?",
    "?",
    "?",
    "known: ours (dstorage, kernel read, landing)",
    "known: mapping change",
    "known: level allocated (F25)",
    "known: stale (the net)",
];

static EVENTS: std::sync::Mutex<Option<std::collections::HashMap<usize, [(u8, u64); 4]>>> = std::sync::Mutex::new(None);

fn ms_now() -> u64 {
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64 + 1
}

fn event(kind: u8, at: usize, len: usize) {
    if gpu_only_mode() == 0 || len == 0 {
        return;
    }
    let Some(((c0, c1), (g0, g1))) = streaming() else { return };
    let end = at.saturating_add(len);
    if !(at < c1 && c0 < end || at < g1 && g0 < end) {
        return;
    }
    let now = ms_now();
    let mut ev = EVENTS.lock().unwrap_or_else(|e| e.into_inner());
    let ev = ev.get_or_insert_with(Default::default);
    if ev.len() > 1 << 20 {
        ev.clear();
    }
    for b in at / BLOCK..end.div_ceil(BLOCK).min(at / BLOCK + 4096) {
        let e = ev.entry(b).or_insert([(0, 0); 4]);
        e.rotate_right(1);
        e[0] = (kind, now);
    }
}

fn stale_case(what: &str, at: usize, len: usize, texture: usize) {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    if N.fetch_add(1, Relaxed) >= 50 {
        return;
    }
    let now = ms_now();
    let ev = EVENTS.lock().unwrap_or_else(|e| e.into_inner());
    let mut said = Vec::new();
    for b in at / BLOCK..at.saturating_add(len).div_ceil(BLOCK).min(at / BLOCK + 8) {
        let list: Vec<String> = ev
            .as_ref()
            .and_then(|m| m.get(&b))
            .map(|e| {
                e.iter()
                    .filter(|x| x.1 != 0)
                    .map(|&(k, t)| format!("{} {} ms ago", EVENT_NAMES.get(k as usize).copied().unwrap_or("?"), now - t))
                    .collect()
            })
            .unwrap_or_default();
        said.push(format!(
            "block {:#x}: {}",
            b * BLOCK,
            if list.is_empty() { "no event of ours or known since the start".to_string() } else { list.join(", ") }
        ));
    }
    tracing::warn!(
        at = format_args!("{at:#x}"),
        bytes = len,
        texture = format_args!("{texture:#x}"),
        "GPU-only stale case ({what}): the last events on its blocks: {}",
        said.join(" | ")
    );
}

/// The runtime's region: CPU-view base, bytes, GPU-view base, the title's page table (0s until set).
pub type Region = fn() -> (usize, usize, usize, usize);
static REGION: AtomicUsize = AtomicUsize::new(0);

pub fn set_region(f: Region) {
    REGION.store(f as usize, Relaxed);
}

fn region() -> (usize, usize, usize, usize) {
    match REGION.load(Relaxed) {
        0 => (0, 0, 0, 0),
        f => {
            // SAFETY: set from a `Region`.
            let q: Region = unsafe { std::mem::transmute::<usize, Region>(f) };
            q()
        }
    }
}

/// Seconds after the meter's start at which the sampling begins (`VICTORY_PAGE_KINDS_TOUCH_AT`, default 120:
/// after the load; 0: off).
fn touch_at() -> u64 {
    static AT: OnceLock<u64> = OnceLock::new();
    *AT.get_or_init(|| {
        std::env::var("VICTORY_PAGE_KINDS_TOUCH_AT").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(120)
    })
}

/// Per region page (index) of kind 0 being watched: touched since it was taken out of the working set.
static TOUCHED: std::sync::Mutex<Option<std::collections::HashMap<usize, bool>>> = std::sync::Mutex::new(None);
/// Since the start: pages taken out of the working set, pages that left kind 0 while watched.
static TOUCH_N: [AtomicU64; 2] = [const { AtomicU64::new(0) }; 2];

/// One look, from the 5 s line: kind-0 pages new to the look are taken out of the process's working set at
/// their CPU-view address (`VirtualUnlock` of an unlocked range: no protection change, no exception; a
/// later access by any thread brings the page back by a soft fault the kernel takes alone), and the
/// working set is read (`QueryWorkingSetEx`: valid = touched since). The title's GPU view, our own section
/// views and our DirectStorage writes are other addresses and do not count; reads of ours through the
/// CPU-view address do (the "texture copy" / "buffer copy" classes may hold them).
#[cfg(windows)]
pub fn touch_line(since_start: u64) -> String {
    use windows_sys::Win32::System::Memory::VirtualUnlock;
    use windows_sys::Win32::System::ProcessStatus::{K32QueryWorkingSetEx, PSAPI_WORKING_SET_EX_INFORMATION};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let at = touch_at();
    if at == 0 {
        return String::new();
    }
    if since_start < at {
        return format!("; kind-0 touch sampling starts at {at} s");
    }
    let (cpu, bytes, gpu, table) = region();
    if cpu == 0 || table == 0 || bytes == 0 {
        return String::new();
    }
    let pages = bytes / BLOCK;
    let mut t = TOUCHED.lock().unwrap_or_else(|e| e.into_inner());
    let t = t.get_or_insert_with(Default::default);
    for i in 0..pages {
        // SAFETY: the title's table holds a u64 per page of the region.
        let e = unsafe { ((table + i * 8) as *const u64).read_volatile() };
        let both = e >> 62 & 1 == 1 && e >> 63 == 1;
        if both && !t.contains_key(&i) {
            // SAFETY: an address range; the call fails harmlessly if nothing is there.
            unsafe { VirtualUnlock((cpu + i * BLOCK) as *const _, BLOCK) };
            t.insert(i, false);
            TOUCH_N[0].fetch_add(1, Relaxed);
        } else if !both && t.remove(&i).is_some() {
            TOUCH_N[1].fetch_add(1, Relaxed);
        }
    }
    // The ones not touched yet, 16 entries of 4 KiB each.
    let open: Vec<usize> = t.iter().filter(|e| !*e.1).map(|e| *e.0).collect();
    for chunk in open.chunks(8192) {
        let mut info: Vec<PSAPI_WORKING_SET_EX_INFORMATION> = chunk
            .iter()
            .flat_map(|&i| (0..16).map(move |k| PSAPI_WORKING_SET_EX_INFORMATION {
                VirtualAddress: (cpu + i * BLOCK + k * 4096) as *mut _,
                ..Default::default()
            }))
            .collect();
        // SAFETY: an array of entries of the stated size.
        let ok = unsafe {
            K32QueryWorkingSetEx(
                GetCurrentProcess(),
                info.as_mut_ptr().cast(),
                (info.len() * size_of::<PSAPI_WORKING_SET_EX_INFORMATION>()) as u32,
            )
        } != 0;
        if !ok {
            continue;
        }
        for (n, &i) in chunk.iter().enumerate() {
            // SAFETY: plain flags the call filled in.
            if info[n * 16..n * 16 + 16].iter().any(|w| unsafe { w.VirtualAttributes.Flags } & 1 != 0) {
                t.insert(i, true);
            }
        }
    }
    // By what the page holds (either view of it): [texture copy, buffer copy, neither] x [touched, never].
    let mut by = [[0u64; 2]; 3];
    for (&i, &touched) in t.iter() {
        let (c, g) = (cpu + i * BLOCK, gpu + i * BLOCK);
        let class = if crate::resource::texture_copy_over(c as u64) || (gpu != 0 && crate::resource::texture_copy_over(g as u64)) {
            0
        } else if crate::shadow::copy_over(c as u64) || (gpu != 0 && crate::shadow::copy_over(g as u64)) {
            1
        } else {
            2
        };
        by[class][usize::from(!touched)] += BLOCK as u64;
    }
    let mb = |b: u64| b as f64 / 1048576.0;
    format!(
        "; kind-0 (both) pages, the title's CPU-view address touched since taken out of the working set (sampling since {at} s; {} pages taken out, {} left kind 0): \
         holding a texture copy touched {:.0} MB / never {:.0} MB, a buffer copy touched {:.0} / never {:.0} MB, neither touched {:.0} / never {:.0} MB",
        TOUCH_N[0].load(Relaxed),
        TOUCH_N[1].load(Relaxed),
        mb(by[0][0]),
        mb(by[0][1]),
        mb(by[1][0]),
        mb(by[1][1]),
        mb(by[2][0]),
        mb(by[2][1]),
    )
}

// ---- (coder K) The check of the linear view: does the title touch pages under the rule through it? ----

/// Blocks newly under the rule, for the net's thread to watch at their linear-view address.
static LINEAR_PENDING: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());
/// Since the start: [0] blocks watched at their linear-view address, [1] unexplained touches (title CPU or the
/// kernel for it; must be 0: the block leaves the rule for good), [2] blocks whose section has no linear view
/// (nothing of the title's to watch there; looked for again each second), [3] known events at the linear view
/// translated to the pages' GPU-view addresses; touches, each noted written and the page watched again,
/// explained by [4] our own reads there, [5] a known write there, [7] a mapping change of the page (the
/// title's mapping executor works through the linear view), [8] a linear address the page no longer has (it
/// was mapped to another pool page since: watched at its new one); [6] blocks with no record of their section
/// (cannot be watched: left the rule for good).
static LINEAR_N: [AtomicU64; 9] = [const { AtomicU64::new(0) }; 9];

/// (coder K2) The explanation marks one round older: a mark explains a touch found in the round after it was
/// set or the one after that, never a touch minutes later.
static OUR_LINEAR_READS_OLD: crate::watch::Bits = crate::watch::Bits::new();
static LINEAR_KNOWN_OLD: crate::watch::Bits = crate::watch::Bits::new();
static REMAPPED_OLD: crate::watch::Bits = crate::watch::Bits::new();

fn linear_watch(blocks: &[usize]) {
    if !cfg!(feature = "diagnostics") {
        return;
    }
    let mut p = LINEAR_PENDING.lock().unwrap_or_else(|e| e.into_inner());
    if p.len() < 1 << 20 {
        p.extend_from_slice(blocks);
    } else {
        // Not watched: they leave the rule for good (today's tracking), counted.
        drop(p);
        for &b in blocks {
            leave_for_good(b);
        }
        LINEAR_N[6].fetch_add(blocks.len() as u64, Relaxed);
    }
}

/// A block that cannot be checked, or was touched with nothing to explain it: today's tracking for good, its
/// copy takes it again.
fn leave_for_good(b: usize) {
    known(Known::Stale, b * BLOCK, BLOCK);
    KEPT.set(b);
    UNTRACKED.clear(b);
    crate::descriptor::host::remapped(b * BLOCK, BLOCK);
}

/// One second's look (net thread): blocks new to the watch have their linear-view page taken out of the
/// working set (`VirtualUnlock` of an unlocked range: no protection change, no exception); a watched page
/// back in the working set was touched through the linear view since. The touch is noted written (the copy
/// takes the page again) whatever explains it; explained (our own read there, a known write there, a mapping
/// change of the page in this round or the one before) the page stays under the rule and is watched again;
/// at a linear address the page no longer has, it is watched at its new one; unexplained, the block leaves
/// the rule for good (logged, counted).
#[cfg(windows)]
fn linear_round() {
    use windows_sys::Win32::System::Memory::VirtualUnlock;
    use windows_sys::Win32::System::ProcessStatus::{K32QueryWorkingSetEx, PSAPI_WORKING_SET_EX_INFORMATION};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    type Watched = std::collections::HashMap<usize, usize>;
    static WATCHED: std::sync::Mutex<Option<(Watched, std::collections::HashSet<usize>)>> = std::sync::Mutex::new(None);
    let pending = std::mem::take(&mut *LINEAR_PENDING.lock().unwrap_or_else(|e| e.into_inner()));
    let mut guard = WATCHED.lock().unwrap_or_else(|e| e.into_inner());
    let (w, none) = guard.get_or_insert_with(Default::default);
    // Blocks no longer under the rule are let go (bounded by what is under it now).
    w.retain(|&b, _| UNTRACKED.any(b, b + 1));
    none.retain(|&b| UNTRACKED.any(b, b + 1));
    let arm = |w: &mut Watched, b: usize, l: usize| {
        // SAFETY: an address range of the title's view; the call fails harmlessly if nothing is there.
        unsafe { VirtualUnlock(l as *const _, BLOCK) };
        for bits in [&OUR_LINEAR_READS, &OUR_LINEAR_READS_OLD, &LINEAR_KNOWN, &LINEAR_KNOWN_OLD] {
            bits.clear(l / BLOCK);
        }
        w.insert(b, l);
    };
    // First look at the ones watched before (a page taken out now is looked at next second).
    let blocks: Vec<(usize, usize)> = w.iter().map(|(&b, &l)| (b, l)).collect();
    let mut touched = std::collections::HashSet::new();
    for chunk in blocks.chunks(8192) {
        let mut info: Vec<PSAPI_WORKING_SET_EX_INFORMATION> = chunk
            .iter()
            .flat_map(|&(_, l)| {
                (0..16).map(move |k| PSAPI_WORKING_SET_EX_INFORMATION { VirtualAddress: (l + k * 4096) as *mut _, ..Default::default() })
            })
            .collect();
        // SAFETY: an array of entries of the stated size.
        let ok = unsafe {
            K32QueryWorkingSetEx(
                GetCurrentProcess(),
                info.as_mut_ptr().cast(),
                (info.len() * size_of::<PSAPI_WORKING_SET_EX_INFORMATION>()) as u32,
            )
        } != 0;
        if !ok {
            continue;
        }
        for (n, &(b, l)) in chunk.iter().enumerate() {
            // SAFETY: plain flags the call filled in.
            if !info[n * 16..n * 16 + 16].iter().any(|e| unsafe { e.VirtualAttributes.Flags } & 1 != 0) {
                continue;
            }
            touched.insert(b);
            // Mapped to another pool page since it was armed: the touch is of another page.
            let now = linear_of(b * BLOCK);
            if now != Some(l) {
                LINEAR_N[8].fetch_add(1, Relaxed);
                w.remove(&b);
                match now {
                    Some(n) => arm(w, b, n),
                    None => {
                        none.insert(b);
                    }
                }
                continue;
            }
            let lb = l / BLOCK;
            let ours = OUR_LINEAR_READS.take(lb) | OUR_LINEAR_READS_OLD.take(lb);
            let kn = LINEAR_KNOWN.take(lb) | LINEAR_KNOWN_OLD.take(lb);
            let remapped = REMAPPED.take(b) | REMAPPED_OLD.take(b);
            // Whatever explains it, what it may have written is taken again.
            crate::watch::note_known(&[b]);
            if ours || kn || remapped {
                LINEAR_N[if ours {
                    4
                } else if kn {
                    5
                } else {
                    7
                }]
                .fetch_add(1, Relaxed);
                arm(w, b, l);
                continue;
            }
            w.remove(&b);
            let hit = LINEAR_N[1].fetch_add(1, Relaxed);
            if hit < 16 {
                tracing::warn!(
                    at = format_args!("{:#x}", b * BLOCK),
                    linear = format_args!("{l:#x}"),
                    table_gpu_only = table_gpu_only(b * BLOCK, BLOCK),
                    linear_is = %describe(l),
                    "VICTORY_GPU_ONLY=1: a page under the rule was touched through the pool's linear view with nothing to explain it (not our read, no known write, no mapping change, same pool page): it leaves the rule for good, its copy takes it again"
                );
            }
            leave_for_good(b);
        }
    }
    // Marks of this round age once; older ones go.
    for &(b, l) in &blocks {
        if touched.contains(&b) {
            continue;
        }
        let lb = l / BLOCK;
        for (new, old) in [(&OUR_LINEAR_READS, &OUR_LINEAR_READS_OLD), (&LINEAR_KNOWN, &LINEAR_KNOWN_OLD)] {
            old.clear(lb);
            if new.take(lb) {
                old.set(lb);
            }
        }
        REMAPPED_OLD.clear(b);
        if REMAPPED.take(b) {
            REMAPPED_OLD.set(b);
        }
    }
    // Blocks whose section had no linear view: a view made since is watched.
    let retry: Vec<usize> = none.iter().copied().collect();
    for b in retry {
        if let Some(l) = linear_of(b * BLOCK) {
            none.remove(&b);
            arm(w, b, l);
            LINEAR_N[0].fetch_add(1, Relaxed);
        }
    }
    for b in pending {
        if !UNTRACKED.any(b, b + 1) || w.contains_key(&b) || none.contains(&b) {
            continue;
        }
        match linear_of(b * BLOCK) {
            Some(l) => {
                arm(w, b, l);
                LINEAR_N[0].fetch_add(1, Relaxed);
            }
            // No record of the page's section: it cannot be checked, so it is not under the rule.
            None if crate::inplace::key_of(b * BLOCK).is_none() => {
                LINEAR_N[6].fetch_add(1, Relaxed);
                leave_for_good(b);
            }
            None => {
                none.insert(b);
                LINEAR_N[2].fetch_add(1, Relaxed);
            }
        }
    }
}

#[cfg(not(windows))]
fn linear_round() {}
