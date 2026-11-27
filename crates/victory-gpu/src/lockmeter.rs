//! (Coder H, ledger F23/F24) The engine's own buffer lock contract, measured,
//! and who writes what the write tracking finds written.
//!
//! The hle side (`victory-hle/src/lockhook.rs`) hooks the inner lock
//! 0x145121690 (every engine lock: the generic lock 0x145121550, the ring
//! reader 0x14524dab0 and 15 direct sites end there, by tail jump, so the
//! return address is the caller's) and the unlock 0x144e8abe0, and tells
//! this module each lock that returned with al = 1 (record R: R+0x30 the
//! object, R+0x38 the subresource, R+0x3c the mode: 1 read, 2 write, 3 both,
//! 4 discard, 5 no-overwrite; R+0 the CPU view, R+0x18 the bytes = the whole
//! buffer, R+8 a texture's row pitch) and each unlock (a copy of R).
//!
//! `VICTORY_LOCK_METER=1` (default off, changes nothing the title or the card
//! sees). Lines a second:
//! - "F23 locks": locks by mode, distinct objects, MB announced for write,
//!   unlocks, unlocks with no lock, windows open now and frame-long ones,
//!   the two exceptions (0x14442f760 unlocks at once and keeps texture-plane
//!   pointers; 0x14173d3c0 never unlocks).
//! - "F23 coverage": every page run the write-watch / guards found written
//!   in a buffer copy's refresh (`Shadow::copy_stale`: pages taken before
//!   and written since, 64 KiB each) is classed page by page, first match:
//!   announced (inside a buffer whose write window was open, or closed
//!   within `VICTORY_LOCK_METER_FRAMES`, default 2); OURS, by path, written
//!   since the page was last taken (asking numbers, exact): the landing of
//!   shader-written bytes (copy-back, also through another view), a list's
//!   copy or a placed copy run on the CPU into title memory (`watch::wrote`),
//!   another copy of ours into title memory (`watch::writing` from inside the
//!   layer); KERNEL for the title (file / DirectStorage reads to addresses,
//!   physical-page writes through a view of ours); MAPPED (the runtime
//!   remapped or committed the page); GPU-ONLY (F24: in the streaming CPU
//!   view with page bit 62 clear, or in the GPU view: the title cannot write
//!   it); arena C (F21); the rest of the graphics heap; TITLE inside a buffer
//!   the lock named for write once but outside a window; TITLE, other.
//! - "F23 title writers": for the two TITLE classes, the bytes by the code
//!   address of a guard fault on that 64 KiB page since its last take (one
//!   sample per page; RVAs at the preferred base), and the bytes with no
//!   fault seen (write-watched only, no writer known).
//! - "F23 unannounced": the top 20 TITLE runs, with the frames they were
//!   found in, the region (streaming CPU view / heap / other), the writer.
//! - "F23 copies": the 8 copies with the most found-written bytes, each with
//!   its split by class (which of them our copy-back writes, which the title).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Mutex, OnceLock};

/// `VICTORY_LOCK_METER=1`.
pub fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("VICTORY_LOCK_METER").is_ok_and(|v| v.trim() == "1"))
}

/// Frames a closed window still counts as announcing (`VICTORY_LOCK_METER_FRAMES`, 0-16, default 2).
fn frames_kept() -> u64 {
    static N: OnceLock<u64> = OnceLock::new();
    *N.get_or_init(|| std::env::var("VICTORY_LOCK_METER_FRAMES").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(2).min(16))
}

/// The exceptions, as caller ranges (preferred addresses).
const UNLOCKS_AT_ONCE: (u64, u64) = (0x1_4442_f760, 0x1_4442_f9a0);
const NEVER_UNLOCKS: (u64, u64) = (0x1_4173_d3c0, 0x1_4173_d460);
const PREFERRED: u64 = 0x1_4000_0000;
const PAGE: usize = crate::watch::PAGE;

/// Since the line: locks by mode (0: other, 1-5), failed locks, bytes
/// announced for write, unlocks, unlocks matching no lock, windows closed in
/// a later frame than they opened, locks from 0x14442f760, from 0x14173d3c0,
/// texture locks (a row pitch), locks with no view or no bytes.
static COUNTS: [AtomicU64; 15] = [const { AtomicU64::new(0) }; 15];
const FAILED: usize = 6;
const WRITE_BYTES: usize = 7;
const UNLOCKS: usize = 8;
const UNMATCHED: usize = 9;
const FRAME_LONG: usize = 10;
const AT_ONCE: usize = 11;
const NEVER: usize = 12;
const TEXTURES: usize = 13;
const EMPTY: usize = 14;

/// The coverage classes, in the order a page is tried.
const ANNOUNCED: usize = 0;
/// Our write paths (`Source` 0..3) are classes 1..4, the kernel's 4..6, mapped 6.
const OURS: usize = 1;
const ALIAS: usize = 7;
const GPU_ONLY: usize = 8;
const ARENA: usize = 9;
const HEAP_REST: usize = 10;
const TITLE_LOCKED: usize = 11;
const TITLE: usize = 12;
const CLASSES: usize = 13;
const CLASS_NAMES: [&str; CLASSES] = [
    "announced by a lock",
    "OURS: landing of shader writes (copy-back)",
    "OURS: list/placed copy run on the CPU",
    "OURS: other copy into title memory",
    "KERNEL: file/DirectStorage read",
    "KERNEL: physical pages through our view",
    "MAPPED/remapped by the runtime",
    "VIA ANOTHER VIEW: a write of ours/the kernel's noted at another address of the same pages",
    "GPU-ONLY page (F24) with no writer known",
    "arena C",
    "rest of graphics heap",
    "TITLE inside a write-locked buffer, outside a window",
    "TITLE, not announced",
];

/// Who wrote title memory, as told to [`note`].
#[derive(Clone, Copy)]
pub(crate) enum Source {
    Landing = 0,
    ListCopy = 1,
    OurCopy = 2,
    KernelRead = 3,
    KernelPages = 4,
    Mapped = 5,
}
const SOURCES: usize = 6;

/// Coverage since the line, bytes by class; then refreshed whole with no
/// write-watch; pages taken for the first time.
static CLASS: [AtomicU64; CLASSES + 2] = [const { AtomicU64::new(0) }; CLASSES + 2];
const WHOLE: usize = CLASSES;
const FIRST: usize = CLASSES + 1;

/// The graphics heap as the title's globals give it: reservation start,
/// C start, C bytes, D start, D bytes, E start, E bytes.
static HEAP: [AtomicU64; 7] = [const { AtomicU64::new(0) }; 7];
/// (F3/F24) The streaming region: CPU view base, bytes, GPU view base, and
/// the page-state table's address ([0x1471bfb08]).
static VIEWS: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
/// The title image's base (for writer RVAs).
static IMAGE: AtomicU64 = AtomicU64::new(0);

struct Window {
    ptr: u64,
    bytes: u64,
    write: bool,
    frame: u64,
}

struct Region {
    end: u64,
    object: u64,
    open: u32,
    /// The frame of the last write lock or unlock.
    last: u64,
}

struct Unannounced {
    from: usize,
    to: usize,
    bytes: u64,
    times: u64,
    frames: u64,
    last_frame: u64,
    copy: u64,
    copy_start: u64,
    copy_len: u64,
    kind: &'static str,
    lock_known: bool,
    writer: u64,
    /// TITLE or GPU_ONLY or ALIAS.
    class: usize,
    /// Other addresses of the run's first page (other views of its pages).
    views: Vec<usize>,
}

#[derive(Default)]
struct Copy {
    start: u64,
    len: u64,
    kind: &'static str,
    refreshes: u64,
    bytes: [u64; CLASSES],
}

#[derive(Default)]
struct State {
    /// Windows open, by object.
    open: HashMap<u64, Vec<Window>>,
    /// Buffers locked for write: by CPU view.
    regions: BTreeMap<u64, Region>,
    longest: u64,
    /// Objects locked since the line.
    objects: HashSet<u64>,
    /// Objects ever locked for write, and those with a write found outside
    /// a window of theirs.
    write_objects: HashSet<u64>,
    fallen: HashSet<u64>,
    /// TITLE runs since the line, by copy and run start.
    unannounced: HashMap<(u64, usize, usize), Unannounced>,
    /// Per copy since the line.
    copies: HashMap<u64, Copy>,
    /// TITLE bytes by writer RVA since the line (0: no fault seen).
    writers: HashMap<u64, u64>,
}

fn state() -> std::sync::MutexGuard<'static, State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner())
}

/// Per 64 KiB page: the asking number current at the last write of each
/// [`Source`] (0: none), and the frame of the last note.
type Notes = HashMap<usize, ([u64; SOURCES], u64)>;
fn notes() -> std::sync::MutexGuard<'static, Notes> {
    static N: OnceLock<Mutex<Notes>> = OnceLock::new();
    N.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner())
}

/// Per 64 KiB page: the last guard fault's code address and the asking
/// number then.
type Faults = HashMap<usize, (u64, u64)>;
fn faults() -> &'static Mutex<Faults> {
    static F: OnceLock<Mutex<Faults>> = OnceLock::new();
    F.get_or_init(Default::default)
}

fn now() -> u64 {
    crate::frame::current()
}

/// The hle side: a lock returned. `ok`: al = 1 (the record's view fields
/// are meaningful only then). `caller`: the return address (preferred base).
#[allow(clippy::too_many_arguments)]
pub fn locked(object: u64, sub: u32, mode: u8, caller: u64, ok: bool, ptr: u64, pitch: u64, bytes: u64) {
    let _ = sub;
    crate::knownwrites::locked(object, mode, ok, ptr, bytes);
    if !on() {
        return;
    }
    if !ok {
        COUNTS[FAILED].fetch_add(1, Relaxed);
        return;
    }
    COUNTS[usize::from(if (1..=5).contains(&mode) { mode } else { 0 })].fetch_add(1, Relaxed);
    if (UNLOCKS_AT_ONCE.0..UNLOCKS_AT_ONCE.1).contains(&caller) {
        COUNTS[AT_ONCE].fetch_add(1, Relaxed);
    }
    if (NEVER_UNLOCKS.0..NEVER_UNLOCKS.1).contains(&caller) {
        COUNTS[NEVER].fetch_add(1, Relaxed);
    }
    if pitch != 0 {
        COUNTS[TEXTURES].fetch_add(1, Relaxed);
    }
    if ptr == 0 || bytes == 0 {
        COUNTS[EMPTY].fetch_add(1, Relaxed);
    }
    let write = (2..=5).contains(&mode);
    let frame = now();
    let mut s = state();
    s.objects.insert(object);
    // Every window is kept, read ones too, so that their unlocks match.
    s.open.entry(object).or_default().push(Window { ptr, bytes, write, frame });
    if !write || ptr == 0 || bytes == 0 {
        return;
    }
    COUNTS[WRITE_BYTES].fetch_add(bytes, Relaxed);
    s.write_objects.insert(object);
    let end = ptr + bytes;
    s.longest = s.longest.max(bytes);
    let r = s.regions.entry(ptr).or_insert(Region { end, object, open: 0, last: frame });
    if r.object != object || r.end != end {
        (r.object, r.end, r.open) = (object, end, 0);
    }
    r.open += 1;
    r.last = frame;
}

/// The hle side: the unlock was entered with a copy of a lock record.
pub fn unlocked(object: u64, ptr: u64) {
    crate::knownwrites::unlocked(object, ptr);
    if !on() {
        return;
    }
    COUNTS[UNLOCKS].fetch_add(1, Relaxed);
    let frame = now();
    let mut s = state();
    let Some(windows) = s.open.get_mut(&object) else {
        COUNTS[UNMATCHED].fetch_add(1, Relaxed);
        return;
    };
    let at = windows.iter().position(|w| w.ptr == ptr).unwrap_or(windows.len() - 1);
    let w = windows.swap_remove(at);
    if windows.is_empty() {
        s.open.remove(&object);
    }
    if w.frame < frame {
        COUNTS[FRAME_LONG].fetch_add(1, Relaxed);
    }
    if w.write
        && let Some(r) = s.regions.get_mut(&w.ptr)
    {
        r.open = r.open.saturating_sub(1);
        r.last = frame;
    }
}

/// The hle side, at each line: the graphics heap as the globals give it.
pub fn heap(reservation: u64, c: (u64, u64), d: (u64, u64), e: (u64, u64)) {
    for (i, v) in [reservation, c.0, c.1, d.0, d.1, e.0, e.1].into_iter().enumerate() {
        HEAP[i].store(v, Relaxed);
    }
}

/// The hle side, at each line: the streaming region (F3) and the page-state
/// table (F24), and the image base.
pub fn views(cpu: u64, len: u64, gpu: u64, table: u64, image: u64) {
    for (i, v) in [cpu, len, gpu, table].into_iter().enumerate() {
        VIEWS[i].store(v, Relaxed);
    }
    IMAGE.store(image, Relaxed);
}

/// The heap's hull: from the reservation (or the first segment) to the end
/// of the last segment.
fn heap_hull() -> (u64, u64) {
    let h: [u64; 7] = std::array::from_fn(|i| HEAP[i].load(Relaxed));
    let segs = [(h[1], h[2]), (h[3], h[4]), (h[5], h[6])];
    let starts = segs.iter().filter(|s| s.1 != 0).map(|s| s.0).chain((h[0] != 0).then_some(h[0]));
    let lo = starts.min().unwrap_or(0);
    let hi = segs.iter().filter(|s| s.1 != 0).map(|s| s.0 + s.1).max().unwrap_or(0);
    if hi <= lo || hi - lo > 1 << 30 { (0, 0) } else { (lo, hi) }
}

/// Title memory `[at, at + len)` was written by `source`: its pages are
/// noted with the asking number current now.
pub(crate) fn note(source: Source, at: usize, len: usize) {
    // (Coder J) The known-writes ledger is told of every write of ours / the kernel's / every mapping change.
    crate::knownwrites::note(source as u8, at, len);
    if !on() || len == 0 {
        return;
    }
    if matches!(source, Source::KernelRead | Source::KernelPages | Source::Mapped) {
        release(at, len);
    }
    let asking = crate::watch::asking_now().max(1);
    let frame = now();
    let mut n = notes();
    for page in at / PAGE..at.saturating_add(len).div_ceil(PAGE) {
        let e = n.entry(page).or_insert(([0; SOURCES], frame));
        e.0[source as usize] = asking;
        e.1 = frame;
    }
}

/// (`lib.rs` exports it as `kernel_will_write`) The runtime is about to have
/// the kernel write title memory for the title: noted, then as before.
pub fn kernel_will_write(at: usize, len: usize) {
    note(Source::KernelRead, at, len);
    crate::watch::writing(at, len);
}

/// The guard handler opened the block of `at` for a write at code `rip`
/// (a title thread). Never waits.
pub(crate) fn fault(at: usize, rip: usize) {
    if !on() && !crate::knownwrites::on() {
        return;
    }
    if let Ok(mut f) = faults().try_lock()
        && (f.len() < 1 << 20 || f.contains_key(&(at / PAGE)))
    {
        f.insert(at / PAGE, (rip as u64, crate::watch::asking_now()));
    }
}

/// Bytes of `pieces` inside the union of `ivs`; `pieces` become the parts
/// outside.
fn cut(pieces: &mut Vec<(u64, u64)>, mut ivs: Vec<(u64, u64)>) -> u64 {
    if ivs.is_empty() || pieces.is_empty() {
        return 0;
    }
    ivs.sort_unstable();
    let mut merged: Vec<(u64, u64)> = Vec::with_capacity(ivs.len());
    for (a, b) in ivs {
        match merged.last_mut() {
            Some(m) if a <= m.1 => m.1 = m.1.max(b),
            _ => merged.push((a, b)),
        }
    }
    let mut inside = 0;
    let mut out = Vec::new();
    for &(a, b) in pieces.iter() {
        let mut at = a;
        for &(x, y) in &merged {
            if y <= at || x >= b {
                continue;
            }
            if x > at {
                out.push((at, x));
            }
            let (lo, hi) = (x.max(at), y.min(b));
            inside += hi - lo;
            at = hi;
            if at >= b {
                break;
            }
        }
        if at < b {
            out.push((at, b));
        }
    }
    *pieces = out;
    inside
}

/// The intervals of `map` (start -> end in the value) overlapping `[a, b)`
/// that `keep` keeps.
fn overlapping<V>(map: &BTreeMap<u64, V>, longest: u64, a: u64, b: u64, end: impl Fn(&V) -> u64, mut keep: impl FnMut(&V) -> bool) -> Vec<(u64, u64)> {
    map.range(a.saturating_sub(longest)..b).filter(|(_, v)| end(v) > a && keep(v)).map(|(&k, v)| (k, end(v))).collect()
}

/// The kind of host copy, for the lists.
pub(crate) fn kind(window: bool, mapped: bool, card: bool) -> &'static str {
    match (window, mapped, card) {
        (true, true, _) => "window (mapped card memory)",
        (true, false, _) => "window",
        (false, _, true) => "buffer copy (card)",
        (false, _, false) => "buffer copy",
    }
}

/// Where `address` lies: the streaming region's CPU view (with its F24 page
/// bits), its GPU view, the graphics heap, elsewhere.
#[derive(Clone, Copy, PartialEq)]
enum Place {
    /// Page bits 62 (CPU-mapped) and 63 (GPU-mapped); `None`: not readable.
    CpuView(Option<(bool, bool)>),
    GpuView,
    Heap,
    Other,
}

fn place(address: u64) -> Place {
    let [cpu, len, gpu, table] = [0, 1, 2, 3].map(|i| VIEWS[i].load(Relaxed));
    if len != 0 && address >= cpu && address < cpu + len {
        let page = (address - cpu) >> 16;
        let mut b = [0u8; 8];
        let bits = (table != 0 && crate::host::read_into((table + page * 8) as *const u8, &mut b)).then(|| {
            let v = u64::from_le_bytes(b);
            (v >> 62 & 1 != 0, v >> 63 != 0)
        });
        return Place::CpuView(bits);
    }
    if len != 0 && address >= gpu && address < gpu + len {
        return Place::GpuView;
    }
    let (h0, h1) = heap_hull();
    if address >= h0 && address < h1 {
        return Place::Heap;
    }
    Place::Other
}

/// (Coder J, `VICTORY_KNOWN_WRITES`) Where title address `address` lies, for
/// the ledger: 0 arena C, 1 the rest of the graphics heap, 2 a GPU-only page
/// (F24) or the GPU view, 3 elsewhere; with a name.
pub(crate) fn region_of(address: u64) -> (u8, &'static str) {
    let (c0, c1) = (HEAP[1].load(Relaxed), HEAP[2].load(Relaxed));
    if c1 != 0 && address >= c0 && address < c0 + c1 {
        return (0, "arena C");
    }
    let (h0, h1) = heap_hull();
    if address >= h0 && address < h1 {
        return (1, "rest of graphics heap");
    }
    match place(address) {
        p @ (Place::CpuView(Some((false, _))) | Place::GpuView) => (2, place_name(p)),
        p => (3, place_name(p)),
    }
}

/// (Coder J) Arena C (start, bytes) and the heap's hull (from, to), with no
/// read of the page bits.
pub(crate) fn heap_ranges() -> ((u64, u64), (u64, u64)) {
    ((HEAP[1].load(Relaxed), HEAP[2].load(Relaxed)), heap_hull())
}

/// (Coder J) The code address (image addresses at 0x140000000) of the last
/// guard fault on 64 KiB page `page` at or after asking `since`; 0: none seen.
pub(crate) fn writer_of(page: usize, since: u64) -> u64 {
    let f = faults().lock().unwrap_or_else(|e| e.into_inner());
    let writer = f.get(&page).filter(|w| w.1 >= since).map_or(0, |w| w.0);
    let image = IMAGE.load(Relaxed);
    if writer >= image && writer < image + 0x1000_0000 { writer - image + PREFERRED } else { writer }
}

/// (Coder J) A name for a writer from [`writer_of`]: the title's code by its
/// address at the preferred base, other code by module and symbol.
pub(crate) fn writer_name(address: u64) -> String {
    if address == 0 {
        return "not seen".to_string();
    }
    if (PREFERRED..PREFERRED + 0x1000_0000).contains(&address) {
        return format!("title {address:#x}");
    }
    static SYMBOLS: Mutex<Option<Option<Symbols>>> = Mutex::new(None);
    let mut symbols = SYMBOLS.lock().unwrap_or_else(|e| e.into_inner());
    let symbols = symbols.get_or_insert_with(Symbols::load);
    format!("{address:#x} = {}", code_name(symbols, address))
}

fn place_name(p: Place) -> &'static str {
    match p {
        Place::CpuView(Some((true, _))) => "streaming CPU view, CPU-mapped",
        Place::CpuView(Some((false, true))) => "streaming CPU view, GPU-only page",
        Place::CpuView(Some((false, false))) => "streaming CPU view, page unmapped",
        Place::CpuView(None) => "streaming CPU view, page bits unreadable",
        Place::GpuView => "streaming GPU view",
        Place::Heap => "graphics heap",
        Place::Other => "other memory",
    }
}

/// Bytes left to compare in this second ([`compared`]).
static COMPARE_LEFT: AtomicU64 = AtomicU64::new(16 << 20);
/// Since the line, by class (ALIAS, GPU_ONLY, TITLE_LOCKED, TITLE as 0..4):
/// bytes compared with the copy's bytes from its last take, bytes in dwords
/// that differ, bytes of pieces found wholly unchanged.
static COMPARED: [[AtomicU64; 3]; 4] = [const { [const { AtomicU64::new(0) }; 3] }; 4];
/// Bytes of mapped windows left uncompared (card memory the CPU reads slowly).
static NOT_COMPARED: AtomicU64 = AtomicU64::new(0);

fn compare_slot(class: usize) -> Option<usize> {
    match class {
        ALIAS => Some(0),
        GPU_ONLY => Some(1),
        TITLE_LOCKED => Some(2),
        TITLE => Some(3),
        _ => None,
    }
}

/// The caller compared `bytes` of title memory (a piece [`found`] returned)
/// with its copy's bytes from the last take: `changed` bytes lie in dwords
/// that differ.
pub(crate) fn compared(class: usize, bytes: u64, changed: u64) {
    let Some(k) = compare_slot(class) else { return };
    COMPARED[k][0].fetch_add(bytes, Relaxed);
    COMPARED[k][1].fetch_add(changed, Relaxed);
    if changed == 0 {
        COMPARED[k][2].fetch_add(bytes, Relaxed);
    }
}

/// A mapped window's pieces were left uncompared.
pub(crate) fn not_compared(bytes: u64) {
    NOT_COMPARED.fetch_add(bytes, Relaxed);
}

/// Bytes in the dwords of `a` and `b` that differ.
pub(crate) fn changed_bytes(a: &[u8], b: &[u8]) -> u64 {
    let n = a.len().min(b.len()) & !3;
    let (x, y) = (a[..n].as_chunks::<4>().0, b[..n].as_chunks::<4>().0);
    x.iter().zip(y).filter(|(p, q)| p != q).count() as u64 * 4
}

/// A buffer copy's refresh found pages written (`runs`: from, to, taken
/// before; `taken`: per page taken before, the asking it was last taken
/// at). Classed as the module says. Returns the pieces of the classes with
/// no writer of ours known (from, to, class), to be compared with the copy
/// ([`compared`]), within a budget of 16 MB a second.
pub(crate) fn found(
    copy: u64,
    copy_start: u64,
    copy_len: u64,
    kind: &'static str,
    runs: &[(usize, usize, bool)],
    taken: &[(usize, u64)],
) -> Vec<(usize, usize, usize)> {
    let mut check = Vec::new();
    if !on() {
        return check;
    }
    let frame = now();
    let kept = frames_kept();
    let recent = |last: u64| frame.saturating_sub(last) <= kept;
    let (c0, c1) = (HEAP[1].load(Relaxed), HEAP[2].load(Relaxed));
    let (h0, h1) = heap_hull();
    let taken_at = |page: usize| taken.binary_search_by_key(&page, |t| t.0).map_or(0, |i| taken[i].1);
    // The other addresses of the runs' pages (asked before any lock of ours
    // is taken: the views' lock is held by writers that note to us).
    let mut others: HashMap<usize, Vec<usize>> = HashMap::new();
    for &(from, to, known) in runs {
        if known && from < to {
            for (page, at) in crate::inplace::shown_elsewhere(from & !(PAGE - 1), to) {
                others.insert(page / PAGE, at);
            }
        }
    }
    let mut sums = [0u64; CLASSES];
    let mut first = 0u64;
    // Pieces with no writer of ours known: (run from, run to, page, class, piece from, piece to).
    let mut nobody: Vec<(usize, usize, usize, usize, u64, u64)> = Vec::new();
    {
        let mut s = state();
        let n = notes();
        for &(from, to, known) in runs {
            if from >= to {
                continue;
            }
            if !known {
                first += (to - from) as u64;
                continue;
            }
            for page in from / PAGE..to.div_ceil(PAGE) {
                let (a, b) = ((page * PAGE).max(from) as u64, ((page + 1) * PAGE).min(to) as u64);
                let mut pieces = vec![(a, b)];
                let announced = overlapping(&s.regions, s.longest, a, b, |r| r.end, |r| r.open > 0 || recent(r.last));
                sums[ANNOUNCED] += cut(&mut pieces, announced);
                if pieces.is_empty() {
                    continue;
                }
                let left = |p: &Vec<(u64, u64)>| p.iter().map(|x| x.1 - x.0).sum::<u64>();
                // Written by a source since the page was last taken ...
                let t = taken_at(page);
                let by = |p: usize| n.get(&p).and_then(|&(stamps, _)| (0..SOURCES).find(|&k| stamps[k] != 0 && stamps[k] >= t));
                if let Some(src) = by(page) {
                    sums[OURS + src] += left(&pieces);
                    continue;
                }
                // ... or at another address of the same pages.
                let mut put = |class: usize, pieces: &Vec<(u64, u64)>, sums: &mut [u64; CLASSES]| {
                    sums[class] += left(pieces);
                    nobody.extend(pieces.iter().map(|&(x, y)| (from, to, page, class, x, y)));
                };
                if others.get(&page).is_some_and(|at| at.iter().any(|&o| by(o / PAGE).is_some())) {
                    put(ALIAS, &pieces, &mut sums);
                    continue;
                }
                if let Place::CpuView(Some((false, _))) | Place::GpuView = place(a) {
                    put(GPU_ONLY, &pieces, &mut sums);
                    continue;
                }
                if c1 != 0 {
                    sums[ARENA] += cut(&mut pieces, vec![(c0, c0 + c1)]);
                }
                if h1 > h0 {
                    sums[HEAP_REST] += cut(&mut pieces, vec![(h0, h1)]);
                }
                if pieces.is_empty() {
                    continue;
                }
                let mut objects = Vec::new();
                let known_regions = overlapping(&s.regions, s.longest, a, b, |r| r.end, |r| {
                    let stale = r.open == 0 && !recent(r.last);
                    if stale {
                        objects.push(r.object);
                    }
                    stale
                });
                let mut inside = pieces.clone();
                let stale = cut(&mut pieces, known_regions);
                if stale != 0 {
                    s.fallen.extend(objects);
                    // The parts inside: what `pieces` no longer holds.
                    cut(&mut inside, pieces.clone());
                    put(TITLE_LOCKED, &inside, &mut sums);
                }
                if left(&pieces) != 0 {
                    put(TITLE, &pieces, &mut sums);
                }
            }
        }
        drop(n);
        // The writers of those pages: a guard fault there since the take.
        let f = faults().lock().unwrap_or_else(|e| e.into_inner());
        let image = IMAGE.load(Relaxed);
        // (run from, run to, group, bytes, writer, inside a locked buffer)
        let mut by_run: Vec<(usize, usize, usize, u64, u64, bool)> = Vec::new();
        for &(from, to, page, class, x, y) in &nobody {
            let bytes = y - x;
            let writer = f.get(&page).filter(|w| w.1 >= taken_at(page)).map_or(0, |w| w.0);
            let rva = if writer >= image && writer < image + 0x1000_0000 { writer - image + PREFERRED } else { writer };
            if class == TITLE || class == TITLE_LOCKED {
                *s.writers.entry(rva).or_insert(0) += bytes;
            }
            let group = if class == TITLE_LOCKED { TITLE } else { class };
            match by_run.last_mut() {
                Some(r) if r.0 == from && r.2 == group => {
                    r.3 += bytes;
                    if r.4 == 0 {
                        r.4 = rva;
                    }
                    r.5 |= class == TITLE_LOCKED;
                }
                _ => by_run.push((from, to, group, bytes, rva, class == TITLE_LOCKED)),
            }
            if COMPARE_LEFT.load(Relaxed) >= bytes {
                COMPARE_LEFT.fetch_sub(bytes, Relaxed);
                check.push((x as usize, y as usize, class));
            }
        }
        drop(f);
        for (from, to, group, bytes, writer, locked) in by_run {
            let key = (copy, from, group);
            if s.unannounced.len() >= 1 << 16 && !s.unannounced.contains_key(&key) {
                continue;
            }
            let views = others.get(&(from / PAGE)).map(|v| v.iter().take(3).copied().collect()).unwrap_or_default();
            let u = s.unannounced.entry(key).or_insert(Unannounced {
                from,
                to,
                bytes: 0,
                times: 0,
                frames: 0,
                last_frame: u64::MAX,
                copy,
                copy_start,
                copy_len,
                kind,
                lock_known: false,
                writer: 0,
                class: group,
                views,
            });
            u.to = u.to.max(to);
            u.bytes += bytes;
            u.times += 1;
            if u.last_frame != frame {
                u.frames += 1;
                u.last_frame = frame;
            }
            u.lock_known |= locked;
            if u.writer == 0 {
                u.writer = writer;
            }
        }
        if sums.iter().any(|&v| v != 0) && (s.copies.len() < 1 << 16 || s.copies.contains_key(&copy)) {
            let c = s.copies.entry(copy).or_default();
            (c.start, c.len, c.kind) = (copy_start, copy_len, kind);
            c.refreshes += 1;
            for (k, v) in sums.iter().enumerate() {
                c.bytes[k] += v;
            }
        }
    }
    CLASS[FIRST].fetch_add(first, Relaxed);
    for (i, v) in sums.into_iter().enumerate() {
        if v != 0 {
            CLASS[i].fetch_add(v, Relaxed);
        }
    }
    check
}

/// A copy's refresh took `[from, to)` whole: no write-watch there.
pub(crate) fn whole(from: usize, to: usize) {
    if on() && to > from {
        CLASS[WHOLE].fetch_add((to - from) as u64, Relaxed);
    }
}

/// The lines (the hle side's meter thread, once a second); `frames` since the last.
pub fn line(frames: f64) {
    let frames = frames.max(1.0);
    let c: Vec<u64> = COUNTS.iter().map(|v| v.swap(0, Relaxed)).collect();
    let k: Vec<u64> = CLASS.iter().map(|v| v.swap(0, Relaxed)).collect();
    let frame = now();
    let mut s = state();
    let distinct = s.objects.len();
    s.objects.clear();
    let open: usize = s.open.values().map(Vec::len).sum();
    let open_old: usize = s.open.values().flatten().filter(|w| w.frame < frame).count();
    let open_mb: f64 = s.open.values().flatten().filter(|w| w.write).map(|w| w.bytes).sum::<u64>() as f64 / 1048576.0;
    let (write_objects, fallen) = (s.write_objects.len(), s.fallen.len());
    if s.regions.len() > 1 << 18 {
        s.regions.retain(|_, r| r.open > 0 || frame.saturating_sub(r.last) < 600);
        s.longest = s.regions.iter().map(|(k, r)| r.end - k).max().unwrap_or(0);
    }
    let mut top: Vec<Unannounced> = s.unannounced.drain().map(|(_, u)| u).collect();
    let mut copies: Vec<(u64, Copy)> = s.copies.drain().collect();
    let mut writers: Vec<(u64, u64)> = s.writers.drain().collect();
    let regions = s.regions.len();
    drop(s);
    {
        let mut n = notes();
        if n.len() > 1 << 20 {
            n.retain(|_, v| frame.saturating_sub(v.1) < 1800);
        }
    }
    let mb = |v: u64| v as f64 / 1048576.0 / frames;
    tracing::info!(
        "F23 locks per frame over {frames:.0} frames (VICTORY_LOCK_METER=1): read {:.1}, write {:.1}, both {:.1}, discard {:.1}, no-overwrite {:.1}, \
         other mode {:.1}, failed {:.1}; textures {:.1}, no view {:.1}; distinct objects {distinct} in the second; announced for write {:.2} MB/frame; \
         unlocks {:.1}, unlocks with no lock {:.1}; windows open now {open} ({open_old} from an earlier frame, {open_mb:.1} MB for write), windows closed a \
         frame or more after they opened {:.1}; exceptions: 0x14442f760 (unlocks at once) {:.1}, 0x14173d3c0 (never unlocks) {:.1}; buffers known \
         {regions}, objects locked for write {write_objects}, of them written outside a window {fallen}",
        c[1] as f64 / frames,
        c[2] as f64 / frames,
        c[3] as f64 / frames,
        c[4] as f64 / frames,
        c[5] as f64 / frames,
        c[0] as f64 / frames,
        c[FAILED] as f64 / frames,
        c[TEXTURES] as f64 / frames,
        c[EMPTY] as f64 / frames,
        mb(c[WRITE_BYTES]),
        c[UNLOCKS] as f64 / frames,
        c[UNMATCHED] as f64 / frames,
        c[FRAME_LONG] as f64 / frames,
        c[AT_ONCE] as f64 / frames,
        c[NEVER] as f64 / frames,
    );
    let found: u64 = k[..CLASSES].iter().sum();
    let pct = |v: u64| if found == 0 { 0.0 } else { v as f64 * 100.0 / found as f64 };
    let ours: u64 = k[OURS..OURS + 3].iter().sum();
    let kernel: u64 = k[OURS + 3..OURS + 6].iter().sum();
    let title: u64 = k[ARENA..CLASSES].iter().sum();
    let classes: Vec<String> = (0..CLASSES).map(|i| format!("{} {:.2} MB {:.1}%", CLASS_NAMES[i], mb(k[i]), pct(k[i]))).collect();
    let (h0, h1) = heap_hull();
    tracing::info!(
        "F23 coverage per frame (VICTORY_LOCK_METER=1, windows kept {} frames): pages found written in buffer copies' refreshes {:.2} MB = \
         announced {:.1}% / OURS {:.1}% / KERNEL+MAPPED {:.1}% / GPU-ONLY {:.1}% / TITLE (arena, heap, other) {:.1}%; by class: {}; besides: refreshed whole \
         with no write-watch {:.2} MB, taken for the first time {:.2} MB; heap {h0:#x}..{h1:#x}, arena C {:#x} + {} MB, streaming CPU view {:#x} + {} GB, \
         GPU view {:#x}, page table {:#x}",
        frames_kept(),
        mb(found),
        pct(k[ANNOUNCED]),
        pct(ours),
        pct(kernel),
        pct(k[GPU_ONLY]),
        pct(title),
        classes.join(", "),
        mb(k[WHOLE]),
        mb(k[FIRST]),
        HEAP[1].load(Relaxed),
        HEAP[2].load(Relaxed) >> 20,
        VIEWS[0].load(Relaxed),
        VIEWS[1].load(Relaxed) >> 30,
        VIEWS[2].load(Relaxed),
        VIEWS[3].load(Relaxed),
    );
    if !writers.is_empty() {
        writers.sort_unstable_by(|a, b| b.1.cmp(&a.1));
        let unknown = writers.iter().find(|w| w.0 == 0).map_or(0, |w| w.1);
        let all: u64 = writers.iter().map(|w| w.1).sum();
        let list: Vec<String> =
            writers.iter().filter(|w| w.0 != 0).take(20).map(|w| format!("{:#x} {:.3} MB", w.0, mb(w.1))).collect();
        tracing::info!(
            "F23 title writers per frame (VICTORY_LOCK_METER=1): TITLE pages {:.2} MB, with no guard fault seen since their take (write-watched only) \
             {:.2} MB; by the code address of the last guard fault on the page ({} addresses, image addresses at 0x140000000): {}",
            mb(all),
            mb(unknown),
            writers.len() - usize::from(unknown != 0),
            list.join(", ")
        );
    }
    {
        let k: Vec<[u64; 3]> = COMPARED.iter().map(|c| [0, 1, 2].map(|i| c[i].swap(0, Relaxed))).collect();
        let names = ["VIA ANOTHER VIEW", "GPU-ONLY", "TITLE inside a locked buffer", "TITLE not announced"];
        let parts: Vec<String> = (0..4)
            .map(|i| {
                format!(
                    "{} compared {:.2} MB: bytes in changed dwords {:.2} MB ({:.1}%), pieces wholly unchanged {:.2} MB",
                    names[i],
                    k[i][0] as f64 / 1048576.0,
                    k[i][1] as f64 / 1048576.0,
                    if k[i][0] == 0 { 0.0 } else { k[i][1] as f64 * 100.0 / k[i][0] as f64 },
                    k[i][2] as f64 / 1048576.0
                )
            })
            .collect();
        tracing::info!(
            "F23 real changes in the last second (VICTORY_LOCK_METER=1; title bytes against the copy's bytes from the last take, at most 16 MB a \
             second; not compared: {:.2} MB of mapped card windows): {}",
            NOT_COMPARED.swap(0, Relaxed) as f64 / 1048576.0,
            parts.join("; ")
        );
        COMPARE_LEFT.store(16 << 20, Relaxed);
    }
    if !top.is_empty() {
        top.sort_unstable_by(|a, b| b.bytes.cmp(&a.bytes));
        let describe = |u: &Unannounced| {
            let views: Vec<String> = u.views.iter().map(|&v| format!("{v:#x} ({})", place_name(place(v as u64)))).collect();
            format!(
                "{:.3} MB/frame, {} refreshes in {} frames, run {:#x}..{:#x} ({}{}) of {} #{} {:#x}+{} KiB, writer {}{}",
                u.bytes as f64 / 1048576.0 / frames,
                u.times,
                u.frames,
                u.from,
                u.to,
                place_name(place(u.from as u64)),
                if views.is_empty() { String::new() } else { format!("; its first page shown also at {}", views.join(", ")) },
                u.kind,
                u.copy,
                u.copy_start,
                u.copy_len >> 10,
                if u.writer == 0 { "not seen".to_string() } else { format!("{:#x}", u.writer) },
                if u.lock_known { " (inside a buffer locked for write)" } else { "" }
            )
        };
        let mut targets = Vec::new();
        for (group, name) in [(TITLE, "TITLE"), (GPU_ONLY, "GPU-ONLY"), (ALIAS, "VIA ANOTHER VIEW")] {
            let of: Vec<&Unannounced> = top.iter().filter(|u| u.class == group).collect();
            if of.is_empty() {
                continue;
            }
            let list: Vec<String> = of.iter().take(if group == TITLE { 20 } else { 10 }).map(|u| describe(u)).collect();
            tracing::info!("F23 unannounced, top {} of {} {name} runs (VICTORY_LOCK_METER=1): {}", list.len(), of.len(), list.join("; "));
            if group != ALIAS {
                targets.extend(of.iter().take(5).map(|u| (u.from, u.to.min(u.from + (4 << 20)), group, u.copy)));
            }
        }
        *TARGETS.lock().unwrap_or_else(|e| e.into_inner()) = targets;
    }
    if !copies.is_empty() {
        copies.sort_unstable_by_key(|c| std::cmp::Reverse(c.1.bytes.iter().sum::<u64>()));
        let short = ["lock", "landing", "list copy", "our copy", "kernel read", "kernel pages", "mapped", "other view", "GPU-only", "arena", "heap", "title locked", "title"];
        let list: Vec<String> = copies
            .iter()
            .take(8)
            .map(|(id, c)| {
                let split: Vec<String> =
                    (0..CLASSES).filter(|&i| c.bytes[i] != 0).map(|i| format!("{} {:.2}", short[i], mb(c.bytes[i]))).collect();
                format!(
                    "{} #{id} {:#x}+{} KiB ({}): {} refreshes, MB/frame {}",
                    c.kind,
                    c.start,
                    c.len >> 10,
                    place_name(place(c.start)),
                    c.refreshes,
                    split.join(" ")
                )
            })
            .collect();
        tracing::info!("F23 copies with the most pages found written, of {} (VICTORY_LOCK_METER=1): {}", copies.len(), list.join("; "));
    }
}

// --- Writer probes (`VICTORY_LOCK_METER=1`; `VICTORY_LOCK_PROBE=0` turns them off) ---
//
// Once a second the top 5 TITLE and the top 5 GPU-ONLY runs of the last
// second (at most 4 MB each) are made read-only for one frame, 64 KiB page by
// page, where they are plain read-write memory the guards do not hold
// (`watch::probe_protect`, under the guards' lock). A write to one faults:
// the guard handler first asks [`probe_hit`], which records the thread and
// the code (unwound, 8 frames), makes the page writable again, and lets the
// handler go on as before (so a page the guards took meanwhile is still
// theirs). Kernel writes for the title (file and DirectStorage reads, which
// would fail on a read-only page rather than fault) and the runtime's
// remapping let go of the pages under them first ([`note`]). After the frame
// the rest are made writable again (only those the guards do not hold).

/// The runs to probe next (from, to, class, copy).
static TARGETS: Mutex<Vec<(usize, usize, usize, u64)>> = Mutex::new(Vec::new());
/// Pages armed now (64 KiB page numbers, sorted), each with whether it is
/// still read-only by our hand.
static ARMED: std::sync::RwLock<Vec<(usize, std::sync::atomic::AtomicBool)>> = std::sync::RwLock::new(Vec::new());
static ARMED_ANY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

const DEPTH: usize = 8;

struct Hit {
    page: usize,
    thread: u32,
    frames: [u64; DEPTH],
}

static HITS: Mutex<Vec<Hit>> = Mutex::new(Vec::new());
/// Pages released early for a kernel write or a remapping, since the line.
static RELEASED: AtomicU64 = AtomicU64::new(0);

fn probes_on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| on() && std::env::var("VICTORY_LOCK_PROBE").map_or(true, |v| v.trim() != "0"))
}

/// The CONTEXT at the 16-byte alignment the unwind functions need.
#[repr(C, align(16))]
struct Aligned16(windows_sys::Win32::System::Diagnostics::Debug::CONTEXT);

/// The guard handler: a write fault at `at`. If `at` is in a page we armed,
/// the writer is recorded and the page made writable again. Returns nothing:
/// the handler goes on as it would (the write is retried once the page is
/// writable). Never waits for a lock.
pub(crate) fn probe_hit(at: usize, context: *const windows_sys::Win32::System::Diagnostics::Debug::CONTEXT) {
    use std::sync::atomic::Ordering;
    use windows_sys::Win32::System::Diagnostics::Debug::{RtlLookupFunctionEntry, RtlVirtualUnwind, UNW_FLAG_NHANDLER};
    use windows_sys::Win32::System::Memory::{PAGE_READWRITE, VirtualProtect};
    use windows_sys::Win32::System::Threading::{GetCurrentThreadId, GetCurrentThreadStackLimits};
    if !ARMED_ANY.load(Ordering::Acquire) {
        return;
    }
    let page = at / PAGE;
    let Ok(armed) = ARMED.try_read() else { return };
    let Ok(i) = armed.binary_search_by_key(&page, |a| a.0) else { return };
    // Writable again in any case (a page let go a moment ago may still be
    // read-only), unless the guards hold it now: then their handling follows.
    let first = armed[i].1.swap(false, Ordering::AcqRel);
    if !crate::watch::guarded_block(page) {
        let mut old = 0;
        // SAFETY: a page we made read-only; only its protection changes back.
        unsafe { VirtualProtect((page * PAGE) as *const _, PAGE, PAGE_READWRITE, &mut old) };
    }
    drop(armed);
    if !first {
        return;
    }
    let mut frames = [0u64; DEPTH];
    // SAFETY: the context the system passes, copied; the unwind reads only
    // this thread's stack within its limits.
    unsafe {
        let mut c = Aligned16(*context);
        let (mut low, mut high) = (0usize, 0usize);
        GetCurrentThreadStackLimits(&mut low, &mut high);
        frames[0] = c.0.Rip;
        for f in frames.iter_mut().skip(1) {
            let rsp = c.0.Rsp as usize;
            if rsp < low || rsp + 8 > high {
                break;
            }
            let mut image = 0;
            let entry = RtlLookupFunctionEntry(c.0.Rip, &mut image, std::ptr::null_mut());
            if entry.is_null() {
                c.0.Rip = (rsp as *const u64).read();
                c.0.Rsp += 8;
            } else {
                let (mut data, mut frame) = (std::ptr::null_mut(), 0);
                RtlVirtualUnwind(UNW_FLAG_NHANDLER, image, c.0.Rip, entry, &mut c.0, &mut data, &mut frame, std::ptr::null_mut());
            }
            if c.0.Rip == 0 {
                break;
            }
            *f = c.0.Rip;
        }
    }
    // SAFETY: plain query.
    let thread = unsafe { GetCurrentThreadId() };
    if let Ok(mut hits) = HITS.try_lock()
        && hits.len() < hits.capacity()
    {
        hits.push(Hit { page, thread, frames });
    }
}

/// Kernel writes and remappings: the armed pages under `[at, at + len)` are
/// writable again first.
fn release(at: usize, len: usize) {
    use std::sync::atomic::Ordering;
    if !ARMED_ANY.load(Ordering::Acquire) || len == 0 {
        return;
    }
    let (first, last) = (at / PAGE, at.saturating_add(len).div_ceil(PAGE));
    let mut pages = Vec::new();
    {
        let armed = ARMED.read().unwrap_or_else(|e| e.into_inner());
        let from = armed.partition_point(|a| a.0 < first);
        for a in armed[from..].iter().take_while(|a| a.0 < last) {
            if a.1.swap(false, Ordering::AcqRel) {
                pages.push(a.0);
            }
        }
    }
    if !pages.is_empty() {
        RELEASED.fetch_add(pages.len() as u64, Relaxed);
        crate::watch::probe_unprotect(&pages);
    }
}

/// The thread's name, or its id.
fn thread_name(id: u32) -> String {
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree};
    use windows_sys::Win32::System::Threading::{GetThreadDescription, OpenThread, THREAD_QUERY_LIMITED_INFORMATION};
    // SAFETY: a handle opened and closed here; the name is the system's,
    // freed as asked.
    unsafe {
        let thread = OpenThread(THREAD_QUERY_LIMITED_INFORMATION, 0, id);
        if thread.is_null() {
            return format!("thread {id}");
        }
        let mut name = std::ptr::null_mut();
        let mut text = String::new();
        if GetThreadDescription(thread, &mut name) >= 0 && !name.is_null() {
            let len = (0..).take_while(|&i| *name.add(i) != 0).count();
            text = String::from_utf16_lossy(std::slice::from_raw_parts(name, len));
            LocalFree(name.cast());
        }
        CloseHandle(thread);
        if text.is_empty() { format!("thread {id}") } else { format!("{text} ({id})") }
    }
}

/// The system's symbol library, for our own code.
struct Symbols {
    from_address: unsafe extern "system" fn(*mut std::ffi::c_void, u64, *mut u64, *mut u8) -> i32,
    process: *mut std::ffi::c_void,
}

// SAFETY: a function pointer and the process pseudo-handle.
unsafe impl Send for Symbols {}

impl Symbols {
    fn load() -> Option<Symbols> {
        use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExA};
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        type Initialize = unsafe extern "system" fn(*mut std::ffi::c_void, *const u16, i32) -> i32;
        type Options = unsafe extern "system" fn(u32) -> u32;
        // SAFETY: the system's own library and functions, by their names and signatures.
        unsafe {
            let module = LoadLibraryExA(c"dbghelp.dll".as_ptr().cast(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
            if module.is_null() {
                return None;
            }
            let options: Options = std::mem::transmute(GetProcAddress(module, c"SymSetOptions".as_ptr().cast())?);
            let initialize: Initialize = std::mem::transmute(GetProcAddress(module, c"SymInitializeW".as_ptr().cast())?);
            let from_address = std::mem::transmute::<
                unsafe extern "system" fn() -> isize,
                unsafe extern "system" fn(*mut std::ffi::c_void, u64, *mut u64, *mut u8) -> i32,
            >(GetProcAddress(module, c"SymFromAddr".as_ptr().cast())?);
            let process = GetCurrentProcess();
            options(0x2 | 0x4 | 0x200);
            let beside = std::env::current_exe().ok().and_then(|p| Some(p.parent()?.to_path_buf())).unwrap_or_default();
            let beside: Vec<u16> = std::os::windows::ffi::OsStrExt::encode_wide(beside.as_os_str()).chain([0]).collect();
            initialize(process, beside.as_ptr(), 1);
            Some(Symbols { from_address, process })
        }
    }

    fn name(&self, address: u64) -> Option<String> {
        const NAME: usize = 84;
        const ROOM: usize = 512;
        let mut info = [0u64; (NAME + ROOM) / 8 + 1];
        let at = info.as_mut_ptr().cast::<u8>();
        let mut off = 0u64;
        // SAFETY: a buffer of SYMBOL_INFO's size and the name's, its size fields set.
        unsafe {
            at.cast::<u32>().write(88);
            at.add(80).cast::<u32>().write(ROOM as u32);
            if (self.from_address)(self.process, address, &mut off, at) == 0 {
                return None;
            }
            let name = std::ffi::CStr::from_ptr(at.add(NAME).cast());
            Some(format!("{}+{off:#x}", name.to_string_lossy()))
        }
    }
}

/// A name for code address `address`: the title image's by its address at
/// the preferred base, a module's by symbol or module+offset.
fn code_name(symbols: &Option<Symbols>, address: u64) -> String {
    use windows_sys::Win32::System::LibraryLoader::{
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, GetModuleFileNameW, GetModuleHandleExW,
    };
    let image = IMAGE.load(Relaxed);
    if image != 0 && address >= image && address < image + 0x1000_0000 {
        return format!("title {:#x}", address - image + PREFERRED);
    }
    let mut module = std::ptr::null_mut();
    let mut path = [0u16; 260];
    // SAFETY: an address, no name; plain queries.
    let (file, off) = unsafe {
        let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
        if GetModuleHandleExW(flags, address as *const u16, &mut module) == 0 {
            return format!("{address:#x} (no module)");
        }
        let len = GetModuleFileNameW(module, path.as_mut_ptr(), path.len() as u32) as usize;
        let path = String::from_utf16_lossy(&path[..len]);
        (path.rsplit('\\').next().unwrap_or("?").to_string(), address - module as u64)
    };
    match symbols.as_ref().and_then(|s| s.name(address)) {
        Some(name) => format!("{file}!{name}"),
        None => format!("{file}+{off:#x}"),
    }
}

/// Once a second, from the hle side's meter thread after [`line`]: arms the
/// targets for one frame, then says what wrote them ("F23 probe").
pub fn probe_round() {
    use std::sync::atomic::{AtomicBool, Ordering};
    if !probes_on() {
        return;
    }
    let targets = std::mem::take(&mut *TARGETS.lock().unwrap_or_else(|e| e.into_inner()));
    if targets.is_empty() {
        return;
    }
    // The handler that takes the faults (installed with the safe copies, by default).
    crate::watch::install();
    // Pages of the targets, less those the kernel wrote lately (a read may
    // still be under way).
    let frame = now();
    let mut pages: Vec<(usize, usize)> = Vec::new();
    {
        let n = notes();
        for (t, &(from, to, ..)) in targets.iter().enumerate() {
            for page in from / PAGE..to.div_ceil(PAGE) {
                let busy = n.get(&page).is_some_and(|&(stamps, last)| {
                    (stamps[Source::KernelRead as usize] != 0 || stamps[Source::Mapped as usize] != 0) && frame.saturating_sub(last) < 120
                });
                if !busy {
                    pages.push((page, t));
                }
            }
        }
    }
    pages.sort_unstable();
    pages.dedup_by_key(|p| p.0);
    let wanted: Vec<usize> = pages.iter().map(|p| p.0).collect();
    {
        let mut h = HITS.lock().unwrap_or_else(|e| e.into_inner());
        h.clear();
        h.reserve(1024);
    }
    // Known to the handler before any is read-only.
    {
        let mut a = ARMED.write().unwrap_or_else(|e| e.into_inner());
        *a = wanted.iter().map(|&p| (p, AtomicBool::new(true))).collect();
    }
    ARMED_ANY.store(true, Ordering::Release);
    let armed = crate::watch::probe_protect(&wanted);
    {
        let a = ARMED.read().unwrap_or_else(|e| e.into_inner());
        for x in a.iter() {
            if armed.binary_search(&x.0).is_err() {
                x.1.store(false, Ordering::Release);
            }
        }
    }
    let began = std::time::Instant::now();
    // One frame: until the frame number has moved on twice (a whole frame
    // between), at most 200 ms.
    while now() < frame + 2 && began.elapsed() < std::time::Duration::from_millis(200) {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let held_ms = began.elapsed().as_secs_f64() * 1e3;
    let frames_held = now().saturating_sub(frame);
    // Writable again while the handler still knows the pages; forgotten after.
    let rest: Vec<usize> = {
        let a = ARMED.read().unwrap_or_else(|e| e.into_inner());
        a.iter().filter(|x| x.1.swap(false, Ordering::AcqRel)).map(|x| x.0).collect()
    };
    crate::watch::probe_unprotect(&rest);
    ARMED_ANY.store(false, Ordering::Release);
    ARMED.write().unwrap_or_else(|e| e.into_inner()).clear();
    let hits = std::mem::take(&mut *HITS.lock().unwrap_or_else(|e| e.into_inner()));
    // Per target: pages armed, pages that faulted; and the writers.
    static SYMBOLS: Mutex<Option<Option<Symbols>>> = Mutex::new(None);
    let mut symbols = SYMBOLS.lock().unwrap_or_else(|e| e.into_inner());
    let symbols = symbols.get_or_insert_with(Symbols::load);
    let target_of = |page: usize| pages.binary_search_by_key(&page, |p| p.0).ok().map(|i| pages[i].1);
    let mut lines = Vec::new();
    for (t, &(from, to, class, copy)) in targets.iter().enumerate() {
        let armed_here = armed.iter().filter(|&&p| target_of(p) == Some(t)).count();
        let hit: Vec<&Hit> = hits.iter().filter(|h| target_of(h.page) == Some(t)).collect();
        let mut writers: Vec<(String, usize)> = Vec::new();
        for h in &hit {
            let stack: Vec<String> = h.frames.iter().take_while(|&&f| f != 0).take(5).map(|&f| code_name(symbols, f)).collect();
            let w = format!("{} at {}", thread_name(h.thread), stack.join(" < "));
            match writers.iter_mut().find(|x| x.0 == w) {
                Some(x) => x.1 += 1,
                None => writers.push((w, 1)),
            }
        }
        writers.sort_by(|a, b| b.1.cmp(&a.1));
        let ws: Vec<String> = writers.iter().take(3).map(|(w, n)| format!("{n} pages by {w}")).collect();
        lines.push(format!(
            "{} run {from:#x}..{to:#x} of copy #{copy} ({}): armed {armed_here} of {} pages, faulted {}{}",
            if class == GPU_ONLY { "GPU-ONLY" } else { "TITLE" },
            place_name(place(from as u64)),
            (to - from).div_ceil(PAGE),
            hit.len(),
            if ws.is_empty() { String::new() } else { format!(": {}", ws.join("; ")) }
        ));
    }
    tracing::info!(
        "F23 probe (VICTORY_LOCK_METER=1): {} pages read-only for {held_ms:.0} ms ({frames_held} frame numbers), {} faulted, {} let go early for a \
         kernel write or a remapping; no fault on a run found written every frame = not a CPU write in user mode at that address (the card, the \
         kernel, or another view of its pages): {}",
        armed.len(),
        hits.len(),
        RELEASED.swap(0, Relaxed),
        lines.join(" | ")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cut_takes_out_the_union_and_leaves_the_rest() {
        let mut p = vec![(0, 100), (200, 300)];
        let inside = cut(&mut p, vec![(50, 60), (55, 70), (250, 400), (90, 210)]);
        // 50..70 (20), 90..100 (10), 200..210 (10), 250..300 (50)
        assert_eq!(inside, 90);
        assert_eq!(p, vec![(0, 50), (70, 90), (210, 250)]);
        let mut q = vec![(10, 20)];
        assert_eq!(cut(&mut q, vec![]), 0);
        assert_eq!(q, vec![(10, 20)]);
        assert_eq!(cut(&mut q, vec![(0, 30)]), 10);
        assert!(q.is_empty());
    }

    #[test]
    fn overlapping_finds_ranges_that_began_before() {
        let mut m = BTreeMap::new();
        m.insert(0u64, 100u64);
        m.insert(150, 160);
        m.insert(500, 600);
        let got = overlapping(&m, 100, 90, 155, |&e| e, |_| true);
        assert_eq!(got, vec![(0, 100), (150, 160)]);
    }
}
