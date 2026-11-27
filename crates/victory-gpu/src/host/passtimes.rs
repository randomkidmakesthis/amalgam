//! (Fox T) Diagnostics: the card's time per pass, by program name
//! (`VICTORY_PASS_TIMES=1`). A GPU timestamp before and after each title
//! draw, dispatch and indirect execution a host list records, and around
//! the layer's own argument clamps, in a query heap of the list's own; the
//! stamps are resolved at the list's close and read at its reset (when every
//! submission of it has run, so nothing waits for them). Summed by program
//! name (`group:CS_Name_hash`, what the title's containers carry) and by
//! family (the name without its group and its hash suffix: the ~1,000
//! `material:CS_RaytracingResolve_xxxxxxxx` add up to one line), every frame
//! line.
//!
//! What it costs: two timestamps per command (on this card a timestamp
//! waits for the work before it, so commands that ran side by side run one
//! after the other: the times are each command's alone, and the frame is
//! slower). What it changes: nothing a shader reads or writes; no barrier,
//! no wait of the CPU.
//!
//! What is not timed: commands past [`MOST`] pairs in one list (counted),
//! preludes and other lists of the layer's own (the card's own stamps,
//! `VICTORY_CARD_SPANS=1`, have the submissions whole), and the second and
//! later runs of a list submitted more than once before its reset (only its
//! last run is read).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use windows::Win32::Graphics::Direct3D12::*;

/// Command pairs per list; the last query is the close.
const MOST: u32 = 32767;
/// Stamps recorded back to back at the close, nothing between them: what a
/// stamp itself costs (an empty pair, an empty gap).
const CALIBRATION: u32 = 8;
/// The first calibration stamp; the last query is the close.
const CALIBRATED: u32 = 2 * MOST;
const QUERIES: u32 = 2 * MOST + CALIBRATION + 1;

/// `VICTORY_PASS_TIMES=1`, or `=2` ([`serial`]).
pub(crate) fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        let on = std::env::var_os("VICTORY_PASS_TIMES").is_some_and(|v| v == "1" || v == "2");
        if on {
            tracing::info!(
                serial = serial(),
                "pass times on (VICTORY_PASS_TIMES): two GPU timestamps around every title command; the run is slower"
            );
        }
        on
    })
}

/// `VICTORY_PASS_TIMES=2`: before each stamp the card finishes everything
/// recorded before it (a global barrier, `place::wait_for_all`). A stamp alone
/// is written when the card's front reaches it, while earlier draws and
/// dispatches may still be running: their time then lands in whatever
/// interval holds the next wait (run-193: the same mesh draws 6 ms with fewer
/// stamps, 22-32 ms with more). With the waits each command's interval is its
/// own work, alone. Slower still; what is drawn is the same.
pub(crate) fn serial() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_PASS_TIMES").is_some_and(|v| v == "2"))
}

/// `VICTORY_PASS_STEPS=1`: the layer's own steps before a draw or dispatch
/// are timed too (`crate::draw::LayerStep`; some 200,000 a frame).
pub(crate) fn steps() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_PASS_STEPS").is_some_and(|v| v == "1"))
}

/// Everything before is done (with [`serial`]).
fn settle(list: &ID3D12GraphicsCommandList) {
    if serial()
        && let Some(host) = super::host()
    {
        // SAFETY: an open list.
        unsafe { super::place::wait_for_all(host, list) };
    }
}

/// What a timed command is.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Kind {
    Dispatch,
    IndirectDispatch,
    Draw,
    IndirectDraw,
    Mesh,
    IndirectMesh,
    /// The layer's clamp of indirect arguments: one record, a window, a batch.
    Clamp,
    /// (Title list methods that are not draws or dispatches, with what the
    /// layer records for them.)
    TitleBarrier,
    TitleCopy,
    TitleResolve,
    TitleClear,
    TitleQuery,
    TitleSync,
    TitleWait,
    TitleWrite,
    /// The layer's own steps before a draw or dispatch (`crate::draw`).
    Layer,
}

impl Kind {
    fn word(self) -> &'static str {
        match self {
            Kind::Dispatch => "dispatch",
            Kind::IndirectDispatch => "indirect-dispatch",
            Kind::Draw => "draw",
            Kind::IndirectDraw => "indirect-draw",
            Kind::Mesh => "mesh",
            Kind::IndirectMesh => "indirect-mesh",
            Kind::Clamp => "layer-clamp",
            Kind::TitleBarrier => "title-barrier",
            Kind::TitleCopy => "title-copy",
            Kind::TitleResolve => "title-resolve",
            Kind::TitleClear => "title-clear",
            Kind::TitleQuery => "title-query",
            Kind::TitleSync => "title-sync",
            Kind::TitleWait => "title-wait",
            Kind::TitleWrite => "title-write",
            Kind::Layer => "layer-step",
        }
    }
}

/// A list's stamps.
pub(crate) struct Passes {
    heap: ID3D12QueryHeap,
    back: ID3D12Resource,
    /// Per pair: what it timed.
    names: Vec<(Kind, Arc<str>)>,
    /// A begin is recorded without its end.
    open: bool,
    /// Pairs resolved at the close, to be read at the reset.
    resolved: u32,
    /// Commands not timed (past [`MOST`]).
    missed: u64,
}

// SAFETY: free-threaded objects, used behind the list's lock.
unsafe impl Send for Passes {}

fn make() -> Option<Passes> {
    let host = super::host()?;
    let mut heap: Option<ID3D12QueryHeap> = None;
    // SAFETY: plain object creation.
    unsafe {
        host.device
            .CreateQueryHeap(
                &D3D12_QUERY_HEAP_DESC { Type: D3D12_QUERY_HEAP_TYPE_TIMESTAMP, Count: QUERIES, NodeMask: 0 },
                &mut heap,
            )
            .ok()?;
    }
    let back = host.read_back_buffer(u64::from(QUERIES) * 8)?;
    Some(Passes { heap: heap?, back, names: Vec::new(), open: false, resolved: 0, missed: 0 })
}

/// A command of `kind` named `name` is about to be recorded into `list`.
pub(crate) fn begin(
    slot: &Mutex<Option<Option<Passes>>>,
    list: &ID3D12GraphicsCommandList,
    kind: Kind,
    name: impl FnOnce() -> Arc<str>,
) {
    let mut slot = slot.lock().unwrap_or_else(|e| e.into_inner());
    let Some(p) = slot.get_or_insert_with(make).as_mut() else {
        return;
    };
    if p.open {
        // An end that never came (a command refused after its begin): the
        // pair is dropped, not counted.
        p.names.pop();
        p.open = false;
    }
    let i = p.names.len() as u32;
    if i >= MOST {
        p.missed += 1;
        return;
    }
    settle(list);
    // SAFETY: an open list and our heap.
    unsafe { list.EndQuery(&p.heap, D3D12_QUERY_TYPE_TIMESTAMP, 2 * i) };
    p.names.push((kind, name()));
    p.open = true;
}

/// The command [`begin`] was for is recorded.
pub(crate) fn end(slot: &Mutex<Option<Option<Passes>>>, list: &ID3D12GraphicsCommandList) {
    let mut slot = slot.lock().unwrap_or_else(|e| e.into_inner());
    let Some(Some(p)) = slot.as_mut() else { return };
    if !p.open {
        return;
    }
    let i = p.names.len() as u32 - 1;
    settle(list);
    // SAFETY: an open list and our heap.
    unsafe { list.EndQuery(&p.heap, D3D12_QUERY_TYPE_TIMESTAMP, 2 * i + 1) };
    p.open = false;
}

/// The list is about to close: the close's stamp, and all resolved.
pub(crate) fn close(slot: &Mutex<Option<Option<Passes>>>, list: &ID3D12GraphicsCommandList) {
    let mut slot = slot.lock().unwrap_or_else(|e| e.into_inner());
    let Some(Some(p)) = slot.as_mut() else { return };
    if p.open {
        p.names.pop();
        p.open = false;
    }
    let n = p.names.len() as u32;
    if n == 0 {
        p.resolved = 0;
        return;
    }
    settle(list);
    // SAFETY: an open list, our heap and read-back buffer.
    unsafe {
        list.EndQuery(&p.heap, D3D12_QUERY_TYPE_TIMESTAMP, QUERIES - 1);
        for k in 0..CALIBRATION {
            list.EndQuery(&p.heap, D3D12_QUERY_TYPE_TIMESTAMP, CALIBRATED + k);
        }
        list.ResolveQueryData(&p.heap, D3D12_QUERY_TYPE_TIMESTAMP, 0, 2 * n, &p.back, 0);
        list.ResolveQueryData(&p.heap, D3D12_QUERY_TYPE_TIMESTAMP, CALIBRATED, CALIBRATION + 1, &p.back, u64::from(CALIBRATED) * 8);
    }
    p.resolved = n;
}

/// Sums since the last line.
#[derive(Default)]
struct Sums {
    /// By (kind, name): commands, ticks, the longest.
    by: HashMap<(Kind, Arc<str>), (u64, u64, u64)>,
    lists: u64,
    /// Ticks from each list's first timed command to its close.
    spans: u64,
    missed: u64,
    /// Pairs whose stamps were not there (never written, or nonsense).
    empty: u64,
    /// (Between one timed command's end and the next one's start, in the
    /// same list) ticks by size: under 5 us, 5-20, 20-100, 100 us and more;
    /// and how many.
    gaps: [(u64, u64); 4],
    /// Before the first timed command is nothing timed; after the last one,
    /// up to the close: ticks.
    tail: u64,
    /// Big gaps (20 us and more) by the command after them: how many, ticks.
    before: HashMap<(Kind, Arc<str>), (u64, u64)>,
    /// Calibration: empty pairs and empty gaps measured, their ticks.
    stamp_pairs: (u64, u64),
    stamp_gaps: (u64, u64),
}

static SUMS: Mutex<Option<Sums>> = Mutex::new(None);

/// Every submission of the list has run (its reset): its stamps are read.
pub(crate) fn reset(slot: &Mutex<Option<Option<Passes>>>) {
    let mut slot = slot.lock().unwrap_or_else(|e| e.into_inner());
    let Some(Some(p)) = slot.as_mut() else { return };
    let n = std::mem::take(&mut p.resolved) as usize;
    let names = std::mem::take(&mut p.names);
    p.open = false;
    let missed = std::mem::take(&mut p.missed);
    let mut sums = SUMS.lock().unwrap_or_else(|e| e.into_inner());
    let sums = sums.get_or_insert_with(Sums::default);
    sums.missed += missed;
    if n == 0 || names.len() < n {
        return;
    }
    let mut data = std::ptr::null_mut();
    // SAFETY: a read-back buffer the finished list resolved into; written
    // back to zeros so that a list reset without having run reads none.
    let (stamps, close, calibration) = unsafe {
        if p.back.Map(0, None, Some(&mut data)).is_err() {
            return;
        }
        let at = data.cast::<u64>();
        let stamps = std::slice::from_raw_parts(at, 2 * n).to_vec();
        let close = at.add(QUERIES as usize - 1).read();
        let calibration = std::slice::from_raw_parts(at.add(CALIBRATED as usize), CALIBRATION as usize).to_vec();
        std::ptr::write_bytes(at, 0, 2 * n);
        std::ptr::write_bytes(at.add(CALIBRATED as usize), 0, CALIBRATION as usize + 1);
        p.back.Unmap(0, None);
        (stamps, close, calibration)
    };
    let per_us = ticks_per_ms() / 1000.0;
    if calibration.iter().all(|&c| c != 0) && calibration.windows(2).all(|w| w[1] >= w[0]) {
        for k in 0..CALIBRATION as usize - 1 {
            let t = calibration[k + 1] - calibration[k];
            let s = if k % 2 == 0 { &mut sums.stamp_pairs } else { &mut sums.stamp_gaps };
            s.0 += 1;
            s.1 += t;
        }
    }
    // The gaps between timed commands, and after the last one.
    let mut last_end: Option<u64> = None;
    for (i, (kind, name)) in names.iter().take(n).enumerate() {
        let (a, b) = (stamps[2 * i], stamps[2 * i + 1]);
        if a == 0 || b < a {
            continue;
        }
        if let Some(e) = last_end
            && a >= e
        {
            let gap = a - e;
            let us = gap as f64 / per_us.max(1e-9);
            let bucket = match us {
                x if x < 5.0 => 0,
                x if x < 20.0 => 1,
                x if x < 100.0 => 2,
                _ => 3,
            };
            sums.gaps[bucket].0 += 1;
            sums.gaps[bucket].1 += gap;
            if bucket >= 2 {
                let e = sums.before.entry((*kind, name.clone())).or_insert((0, 0));
                e.0 += 1;
                e.1 += gap;
            }
        }
        last_end = Some(b);
    }
    if let Some(e) = last_end
        && close >= e
    {
        sums.tail += close - e;
    }
    let first = stamps[0];
    if first != 0 && close >= first {
        sums.lists += 1;
        sums.spans += close - first;
    }
    for (i, (kind, name)) in names.into_iter().take(n).enumerate() {
        let (a, b) = (stamps[2 * i], stamps[2 * i + 1]);
        if a == 0 || b < a {
            sums.empty += 1;
            continue;
        }
        let e = sums.by.entry((kind, name)).or_insert((0, 0, 0));
        e.0 += 1;
        e.1 += b - a;
        e.2 = e.2.max(b - a);
    }
}

/// The family of a program label: each stage's name without the group in
/// front (`group:`) and the hash at the end (`_` and 8 hex digits).
fn family(label: &str) -> String {
    if label.starts_with("bvh-builder") {
        return "bvh-builder (every stage of the console's structure builder)".to_owned();
    }
    label
        .split(" + ")
        .map(|part| {
            let part = part.rsplit(':').next().unwrap_or(part);
            match part.rsplit_once('_') {
                Some((head, tail)) if tail.len() == 8 && tail.bytes().all(|b| b.is_ascii_hexdigit()) => head,
                _ => part,
            }
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

/// The card's timestamp ticks per millisecond.
fn ticks_per_ms() -> f64 {
    static TICKS_PER_MS: OnceLock<f64> = OnceLock::new();
    *TICKS_PER_MS.get_or_init(|| {
        // SAFETY: a plain query.
        super::host().and_then(|h| unsafe { h.queue.GetTimestampFrequency() }.ok()).unwrap_or(1) as f64 / 1000.0
    })
}

/// For the frame lines: what was read since the last one, per frame.
pub(crate) fn line(frames: f64) {
    if !on() {
        return;
    }
    let Some(sums) = SUMS.lock().unwrap_or_else(|e| e.into_inner()).take() else {
        return;
    };
    let per_ms = ticks_per_ms();
    let frames = frames.max(1.0);
    let ms = |ticks: u64| ticks as f64 / per_ms / frames;
    let timed: u64 = sums.by.values().map(|v| v.1).sum();
    let commands: u64 = sums.by.values().map(|v| v.0).sum();
    let mut kinds: Vec<(Kind, u64, u64)> = Vec::new();
    for ((kind, _), (n, t, _)) in &sums.by {
        match kinds.iter_mut().find(|k| k.0 == *kind) {
            Some(k) => (k.1, k.2) = (k.1 + n, k.2 + t),
            None => kinds.push((*kind, *n, *t)),
        }
    }
    kinds.sort_by_key(|k| std::cmp::Reverse(k.2));
    let kinds: Vec<String> =
        kinds.iter().map(|(k, n, t)| format!("{} {:.1} ms in {:.0}", k.word(), ms(*t), *n as f64 / frames)).collect();
    tracing::info!(
        "pass times per frame (VICTORY_PASS_TIMES): {:.1} timed lists, {:.1} ms from their first timed command to their close; \
         timed commands {:.1} ms in {:.0} ({}); the rest of those lists (barriers, tables, copies of the layer's between commands) {:.1} ms; \
         not timed: {:.1} commands past the list's {MOST}, {:.1} without stamps",
        sums.lists as f64 / frames,
        ms(sums.spans),
        ms(timed),
        commands as f64 / frames,
        kinds.join(", "),
        ms(sums.spans.saturating_sub(timed)),
        sums.missed as f64 / frames,
        sums.empty as f64 / frames,
    );
    let us = |(n, t): (u64, u64)| if n == 0 { 0.0 } else { t as f64 / per_ms * 1000.0 / n as f64 };
    let gap_all: u64 = sums.gaps.iter().map(|g| g.1).sum();
    tracing::info!(
        "pass times, between the timed commands per frame: {:.1} ms in {:.0} gaps (under 5 us: {:.1} ms in {:.0}; 5-20 us: {:.1} ms in {:.0};          20-100 us: {:.1} ms in {:.0}; 100 us and more: {:.1} ms in {:.0}); after the last one up to the close {:.1} ms;          a stamp's own cost (back to back, nothing between): pair {:.2} us, gap {:.2} us, so about {:.1} ms of the gaps are the meter's",
        ms(gap_all),
        sums.gaps.iter().map(|g| g.0).sum::<u64>() as f64 / frames,
        ms(sums.gaps[0].1),
        sums.gaps[0].0 as f64 / frames,
        ms(sums.gaps[1].1),
        sums.gaps[1].0 as f64 / frames,
        ms(sums.gaps[2].1),
        sums.gaps[2].0 as f64 / frames,
        ms(sums.gaps[3].1),
        sums.gaps[3].0 as f64 / frames,
        ms(sums.tail),
        us(sums.stamp_pairs),
        us(sums.stamp_gaps),
        (us(sums.stamp_pairs) + us(sums.stamp_gaps)) * commands as f64 / frames / 1000.0,
    );
    let mut before: Vec<_> = sums.before.iter().collect();
    before.sort_by_key(|(_, v)| std::cmp::Reverse(v.1));
    for ((kind, name), (n, t)) in before.into_iter().take(15) {
        tracing::info!(
            "  gap of 20 us or more before: {:.2} ms a frame in {:.1} a frame, {:.0} us each: {} {}",
            ms(*t),
            *n as f64 / frames,
            *t as f64 / per_ms * 1000.0 / (*n).max(1) as f64,
            kind.word(),
            name.chars().take(200).collect::<String>()
        );
    }
    let share = |t: u64| 100.0 * t as f64 / timed.max(1) as f64;
    // By family.
    let mut families: HashMap<(Kind, String), (u64, u64, u64, u64)> = HashMap::new();
    for ((kind, name), (n, t, longest)) in &sums.by {
        let e = families.entry((*kind, family(name))).or_insert((0, 0, 0, 0));
        e.0 += n;
        e.1 += t;
        e.2 = e.2.max(*longest);
        e.3 += 1;
    }
    let mut families: Vec<_> = families.into_iter().collect();
    families.sort_by_key(|(_, v)| std::cmp::Reverse(v.1));
    for ((kind, name), (n, t, longest, programs)) in families.into_iter().take(40) {
        tracing::info!(
            "  pass family: {:.2} ms a frame ({:.1} %) in {:.1} {} a frame, {:.1} us each, the longest {:.0} us, {programs} programs: {}",
            ms(t),
            share(t),
            n as f64 / frames,
            kind.word(),
            t as f64 / per_ms * 1000.0 / n.max(1) as f64,
            longest as f64 / per_ms * 1000.0,
            name.chars().take(300).collect::<String>()
        );
    }
    let mut by: Vec<_> = sums.by.into_iter().collect();
    by.sort_by_key(|(_, v)| std::cmp::Reverse(v.1));
    for ((kind, name), (n, t, longest)) in by.into_iter().take(40) {
        tracing::info!(
            "  pass: {:.2} ms a frame ({:.1} %) in {:.1} {} a frame, {:.1} us each, the longest {:.0} us: {}",
            ms(t),
            share(t),
            n as f64 / frames,
            kind.word(),
            t as f64 / per_ms * 1000.0 / n.max(1) as f64,
            longest as f64 / per_ms * 1000.0,
            name.chars().take(300).collect::<String>()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::family;

    #[test]
    fn families_leave_out_group_and_hash() {
        assert_eq!(family("billboard_nobump:CS_RaytracingResolve_a56bfb9c"), "CS_RaytracingResolve");
        assert_eq!(family("cloth_default:CS_RaytracingResolve_0123abcd"), "CS_RaytracingResolve");
        assert_eq!(
            family("indirect_diffuse_lighting:CS_ScreenProbes_EvaluateRayRequests_RayBinning"),
            "CS_ScreenProbes_EvaluateRayRequests_RayBinning"
        );
        assert_eq!(family("vertex 0602c2384acbb3b2 + pixel a78d088fa55aa495"), "vertex 0602c2384acbb3b2 + pixel a78d088fa55aa495");
        assert_eq!(family("bvh-builder compute 052a2316a03b9ba1").split(' ').next(), Some("bvh-builder"));
    }
}
