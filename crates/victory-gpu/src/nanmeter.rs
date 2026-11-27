//! (Fox T, diagnostics) Which programs make NaNs and infinities
//! (`VICTORY_NAN_METER=1`). The translator makes every float a program
//! stores through a typed view or exports to a float colour target count
//! itself, before any guard, when it is a NaN or an infinity
//! (`victory_shader::emit::SLOT_NAN`): an atomic add to the program's four
//! dwords in a buffer of ours the CPU maps. Each title pipeline gets its
//! place on first use; the frame lines read and clear the counts.
//! What the programs compute is unchanged.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use victory_shader::emit::{NAN_ORIGINS, NAN_ORIGIN_DWORDS, NAN_SLOTS};
use windows::Win32::Graphics::Direct3D12::ID3D12Resource;

/// `VICTORY_NAN_METER=1` (the translator reads the same switch).
pub(crate) fn on() -> bool {
    victory_shader::emit::nan_meter()
}

struct Counts {
    _buffer: ID3D12Resource,
    data: usize,
    gpu: u64,
}

// SAFETY: a free-threaded object and its mapping.
unsafe impl Send for Counts {}
// SAFETY: as above.
unsafe impl Sync for Counts {}

fn counts() -> Option<&'static Counts> {
    static MADE: OnceLock<Option<Counts>> = OnceLock::new();
    MADE.get_or_init(|| {
        let host = crate::host::host()?;
        let bytes = (u64::from(NAN_SLOTS) * 4 + u64::from(NAN_ORIGINS * NAN_ORIGIN_DWORDS)) * 4;
        let (buffer, data) = host.shared_buffer(bytes)?;
        // SAFETY: a fresh mapping of that many bytes.
        unsafe { std::ptr::write_bytes(data, 0, bytes as usize) };
        // SAFETY: a live buffer.
        let gpu = unsafe { buffer.GetGPUVirtualAddress() };
        tracing::info!("NaN meter on (VICTORY_NAN_METER=1): NaNs and infinities counted per program where they are stored");
        Some(Counts { _buffer: buffer, data: data as usize, gpu })
    })
    .as_ref()
}

/// Where the shaders count (its GPU address), for the root UAV.
pub(crate) fn address() -> Option<u64> {
    counts().map(|c| c.gpu)
}

/// Places by title pipeline, and their names.
static PLACES: Mutex<Option<(HashMap<usize, u32>, Vec<Arc<str>>)>> = Mutex::new(None);

/// The place of the title pipeline `pipeline` (its programs' names `label`).
pub(crate) fn slot(pipeline: usize, label: impl FnOnce() -> Arc<str>) -> u32 {
    let mut places = PLACES.lock().unwrap_or_else(|e| e.into_inner());
    let (by, names) = places.get_or_insert_with(|| (HashMap::new(), Vec::new()));
    if let Some(&at) = by.get(&pipeline) {
        return at;
    }
    let at = (names.len() as u32).min(NAN_SLOTS - 1);
    if names.len() < NAN_SLOTS as usize - 1 {
        names.push(label());
    }
    by.insert(pipeline, at);
    at
}

/// For the frame lines: the programs with counts since the last one.
pub(crate) fn line(frames: f64) {
    if !on() {
        return;
    }
    let Some(c) = counts() else { return };
    let names: Vec<Arc<str>> = PLACES.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|p| p.1.clone()).unwrap_or_default();
    let mut rows: Vec<(u64, String)> = Vec::new();
    let mut all = [0u64; 4];
    for (i, name) in names.iter().enumerate() {
        // SAFETY: inside the mapping; the card may add while this reads and
        // clears (a count lost to the race goes to the next line or nowhere).
        let v: [u32; 4] = unsafe {
            let at = (c.data as *mut u32).add(i * 4);
            let v = [at.read_volatile(), at.add(1).read_volatile(), at.add(2).read_volatile(), at.add(3).read_volatile()];
            if v != [0; 4] {
                std::ptr::write_bytes(at, 0, 4);
            }
            v
        };
        if v == [0; 4] {
            continue;
        }
        for k in 0..4 {
            all[k] += u64::from(v[k]);
        }
        let total = v.iter().map(|&x| u64::from(x)).sum::<u64>();
        let _ = total;
        rows.push((
            u64::from(v[0]) + u64::from(v[1]) + u64::from(v[3]),
            format!(
                "{} (float-made stores NaN {:.0} inf {:.0}; integer or unknown-origin stores with such bits {:.0}; colour exports {:.0})",
                name.chars().take(140).collect::<String>(),
                f64::from(v[0]) / frames,
                f64::from(v[1]) / frames,
                f64::from(v[2]) / frames,
                f64::from(v[3]) / frames
            ),
        ));
    }
    origins(c, frames);
    if rows.is_empty() {
        return;
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    let f = frames.max(1.0);
    tracing::info!(
        "NaNs and infinities made per frame (VICTORY_NAN_METER): stores of values a float operation made: NaN {:.0} inf {:.0}; stores of integer or unknown-origin values with NaN/inf bits (likely integers) {:.0}; colour exports {:.0}; by {} programs; the most, by float-made ones: {}",
        all[0] as f64 / f,
        all[1] as f64 / f,
        all[2] as f64 / f,
        all[3] as f64 / f,
        rows.len(),
        rows.iter().take(12).map(|r| r.1.as_str()).collect::<Vec<_>>().join("; ")
    );
}

/// (`VICTORY_NAN_ORIGIN`) Per named program: the instruction dwords whose float
/// results were NaN, the first in the code first.
fn origins(c: &Counts, frames: f64) {
    for k in 0..NAN_ORIGINS as usize {
        let mut hits: Vec<(usize, u32)> = Vec::new();
        for d in 0..NAN_ORIGIN_DWORDS as usize {
            // SAFETY: inside the mapping (after the programs' counts).
            let n = unsafe {
                let at = (c.data as *mut u32).add(NAN_SLOTS as usize * 4 + k * NAN_ORIGIN_DWORDS as usize + d);
                let n = at.read_volatile();
                if n != 0 {
                    at.write_volatile(0);
                }
                n
            };
            if n != 0 {
                hits.push((d * 4, n));
            }
        }
        if hits.is_empty() {
            continue;
        }
        let text: Vec<String> = hits.iter().take(24).map(|(at, n)| format!("{at:#06x}: {:.0}", f64::from(*n) / frames.max(1.0))).collect();
        tracing::info!(
            "NaN origins of program {k} of VICTORY_NAN_ORIGIN per frame (instruction byte offset: NaN results; the first is where they start): {}",
            text.join(", ")
        );
    }
}
