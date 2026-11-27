//! (C3, item 1c) Texture uploads by cause, always on in diagnostics builds:
//! a line every five seconds, "uploads by cause, last 5 s: ...", count and MB
//! (staging bytes: what the upload detiles and copies; with
//! `VICTORY_PARTIAL_DELTA=1` its own line says what was left out).
//! Causes, first that fits: a one-shot bake; the first upload into a host
//! copy made after an earlier one of the same texture was let go (room rule
//! or idle); the first upload into any other host copy; a texture picked by
//! index of which more is mapped than at its last upload (partial growth);
//! one picked by index whose memory changed after its upload with nothing
//! to tell of it (the recheck); anything else (a draw's table read, a change
//! the write-watch or guard saw, levels judged anew).

#![cfg_attr(not(feature = "diagnostics"), allow(dead_code))]

use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

pub(crate) const BAKE: usize = 0;
pub(crate) const MADE_AGAIN: usize = 1;
pub(crate) const FIRST: usize = 2;
pub(crate) const PARTIAL: usize = 3;
pub(crate) const CHANGED: usize = 4;
pub(crate) const OTHER: usize = 5;
const DRAW_CHANGED: usize = 6;
const DRAW_NEW: usize = 7;
const BAKE_CLEAN: usize = 8;
const NAMES: [&str; 9] = [
    "bake",
    "made again after let go",
    "first",
    "partial growth",
    "changed after upload",
    "other picked by index (levels judged anew, waiting levels, staging gone)",
    "draw read, changed by the check at a submission",
    "draw read, its staging buffer made anew",
    "(of bake) its staging buffer uploaded and not detiled since",
];

thread_local! {
    /// Set by the texture pass around the upload it records: why.
    pub(crate) static NEXT: std::cell::Cell<usize> = const { std::cell::Cell::new(OTHER) };
}

static N: [[AtomicU64; 2]; 9] = [const { [const { AtomicU64::new(0) }; 2] }; 9];
static LINE_MS: AtomicU64 = AtomicU64::new(0);

pub(crate) fn upload(first: bool, remade: bool, baking: bool, picking: bool, staged_before: bool, clean: bool, bytes: u64) {
    if baking && staged_before && clean {
        N[BAKE_CLEAN][0].fetch_add(1, Relaxed);
        N[BAKE_CLEAN][1].fetch_add(bytes, Relaxed);
    }
    let next = if picking {
        NEXT.get()
    } else if staged_before {
        DRAW_CHANGED
    } else {
        DRAW_NEW
    };
    let cause = if baking {
        BAKE
    } else if first && remade {
        MADE_AGAIN
    } else if first {
        FIRST
    } else {
        next
    };
    N[cause][0].fetch_add(1, Relaxed);
    N[cause][1].fetch_add(bytes, Relaxed);
    line();
}

fn line() {
    let now = crate::host::now_ms();
    let last = LINE_MS.load(Relaxed);
    if now < last + 5000 || LINE_MS.compare_exchange(last, now, Relaxed, Relaxed).is_err() {
        return;
    }
    let mut text = String::new();
    let mut total = [0u64; 2];
    for (name, n) in NAMES.iter().zip(&N) {
        let (count, bytes) = (n[0].swap(0, Relaxed), n[1].swap(0, Relaxed));
        if !name.starts_with("(of") {
            total[0] += count;
            total[1] += bytes;
        }
        text.push_str(&format!("; {name} {count} ({} MB)", bytes >> 20));
    }
    let skipped = [0, 1, 2].map(|i| super::BAKE_SKIPPED[i].swap(0, Relaxed));
    tracing::info!(
        "uploads by cause, last 5 s: {} uploads {} MB{text}; bake uploads left out as unwritten (VICTORY_BAKE_DELTA=1) {} ({} MB), bake flush slots whose host copy held their bytes {}",
        total[0],
        total[1] >> 20,
        skipped[0],
        skipped[1] >> 20,
        skipped[2]
    );
}
