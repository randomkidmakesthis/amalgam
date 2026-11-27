//! (Fox F, memory audit item 2) Which of our buffers lies where in the PC's
//! memory (diagnostics; `VICTORY_HOLDERS=1`, off by default).
//!
//! The address-space walk (`copystat::log_memory_map`) groups private and
//! write-combined allocations by size, and run-118 left 6.2 GB of them
//! nameless: 4 of 1,024 MB (2.4 GB committed), 40 of 32 MB (1.25 GB) and 99
//! of 32 MB write-combined (2.5 GB). With this on, every buffer the host maps
//! (upload, shared and mapped card buffers, and the view heaps' CPU sides) is
//! noted with its mapping and the name it is given, until it goes; the walk
//! then prints, per group, its first address and the names of ours found in
//! its allocations, and the bytes of ours by name. A group with none of ours
//! is the driver's or the system's (command allocators, the driver's own
//! heaps), which no buffer of ours maps.

use std::collections::{BTreeMap, HashMap};
use std::ffi::c_void;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use windows::Win32::Graphics::Direct3D12::ID3D12Resource;
use windows::core::{GUID, HRESULT, IUnknown, Interface};

pub(crate) fn on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("VICTORY_HOLDERS").is_ok_and(|v| v.trim() == "1"))
}

struct Mapping {
    len: u64,
    name: String,
}

#[derive(Default)]
struct Registry {
    /// Mapped CPU address: what lies there.
    at: BTreeMap<usize, Mapping>,
    /// A resource's interface pointer: its mapped address.
    of: HashMap<usize, usize>,
}

static REGISTRY: Mutex<Option<Registry>> = Mutex::new(None);

fn with<T>(f: impl FnOnce(&mut Registry) -> T) -> T {
    f(REGISTRY.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Default::default))
}

/// A name's kind: up to its first digit or address (names of single buffers
/// carry sizes and addresses).
fn kind(name: &str) -> &str {
    let end = name.find(|c: char| c.is_ascii_digit()).unwrap_or(name.len());
    name[..end].trim_end_matches(['(', ' ', 'x', ':', '#']).trim_end()
}

/// `resource` is mapped at `at` for `len` bytes, as `what` (until it is
/// named, [`named`]). It is let go of when the resource goes.
pub(crate) fn mapped(resource: &ID3D12Resource, at: *mut u8, len: u64, what: &str) {
    if !on() || at.is_null() {
        return;
    }
    let raw = resource.as_raw() as usize;
    with(|r| {
        r.at.insert(at as usize, Mapping { len, name: what.to_owned() });
        r.of.insert(raw, at as usize);
    });
    gone_with(resource, at as usize);
}

/// Memory that is no resource of ours to watch (a view heap's CPU side):
/// noted for the life of the process.
pub(crate) fn region(at: usize, len: u64, what: &str) {
    if on() && at != 0 {
        with(|r| r.at.insert(at, Mapping { len, name: what.to_owned() }));
    }
}

/// Whether the resource of interface pointer `raw` is noted.
pub(crate) fn noted(raw: usize) -> bool {
    on() && with(|r| r.of.contains_key(&raw))
}

/// The resource of interface pointer `raw` is named `name`.
pub(crate) fn named(raw: usize, name: &str) {
    with(|r| {
        if let Some(at) = r.of.get(&raw).copied()
            && let Some(m) = r.at.get_mut(&at)
        {
            m.name = name.to_owned();
        }
    });
}

/// The kinds of ours mapped in `[base, base + len)` and their bytes.
pub(crate) fn kinds_in(base: usize, len: usize) -> Vec<(String, u64)> {
    let mut out: Vec<(String, u64)> = Vec::new();
    with(|r| {
        for (_, m) in r.at.range(base..base.saturating_add(len)) {
            let k = kind(&m.name);
            match out.iter_mut().find(|(n, _)| n == k) {
                Some(e) => e.1 += m.len,
                None => out.push((k.to_owned(), m.len)),
            }
        }
    });
    out
}

/// All we have mapped, by kind: buffers and bytes, biggest first.
pub(crate) fn by_kind() -> Vec<(String, u64, u64)> {
    let mut out: HashMap<String, (u64, u64)> = HashMap::new();
    with(|r| {
        for m in r.at.values() {
            let e = out.entry(kind(&m.name).to_owned()).or_default();
            e.0 += 1;
            e.1 += m.len;
        }
    });
    let mut out: Vec<(String, u64, u64)> = out.into_iter().map(|(k, (n, b))| (k, n, b)).collect();
    out.sort_by_key(|e| std::cmp::Reverse(e.2));
    out
}

/// An object the resource holds and releases when it goes.
#[repr(C)]
struct Gone {
    methods: &'static [*const (); 3],
    references: AtomicU32,
    at: usize,
    raw: usize,
}

struct Methods([*const (); 3]);
// SAFETY: function addresses only.
unsafe impl Sync for Methods {}

static GONE: Methods = Methods([query as *const (), add_ref as *const (), release as *const ()]);

unsafe extern "system" fn query(this: *mut c_void, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    // SAFETY: COM's arguments.
    unsafe {
        if out.is_null() {
            return HRESULT(0x8000_4003_u32 as i32);
        }
        if iid.as_ref().is_some_and(|iid| *iid == IUnknown::IID) {
            add_ref(this);
            *out = this;
            return HRESULT(0);
        }
        *out = std::ptr::null_mut();
    }
    HRESULT(0x8000_4002_u32 as i32)
}

unsafe extern "system" fn add_ref(this: *mut c_void) -> u32 {
    // SAFETY: one of ours, alive while a reference to it is.
    unsafe { (*this.cast::<Gone>()).references.fetch_add(1, Ordering::Relaxed) + 1 }
}

unsafe extern "system" fn release(this: *mut c_void) -> u32 {
    // SAFETY: as above; the last reference frees the box it was made as.
    unsafe {
        let left = (*this.cast::<Gone>()).references.fetch_sub(1, Ordering::AcqRel) - 1;
        if left == 0 {
            let gone = Box::from_raw(this.cast::<Gone>());
            with(|r| {
                if r.of.get(&gone.raw) == Some(&gone.at) {
                    r.of.remove(&gone.raw);
                    r.at.remove(&gone.at);
                }
            });
        }
        left
    }
}

fn gone_with(resource: &ID3D12Resource, at: usize) {
    const KEY: GUID = GUID::from_u128(0x2b7c_51d4_8e0a_4f6c_9d13_6a5e_0c47_b821);
    let raw = resource.as_raw() as usize;
    let made = Box::into_raw(Box::new(Gone { methods: &GONE.0, references: AtomicU32::new(1), at, raw }));
    // SAFETY: a COM object of ours with one reference, which this takes and
    // drops at the end; the resource takes its own.
    unsafe {
        let object = IUnknown::from_raw(made.cast());
        let _ = resource.SetPrivateDataInterface(&KEY, &object);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn kinds_leave_sizes_and_addresses_out() {
        assert_eq!(super::kind("staging of 0x1f2 128x128x1 f71"), "staging of");
        assert_eq!(super::kind("staging of textures (a block)"), "staging of textures (a block)");
        assert_eq!(super::kind("bytes a submission sends up"), "bytes a submission sends up");
    }
}
