//! (Fox T) The verify harness of the stale-page refresh
//! (`VICTORY_REFRESH_STALE=verify`, W's): at each refresh of a copy, today's
//! look (`Shadow::copy_stale`, which pages of the range it takes) is the
//! truth; the stale path says which pages it would take
//! (`Shadow::stale_pages`, W's). W calls [`verify`] from the hook
//! `crate::copystep::stale_verify`. Counted per frame:
//!
//! - pages today's look takes that the stale set lacks: must be 0. The first
//!   30 are logged with the copy, the page, the range and whether its bytes in
//!   the copy differ from title memory now (a miss that would have left the
//!   copy wrong) or not (a page the look takes anyway, its bytes the same);
//! - bytes of those pages that differ between the copy and title memory
//!   (what the stale path would have left stale);
//! - pages the stale set has that today's look does not take (cost only).
//!
//! Nothing here changes what is taken: the harness only reads.

use std::sync::atomic::{AtomicU64, Ordering};

/// `VICTORY_REFRESH_STALE=verify`.
pub(crate) fn on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    // (W) Under `=1` too: its one-in-64 refreshes the old way are checked and the line printed.
    *ON.get_or_init(|| std::env::var("VICTORY_REFRESH_STALE").is_ok_and(|v| v == "verify" || v == "1"))
}

/// Refreshes checked, pages today's look took, pages the stale set lacked,
/// of them with bytes that differ now, bytes differing, pages the stale set
/// had beyond, misses not compared.
static COUNTS: [AtomicU64; 8] = [const { AtomicU64::new(0) }; 8];

/// The hook's body (W calls it from `crate::copystep::stale_verify`, where the
/// stale path would branch, before anything is taken): the copy (its id, start
/// and length), the refresh's range, `truth` the 64 KiB page numbers today's
/// look takes (`copy_stale`'s runs), `stale` those `Shadow::stale_pages` says.
pub(crate) fn verify(copy: u64, start: u64, len: u64, from: usize, to: usize, truth: &[usize], stale: &[usize]) {
    let (mut truth, mut stale) = (truth.to_vec(), stale.to_vec());
    truth.sort_unstable();
    truth.dedup();
    stale.sort_unstable();
    stale.dedup();
    COUNTS[0].fetch_add(1, Ordering::Relaxed);
    COUNTS[1].fetch_add(truth.len() as u64, Ordering::Relaxed);
    const PAGE: usize = 1 << 16;
    static SAID: AtomicU64 = AtomicU64::new(0);
    // The copy, found by its start, for the byte compare (read only: its
    // CPU side against title memory, before the take).
    let shadow = crate::shadow::find_any(start, 1).filter(|s| s.span() == (start, len));
    let differs = |a: usize, b: usize| -> Option<u64> {
        let s = shadow.as_ref()?;
        let mut title = vec![0u8; b - a];
        if !crate::host::read_into(a as *const u8, &mut title) {
            return None;
        }
        // SAFETY: inside the copy's CPU side (`[a, b)` lies in the refreshed
        // range, inside the copy); read only.
        let held = unsafe { std::slice::from_raw_parts(s.card_data().add(a - start as usize), b - a) };
        Some(held.iter().zip(&title).filter(|(x, y)| x != y).count() as u64)
    };
    for &page in &truth {
        if stale.binary_search(&page).is_ok() {
            continue;
        }
        COUNTS[2].fetch_add(1, Ordering::Relaxed);
        let (a, b) = ((page * PAGE).max(from), ((page + 1) * PAGE).min(to));
        let diff = if a < b { differs(a, b) } else { Some(0) };
        match diff {
            Some(0) => {}
            Some(n) => {
                COUNTS[3].fetch_add(1, Ordering::Relaxed);
                COUNTS[4].fetch_add(n, Ordering::Relaxed);
            }
            None => {
                COUNTS[6].fetch_add(1, Ordering::Relaxed);
            }
        }
        if SAID.fetch_add(1, Ordering::Relaxed) < 30 {
            tracing::warn!(
                copy,
                start = format_args!("{start:#x}"),
                len = format_args!("{len:#x}"),
                page = format_args!("{:#x}", page * PAGE),
                range = format_args!("{from:#x}..{to:#x}"),
                differing_bytes = ?diff,
                "refresh stale verify: a page today's look takes that the stale set lacks"
            );
        }
    }
    let extra = stale.iter().filter(|p| truth.binary_search(p).is_err()).count();
    COUNTS[5].fetch_add(extra as u64, Ordering::Relaxed);
}

/// For the frame lines.
pub(crate) fn line(frames: f64) {
    if !on() {
        return;
    }
    let v: [u64; 8] = std::array::from_fn(|i| COUNTS[i].swap(0, Ordering::Relaxed));
    let f = frames.max(1.0);
    tracing::info!(
        "refresh stale verify (VICTORY_REFRESH_STALE=verify) per frame: {:.0} refreshes checked, {:.0} pages taken by today's look; \
         the stale set lacked {:.1} (MUST BE 0), {:.1} of them with bytes differing now ({:.0} bytes; {:.1} not comparable); it had {:.0} beyond (cost only)",
        v[0] as f64 / f,
        v[1] as f64 / f,
        v[2] as f64 / f,
        v[3] as f64 / f,
        v[4] as f64 / f,
        v[6] as f64 / f,
        v[5] as f64 / f,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn misses_and_extras_are_counted_apart() {
        for c in &COUNTS {
            c.store(0, Ordering::Relaxed);
        }
        // Truth takes pages 1, 2, 5; the stale set says 2, 3, 5: page 1 is a
        // miss (no copy at that start: not comparable), page 3 an extra.
        verify(7, 0x1234_0000_0000, 0x10_0000, 0x1_0000, 0x6_0000, &[5, 1, 2], &[3, 2, 5]);
        let v: Vec<u64> = COUNTS.iter().map(|c| c.load(Ordering::Relaxed)).collect();
        assert_eq!((v[0], v[1], v[2], v[5], v[6]), (1, 3, 1, 1, 1));
    }
}
