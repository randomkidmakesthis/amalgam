//! (Fox port A) Notes the runtime (`victory-hle`) makes about title memory
//! that the fox base's GPU side has no function for yet. Each is a plain
//! function the runtime calls; the GPU code that acts on it registers
//! itself here (`on_*`, once), so the runtime's side and the GPU side can
//! be ported separately. Unregistered: counted only.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

static UNMAPPED: OnceLock<fn(usize, usize)> = OnceLock::new();

/// Calls and bytes of [`title_memory_unmapped`] since the start.
pub static UNMAPPED_SEEN: [AtomicU64; 2] = [const { AtomicU64::new(0) }; 2];

/// The runtime really unmapped title memory `[at, at + len)` (a view taken
/// away, not mapped again there: the old tree's `title_memory_unmapped`,
/// which `VICTORY_UNMAP_EXACT` (host copies let go only under memory really
/// unmapped) and `VICTORY_WINDOW_UNMAP` (card windows let go on a real
/// unmap) act on). Called after the unmapping, outside nothing in
/// particular; with `VICTORY_MAP_DEFER` on the thread that applies it.
pub fn title_memory_unmapped(at: usize, len: usize) {
    let _notes = crate::watch::NotesTimed::new();
    if len == 0 {
        return;
    }
    UNMAPPED_SEEN[0].fetch_add(1, Relaxed);
    UNMAPPED_SEEN[1].fetch_add(len as u64, Relaxed);
    if let Some(f) = UNMAPPED.get() {
        f(at, len);
    }
}

/// Registers what [`title_memory_unmapped`] calls (once; later calls do
/// nothing). For the GPU side's `VICTORY_UNMAP_EXACT` / `VICTORY_WINDOW_UNMAP`.
#[allow(dead_code)]
pub(crate) fn on_title_memory_unmapped(f: fn(usize, usize)) {
    let _ = UNMAPPED.set(f);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmapped_is_counted_and_passed_on() {
        static GOT: AtomicU64 = AtomicU64::new(0);
        fn f(_: usize, len: usize) {
            GOT.fetch_add(len as u64, Relaxed);
        }
        on_title_memory_unmapped(f);
        let before = UNMAPPED_SEEN[1].load(Relaxed);
        title_memory_unmapped(0x1000_0000, 0x1_0000);
        title_memory_unmapped(0x1000_0000, 0);
        assert_eq!(UNMAPPED_SEEN[1].load(Relaxed) - before, 0x1_0000);
        assert!(GOT.load(Relaxed) >= 0x1_0000);
    }
}

/// (F1, F4, F9, F14 hooks of `victory-hle/src/readback.rs`) What the title's
/// hooked code tells, forwarded to the GPU side's read-back set and stream
/// meters once they are ported to the fox base and registered ([`set`]);
/// counted only until then. Switches (all default OFF in the fox port):
/// `VICTORY_READBACK_SET=1` (the read-back ring hook; the old tree had it on),
/// `VICTORY_READBACK_LOCKS=1` (the generic buffer lock/unlock and the ring
/// reader hooks), `VICTORY_STREAM_METER=1` (the streamer's group-drop and the
/// drawable-removal hooks).
pub mod readback {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

    /// The GPU side's receivers (the old tree's `victory_gpu::readback::*`
    /// functions, same arguments).
    #[derive(Clone, Copy)]
    pub struct Sink {
        pub ring_created: fn(ring: usize, alias_at: u64, site: u64, name: &'static str),
        pub lock_entered: fn(handle: u64, record: u64, r8: u64, r9: u64, caller: u64),
        pub unlocked: fn(record: u64),
        pub lock_returned: fn(handle: u64, record: u64, caller: u64, ok: bool),
        pub ring_read: fn(ring: u64, record: u64, ok: bool),
        pub ring_marked: fn(ring: u64),
        pub group_dropped: fn(group: u64),
        pub drawable_removed: fn(entity: u64, flags: u64, caller: u64, slot: u64),
    }

    static SINK: OnceLock<Sink> = OnceLock::new();
    static IMAGE: AtomicU64 = AtomicU64::new(0);
    /// Calls seen by kind (the order of [`Sink`]'s fields).
    pub static SEEN: [AtomicU64; 8] = [const { AtomicU64::new(0) }; 8];

    /// Registers the receivers (once).
    pub fn set(sink: Sink) {
        let _ = SINK.set(sink);
    }

    /// The title's image base (the hooks' addresses are relative to it).
    pub fn image_base(base: u64) {
        IMAGE.store(base, Relaxed);
    }

    /// The image base [`image_base`] was told (0: none yet).
    pub fn image() -> u64 {
        IMAGE.load(Relaxed)
    }

    fn switch(name: &str) -> bool {
        std::env::var(name).is_ok_and(|v| v.trim() == "1")
    }

    pub fn set_on() -> bool {
        static ON: OnceLock<bool> = OnceLock::new();
        *ON.get_or_init(|| switch("VICTORY_READBACK_SET"))
    }

    pub fn locks_on() -> bool {
        static ON: OnceLock<bool> = OnceLock::new();
        *ON.get_or_init(|| set_on() && switch("VICTORY_READBACK_LOCKS"))
    }

    pub fn reads_on() -> bool {
        locks_on()
    }

    pub fn stream_on() -> bool {
        static ON: OnceLock<bool> = OnceLock::new();
        *ON.get_or_init(|| switch("VICTORY_STREAM_METER"))
    }

    fn seen(i: usize) -> Option<&'static Sink> {
        SEEN[i].fetch_add(1, Relaxed);
        SINK.get()
    }

    pub fn ring_created(ring: usize, alias_at: u64, site: u64, name: &'static str) {
        if let Some(s) = seen(0) {
            (s.ring_created)(ring, alias_at, site, name);
        }
    }
    pub fn lock_entered(handle: u64, record: u64, r8: u64, r9: u64, caller: u64) {
        if let Some(s) = seen(1) {
            (s.lock_entered)(handle, record, r8, r9, caller);
        }
    }
    pub fn unlocked(record: u64) {
        if let Some(s) = seen(2) {
            (s.unlocked)(record);
        }
    }
    pub fn lock_returned(handle: u64, record: u64, caller: u64, ok: bool) {
        if let Some(s) = seen(3) {
            (s.lock_returned)(handle, record, caller, ok);
        }
    }
    pub fn ring_read(ring: u64, record: u64, ok: bool) {
        if let Some(s) = seen(4) {
            (s.ring_read)(ring, record, ok);
        }
    }
    pub fn ring_marked(ring: u64) {
        if let Some(s) = seen(5) {
            (s.ring_marked)(ring);
        }
    }
    pub fn group_dropped(group: u64) {
        if let Some(s) = seen(6) {
            (s.group_dropped)(group);
        }
    }
    pub fn drawable_removed(entity: u64, flags: u64, caller: u64, slot: u64) {
        if let Some(s) = seen(7) {
            (s.drawable_removed)(entity, flags, caller, slot);
        }
    }
}
