//! (Fox T, diagnostics) The shape of a submission's list steps, for running
//! them on several threads (`VICTORY_PREPARE_SHAPE=1`): the steps of the
//! submission's lists, in order, cut where a step may write any title memory
//! (`Step::Run`), and in each piece grouped into chains by the memory they
//! touch: a step of a host copy (refresh, declared write) is that copy's; a
//! title copy joins its destination's and its source's; a fill, a host step
//! or a placed copy is the memory it reads (and writes). Chains of a piece
//! could run side by side, each in order. Measures only.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{HostList, Step};

/// `VICTORY_PREPARE_SHAPE=1`.
pub(super) fn on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_PREPARE_SHAPE").is_some_and(|v| v == "1"))
}

/// Submissions, steps, pieces, chains, steps of each piece's largest chain
/// (summed), refreshes; submissions whose largest chains hold more than half
/// of the steps, more than nine tenths.
static SHAPE: [AtomicU64; 8] = [const { AtomicU64::new(0) }; 8];

/// Union-find over memory keys.
struct Chains {
    parent: Vec<usize>,
    steps: Vec<u64>,
    keys: HashMap<usize, usize>,
}

impl Chains {
    fn new() -> Self {
        Chains { parent: Vec::new(), steps: Vec::new(), keys: HashMap::new() }
    }
    fn node(&mut self, key: usize) -> usize {
        if let Some(&n) = self.keys.get(&key) {
            return self.find(n);
        }
        let n = self.parent.len();
        self.parent.push(n);
        self.steps.push(0);
        self.keys.insert(key, n);
        n
    }
    fn find(&mut self, mut n: usize) -> usize {
        while self.parent[n] != n {
            self.parent[n] = self.parent[self.parent[n]];
            n = self.parent[n];
        }
        n
    }
    fn join(&mut self, a: usize, b: usize) -> usize {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.parent[b] = a;
            self.steps[a] += self.steps[b];
            self.steps[b] = 0;
        }
        a
    }
    /// Ends a piece: (chains, steps of the largest).
    fn end(&mut self) -> (u64, u64) {
        let mut chains = 0;
        let mut most = 0;
        for n in 0..self.parent.len() {
            if self.parent[n] == n && self.steps[n] > 0 {
                chains += 1;
                most = most.max(self.steps[n]);
            }
        }
        *self = Chains::new();
        (chains, most)
    }
}

/// The key of title memory at `address`: its host copy, or its 64 KiB page.
fn memory(address: usize, len: usize) -> usize {
    match crate::shadow::find_any(address as u64, len.max(1) as u64) {
        Some(s) => std::sync::Arc::as_ptr(&s) as usize,
        None => (address >> 16) | 1 << 63,
    }
}

/// Per page chains: keys of the 64 KiB pages of `[address, address + len)`.
fn pages(address: usize, len: usize) -> impl Iterator<Item = usize> {
    let first = address >> 16;
    let last = (address + len.max(1) - 1) >> 16;
    (first..=last).map(|p| p | 1 << 62)
}

/// Joins every page of a range in `chains`; the chain's node.
fn join_pages(chains: &mut Chains, address: usize, len: usize, also: Option<usize>) -> usize {
    let mut node = also;
    for p in pages(address, len) {
        let n = chains.node(p);
        node = Some(match node {
            Some(m) => chains.join(m, n),
            None => n,
        });
    }
    node.unwrap_or_else(|| chains.node(usize::MAX))
}

/// Of the host copy with the most refresh steps in each submission, summed
/// over submissions: refresh steps, distinct 64 KiB pages, distinct ranges,
/// steps repeating a range refreshed earlier in the submission with no title
/// write between, bytes asked; and per page chains: steps, the largest
/// chains' steps. And the copy last seen on top: start, length.
static TOP: [AtomicU64; 9] = [const { AtomicU64::new(0) }; 9];

/// Measures the steps of `runs` (before they are run).
pub(super) fn measure(runs: &[&HostList]) {
    measure_pages(runs);
    let mut chains = Chains::new();
    let (mut steps, mut pieces, mut all_chains, mut largest, mut refreshes) = (0u64, 1u64, 0u64, 0u64, 0u64);
    for list in runs {
        let deferred = list.deferred();
        for step in &deferred.copies {
            steps += 1;
            let node = match step {
                Step::Run(_) => {
                    let (c, m) = chains.end();
                    all_chains += c;
                    largest += m;
                    pieces += 1;
                    continue;
                }
                Step::Refresh(s, _, _) | Step::GpuWrite(s, _, _) => {
                    if matches!(step, Step::Refresh(..)) {
                        refreshes += 1;
                    }
                    chains.node(std::sync::Arc::as_ptr(s) as usize)
                }
                Step::Copy(dst, src, n) => {
                    let a = chains.node(memory(*dst, *n));
                    let b = chains.node(memory(*src, *n));
                    chains.join(a, b)
                }
                Step::Fill(_, src, n, _) => chains.node(memory(*src, *n)),
                Step::Host(_, (from, to)) => {
                    if to > from {
                        chains.node(memory(*from, to - from))
                    } else {
                        chains.node(usize::MAX)
                    }
                }
                Step::Texture(t) => chains.node(std::sync::Arc::as_ptr(t) as usize),
                Step::Plane((a, b), _) => chains.node(memory(*a, b.saturating_sub(*a))),
                Step::Placed(p) => {
                    let a = chains.node(std::sync::Arc::as_ptr(&p.shadow) as usize);
                    let b = chains.node(memory(p.src, p.len));
                    chains.join(a, b)
                }
            };
            chains.steps[node] += 1;
        }
    }
    let (c, m) = chains.end();
    all_chains += c;
    largest += m;
    SHAPE[0].fetch_add(1, Ordering::Relaxed);
    SHAPE[1].fetch_add(steps, Ordering::Relaxed);
    SHAPE[2].fetch_add(pieces, Ordering::Relaxed);
    SHAPE[3].fetch_add(all_chains, Ordering::Relaxed);
    SHAPE[4].fetch_add(largest, Ordering::Relaxed);
    SHAPE[5].fetch_add(refreshes, Ordering::Relaxed);
    if steps > 0 && largest * 2 > steps {
        SHAPE[6].fetch_add(1, Ordering::Relaxed);
    }
    if steps > 0 && largest * 10 > steps * 9 {
        SHAPE[7].fetch_add(1, Ordering::Relaxed);
    }
}

/// For the frame lines.
pub(super) fn line(frames: f64) {
    if !on() {
        return;
    }
    {
        let t: [u64; 9] = std::array::from_fn(|i| TOP[i].swap(0, Ordering::Relaxed));
        let f = frames.max(1.0);
        tracing::info!(
            "list steps by 64 KiB page per frame (VICTORY_PREPARE_SHAPE): {:.0} steps, the pieces' largest page chains hold {:.0} ({:.1} %);              the most refreshed copy of each submission (last: {:#x}+{:#x}): {:.0} refresh steps over {:.0} distinct pages and {:.0} distinct ranges,              {:.0} repeating a range refreshed earlier in the submission with no title write between, {:.1} MB asked",
            t[5] as f64 / f,
            t[6] as f64 / f,
            100.0 * t[6] as f64 / t[5].max(1) as f64,
            t[7],
            t[8],
            t[0] as f64 / f,
            t[1] as f64 / f,
            t[2] as f64 / f,
            t[3] as f64 / f,
            t[4] as f64 / f / (1 << 20) as f64,
        );
    }
    let v: [u64; 8] = std::array::from_fn(|i| SHAPE[i].swap(0, Ordering::Relaxed));
    let f = frames.max(1.0);
    tracing::info!(
        "list steps' shape per frame (VICTORY_PREPARE_SHAPE): {:.0} submissions, {:.0} steps ({:.0} refreshes) in {:.0} pieces between steps that may write \
         any title memory, {:.0} chains; the pieces' largest chains hold {:.0} steps ({:.1} %: the most a wide run could leave on one thread); \
         submissions whose largest chains hold over half {:.1}, over nine tenths {:.1}",
        v[0] as f64 / f,
        v[1] as f64 / f,
        v[5] as f64 / f,
        v[2] as f64 / f,
        v[3] as f64 / f,
        v[4] as f64 / f,
        100.0 * v[4] as f64 / v[1].max(1) as f64,
        v[6] as f64 / f,
        v[7] as f64 / f,
    );
}

/// Chains by page, and the repeats of the most refreshed copy.
fn measure_pages(runs: &[&HostList]) {
    use std::collections::HashSet;
    let mut chains = Chains::new();
    let mut largest = 0u64;
    let mut steps = 0u64;
    // Per copy: refresh steps, pages, ranges, repeats, bytes.
    let mut copies: HashMap<usize, (u64, HashSet<usize>, HashSet<(usize, usize)>, u64, u64, (u64, u64))> = HashMap::new();
    // Ranges refreshed since the last step that writes title memory.
    let mut since: HashSet<(usize, usize, usize)> = HashSet::new();
    for list in runs {
        let deferred = list.deferred();
        for step in &deferred.copies {
            steps += 1;
            let node = match step {
                Step::Run(_) => {
                    largest += chains.end().1;
                    since.clear();
                    continue;
                }
                Step::Refresh(sh, a, n) => {
                    let key = std::sync::Arc::as_ptr(sh) as usize;
                    let e = copies.entry(key).or_insert_with(|| (0, HashSet::new(), HashSet::new(), 0, 0, sh.span()));
                    e.0 += 1;
                    e.1.extend(pages(*a, *n));
                    e.2.insert((*a, *n));
                    if !since.insert((key, *a, *n)) {
                        e.3 += 1;
                    }
                    e.4 += *n as u64;
                    join_pages(&mut chains, *a, *n, None)
                }
                Step::GpuWrite(_, a, n) => join_pages(&mut chains, *a, *n, None),
                Step::Copy(dst, src, n) => {
                    since.clear();
                    let d = join_pages(&mut chains, *dst, *n, None);
                    join_pages(&mut chains, *src, *n, Some(d))
                }
                Step::Fill(_, src, n, _) => join_pages(&mut chains, *src, *n, None),
                Step::Host(_, (from, to)) => {
                    if to > from {
                        join_pages(&mut chains, *from, to - from, None)
                    } else {
                        chains.node(usize::MAX)
                    }
                }
                Step::Texture(t) => chains.node(std::sync::Arc::as_ptr(t) as usize),
                Step::Plane((a, b), _) => join_pages(&mut chains, *a, b.saturating_sub(*a), None),
                Step::Placed(p) => {
                    since.clear();
                    let d = join_pages(&mut chains, p.dst, p.len, None);
                    join_pages(&mut chains, p.src, p.len, Some(d))
                }
            };
            chains.steps[node] += 1;
        }
    }
    largest += chains.end().1;
    TOP[5].fetch_add(steps, Ordering::Relaxed);
    TOP[6].fetch_add(largest, Ordering::Relaxed);
    if let Some((_, top)) = copies.into_iter().max_by_key(|(_, c)| c.0) {
        TOP[0].fetch_add(top.0, Ordering::Relaxed);
        TOP[1].fetch_add(top.1.len() as u64, Ordering::Relaxed);
        TOP[2].fetch_add(top.2.len() as u64, Ordering::Relaxed);
        TOP[3].fetch_add(top.3, Ordering::Relaxed);
        TOP[4].fetch_add(top.4, Ordering::Relaxed);
        TOP[7].store(top.5.0, Ordering::Relaxed);
        TOP[8].store(top.5.1, Ordering::Relaxed);
    }
}
