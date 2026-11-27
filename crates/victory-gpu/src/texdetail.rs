//! Finding F11 (meter only; diagnostics builds; fox port C2 from victory-dev:
//! OFF by default, `VICTORY_TEXTURE_DETAIL=16` looks at one frame in 16; with
//! it on, a bound texture's host copy may be made a moment earlier than its
//! draw would have, as in the old tree): whether blurry or missing
//! textures are the title holding low detail or the layer showing stale
//! copies. When a streamed texture changes level the title moves it and
//! rewrites its shader view's record (MostDetailedMip = the first resident
//! mip); with nothing resident it copies a 0x20-byte default record (the
//! "missing texture" placeholder) from the pointers at 0x1475e7540 + 8 * i.
//! Over the texture views draws bind (each title record once per frame):
//! the first mips, the default records, and for first mip 0 the state of
//! the host copy; and textures made at a new address with a description
//! seen before (a level change re-addressed). A line every five seconds:
//! "texture detail: ...".

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

use crate::descriptor::View;
use crate::object::This;

fn on() -> bool {
    every() != 0
}

/// (P3, recording cost) One frame in how many is looked at
/// (`VICTORY_TEXTURE_DETAIL`: 0 off, 1 every frame as before, N one in N;
/// fox port: default 0). Its lock, record reads and staging queries took about 45 % of
/// `draw::readable` on the recording threads (run-79 profile); the meter's
/// counts are shares, which a sample of frames keeps.
fn every() -> u64 {
    static N: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *N.get_or_init(|| match std::env::var("VICTORY_TEXTURE_DETAIL") {
        Ok(v) => v.trim().parse().unwrap_or(0),
        Err(_) => 0,
    })
}

/// Since the last line: bound records; first mip 0, 1, 2, 3, 4+; default
/// records; of first mip 0: complete, partial, not uploaded, stale, null;
/// records not known (no title address); re-addressed; of them made anew,
/// reused.
static N: [AtomicU64; 15] = [const { AtomicU64::new(0) }; 15];
const BOUND: usize = 0;
const MIP: usize = 1;
const DEFAULTS: usize = 6;
const COMPLETE: usize = 7;
const PARTIAL: usize = 8;
const NOT_UPLOADED: usize = 9;
const STALE: usize = 10;
const NULL: usize = 11;
const UNKNOWN: usize = 12;
const READDRESSED: usize = 13;
const NEW_COPY: usize = 14;

struct Seen {
    frame: u64,
    records: HashSet<u64>,
    /// The default records (0x20 bytes each) and when read (ms).
    defaults: Vec<[u8; 32]>,
    read_ms: u64,
    line_ms: u64,
}

static SEEN: Mutex<Option<Seen>> = Mutex::new(None);

fn read32(at: u64) -> Option<[u8; 32]> {
    let mut b = [0u8; 32];
    (at != 0 && crate::host::read_into(at as *const u8, &mut b)).then_some(b)
}

/// The title's 8 bytes at its virtual address `va` (image base 0x1_4000_0000).
fn title_u64(va: u64) -> Option<u64> {
    let base = crate::hlehooks::readback::image();
    if base == 0 {
        return None;
    }
    let mut b = [0u8; 8];
    crate::host::read_into((base + va - 0x1_4000_0000) as *const u8, &mut b).then(|| u64::from_le_bytes(b))
}

/// The default records from the title's table (non-null pointers only).
fn defaults() -> Vec<[u8; 32]> {
    let mut out = Vec::new();
    for i in 0..16u64 {
        if let Some(p) = title_u64(0x1_475e_7540 + 8 * i)
            && p != 0
            && let Some(r) = read32(p)
            && !out.contains(&r)
        {
            out.push(r);
        }
    }
    out
}

/// The first mip of a texture view's description, if it is a texture view
/// (dimensions 2-9: word 4 is MostDetailedMip for each).
fn first_mip(desc: &[u32]) -> Option<u32> {
    (desc.len() > 4 && (2..=9).contains(&desc[1])).then(|| desc[4].min(15))
}

/// A shader view a draw binds: `view` as the title wrote it, `placed` the
/// view the layer made of it (a placed texture), `record` its title
/// descriptor's address (0: not known).
pub(crate) fn bound(view: &View, placed: Option<&View>, record: u64) {
    let n = every();
    if n == 0 || (n > 1 && !crate::frame::current().is_multiple_of(n)) {
        return;
    }
    // The texture's first mip and the layer's resource, or not a texture.
    let (mip, resource) = match (view, placed) {
        (View::Placed(false, res, desc), p) => {
            if res[0] == 1 {
                return;
            }
            let r = match p {
                Some(View::Shader(r, _)) => *r,
                _ => 0,
            };
            (first_mip(desc).unwrap_or(0), r)
        }
        (View::Shader(r, Some(desc)), _) => match first_mip(desc) {
            Some(m) => (m, *r),
            None => return,
        },
        (View::Shader(r, None), _) => {
            if crate::resource::buffer_bytes(*r as This).is_some() {
                return;
            }
            (0, *r)
        }
        _ => return,
    };
    let now = crate::host::now_ms();
    let is_default = {
        let mut g = SEEN.lock().unwrap_or_else(|e| e.into_inner());
        let s = g.get_or_insert_with(|| Seen { frame: u64::MAX, records: HashSet::new(), defaults: Vec::new(), read_ms: 0, line_ms: now });
        let frame = crate::frame::current();
        if s.frame != frame {
            s.frame = frame;
            s.records.clear();
        }
        if record == 0 {
            N[UNKNOWN].fetch_add(1, Relaxed);
            return;
        }
        if !s.records.insert(record) {
            return;
        }
        if s.defaults.is_empty() && now > s.read_ms + 1000 || now > s.read_ms + 5000 {
            s.read_ms = now;
            s.defaults = defaults();
        }
        let d = !s.defaults.is_empty() && read32(record).is_some_and(|r| s.defaults.contains(&r));
        if now > s.line_ms + 5000 {
            s.line_ms = now;
            line();
        }
        d
    };
    N[BOUND].fetch_add(1, Relaxed);
    N[MIP + mip.min(4) as usize].fetch_add(1, Relaxed);
    if is_default {
        N[DEFAULTS].fetch_add(1, Relaxed);
    }
    if mip != 0 {
        return;
    }
    let slot = if resource == 0 || crate::resource::host_resource(resource as This).is_none() {
        NULL
    } else {
        match crate::texture::staged_state(resource as This) {
            // No staging: a texture the card writes, or never uploaded.
            None => {
                if crate::resource::host_resource(resource as This).is_some_and(crate::texture::title_written) {
                    NOT_UPLOADED
                } else {
                    COMPLETE
                }
            }
            Some((false, _)) => NOT_UPLOADED,
            Some((true, true)) => STALE,
            Some((true, false)) => match crate::texture::staged(resource as This, false) {
                Some((_, false)) => PARTIAL,
                _ => COMPLETE,
            },
        }
    };
    N[slot].fetch_add(1, Relaxed);
}

/// A texture was made at `address` for a description: counted as
/// re-addressed when that description was seen at another address (the
/// layer keys host copies by address and description, so it is a new copy).
pub(crate) fn texture_made(address: u64, desc: &[u32; 14]) {
    if !on() {
        return;
    }
    static LAST: Mutex<Option<HashMap<[u32; 14], u64>>> = Mutex::new(None);
    let mut g = LAST.lock().unwrap_or_else(|e| e.into_inner());
    let m = g.get_or_insert_with(HashMap::new);
    if m.len() > 1 << 18 {
        m.clear();
    }
    if let Some(old) = m.insert(*desc, address)
        && old != address
    {
        N[READDRESSED].fetch_add(1, Relaxed);
        N[NEW_COPY].fetch_add(1, Relaxed);
    }
}

fn line() {
    let n: Vec<u64> = N.iter().map(|a| a.swap(0, Relaxed)).collect();
    tracing::info!(
        "texture detail: bound SRV textures {}; first mip 0/1/2/3/4+: {}/{}/{}/{}/{}; default placeholders {}; of first-mip-0: complete {}, \
         partial {}, not uploaded {}, stale {}, null {}; re-addressed {} (new copy {}, reused {}); records without a title address {} \
         (5 s, each record once per frame, one frame in {} looked at (VICTORY_TEXTURE_DETAIL); 'complete' includes textures the card writes)",
        n[BOUND],
        n[MIP],
        n[MIP + 1],
        n[MIP + 2],
        n[MIP + 3],
        n[MIP + 4],
        n[DEFAULTS],
        n[COMPLETE],
        n[PARTIAL],
        n[NOT_UPLOADED],
        n[STALE],
        n[NULL],
        n[READDRESSED],
        n[NEW_COPY],
        n[READDRESSED] - n[NEW_COPY],
        n[UNKNOWN],
        every(),
    );
}
