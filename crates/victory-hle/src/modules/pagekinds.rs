//! (coder G, ledger F24/F25) `VICTORY_PAGE_KINDS=1` (diagnostics builds; default off): which pool pages the
//! title shows in its CPU view and which in the GPU view only, from our own notes of its mapping calls
//! (`xmem::Shown::note/forget`: every title view of a pool page; an address counts as GPU view once
//! `map_alias`/`map_aliases` (CopyPageMappingsBatchX) mapped it, the CPU view otherwise).
//!
//! A line every 5 s: pool MB GPU-only / CPU-only / both; DirectStorage MB written into GPU-only pages vs
//! CPU-visible vs pages no view shows; the GPU side's part (`victory_gpu::pagekinds::line`: host texture copies
//! whose whole source is GPU-only, the layer's work on GPU-only pages, title-code writes found there); and the
//! title's texture-level allocator (F25): alloc 0x14034bc70 (ecx size -> rax GPU-view address) and free
//! 0x1403716a0 (rcx address, edx size) hooked at entry: allocations and frees (count, MB, slab = size < 64 KiB
//! or 0x18000, large), an address handed out again with no mapping change of its pages since its free, and for
//! each free whether a host texture copy lay over the range and how long it had been alive.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering::Relaxed};
use std::sync::{LazyLock, OnceLock, RwLock};

use victory_loader::callhook::{self, Regs};

const PAGE: usize = 64 << 10;

/// Whether the meter runs (the GPU side's switch).
pub(crate) fn on() -> bool {
    victory_gpu::pagekinds::tracked()
}

/// A pool page: its section and index.
type Key = (usize, usize);

#[derive(Default)]
struct Kinds {
    /// Every title address (64 KiB) that shows a pool page, and whether it is a GPU-view address.
    at: HashMap<usize, (Key, bool)>,
    /// Per pool page: CPU-view addresses, GPU-view addresses showing it.
    counts: HashMap<Key, (u16, u16)>,
    /// GPU-view addresses (64 KiB) learned from the mapping copies.
    gpu: HashSet<usize>,
    /// Per GPU-view address: mapping changes so far (for reuse without a remap).
    seq: HashMap<usize, u32>,
    /// (B verify) Per title address: mapping changes; per pool page: our DirectStorage writes.
    address_changes: HashMap<usize, u32>,
    page_writes: HashMap<Key, u32>,
}

static KINDS: LazyLock<RwLock<Kinds>> = LazyLock::new(Default::default);

fn kinds_mut() -> std::sync::RwLockWriteGuard<'static, Kinds> {
    KINDS.write().unwrap_or_else(|e| e.into_inner())
}

fn kinds() -> std::sync::RwLockReadGuard<'static, Kinds> {
    KINDS.read().unwrap_or_else(|e| e.into_inner())
}

/// `CopyPageMappingsBatchX` maps `[to, to + len)`: GPU-view addresses.
pub(crate) fn gpu_view(to: usize, len: usize) {
    if !on() || len == 0 {
        return;
    }
    let mut k = kinds_mut();
    for a in to / PAGE..to.saturating_add(len).div_ceil(PAGE) {
        k.gpu.insert(a);
    }
}

/// A title view at `base` of `len` bytes now shows pool pages from `first` on (`xmem::Shown::note`).
pub(crate) fn shown(base: usize, len: usize, section: usize, first: usize) {
    if !on() {
        return;
    }
    started();
    let mut k = kinds_mut();
    for i in 0..len / PAGE {
        let a = base / PAGE + i;
        let key = (section, first + i);
        let gpu = k.gpu.contains(&a);
        if let Some((old, old_gpu)) = k.at.insert(a, (key, gpu)) {
            drop_count(&mut k, old, old_gpu);
        }
        *k.address_changes.entry(a).or_default() += 1;
        let c = k.counts.entry(key).or_default();
        if gpu {
            c.1 = c.1.saturating_add(1);
            *k.seq.entry(a).or_default() += 1;
        } else {
            c.0 = c.0.saturating_add(1);
        }
    }
}

/// The view at `base` of `len` bytes is gone (`xmem::Shown::forget`).
pub(crate) fn unshown(base: usize, len: usize) {
    if !on() {
        return;
    }
    let mut k = kinds_mut();
    for i in 0..len / PAGE {
        let a = base / PAGE + i;
        if let Some((key, gpu)) = k.at.remove(&a) {
            *k.address_changes.entry(a).or_default() += 1;
            drop_count(&mut k, key, gpu);
            if gpu {
                *k.seq.entry(a).or_default() += 1;
            }
        }
    }
}

fn drop_count(k: &mut Kinds, key: Key, gpu: bool) {
    if let Some(c) = k.counts.get_mut(&key) {
        if gpu {
            c.1 = c.1.saturating_sub(1);
        } else {
            c.0 = c.0.saturating_sub(1);
        }
        if *c == (0, 0) {
            k.counts.remove(&key);
        }
    }
}

/// 0: GPU-only, 1: CPU-visible, 2: no view.
fn class_of(k: &Kinds, key: &Key) -> usize {
    match k.counts.get(key) {
        Some(&(0, g)) if g > 0 => 0,
        Some(&(c, _)) if c > 0 => 1,
        _ => 2,
    }
}

// ---- The title's own truth (F24): its streaming region and page state table ----

/// RVAs: the CPU-view region's base and bytes (read by the appender 0x1400f7c20), the GPU view's base (F3),
/// the page state table (u64 per 64 KiB page of the region: bit 62 CPU-mapped, bit 63 GPU-mapped; writer
/// 0x14555b2c0 indexes it by page of the region).
const CPU_BASE: usize = 0x09e1_dac0;
const CPU_BYTES: usize = 0x09e1_dac8;
const GPU_BASE: usize = 0x09e1_dad0;
const STATE: usize = 0x071b_fb08;

static IMAGE: AtomicUsize = AtomicUsize::new(0);

fn global(rva: usize) -> usize {
    match IMAGE.load(Relaxed) {
        0 => 0,
        // SAFETY: a u64 of the title's mapped image (.data).
        base => unsafe { ((base + rva) as *const usize).read_volatile() },
    }
}

/// The region: CPU-view base, bytes, GPU-view base, state table (0s until the title set them).
fn region() -> (usize, usize, usize, usize) {
    (global(CPU_BASE), global(CPU_BYTES), global(GPU_BASE), global(STATE))
}

/// Which view of the region `at` lies in (0 CPU view, 1 GPU view) and its page index.
fn index_of(at: usize, r: (usize, usize, usize, usize)) -> Option<(u8, usize)> {
    let (cpu, bytes, gpu, _) = r;
    if bytes == 0 {
        return None;
    }
    if cpu != 0 && (cpu..cpu + bytes).contains(&at) {
        return Some((0, (at - cpu) / PAGE));
    }
    if gpu != 0 && (gpu..gpu + bytes).contains(&at) {
        return Some((1, (at - gpu) / PAGE));
    }
    None
}

/// The title's state of page `index`: 0 GPU-only, 1 CPU-visible, 2 neither mapped.
fn state_class(index: usize, r: (usize, usize, usize, usize)) -> usize {
    let table = r.3;
    if table == 0 || index >= r.1 / PAGE {
        return 2;
    }
    // SAFETY: the title's table holds a u64 per page of the region.
    let e = unsafe { ((table + index * 8) as *const u64).read_volatile() };
    match (e >> 62 & 1, e >> 63) {
        (0, 1) => 0,
        (1, _) => 1,
        _ => 2,
    }
}

/// 0 CPU view, 1 GPU view, 2 neither (where the GPU side's copies lie).
fn view_of(at: usize) -> u8 {
    index_of(at, region()).map_or(2, |v| v.0)
}

/// Bytes of `[at, at + len)` on pages the title keeps GPU-only, on CPU-visible ones, elsewhere (the GPU side's
/// query), by the title's own state table.
fn query(at: usize, len: usize) -> [u64; 3] {
    let mut out = [0u64; 3];
    if len == 0 {
        return out;
    }
    let r = region();
    let end = at.saturating_add(len);
    let mut a = at / PAGE * PAGE;
    while a < end {
        let n = ((a + PAGE).min(end) - a.max(at)) as u64;
        let class = index_of(a, r).map_or(2, |(_, i)| state_class(i, r));
        out[class] += n;
        a += PAGE;
    }
    out
}

/// (coder K, `VICTORY_GPU_ONLY=1`) Whether every 64 KiB page of `[at, at + len)` lies in the region and is
/// GPU-only by the title's table (the region read once).
fn all_gpu_only(at: usize, len: usize) -> bool {
    if len == 0 {
        return false;
    }
    let r = region();
    if r.3 == 0 {
        return false;
    }
    let end = at.saturating_add(len);
    let mut a = at / PAGE * PAGE;
    while a < end {
        match index_of(a, r) {
            Some((_, i)) if state_class(i, r) == 0 => {}
            _ => return false,
        }
        a += PAGE;
    }
    true
}

/// (coder K) For an address: 0 in the region's CPU view and CPU-unmapped by the title's table, 1 CPU-mapped
/// there, 2 in the GPU view, 3 outside the region.
fn cpu_view_state(at: usize) -> u8 {
    let r = region();
    match index_of(at, r) {
        Some((0, i)) => {
            if r.3 == 0 || i >= r.1 / PAGE {
                return 1;
            }
            // SAFETY: the title's table holds a u64 per page of the region.
            let e = unsafe { ((r.3 + i * 8) as *const u64).read_volatile() };
            u8::from(e >> 62 & 1 == 1)
        }
        Some(_) => 2,
        None => 3,
    }
}

/// What our own notes say (run-134's inference: an address the mapping copies mapped is GPU view).
fn notes_class(at: usize) -> usize {
    let k = kinds();
    k.at.get(&(at / PAGE)).map_or(2, |(key, _)| class_of(&k, key))
}

/// DirectStorage wrote into these pool pages (`pages`, for verify), shown at `shown` (all title views; each
/// page of the region counted once).
pub(crate) fn written(pages: &[(Key, usize)], shown: &[(usize, usize)]) {
    if !on() {
        return;
    }
    let r = region();
    let mut seen = HashSet::new();
    for &(at, len) in shown {
        let end = at + len;
        let mut a = at / PAGE * PAGE;
        while a < end {
            let n = ((a + PAGE).min(end) - a.max(at)) as u64;
            match index_of(a, r) {
                Some((_, i)) => {
                    if seen.insert(i) {
                        WRITTEN[state_class(i, r)].fetch_add(n, Relaxed);
                    }
                }
                None => {
                    WRITTEN[3].fetch_add(n, Relaxed);
                }
            }
            WRITTEN_NOTES[notes_class(a)].fetch_add(n, Relaxed);
            a += PAGE;
        }
    }
    if shown.is_empty() {
        WRITTEN[3].fetch_add(pages.iter().map(|p| p.1 as u64).sum(), Relaxed);
    }
    if victory_gpu::pagekinds::gpu_only_mode() == 2 {
        let mut k = kinds_mut();
        for (key, _) in pages {
            *k.page_writes.entry(*key).or_default() += 1;
        }
    }
}

/// The same split by our notes (every view counted).
static WRITTEN_NOTES: [AtomicU64; 3] = [const { AtomicU64::new(0) }; 3];

// ---- The title's mapping calls (F24): 0x1400f7c20(batch, kind, CPU-view address, bytes) ----
const MAPPING: usize = 0x000f_7c20;
/// `push r14; push rsi; push rdi; push rbx; sub rsp, 0x28` (checked against the exe file).
const MAPPING_PROLOGUE: [u8; 9] = [0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x28];
/// Per kind 0..5 (map both, CPU only, GPU only, unmap both, CPU, GPU) and other: calls, bytes.
static MAPPINGS: [[AtomicU64; 2]; 7] = [const { [const { AtomicU64::new(0) }; 2] }; 7];

unsafe extern "C" fn mapping_entered(regs: *mut Regs) {
    // SAFETY: the thunk's saved registers.
    let (kind, bytes) = unsafe { ((*regs).rdx & 0xffff_ffff, (*regs).r9) };
    let k = (kind as usize).min(6);
    MAPPINGS[k][0].fetch_add(1, Relaxed);
    MAPPINGS[k][1].fetch_add(bytes, Relaxed);
}

/// (B verify) Legitimate changes of what the title address `at` shows: mapping changes there plus our
/// DirectStorage writes into the pool page it shows.
fn legit(at: usize) -> u32 {
    let k = kinds();
    let a = at / PAGE;
    let mapped = k.address_changes.get(&a).copied().unwrap_or(0);
    let written = k.at.get(&a).and_then(|(key, _)| k.page_writes.get(key)).copied().unwrap_or(0);
    mapped.wrapping_add(written)
}

/// DirectStorage bytes written since the last line: into GPU-only, CPU-visible, unshown pages.
static WRITTEN: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];

// ---- F25: the texture-level allocator ----

/// RVAs (preferred base 0x140000000) and prologues, checked against the exe file
/// (`push r15; push r14; push r12; push rsi; push rdi; push rbp; push rbx` / without `push rbp`).
const ALLOC: usize = 0x0034_bc70;
const ALLOC_PROLOGUE: [u8; 10] = [0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53];
const FREE: usize = 0x0037_16a0;
const FREE_PROLOGUE: [u8; 9] = [0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x53];

fn slab(size: u64) -> bool {
    size < 0x1_0000 || size == 0x1_8000
}

static AFTER: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    /// Return addresses taken at the allocator's entry, with the size asked.
    static TAKEN: std::cell::RefCell<Vec<(u64, u64)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Since the last line: allocations (slab, large: count, bytes), frees (slab, large: count, bytes), addresses
/// handed out again with no mapping change since their free, frees with a host copy over them, frees whose
/// copy had lived < 5 s / 5-60 s / longer (census grain), allocations that failed (rax 0).
static LEVELS: [AtomicU64; 15] = [const { AtomicU64::new(0) }; 15];

/// Freed addresses: size, the mapping changes of their first page when freed.
static FREED: LazyLock<std::sync::Mutex<HashMap<u64, (u64, u32)>>> = LazyLock::new(Default::default);

fn seq_at(at: u64) -> u32 {
    kinds().seq.get(&(at as usize / PAGE)).copied().unwrap_or(0)
}

unsafe extern "C" fn alloc_entered(regs: *mut Regs) {
    // SAFETY: the thunk's saved registers; `ret` is the return address.
    let regs = unsafe { &mut *regs };
    let size = regs.rcx & 0xffff_ffff;
    TAKEN.with(|t| t.borrow_mut().push((regs.ret, size)));
    regs.ret = AFTER.load(Relaxed) as u64;
}

unsafe extern "C" fn alloc_returned(rax: u64, slot: *mut u64) {
    let Some((ret, size)) = TAKEN.with(|t| t.borrow_mut().pop()) else {
        std::process::abort();
    };
    // SAFETY: the stub's slot for the return address.
    unsafe { *slot = ret };
    let _ = std::panic::catch_unwind(|| {
        if rax == 0 {
            LEVELS[14].fetch_add(1, Relaxed);
            return;
        }
        let k = if slab(size) { 0 } else { 2 };
        LEVELS[k].fetch_add(1, Relaxed);
        LEVELS[k + 1].fetch_add(size, Relaxed);
        let freed = FREED.lock().unwrap_or_else(|e| e.into_inner()).remove(&rax);
        if let Some((_, seq)) = freed
            && seq == seq_at(rax)
        {
            LEVELS[8].fetch_add(1, Relaxed);
        }
        // (coder K, `VICTORY_GPU_ONLY=1`) A new level at this address: copies over it take it again.
        victory_gpu::pagekinds::level_allocated(rax as usize, size as usize);
    });
}

unsafe extern "C" fn free_entered(regs: *mut Regs) {
    // SAFETY: the thunk's saved registers.
    let (at, size) = unsafe { ((*regs).rcx, (*regs).rdx & 0xffff_ffff) };
    let _ = std::panic::catch_unwind(|| {
        let k = if slab(size) { 4 } else { 6 };
        LEVELS[k].fetch_add(1, Relaxed);
        LEVELS[k + 1].fetch_add(size, Relaxed);
        let seq = seq_at(at);
        {
            let mut freed = FREED.lock().unwrap_or_else(|e| e.into_inner());
            if freed.len() < 1 << 20 {
                freed.insert(at, (size, seq));
            }
        }
        // (coder K, `VICTORY_GPU_ONLY=1`) The level's copy goes at the next sweep.
        victory_gpu::pagekinds::level_freed(at as usize, size as usize);
        if let Some(age) = victory_gpu::pagekinds::copy_over(at as usize, size as usize) {
            LEVELS[9].fetch_add(1, Relaxed);
            LEVELS[if age < 5_000 { 10 } else if age < 60_000 { 11 } else { 12 }].fetch_add(1, Relaxed);
        }
    });
}

/// Hooks the allocator when the meter is on.
///
/// # Safety
/// `base` is the title's mapped image, and no title code runs yet.
pub(crate) unsafe fn install(base: usize) {
    if !on() {
        return;
    }
    IMAGE.store(base, Relaxed);
    started();
    // SAFETY: the caller's promise; `hook_entry` checks the prologue.
    match unsafe { callhook::hook_entry(base + MAPPING, &MAPPING_PROLOGUE, mapping_entered) } {
        Ok(()) => tracing::info!("F24: mapping call 0x1400f7c20 hooked (VICTORY_PAGE_KINDS=1)"),
        Err(e) => tracing::warn!(error = %e, "F24: mapping call not hooked: the bytes differ (another build?)"),
    }
    let Ok(after) = callhook::after_stub(alloc_returned) else {
        tracing::warn!("page kinds: no stub; the level allocator is not hooked");
        return;
    };
    AFTER.store(after, Relaxed);
    // SAFETY: the caller's promise; `hook_entry` checks the prologue.
    match unsafe { callhook::hook_entry(base + ALLOC, &ALLOC_PROLOGUE, alloc_entered) } {
        Ok(()) => tracing::info!("F25: texture-level alloc 0x14034bc70 hooked (VICTORY_PAGE_KINDS=1)"),
        Err(e) => tracing::warn!(error = %e, "F25: texture-level alloc not hooked: the bytes differ (another build?)"),
    }
    // SAFETY: as above.
    match unsafe { callhook::hook_entry(base + FREE, &FREE_PROLOGUE, free_entered) } {
        Ok(()) => tracing::info!("F25: texture-level free 0x1403716a0 hooked (VICTORY_PAGE_KINDS=1)"),
        Err(e) => tracing::warn!(error = %e, "F25: texture-level free not hooked: the bytes differ (another build?)"),
    }
}

/// The query for the GPU side and the 5 s line (once).
fn started() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        victory_gpu::pagekinds::set_query(query);
        victory_gpu::pagekinds::set_legit(legit);
        victory_gpu::pagekinds::set_gpu_only(all_gpu_only);
        victory_gpu::pagekinds::set_region(region);
        victory_gpu::pagekinds::set_cpu_view(cpu_view_state);
        victory_gpu::pagekinds::set_describe(super::xmem::describe);
        victory_gpu::pagekinds::set_our_views(super::xmem::section_windows());
        victory_gpu::pagekinds::set_linear_views(super::xmem::linear_views());
        tracing::info!("page kinds meter on (VICTORY_PAGE_KINDS=1): 'page kinds' line every 5 s");
        let _ = std::thread::Builder::new().name("victory-pagekinds".into()).spawn(|| {
            let mut frame = victory_gpu::frame::current();
            let began = std::time::Instant::now();
            loop {
                std::thread::sleep(std::time::Duration::from_secs(5));
                let now = victory_gpu::frame::current();
                let frames = now.saturating_sub(frame) as f64;
                frame = now;
                line(5.0, frames, began.elapsed().as_secs());
            }
        });
    });
}

fn mb(b: u64) -> f64 {
    b as f64 / 1048576.0
}

/// Pool pages GPU-only, CPU-only, both, by our notes.
fn pool_notes() -> [u64; 3] {
    let k = kinds();
    let mut out = [0u64; 3];
    for &(c, g) in k.counts.values() {
        out[match (c, g) {
            (0, _) => 0,
            (_, 0) => 1,
            _ => 2,
        }] += 1;
    }
    out
}

/// Pages of the region by the title's table: GPU-only, CPU-only, both, neither.
fn pool_title() -> [u64; 4] {
    let r = region();
    let mut out = [0u64; 4];
    if r.3 == 0 || r.1 == 0 {
        return out;
    }
    for i in 0..r.1 / PAGE {
        // SAFETY: the title's table holds a u64 per page of the region.
        let e = unsafe { ((r.3 + i * 8) as *const u64).read_volatile() };
        out[match (e >> 62 & 1, e >> 63) {
            (0, 1) => 0,
            (1, 0) => 1,
            (1, 1) => 2,
            _ => 3,
        }] += 1;
    }
    out
}

fn line(seconds: f64, frames: f64, since_start: u64) {
    // (coder K) Our own section views, for the GPU-only rule (live: told again every 5 s).
    victory_gpu::pagekinds::set_our_views(super::xmem::section_windows());
    victory_gpu::pagekinds::set_linear_views(super::xmem::linear_views());
    let t = pool_title();
    let p = pool_notes();
    let r = region();
    let w: Vec<u64> = WRITTEN.iter().map(|c| c.swap(0, Relaxed)).collect();
    let wn: Vec<u64> = WRITTEN_NOTES.iter().map(|c| c.swap(0, Relaxed)).collect();
    let l: Vec<u64> = LEVELS.iter().map(|c| c.swap(0, Relaxed)).collect();
    let m: Vec<String> = (0..7)
        .map(|k| format!("{k}: {} ({:.1} MB)", MAPPINGS[k][0].swap(0, Relaxed), mb(MAPPINGS[k][1].swap(0, Relaxed))))
        .collect();
    let windows: Vec<String> =
        super::xmem::section_windows().iter().map(|(a, n)| format!("{a:#x}+{} MB", n >> 20)).collect();
    tracing::info!(
        "page kinds (VICTORY_PAGE_KINDS=1, F24) by the title's table [0x1471bfb08]: GPU-only {:.0} MB, CPU only {:.0} MB, both {:.0} MB, unmapped {:.0} MB; \
         our notes say GPU-only {:.0} / CPU only {:.0} / both {:.0} MB; views: CPU region {:#x}+{} MB, GPU region {:#x}, our section views {}; \
         title mapping calls 0x1400f7c20 by kind in {seconds:.0} s: {}; \
         dstorage written in {seconds:.0} s (title's table): into GPU-only pages {:.1} MB, CPU-visible {:.1} MB, in the region but unmapped by the title's table at completion {:.1} MB, shown by no view of the region (plain destinations, pages no title view shows) {:.1} MB \
         (our notes: {:.1} / {:.1} / {:.1}); {}{}",
        mb(t[0] * PAGE as u64),
        mb(t[1] * PAGE as u64),
        mb(t[2] * PAGE as u64),
        mb(t[3] * PAGE as u64),
        mb(p[0] * PAGE as u64),
        mb(p[1] * PAGE as u64),
        mb(p[2] * PAGE as u64),
        r.0,
        r.1 >> 20,
        r.2,
        windows.join(", "),
        m.join(", "),
        mb(w[0]),
        mb(w[1]),
        mb(w[2]),
        mb(w[3]),
        mb(wn[0]),
        mb(wn[1]),
        mb(wn[2]),
        victory_gpu::pagekinds::line(seconds, frames, view_of),
        // (coder K, step 5) Kind-0 pages the title's CPU side touched.
        if victory_gpu::pagekinds::on() { victory_gpu::pagekinds::touch_line(since_start) } else { String::new() },
    );
    tracing::info!(
        "texture levels (F25, VICTORY_PAGE_KINDS=1) in {seconds:.0} s: allocated slab {} ({:.1} MB), large {} ({:.1} MB), failed {}; \
         freed slab {} ({:.1} MB), large {} ({:.1} MB); handed out again with no mapping change since the free {}; \
         frees with a host copy over them {} (alive <5 s {}, 5-60 s {}, longer {})",
        l[0],
        mb(l[1]),
        l[2],
        mb(l[3]),
        l[14],
        l[4],
        mb(l[5]),
        l[6],
        mb(l[7]),
        l[8],
        l[9],
        l[10],
        l[11],
        l[12],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_follow_the_views() {
        let mut k = Kinds::default();
        let key = (1, 7);
        k.counts.insert(key, (0, 1));
        assert_eq!(class_of(&k, &key), 0);
        k.counts.insert(key, (1, 1));
        assert_eq!(class_of(&k, &key), 1);
        k.counts.insert(key, (1, 0));
        assert_eq!(class_of(&k, &key), 1);
        assert_eq!(class_of(&k, &(1, 8)), 2);
        assert!(slab(0x8000) && slab(0x1_8000) && !slab(0x1_0000) && !slab(0x4_0000));
    }
}
