//! Hang watch (`VICTORY_HANG_WATCH_S`, default 8 in diagnostics builds; 0
//! off): when no frame has begun and nothing was presented for that many
//! seconds while the title runs, says once (per stall) where the layer's
//! threads are waiting, which of its locks are held, and then every
//! thread's stack (the dumper the loader registers, `set_stacks`).
//!
//! Runs 63-70 hung 10 s into the scene and were then killed by the title's
//! own watchdog with nothing in the log saying why: the submit thread had
//! taken a list's step lock twice. From now on the next hang is one log
//! read: "hang watch: no frame for 8 s ...; <thread> waiting in <site> for
//! T s; ...; locks held now: ...; stacks follow".
//!
//! Sites: [`at`] marks the calling thread's current wait site until the
//! guard drops (two relaxed stores; the slot is made once per thread).

use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex, OnceLock};

struct Slot {
    name: String,
    tid: u32,
    /// The site, a `&'static &'static str` (null: none).
    site: AtomicPtr<&'static str>,
    /// When the site was entered (ms since START).
    since: AtomicU64,
}

static SLOTS: Mutex<Vec<Arc<Slot>>> = Mutex::new(Vec::new());
static START: OnceLock<std::time::Instant> = OnceLock::new();

fn now_ms() -> u64 {
    START.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64
}

thread_local! {
    static SLOT: Arc<Slot> = {
        let slot = Arc::new(Slot {
            name: this_thread_name(),
            tid: this_thread_id(),
            site: AtomicPtr::new(std::ptr::null_mut()),
            since: AtomicU64::new(0),
        });
        SLOTS.lock().unwrap_or_else(|e| e.into_inner()).push(slot.clone());
        slot
    };
}

#[cfg(windows)]
fn this_thread_id() -> u32 {
    // SAFETY: plain query.
    unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() }
}
#[cfg(not(windows))]
fn this_thread_id() -> u32 {
    0
}

fn this_thread_name() -> String {
    if let Some(n) = std::thread::current().name() {
        return n.to_string();
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::LocalFree;
        use windows_sys::Win32::System::Threading::{GetCurrentThread, GetThreadDescription};
        let mut text: windows_sys::core::PWSTR = std::ptr::null_mut();
        // SAFETY: this thread's pseudo handle; the text is freed below.
        let hr = unsafe { GetThreadDescription(GetCurrentThread(), &mut text) };
        if hr >= 0 && !text.is_null() {
            // SAFETY: a NUL-terminated wide string the call allocated.
            let name = unsafe {
                let mut n = 0;
                while *text.add(n) != 0 {
                    n += 1;
                }
                String::from_utf16_lossy(std::slice::from_raw_parts(text, n))
            };
            // SAFETY: memory the call allocated with LocalAlloc.
            unsafe { LocalFree(text.cast()) };
            if !name.is_empty() {
                return name;
            }
        }
    }
    String::from("(unnamed)")
}

/// The calling thread is at `site` until the guard drops (then back at the
/// site it was at before).
pub struct Site {
    was: *mut &'static str,
    since: u64,
}

impl Drop for Site {
    fn drop(&mut self) {
        let _ = SLOT.try_with(|s| {
            s.site.store(self.was, Relaxed);
            s.since.store(self.since, Relaxed);
        });
    }
}

/// Marks the calling thread as at `site` (`&"what it waits for"`).
#[must_use]
pub fn at(site: &'static &'static str) -> Site {
    let now = now_ms();
    SLOT.try_with(|s| Site {
        was: s.site.swap(std::ptr::from_ref(site).cast_mut(), Relaxed),
        since: s.since.swap(now, Relaxed),
    })
    // (Lazily: an unused fallback's drop would clear the site just set.)
    .unwrap_or_else(|_| Site { was: std::ptr::null_mut(), since: 0 })
}

/// Every marked thread now: `name (tid) in <site> for T s`, idle ones last.
pub fn sites() -> String {
    let now = now_ms();
    let slots: Vec<Arc<Slot>> = SLOTS.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let mut out = Vec::new();
    for s in slots {
        let p = s.site.load(Relaxed);
        if p.is_null() {
            out.push((u64::MAX, format!("\"{}\" ({}) not at a marked site", s.name, s.tid)));
            continue;
        }
        // SAFETY: only ever a `&'static &'static str` (see `at`).
        let site: &'static str = unsafe { *p };
        let for_ms = now.saturating_sub(s.since.load(Relaxed));
        out.push((u64::MAX - for_ms, format!("\"{}\" ({}) waiting in {site} for {:.1} s", s.name, s.tid, for_ms as f64 / 1e3)));
    }
    out.sort();
    out.into_iter().map(|e| e.1).collect::<Vec<_>>().join("; ")
}

/// Prints every thread's stack (the loader's `dump_stacks`).
static STACKS: OnceLock<fn()> = OnceLock::new();

/// Registers the stack dumper (once; later calls do nothing).
pub fn set_stacks(dump: fn()) {
    let _ = STACKS.set(dump);
}

/// The layer's state for the report (locks held, flights): set by the
/// code that knows it (the host's, once ported to the fox base); "-" else.
static STATE: OnceLock<fn() -> String> = OnceLock::new();

/// Registers [`STATE`] (once).
pub fn set_state(f: fn() -> String) {
    let _ = STATE.set(f);
}

/// Seconds without a frame before the watch speaks (0: off).
pub fn seconds() -> u64 {
    static S: OnceLock<u64> = OnceLock::new();
    *S.get_or_init(|| std::env::var("VICTORY_HANG_WATCH_S").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(8))
}

/// Starts the watch (diagnostics builds, once; from the first frame:
/// `crate::frame` calls it when the title begins a frame).
pub(crate) fn start() {
    #[cfg(all(windows, feature = "diagnostics"))]
    {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            let n = seconds();
            if n == 0 {
                return;
            }
            let _ = START.get_or_init(std::time::Instant::now);
            if let Err(e) = std::thread::Builder::new().name("victory-hang-watch".into()).spawn(move || watch(n)) {
                tracing::warn!(error = %e, "no hang watch thread (VICTORY_HANG_WATCH_S)");
            } else {
                tracing::info!(seconds = n, "hang watch on: where the layer's threads wait is said once a stall when no frame begins for this long (VICTORY_HANG_WATCH_S, 0 off)");
            }
        });
    }
}

/// Progress: frames begun, draws run on the host (the fox base counts no
/// presents of its own outside the host).
#[cfg(windows)]
fn progress() -> (u64, u64) {
    (crate::frame::current(), crate::draw::drawn())
}

#[cfg(all(windows, feature = "diagnostics"))]
fn watch(n: u64) {
    let mut last = progress();
    let mut since = std::time::Instant::now();
    let mut said = false;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let now = progress();
        if now != last {
            if said {
                tracing::warn!(
                    "hang watch: frames go on again after {:.1} s (frame {})",
                    since.elapsed().as_secs_f64(),
                    now.0
                );
            }
            last = now;
            since = std::time::Instant::now();
            said = false;
            continue;
        }
        if said || since.elapsed().as_secs() < n {
            continue;
        }
        said = true;
        let line = report(since.elapsed().as_secs_f64(), now);
        tracing::warn!("{line}");
        // Also straight to stderr: a hang may be inside the log's own lock.
        eprintln!("VIctory: {line}");
        if let Some(dump) = STACKS.get() {
            eprintln!("VIctory: hang watch: stacks follow");
            dump();
        }
    }
}

/// The text of a stall's report.
#[cfg(all(windows, feature = "diagnostics"))]
pub(crate) fn report(stalled_s: f64, (frame, presents): (u64, u64)) -> String {
    format!(
        "hang watch: no frame for {stalled_s:.0} s (frame {frame}, draws {presents}); {}; layer state: {} (VICTORY_HANG_WATCH_S={})",
        sites(),
        STATE.get().map_or_else(|| "-".to_string(), |f| f()),
        seconds()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sites_are_marked_and_restored() {
        {
            let _a = at(&"outer wait");
            {
                let _b = at(&"inner wait");
                assert!(sites().contains("waiting in inner wait"), "{}", sites());
            }
            assert!(sites().contains("waiting in outer wait"), "{}", sites());
        }
        let here = this_thread_id();
        assert!(sites().contains(&format!("({here}) not at a marked site")), "{}", sites());
    }
}
