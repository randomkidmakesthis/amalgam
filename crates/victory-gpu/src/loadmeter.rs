//! (coder G) Load-read meter, `VICTORY_LOAD_READ_METER=1` (diagnostics builds; default off): how much of what
//! DirectStorage loads into title memory the title's CPU side ever touches afterwards.
//!
//! Every completed request is a *load*: its bytes, its options and every title address that shows them
//! (CPU view and GPU view of pool pages, or the plain destination). After the layer's write is done, the
//! 4 KiB pages of the load in the title's **CPU view** (pool pages: every view but the GPU view the title
//! builds with `CopyPageMappingsBatchX`; plain destinations: the destination itself) get `PAGE_GUARD` on top of
//! the protection they have. The first access to such a page raises a guard-page violation (the system drops
//! the bit before it tells us), which [`fault`] (called first in `watch`'s vectored handler) notes: read or
//! write, the code that did it (title code with its RVA, layer code, or system code on behalf of either: the
//! stack is walked to the first title or layer frame), and the time since the load; then the access goes on as
//! if nothing happened. One fault per page per load. Never protected: the GPU view (the layer reads textures
//! there; per the ledger F3 title CPU code never dereferences it), our own whole-section view (DirectStorage
//! writes there), and anything not committed readable.
//! Pages that kernel code is about to write for the title (`watch::kernel_writing`: file reads, raw
//! DirectStorage reads) are disarmed first and noted as rewritten. Pages unmapped while still armed are
//! "released untouched" (or "guard lost" when something set another protection over the guard meanwhile).
//! What a load is (texture, buffer the card uses, neither) comes from the views the title describes, looked up
//! at report time over all of the load's addresses, so a view written after the load still counts.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

const PAGE: usize = 4096;
const POOL_PAGE: usize = 64 << 10;
const STATUS_GUARD_PAGE_VIOLATION: i32 = 0x8000_0001_u32 as i32;
const PAGE_GUARD: u32 = 0x100;

/// Whether the meter runs.
pub fn on() -> bool {
    #[cfg(test)]
    if tests::FORCE.load(Relaxed) {
        return true;
    }
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        let on = cfg!(feature = "diagnostics")
            && std::env::var("VICTORY_LOAD_READ_METER").is_ok_and(|v| v.trim() == "1");
        if on {
            crate::watch::install();
            tracing::info!(
                "load-read meter on (VICTORY_LOAD_READ_METER=1): DirectStorage loads guarded in the title's CPU view (PAGE_GUARD), first access per 4 KiB page noted; a measuring run, not a timing run"
            );
        }
        on
    })
}

/// Cheap flag for the fault handler (set once something is armed).
static ARMED_ANY: AtomicBool = AtomicBool::new(false);

/// What happened first to an armed page.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub(crate) enum What {
    TitleRead = 0,
    TitleWrite = 1,
    /// First touched by layer code (an upload, a copy, the prefault thread).
    Layer = 2,
    /// Kernel write for the title or a new load into the same page (disarmed first).
    Rewritten = 3,
    /// Unmapped with the guard still in place.
    ReleasedUntouched = 4,
    /// Unmapped, no fault seen, guard gone (another protection was set over it).
    GuardLost = 5,
    /// Armed again by a later load before anything touched it.
    Replaced = 6,
    /// VirtualProtect refused or the page was not committed readable.
    NotArmed = 7,
}
const WHATS: usize = 8;
const WHAT_NAMES: [&str; WHATS] = [
    "read by title code",
    "written by title code",
    "touched first by layer code",
    "rewritten by a kernel write/new raw load",
    "released (unmapped) untouched",
    "guard lost (unknown)",
    "loaded again untouched",
    "not guardable",
];

/// What the title's views say a range is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ViewKind {
    Texture { format: u32, width: u32, height: u32, levels: u16 },
    Constant,
    Vertex,
    Index,
    /// A buffer view of placed memory.
    BufferView,
}

const KINDS: usize = 3;
const KIND_NAMES: [&str; KINDS] = ["texture", "card buffer", "neither (CPU-only?)"];

fn kind_class(k: Option<ViewKind>) -> usize {
    match k {
        Some(ViewKind::Texture { .. }) => 0,
        Some(_) => 1,
        None => 2,
    }
}

/// Request options folded: bit 0 zlib, bit 1 BCPack, bit 2 page destination.
pub(crate) fn options_class(options: u64) -> u8 {
    (options & 1 != 0) as u8 | ((options & (0xf << 1) != 0) as u8) << 1 | ((options & (1 << 21) != 0) as u8) << 2
}
const OPTION_NAMES: [&str; 8] =
    ["raw", "zlib", "bcpack", "zlib+bcpack", "raw->pages", "zlib->pages", "bcpack->pages", "zlib+bcpack->pages"];

struct Load {
    ms: u32,
    bytes: u64,
    options: u8,
    /// Every title address that shows the load (first few runs).
    shown: Vec<(usize, usize)>,
    kind: Option<ViewKind>,
    /// Bytes armed in the CPU view.
    armed: u64,
    by: [u32; WHATS],
}

#[derive(Default)]
struct State {
    loads: Vec<Load>,
    /// Armed 4 KiB pages: load, bytes of that load in the page.
    armed: HashMap<usize, (u32, u16)>,
    /// 64 KiB pages ever armed (a guard fault there that the table does not know any more is still ours).
    ever: HashSet<usize>,
    /// Title-code accesses: load, RVA, bytes, write.
    title: Vec<(u32, u32, u16, bool)>,
    /// ms from load to first title access: <10 ms, <100 ms, <1 s, <10 s, <60 s, more.
    histogram: [u64; 6],
    gpu_view: HashSet<usize>,
    lost_events: u64,
    faults: u64,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();
fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner())
}

static VIEWS: Mutex<BTreeMap<u64, (u64, ViewKind)>> = Mutex::new(BTreeMap::new());
const VIEWS_MOST: usize = 4 << 20;

fn start() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now)
}
fn now_ms() -> u32 {
    start().elapsed().as_millis() as u32
}

static TITLE_IMAGE: [AtomicU64; 2] = [AtomicU64::new(0), AtomicU64::new(0)];

/// The title's main image (for RVAs and telling title code from ours).
pub fn title_image(base: usize) {
    TITLE_IMAGE[0].store(base as u64, Relaxed);
    TITLE_IMAGE[1].store(base as u64 + image_size(base) as u64, Relaxed);
}

/// The title's main image: start, end (0, 0 until known).
pub(crate) fn title_range() -> (usize, usize) {
    (TITLE_IMAGE[0].load(Relaxed) as usize, TITLE_IMAGE[1].load(Relaxed) as usize)
}

fn image_size(base: usize) -> usize {
    if base == 0 {
        return 0;
    }
    // SAFETY: a mapped PE image: DOS header, then the NT headers at e_lfanew; SizeOfImage at +0x50.
    unsafe {
        let lfanew = ((base + 0x3c) as *const u32).read_unaligned() as usize;
        ((base + lfanew + 0x50) as *const u32).read_unaligned() as usize
    }
}

fn our_image() -> (usize, usize) {
    static OURS: OnceLock<(usize, usize)> = OnceLock::new();
    *OURS.get_or_init(|| {
        // SAFETY: our own exe's handle.
        let base = unsafe { windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null()) } as usize;
        (base, base + image_size(base))
    })
}

/// The GPU view the title builds (`CopyPageMappingsBatchX`): `[to, to + len)` is never armed.
pub fn gpu_view(to: usize, len: usize) {
    if !on() || len == 0 {
        return;
    }
    let mut s = state();
    for p in (to / POOL_PAGE)..to.saturating_add(len).div_ceil(POOL_PAGE) {
        s.gpu_view.insert(p);
    }
}

/// A view the title described: texture, constant buffer, vertex/index buffer.
pub(crate) fn view(address: u64, len: u64, kind: ViewKind) {
    if !on() || address == 0 || len == 0 {
        return;
    }
    let mut v = VIEWS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(&(end, k)) = v.get(&address)
        && end == address + len
        && k == kind
    {
        return;
    }
    if v.len() >= VIEWS_MOST && !matches!(kind, ViewKind::Texture { .. }) {
        return;
    }
    v.insert(address, (address + len, kind));
}

/// A placed view (`create_placed_srv/uav`): resource description words, view words (address at 10/11).
pub(crate) fn placed_view(resource: &[u32; 14], desc: &[u32; 16]) {
    if !on() {
        return;
    }
    let address = u64::from(desc[10]) | u64::from(desc[11]) << 32;
    let dimension = resource[0];
    let width = u64::from(resource[4]) | u64::from(resource[5]) << 32;
    if dimension == 1 {
        view(address, width, ViewKind::BufferView);
        return;
    }
    static SIZES: Mutex<Option<HashMap<[u32; 14], Option<u64>>>> = Mutex::new(None);
    let len = {
        let mut sizes = SIZES.lock().unwrap_or_else(|e| e.into_inner());
        *sizes.get_or_insert_with(HashMap::new).entry(*resource).or_insert_with(|| crate::resource::placed_bytes(resource))
    };
    let kind = ViewKind::Texture {
        format: resource[8],
        width: width as u32,
        height: resource[6],
        levels: (resource[7] >> 16) as u16,
    };
    view(address, len.unwrap_or(width.max(1)), kind);
}

/// A DirectStorage request completed: `shown` are the title addresses that show its bytes now (all views).
/// Arms the CPU-view pages. Call after the layer's write is finished.
pub fn loaded(shown: &[(usize, usize)], options: u64, bytes: usize) {
    if !on() {
        return;
    }
    reporter();
    let ms = now_ms();
    // The pieces to arm: not the GPU view.
    let gpu: Vec<bool> = {
        let s = state();
        shown.iter().map(|&(at, _)| s.gpu_view.contains(&(at / POOL_PAGE))).collect()
    };
    let mut pages: Vec<(usize, u16)> = Vec::new();
    for (&(at, len), gpu) in shown.iter().zip(gpu) {
        if gpu || len == 0 {
            continue;
        }
        let end = at + len;
        let mut p = at / PAGE * PAGE;
        while p < end {
            let lo = p.max(at);
            let hi = (p + PAGE).min(end);
            pages.push((p, (hi - lo) as u16));
            p += PAGE;
        }
    }
    let id;
    {
        let mut s = state();
        id = s.loads.len() as u32;
        s.loads.push(Load {
            ms,
            bytes: bytes as u64,
            options: options_class(options),
            shown: shown.iter().take(8).copied().collect(),
            kind: None,
            armed: 0,
            by: [0; WHATS],
        });
    }
    if pages.is_empty() {
        return;
    }
    // Under the write guards' lock: nothing of theirs changes a protection between our look and our change.
    crate::watch::with_guards_locked(|| {
        // Known as ours BEFORE any guard is set: another worker writing a neighbouring request into the same
        // 4 KiB page (or any thread reading it) may fault the moment the guard is on (run-131 died so: zlib
        // into a shared page while the table did not know it yet).
        {
            let mut s = state();
            for &(p, b) in &pages {
                s.ever.insert(p / POOL_PAGE);
                if let Some((old, ob)) = s.armed.insert(p, (id, b))
                    && old != id
                {
                    s.loads[old as usize].by[What::Replaced as usize] += u32::from(ob);
                }
            }
            ARMED_ANY.store(true, Relaxed);
        }
        let mut armed: Vec<(usize, u16, bool)> = Vec::with_capacity(pages.len());
        let mut i = 0;
        while i < pages.len() {
            let (p, _) = pages[i];
            // A run of consecutive pages.
            let mut j = i + 1;
            while j < pages.len() && pages[j].0 == pages[j - 1].0 + PAGE {
                j += 1;
            }
            let run_end = pages[j - 1].0 + PAGE;
            let mut at = p;
            while at < run_end {
                let (prot, region_end) = query(at);
                let to = region_end.min(run_end).max(at + PAGE);
                let ok = guardable(prot) && protect(at, to - at, prot | PAGE_GUARD);
                let mut k = i;
                while k < j {
                    if pages[k].0 >= at && pages[k].0 < to {
                        armed.push((pages[k].0, pages[k].1, ok));
                    }
                    k += 1;
                }
                at = to;
            }
            i = j;
        }
        let mut s = state();
        let mut total = 0u64;
        let mut refused = 0u32;
        for (p, b, ok) in armed {
            if !ok {
                refused += u32::from(b);
                // Not guarded: not ours to wait for (unless a fault took it already).
                if s.armed.get(&p).is_some_and(|&(i, _)| i == id) {
                    s.armed.remove(&p);
                }
                continue;
            }
            total += u64::from(b);
        }
        let l = &mut s.loads[id as usize];
        l.armed = total;
        l.by[What::NotArmed as usize] += refused;
    });
}

fn guardable(prot: u32) -> bool {
    use windows_sys::Win32::System::Memory::{PAGE_READONLY, PAGE_READWRITE, PAGE_WRITECOPY};
    matches!(prot, PAGE_READWRITE | PAGE_READONLY | PAGE_WRITECOPY)
}

/// Protection of the committed region at `at` (0 if not committed) and where the region ends.
fn query(at: usize) -> (u32, usize) {
    use windows_sys::Win32::System::Memory::{MEM_COMMIT, MEMORY_BASIC_INFORMATION, VirtualQuery};
    // SAFETY: plain data filled in by the query.
    let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: a valid out pointer of the right size.
    let n = unsafe { VirtualQuery(at as *const _, &mut info, size_of_val(&info)) };
    if n == 0 {
        return (0, at + PAGE);
    }
    let end = info.BaseAddress as usize + info.RegionSize;
    (if info.State == MEM_COMMIT { info.Protect } else { 0 }, end)
}

fn protect(at: usize, len: usize, prot: u32) -> bool {
    let mut old = 0;
    // SAFETY: committed title memory; only the guard bit changes.
    unsafe { windows_sys::Win32::System::Memory::VirtualProtect(at as *const _, len, prot, &mut old) != 0 }
}

/// From `watch`'s vectored handler, first: a guard-page violation on a page we armed. True: go on.
///
/// # Safety
/// `info` is the exception being dispatched.
pub(crate) unsafe fn fault(info: *mut windows_sys::Win32::System::Diagnostics::Debug::EXCEPTION_POINTERS) -> bool {
    if !ARMED_ANY.load(Relaxed) {
        return false;
    }
    // SAFETY: the system passes the exception being dispatched.
    let r = unsafe { &*(*info).ExceptionRecord };
    if r.ExceptionCode != STATUS_GUARD_PAGE_VIOLATION || r.NumberParameters < 2 {
        return false;
    }
    let at = r.ExceptionInformation[1];
    let write = r.ExceptionInformation[0] == 1;
    let rip = r.ExceptionAddress as usize;
    let (title_lo, title_hi) = (TITLE_IMAGE[0].load(Relaxed) as usize, TITLE_IMAGE[1].load(Relaxed) as usize);
    let (ours_lo, ours_hi) = our_image();
    // Whose code: the instruction's image, or the first title/layer frame up the stack.
    let who = |pc: usize| {
        if (title_lo..title_hi).contains(&pc) {
            Some((true, pc - title_lo))
        } else if (ours_lo..ours_hi).contains(&pc) {
            Some((false, 0))
        } else {
            None
        }
    };
    let found = who(rip).or_else(|| {
        // SAFETY: the context the system passes; copied, not changed.
        let context = unsafe { *(*info).ContextRecord };
        walk(context, who)
    });
    let ms = now_ms();
    let mut s = state();
    let page = at / PAGE * PAGE;
    let Some((id, bytes)) = s.armed.remove(&page) else {
        // Disarmed or released meanwhile (the guard was still on the page): ours all the same.
        let ours = s.ever.contains(&(at / POOL_PAGE));
        if ours {
            s.lost_events += 1;
        }
        return ours;
    };
    s.faults += 1;
    let (title, rva) = found.unwrap_or((false, 0));
    let what = if title {
        if write { What::TitleWrite } else { What::TitleRead }
    } else {
        What::Layer
    };
    let load_ms = s.loads[id as usize].ms;
    s.loads[id as usize].by[what as usize] += u32::from(bytes);
    if title {
        let waited = ms.saturating_sub(load_ms);
        let bucket = match waited {
            0..10 => 0,
            10..100 => 1,
            100..1000 => 2,
            1000..10_000 => 3,
            10_000..60_000 => 4,
            _ => 5,
        };
        s.histogram[bucket] += 1;
        if s.title.len() < 32 << 20 {
            s.title.push((id, rva as u32, bytes, write));
        }
    }
    true
}

/// The first frame up the stack whose code `who` knows (at most 24 frames).
fn walk(
    context: windows_sys::Win32::System::Diagnostics::Debug::CONTEXT,
    who: impl Fn(usize) -> Option<(bool, usize)>,
) -> Option<(bool, usize)> {
    use windows_sys::Win32::System::Diagnostics::Debug::{RtlLookupFunctionEntry, RtlVirtualUnwind, UNW_FLAG_NHANDLER};
    #[repr(C, align(16))]
    struct Aligned(windows_sys::Win32::System::Diagnostics::Debug::CONTEXT);
    let (mut low, mut high) = (0usize, 0usize);
    // SAFETY: plain out values.
    unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadStackLimits(&mut low, &mut high) };
    let mut aligned = Aligned(context);
    let c = &mut aligned.0;
    for _ in 0..24 {
        let rsp = c.Rsp as usize;
        if rsp < low || rsp + 8 > high {
            return None;
        }
        let mut image = 0;
        // SAFETY: looks up the function table entry covering `Rip`, if any.
        let entry = unsafe { RtlLookupFunctionEntry(c.Rip, &mut image, std::ptr::null_mut()) };
        if entry.is_null() {
            // SAFETY: `rsp` is inside this thread's stack.
            c.Rip = unsafe { (rsp as *const u64).read() };
            c.Rsp += 8;
        } else {
            let (mut data, mut frame) = (std::ptr::null_mut(), 0);
            // SAFETY: `entry` belongs to `image`; the context is our copy.
            unsafe { RtlVirtualUnwind(UNW_FLAG_NHANDLER, image, c.Rip, entry, c, &mut data, &mut frame, std::ptr::null_mut()) };
        }
        if c.Rip == 0 {
            return None;
        }
        if let Some(w) = who(c.Rip as usize) {
            return Some(w);
        }
    }
    None
}

/// Kernel code is about to write `[at, at + len)` for the title (`watch::kernel_writing`): armed pages there
/// are disarmed first (a kernel write into a guard page fails), noted as rewritten.
pub(crate) fn kernel_writing(at: usize, len: usize) {
    if !ARMED_ANY.load(Relaxed) || len == 0 {
        return;
    }
    let mut s = state();
    let mut p = at / PAGE * PAGE;
    while p < at + len {
        if let Some((id, b)) = s.armed.get(&p).copied() {
            let (prot, _) = query(p);
            if prot & PAGE_GUARD != 0 {
                protect(p, PAGE, prot & !PAGE_GUARD);
            }
            s.armed.remove(&p);
            s.loads[id as usize].by[What::Rewritten as usize] += u32::from(b);
        }
        p += PAGE;
    }
}

/// The title unmaps `[at, at + len)` (before the view goes): armed pages there were never touched.
pub(crate) fn unmapping(at: usize, len: usize) {
    if !ARMED_ANY.load(Relaxed) || len == 0 {
        return;
    }
    let mut s = state();
    if s.armed.is_empty() {
        return;
    }
    let mut p = at / PAGE * PAGE;
    let end = at.saturating_add(len);
    // Large ranges with few armed pages: walk the table instead.
    let keys: Vec<usize> = if len / PAGE > s.armed.len() {
        s.armed.keys().copied().filter(|&k| k >= p && k < end).collect()
    } else {
        let mut v = Vec::new();
        while p < end {
            if s.armed.contains_key(&p) {
                v.push(p);
            }
            p += PAGE;
        }
        v
    };
    for k in keys {
        let (id, b) = s.armed.remove(&k).expect("listed");
        let what = if query(k).0 & PAGE_GUARD != 0 { What::ReleasedUntouched } else { What::GuardLost };
        s.loads[id as usize].by[what as usize] += u32::from(b);
    }
}

/// Starts the 10 s report (once) and the exit report.
fn reporter() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = std::thread::Builder::new().name("victory-loadmeter".into()).spawn(|| {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(10));
                report(false);
            }
        });
        unsafe extern "C" {
            fn atexit(f: extern "C" fn()) -> i32;
        }
        extern "C" fn at_exit() {
            report(true);
        }
        // SAFETY: registering a plain function with the CRT.
        unsafe { atexit(at_exit) };
    });
}

fn classify(shown: &[(usize, usize)], views: &BTreeMap<u64, (u64, ViewKind)>) -> Option<ViewKind> {
    let mut best: Option<ViewKind> = None;
    for &(at, len) in shown {
        let (start, end) = (at as u64, (at + len) as u64);
        for (_, &(vend, kind)) in views.range(..end).rev().take(64) {
            if vend > start {
                // A texture wins over a buffer.
                match (best, kind) {
                    (Some(ViewKind::Texture { .. }), _) => {}
                    (_, k @ ViewKind::Texture { .. }) => return Some(k),
                    (None, k) => best = Some(k),
                    _ => {}
                }
            }
        }
    }
    best
}

fn mb(b: u64) -> f64 {
    b as f64 / (1 << 20) as f64
}

/// Folded numbers, for the report and the tests.
#[derive(Default, Debug, Clone)]
pub(crate) struct Summary {
    pub(crate) loads: u64,
    pub(crate) bytes: u64,
    pub(crate) armed: u64,
    pub(crate) still_armed: u64,
    pub(crate) by: [u64; WHATS],
    pub(crate) kind: [[u64; WHATS + 2]; KINDS],
    pub(crate) options: [[u64; WHATS + 2]; 8],
    pub(crate) formats: Vec<(u32, u64, u64)>,
    pub(crate) top: Vec<(u32, u64, [u64; KINDS])>,
}

fn summarize(s: &mut State) -> Summary {
    {
        let views = VIEWS.lock().unwrap_or_else(|e| e.into_inner());
        for l in s.loads.iter_mut() {
            if !matches!(l.kind, Some(ViewKind::Texture { .. })) {
                l.kind = classify(&l.shown, &views).or(l.kind);
            }
        }
    }
    let mut still: HashMap<u32, u64> = HashMap::new();
    for &(id, b) in s.armed.values() {
        *still.entry(id).or_default() += u64::from(b);
    }
    let mut out = Summary::default();
    let mut formats: HashMap<u32, (u64, u64)> = HashMap::new();
    for (i, l) in s.loads.iter().enumerate() {
        out.loads += 1;
        out.bytes += l.bytes;
        out.armed += l.armed;
        let st = still.get(&(i as u32)).copied().unwrap_or(0);
        out.still_armed += st;
        let k = kind_class(l.kind);
        let row = |r: &mut [u64; WHATS + 2]| {
            r[0] += l.bytes;
            r[1] += st;
            for w in 0..WHATS {
                r[2 + w] += u64::from(l.by[w]);
            }
        };
        row(&mut out.kind[k]);
        row(&mut out.options[l.options as usize]);
        for w in 0..WHATS {
            out.by[w] += u64::from(l.by[w]);
        }
        if let Some(ViewKind::Texture { format, .. }) = l.kind {
            let f = formats.entry(format).or_default();
            f.0 += l.bytes;
            f.1 += u64::from(l.by[0]) + u64::from(l.by[1]);
        }
    }
    let mut formats: Vec<(u32, u64, u64)> = formats.into_iter().map(|(f, (a, b))| (f, a, b)).collect();
    formats.sort_by_key(|f| std::cmp::Reverse(f.1));
    formats.truncate(8);
    out.formats = formats;
    let mut top: HashMap<u32, (u64, [u64; KINDS])> = HashMap::new();
    for &(id, rva, b, _) in &s.title {
        let e = top.entry(rva).or_default();
        e.0 += u64::from(b);
        e.1[kind_class(s.loads[id as usize].kind)] += u64::from(b);
    }
    let mut top: Vec<(u32, u64, [u64; KINDS])> = top.into_iter().map(|(r, (b, k))| (r, b, k)).collect();
    top.sort_by_key(|t| std::cmp::Reverse(t.1));
    top.truncate(20);
    out.top = top;
    out
}

fn report(last: bool) {
    static LAST: Mutex<(u64, Option<Instant>)> = Mutex::new((0, None));
    let (sum, faults, hist, lost) = {
        let mut s = state();
        let sum = summarize(&mut s);
        (sum, s.faults, s.histogram, s.lost_events)
    };
    let per_s = {
        let mut l = LAST.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        let r = l.1.map(|t| (faults - l.0) as f64 / (now - t).as_secs_f64().max(0.001)).unwrap_or(0.0);
        *l = (faults, Some(now));
        r
    };
    let armed = sum.armed.max(1) as f64;
    let pct = |b: u64| 100.0 * b as f64 / armed;
    let mut line = format!(
        "load-read meter{} (VICTORY_LOAD_READ_METER=1): loaded {:.1} MB in {} loads, guarded in the CPU view {:.1} MB; still guarded (never touched yet) {:.1} MB ({:.1} %)",
        if last { " at exit" } else { "" },
        mb(sum.bytes),
        sum.loads,
        mb(sum.armed),
        mb(sum.still_armed),
        pct(sum.still_armed)
    );
    for w in 0..WHATS {
        line += &format!("; {} {:.1} MB ({:.1} %)", WHAT_NAMES[w], mb(sum.by[w]), pct(sum.by[w]));
    }
    line += &format!(
        "; first title access after load <10ms {}, <100ms {}, <1s {}, <10s {}, <60s {}, later {} pages; guard faults {} ({:.0}/s), late faults {}",
        hist[0], hist[1], hist[2], hist[3], hist[4], hist[5], faults, per_s, lost
    );
    tracing::info!("{line}");
    let row = |name: &str, r: &[u64; WHATS + 2]| {
        format!(
            "{name}: loaded {:.1} MB, title read {:.1}, title written {:.1}, layer first {:.1}, still untouched {:.1}, released untouched {:.1}, rewritten {:.1}, guard lost {:.1}, not guarded {:.1}",
            mb(r[0]),
            mb(r[2]),
            mb(r[3]),
            mb(r[4]),
            mb(r[1]),
            mb(r[6]),
            mb(r[5]) + mb(r[8]),
            mb(r[7]),
            mb(r[9]),
        )
    };
    let kinds: Vec<String> = (0..KINDS).map(|k| row(KIND_NAMES[k], &sum.kind[k])).collect();
    tracing::info!("load-read meter by kind: {}", kinds.join(" | "));
    let options: Vec<String> =
        (0..8).filter(|&o| sum.options[o][0] != 0).map(|o| row(OPTION_NAMES[o], &sum.options[o])).collect();
    tracing::info!("load-read meter by options: {}", options.join(" | "));
    let formats: Vec<String> =
        sum.formats.iter().map(|(f, a, b)| format!("format {f}: {:.1} MB loaded, {:.1} MB title-touched", mb(*a), mb(*b))).collect();
    tracing::info!("load-read meter textures by format: {}", formats.join("; "));
    let top: Vec<String> = sum
        .top
        .iter()
        .map(|(rva, b, k)| {
            format!("+{rva:#x} {:.2} MB (texture {:.2}, buffer {:.2}, neither {:.2})", mb(*b), mb(k[0]), mb(k[1]), mb(k[2]))
        })
        .collect();
    tracing::info!("load-read meter top title code touching loaded pages (RVA): {}", top.join("; "));
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) static FORCE: AtomicBool = AtomicBool::new(false);

    /// A pagefile section view of `len` bytes, read-write.
    fn view_of(len: usize) -> usize {
        use windows_sys::Win32::System::Memory::{CreateFileMappingW, FILE_MAP_WRITE, MapViewOfFile, PAGE_READWRITE};
        // SAFETY: a fresh pagefile section, mapped whole.
        unsafe {
            let section =
                CreateFileMappingW(-1isize as _, std::ptr::null(), PAGE_READWRITE, 0, len as u32, std::ptr::null());
            assert!(!section.is_null());
            let at = MapViewOfFile(section, FILE_MAP_WRITE, 0, 0, len).Value as usize;
            assert_ne!(at, 0);
            at
        }
    }

    fn by_of(at: usize) -> [u32; WHATS] {
        let s = state();
        let id = s.loads.iter().rposition(|l| l.shown.first().is_some_and(|r| r.0 == at)).expect("a load");
        s.loads[id].by
    }

    /// Layer code reading and writing a guarded load goes on with the right bytes (run-131 died there), is
    /// counted as layer-first once per page; a write by a thread racing the arming never escapes the handler;
    /// the rest is released untouched at the unmap.
    #[test]
    fn layer_access_to_a_guarded_load_goes_on_and_counts() {
        FORCE.store(true, Relaxed);
        crate::watch::install();
        const LEN: usize = 16 * PAGE;
        let at = view_of(LEN);
        for i in 0..LEN {
            // SAFETY: our view.
            unsafe { ((at + i) as *mut u8).write(i as u8) };
        }
        // A writer hammering the last page while the load is armed (another request into a shared page).
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let racer = {
            let stop = stop.clone();
            std::thread::spawn(move || {
                let p = (at + 15 * PAGE + 7) as *mut u8;
                while !stop.load(Relaxed) {
                    // SAFETY: our view.
                    unsafe { p.write_volatile(p.read_volatile()) };
                }
            })
        };
        loaded(&[(at, LEN)], 1 << 21 | 1, LEN);
        stop.store(true, Relaxed);
        racer.join().expect("the racing writer never saw an unhandled guard fault");
        // A read (as an upload would) and a zlib-like write that reads back its own output.
        // SAFETY: our view.
        let got = unsafe { ((at + 3 * PAGE + 5) as *const u8).read_volatile() };
        assert_eq!(got, (3 * PAGE + 5) as u8);
        // SAFETY: our view.
        unsafe {
            let p = (at + 5 * PAGE) as *mut u8;
            p.write_volatile(p.add(1).read_volatile());
        }
        let by = by_of(at);
        assert!(by[What::Layer as usize] >= 2 * PAGE as u32, "{by:?}");
        assert_eq!(by[What::TitleRead as usize] + by[What::TitleWrite as usize], 0);
        // Once per page: a second touch is not a fault.
        // SAFETY: our view.
        let _ = unsafe { ((at + 3 * PAGE) as *const u8).read_volatile() };
        assert_eq!(by_of(at)[What::Layer as usize], by[What::Layer as usize]);
        // A kernel write for the title takes the guard off first.
        kernel_writing(at + 8 * PAGE, PAGE);
        assert_eq!(query(at + 8 * PAGE).0 & PAGE_GUARD, 0);
        assert_eq!(by_of(at)[What::Rewritten as usize], PAGE as u32);
        unmapping(at, LEN);
        let by = by_of(at);
        let all: u32 = by.iter().sum();
        assert_eq!(all, LEN as u32, "{by:?}");
        assert!(by[What::ReleasedUntouched as usize] >= 10 * PAGE as u32, "{by:?}");
        // The bytes are what they were.
        for i in (0..LEN).step_by(97) {
            if i != 5 * PAGE && i != 15 * PAGE + 7 {
                // SAFETY: our view.
                assert_eq!(unsafe { ((at + i) as *const u8).read() }, i as u8);
            }
        }
    }

    #[test]
    fn options_fold() {
        assert_eq!(options_class(0), 0);
        assert_eq!(options_class(1), 1);
        assert_eq!(options_class(3 << 1), 2);
        assert_eq!(options_class(1 | 1 << 21), 5);
        assert_eq!(OPTION_NAMES[options_class(1 | 2 | 1 << 21) as usize], "zlib+bcpack->pages");
    }

    #[test]
    fn kinds_are_found_over_any_view_and_textures_win() {
        let mut v = BTreeMap::new();
        v.insert(0x1000, (0x3000, ViewKind::Constant));
        v.insert(0x10_0000, (0x18_0000, ViewKind::Texture { format: 71, width: 256, height: 256, levels: 9 }));
        assert_eq!(classify(&[(0x2000, 0x100)], &v), Some(ViewKind::Constant));
        assert_eq!(classify(&[(0x5000, 0x100)], &v), None);
        // The CPU view says nothing, the GPU view shows the texture.
        assert!(matches!(classify(&[(0x5000, 0x100), (0x17_0000, 0x1000)], &v), Some(ViewKind::Texture { .. })));
        assert!(matches!(classify(&[(0x2000, 0x100), (0x10_0000, 1)], &v), Some(ViewKind::Texture { .. })));
    }

    #[test]
    fn summary_splits_by_kind_and_counts_untouched() {
        let mut s = State::default();
        let l = |bytes: u64, by: [u32; WHATS], shown: Vec<(usize, usize)>| Load {
            ms: 0,
            bytes,
            options: 4,
            shown,
            kind: None,
            armed: bytes,
            by,
        };
        let mut by = [0; WHATS];
        by[What::TitleRead as usize] = 4096;
        s.loads.push(l(8192, by, vec![(0x7000_0000_0000, 8192)]));
        s.loads.push(l(4096, [0; WHATS], vec![(0x7100_0000_0000, 4096)]));
        s.armed.insert(0x7000_0000_1000, (0, 4096));
        s.armed.insert(0x7100_0000_0000, (1, 4096));
        s.title.push((0, 0x1234, 4096, false));
        let sum = summarize(&mut s);
        assert_eq!(sum.loads, 2);
        assert_eq!(sum.bytes, 12288);
        assert_eq!(sum.still_armed, 8192);
        assert_eq!(sum.by[What::TitleRead as usize], 4096);
        // No views there: both neither.
        assert_eq!(sum.kind[2][0], 12288);
        assert_eq!(sum.options[4][2], 4096);
        assert_eq!(sum.top[0].0, 0x1234);
        assert_eq!(sum.top[0].2[2], 4096);
    }
}
