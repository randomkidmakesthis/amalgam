//! (Fox V; `VICTORY_PAGE_MIGRATE=verify|1`, default off; only with
//! `VICTORY_PAGE_POOL=1`) Pages of copies in sparse page heaps move into
//! fuller heaps, so that the sparse heaps empty and go back (run-173: 110
//! heaps, 1.7-1.85 GB free pages spread over them; a heap goes only when
//! wholly empty). The copy itself stays: the same texture object, the same
//! views; only the memory under some of its tiles changes.
//!
//! A move, on a thread of its own, every [`STEP_MS`]:
//! - the sparsest heap at most a quarter in use is marked emptying (new
//!   copies take pages of it last); copies with pages in it qualify when they
//!   were made over [`SETTLED_MS`] ago, have their host record
//!   (`pool::pages_adopt`), were filled (`holds`), and have no upload made
//!   ahead pending;
//! - new pages for the tiles of it are taken from other heaps (fullest first;
//!   never a new heap); two reserved textures of the copy's description are
//!   made: one over the old pages (source), one over the new (destination),
//!   both on the host's queue (`UpdateTileMappings`) ahead of the copy;
//! - between two title submissions (the submission lock,
//!   `host::between_submissions`): a list copies source to destination
//!   (`CopyResource`; tiles without memory read zeros and drop writes), an
//!   aliasing barrier, and the copy's own tiles are mapped to the new pages.
//!   Nothing of the title's runs between the copy and the mapping, so every
//!   upload lands either before the copy (and is copied) or after the mapping
//!   (on the new pages). The copy's state is not touched (no barrier on it);
//! - the old pages stay taken until the card has run all that and (verify
//!   mode) the old and new pages' tiles were read back and compared byte for
//!   byte; then they rest [`REST`] like any page given back. A mismatch maps
//!   the copy back to its old pages, stops moving for the run, and is logged.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use windows::Win32::Graphics::Direct3D12::*;
use windows::core::Interface;

use super::{HEAP_PAGES, PAGE, REST, copies, grouped, lock, regions};

/// Milliseconds between moves.
const STEP_MS: u64 = 250;
/// A copy made less than this long ago is not moved (uploads made ahead).
const SETTLED_MS: u64 = 2000;
/// The most tiles moved, and bytes of copies copied, in one step.
const STEP_TILES: u32 = 256;
const STEP_BYTES: u64 = 64 << 20;
/// The most copies in one step.
const STEP_COPIES: usize = 8;
/// A heap at most this many pages in use is emptied.
const SPARSE: u32 = HEAP_PAGES / 4;
/// Microseconds a move may hold the submission lock before moving stops.
const HELD_MOST_US: u64 = 2000;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Off,
    Verify,
    On,
}

fn mode() -> Mode {
    static MODE: std::sync::OnceLock<Mode> = std::sync::OnceLock::new();
    *MODE.get_or_init(|| {
        let mode = match std::env::var("VICTORY_PAGE_MIGRATE").as_deref().map(str::trim) {
            Ok("1") => Mode::On,
            Ok("verify") => Mode::Verify,
            _ => Mode::Off,
        };
        if mode != Mode::Off && super::mode() != super::Mode::On {
            tracing::warn!("VICTORY_PAGE_MIGRATE without VICTORY_PAGE_POOL=1: nothing is moved");
            return Mode::Off;
        }
        if mode != Mode::Off {
            tracing::info!(
                verify = mode == Mode::Verify,
                "pages of copies in sparse page heaps move into fuller ones (VICTORY_PAGE_MIGRATE={})",
                if mode == Mode::On { "1" } else { "verify" }
            );
        }
        mode
    })
}

/// Moving stopped for the run (a mismatch).
static STOPPED: AtomicBool = AtomicBool::new(false);

/// Since the start: copies moved, tiles moved, bytes copied; verified moves,
/// mismatches; copies passed over (fresh, not filled, upload pending, no
/// record), steps without pages for a move; heaps marked emptying; µs
/// holding the submission lock (all, the longest), µs waiting for it.
static STATS: [AtomicU64; 13] = [const { AtomicU64::new(0) }; 13];
const S_MOVED: usize = 0;
const S_TILES: usize = 1;
const S_BYTES: usize = 2;
const S_VERIFIED: usize = 3;
const S_MISMATCH: usize = 4;
const S_PASSED: usize = 5;
const S_NO_PAGES: usize = 6;
const S_EMPTYING: usize = 7;
const S_HELD_US: usize = 8;
const S_HELD_MOST: usize = 9;
const S_WAIT_US: usize = 10;
/// Passed over for a packed tail split over heaps; moves whose card work
/// did not finish (the device went): not compared, nothing mapped back.
const S_SPLIT: usize = 11;
const S_UNFINISHED: usize = 12;

/// Starts the moving thread once (from the pool's tick).
pub(crate) fn start() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    if mode() == Mode::Off {
        return;
    }
    STARTED.call_once(|| {
        let Some(device) = crate::host::host().map(|h| h.device.clone()) else { return };
        let _ = std::thread::Builder::new().name("victory page moves".into()).spawn(move || run(&device));
    });
}

/// A move whose card work may not have run yet.
struct Pending {
    value: u64,
    texture: ID3D12Resource,
    raw: usize,
    /// The copy's tiles moved: (tile, old heap, old page, new heap, new page).
    tiles: Vec<(u32, usize, u32, usize, u32)>,
    tiling: super::Tiling,
    /// The two reserved textures and (verify) the read-back buffers.
    keep: Vec<ID3D12Resource>,
    verify: Option<(ID3D12Resource, ID3D12Resource)>,
    slot: usize,
}

struct Slot {
    allocator: ID3D12CommandAllocator,
    list: ID3D12GraphicsCommandList,
    busy: bool,
}

fn run(device: &ID3D12Device) {
    // SAFETY: plain creation.
    let made = unsafe {
        (|| -> windows::core::Result<(ID3D12Fence, Vec<Slot>)> {
            let fence: ID3D12Fence = device.CreateFence(0, D3D12_FENCE_FLAG_NONE)?;
            let mut slots = Vec::new();
            for _ in 0..4 {
                let allocator: ID3D12CommandAllocator = device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)?;
                let list: ID3D12GraphicsCommandList = device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)?;
                list.Close()?;
                slots.push(Slot { allocator, list, busy: false });
            }
            Ok((fence, slots))
        })()
    };
    let Ok((fence, mut slots)) = made else {
        tracing::warn!("no objects for moving texture pages: nothing is moved");
        return;
    };
    let mut value = 0u64;
    let mut pending: Vec<Pending> = Vec::new();
    loop {
        std::thread::sleep(std::time::Duration::from_millis(STEP_MS));
        // SAFETY: a live fence of ours.
        let done = unsafe { fence.GetCompletedValue() };
        let mut still = Vec::new();
        for p in pending.drain(..) {
            if p.value <= done {
                finish(p, &mut slots);
            } else {
                still.push(p);
            }
        }
        pending = still;
        if STOPPED.load(Ordering::Relaxed) {
            continue;
        }
        let Some(slot) = slots.iter().position(|s| !s.busy) else { continue };
        if let Some(p) = step(device, &fence, &mut value, &mut slots, slot) {
            pending.extend(p);
        }
    }
}

/// The old pages of a finished move rest (or, on a mismatch, the copy goes
/// back to them and the new ones rest).
fn finish(p: Pending, slots: &mut [Slot]) {
    slots[p.slot].busy = false;
    let mut back = false;
    // (V10) A device that went signals every fence: its read-backs say
    // nothing, and nothing is to be sent any more.
    // SAFETY: plain query.
    let gone = crate::host::host().is_none_or(|h| unsafe { h.device.GetDeviceRemovedReason() }.is_err());
    if gone {
        STATS[S_UNFINISHED].fetch_add(1, Ordering::Relaxed);
        STOPPED.store(true, Ordering::Relaxed);
        drop(p.keep);
        return;
    }
    if let Some((a, b)) = &p.verify {
        let bytes = p.tiles.len() * PAGE as usize;
        let same = read(a, bytes).zip(read(b, bytes)).is_some_and(|(x, y)| x == y);
        if same {
            STATS[S_VERIFIED].fetch_add(1, Ordering::Relaxed);
        } else {
            STATS[S_MISMATCH].fetch_add(1, Ordering::Relaxed);
            STOPPED.store(true, Ordering::Relaxed);
            tracing::error!(
                texture = p.raw,
                tiles = p.tiles.len(),
                "a moved texture's pages differ from the old ones: it goes back to them, and no more pages are moved this run (VICTORY_PAGE_MIGRATE=verify)"
            );
            back = true;
        }
    }
    if back {
        // The copy's tiles to the old pages again, between submissions.
        let old: Vec<(u32, usize, u32)> = p.tiles.iter().map(|t| (t.0, t.1, t.2)).collect();
        let heaps = heaps_of(old.iter().map(|t| t.1));
        crate::host::between_submissions(|queue| map(queue, &p.texture, &p.tiling, &old, &heaps));
        // Its record: the old pages.
        set_pages(p.raw, &p.tiles, true);
    }
    let gone: Vec<(usize, u32, u32)> =
        p.tiles.iter().map(|t| if back { (t.3, t.4, 1) } else { (t.1, t.2, 1) }).collect();
    lock().resting.push_back((crate::host::now_ms() + REST, grouped(&gone)));
    drop(p.keep);
}

fn read(buffer: &ID3D12Resource, bytes: usize) -> Option<Vec<u8>> {
    let mut data = std::ptr::null_mut();
    // SAFETY: a read-back buffer of ours of `bytes` bytes, whose writes have run.
    unsafe {
        buffer.Map(0, None, Some(&raw mut data)).ok()?;
        let out = std::slice::from_raw_parts(data.cast::<u8>(), bytes).to_vec();
        buffer.Unmap(0, Some(&D3D12_RANGE::default()));
        Some(out)
    }
}

/// The heaps of the pool by place, for the mapping calls.
fn heaps_of(places: impl Iterator<Item = usize>) -> Vec<(usize, ID3D12Heap)> {
    let pages = lock();
    let mut out: Vec<(usize, ID3D12Heap)> = Vec::new();
    for i in places {
        if !out.iter().any(|h| h.0 == i)
            && let Some(Some(h)) = pages.heaps.get(i)
        {
            out.push((i, h.heap.clone()));
        }
    }
    out
}

/// Maps `tiles` (tile, heap place, page) of `texture`, one call per heap,
/// one tile a region.
fn map(queue: &ID3D12CommandQueue, texture: &ID3D12Resource, t: &super::Tiling, tiles: &[(u32, usize, u32)], heaps: &[(usize, ID3D12Heap)]) {
    for (place, heap) in heaps {
        let mine: Vec<&(u32, usize, u32)> = tiles.iter().filter(|x| x.1 == *place).collect();
        if mine.is_empty() {
            continue;
        }
        let mut at = Vec::with_capacity(mine.len());
        let mut size = Vec::with_capacity(mine.len());
        for &&(tile, _, _) in &mine {
            let (a, s) = regions(t, tile, tile + 1);
            at.extend(a);
            size.extend(s);
        }
        let flags = vec![D3D12_TILE_RANGE_FLAG_NONE; mine.len()];
        let starts: Vec<u32> = mine.iter().map(|x| x.2).collect();
        let counts = vec![1u32; mine.len()];
        // SAFETY: a live reserved texture and a live heap of ours; one region
        // and one range of one tile each, in arrays that live through the call.
        unsafe {
            queue.UpdateTileMappings(
                texture,
                at.len() as u32,
                Some(at.as_ptr()),
                Some(size.as_ptr()),
                heap,
                starts.len() as u32,
                Some(flags.as_ptr()),
                Some(starts.as_ptr()),
                Some(counts.as_ptr()),
                D3D12_TILE_MAPPING_FLAG_NONE,
            );
        }
    }
}

/// The copy's record of its pages: moved tiles at their new (or, `back`,
/// old) pages.
fn set_pages(raw: usize, moved: &[(u32, usize, u32, usize, u32)], back: bool) {
    let mut all = copies();
    let Some(copy) = all.as_mut().and_then(|c| c.get_mut(&raw)) else { return };
    let mut tiles: Vec<(usize, u32)> = Vec::new();
    for &(heap, start, n) in &copy.segs {
        for k in 0..n {
            tiles.push((heap, start + k));
        }
    }
    for &(tile, oh, op, nh, np) in moved {
        if let Some(t) = tiles.get_mut(tile as usize) {
            *t = if back { (oh, op) } else { (nh, np) };
        }
    }
    let mut segs: Vec<(usize, u32, u32)> = Vec::new();
    for (heap, page) in tiles {
        match segs.last_mut() {
            Some(last) if last.0 == heap && last.1 + last.2 == page => last.2 += 1,
            _ => segs.push((heap, page, 1)),
        }
    }
    copy.segs = segs;
}

/// One step: the sparsest heap's copies, up to the step's limits.
fn step(device: &ID3D12Device, fence: &ID3D12Fence, value: &mut u64, slots: &mut [Slot], slot: usize) -> Option<Vec<Pending>> {
    let host = crate::host::host()?;
    let now = crate::host::now_ms();
    // The heap: the sparsest at most a quarter in use (resting pages count
    // as free soon), with room for its pages elsewhere.
    let heap = {
        let mut pages = lock();
        let mut resting = vec![0u32; pages.heaps.len()];
        for (_, runs) in &pages.resting {
            for (i, rs) in runs {
                if let Some(r) = resting.get_mut(*i) {
                    *r += rs.iter().map(|x| x.1).sum::<u32>();
                }
            }
        }
        let used = |i: usize, h: &super::Heap| HEAP_PAGES - h.free.pages - resting[i].min(HEAP_PAGES - h.free.pages);
        let elsewhere = |pages: &super::Pages, skip: usize| -> u32 {
            pages.heaps.iter().enumerate().filter(|(i, h)| *i != skip && h.as_ref().is_some_and(|h| !h.emptying)).map(|(_, h)| h.as_ref().map_or(0, |h| h.free.pages)).sum()
        };
        let pick = pages
            .heaps
            .iter()
            .enumerate()
            .filter_map(|(i, h)| {
                let h = h.as_ref()?;
                let u = used(i, h);
                (u > 0 && u <= SPARSE).then_some((h.emptying, u, i))
            })
            // One being emptied first (finish it), then the sparsest.
            .min_by_key(|&(emptying, u, i)| (!emptying, u, i))?;
        if elsewhere(&*pages, pick.2) < pick.1 {
            return None;
        }
        if let Some(Some(h)) = pages.heaps.get_mut(pick.2)
            && !h.emptying
        {
            h.emptying = true;
            STATS[S_EMPTYING].fetch_add(1, Ordering::Relaxed);
        }
        pick.2
    };
    // Its copies.
    let chosen: Vec<(usize, Vec<(usize, u32, u32)>, super::Tiling, D3D12_RESOURCE_DESC, u64, Option<std::sync::Weak<crate::resource::HostTexture>>)> = {
        let all = copies();
        all.as_ref()?
            .iter()
            .filter(|(_, c)| c.segs.iter().any(|s| s.0 == heap))
            .take(STEP_COPIES * 4)
            .map(|(raw, c)| (*raw, c.segs.clone(), c.tiling.clone(), c.desc, c.made, c.host.clone()))
            .collect()
    };
    let (mut tiles_left, mut bytes_left) = (STEP_TILES, STEP_BYTES);
    let mut out = Vec::new();
    let mut slot_now = Some(slot);
    for (raw, segs, tiling, desc, made, record) in chosen {
        if out.len() >= STEP_COPIES {
            break;
        }
        let Some(record) = record.and_then(|w| w.upgrade()) else {
            STATS[S_PASSED].fetch_add(1, Ordering::Relaxed);
            continue;
        };
        if record.texture.as_raw() as usize != raw
            || now < made + SETTLED_MS
            || record.ahead.load(Ordering::Relaxed) == 1
            || record.holds.lock().unwrap_or_else(|e| e.into_inner()).is_none()
        {
            STATS[S_PASSED].fetch_add(1, Ordering::Relaxed);
            continue;
        }
        // Its tiles in the heap.
        let mut affected: Vec<(u32, u32)> = Vec::new();
        let mut tile = 0u32;
        for &(h, start, n) in &segs {
            for k in 0..n {
                if h == heap {
                    affected.push((tile, start + k));
                }
                tile += 1;
            }
        }
        // (V10) The packed levels' tiles go together: a copy whose packed
        // tail lies partly in the heap is passed over (a tail partly mapped
        // in the reserved textures of the move is undefined).
        if let Some((_, packed)) = tiling.packed {
            let first_packed = tiling.total - packed;
            let in_heap = affected.iter().filter(|a| a.0 >= first_packed).count() as u32;
            if in_heap > 0 && in_heap < packed {
                STATS[S_PASSED].fetch_add(1, Ordering::Relaxed);
                STATS[S_SPLIT].fetch_add(1, Ordering::Relaxed);
                continue;
            }
        }
        let n = affected.len() as u32;
        // SAFETY: plain query.
        let size = unsafe { device.GetResourceAllocationInfo(0, &[desc]) }.SizeInBytes;
        if n == 0 || n > tiles_left || size > bytes_left {
            continue;
        }
        let Some(slot) = slot_now.take().or_else(|| slots.iter().position(|s| !s.busy)) else { break };
        // New pages, never in the emptied heap.
        let new = {
            let mut pages = lock();
            take_elsewhere(&mut pages, heap, n)
        };
        let Some(new) = new else {
            STATS[S_NO_PAGES].fetch_add(1, Ordering::Relaxed);
            break;
        };
        let moved: Vec<(u32, usize, u32, usize, u32)> =
            affected.iter().zip(&new).map(|(&(t, op), &(nh, np))| (t, heap, op, nh, np)).collect();
        match one(device, host, fence, value, &mut slots[slot], raw, &record.texture, &tiling, &desc, &moved, mode() == Mode::Verify) {
            Some((v, keep, verify)) => {
                slots[slot].busy = true;
                set_pages(raw, &moved, false);
                STATS[S_MOVED].fetch_add(1, Ordering::Relaxed);
                STATS[S_TILES].fetch_add(u64::from(n), Ordering::Relaxed);
                STATS[S_BYTES].fetch_add(size, Ordering::Relaxed);
                tiles_left -= n;
                bytes_left -= size;
                out.push(Pending {
                    value: v,
                    texture: record.texture.clone(),
                    raw,
                    tiles: moved,
                    tiling,
                    keep,
                    verify,
                    slot,
                });
            }
            None => {
                // Nothing was swapped: the new pages go back.
                let back: Vec<(usize, u32, u32)> = new.iter().map(|&(h, p)| (h, p, 1)).collect();
                let mut pages = lock();
                for (i, rs) in grouped(&back) {
                    if let Some(Some(h)) = pages.heaps.get_mut(i) {
                        h.free.give(&rs);
                    }
                }
                slot_now = Some(slot);
            }
        }
    }
    Some(out)
}

/// `n` free pages of heaps other than `skip` and not being emptied, the
/// fullest first, a page each.
fn take_elsewhere(pages: &mut super::Pages, skip: usize, n: u32) -> Option<Vec<(usize, u32)>> {
    // (V11) Only heaps that every submission begun before their making has
    // left behind: the copy is read by lists already on the card, and new
    // memory under such a texture lost the device (n08/n11; run-183 lost it
    // shortly after the first moves).
    let finished = crate::host::submissions().1;
    let mut order: Vec<(u32, usize)> = pages
        .heaps
        .iter()
        .enumerate()
        .filter_map(|(i, h)| {
            h.as_ref()
                .filter(|h| i != skip && !h.emptying && h.free.pages > 0 && h.made <= finished)
                .map(|h| (h.free.pages, i))
        })
        .collect();
    if order.iter().map(|o| o.0).sum::<u32>() < n {
        return None;
    }
    order.sort_unstable();
    let mut out = Vec::new();
    let mut left = n;
    for (_, i) in order {
        if left == 0 {
            break;
        }
        let Some(Some(h)) = pages.heaps.get_mut(i) else { continue };
        let mut got = Vec::new();
        left -= h.free.take(left, &mut got);
        for (start, k) in got {
            for j in 0..k {
                out.push((i, start + j));
            }
        }
    }
    (left == 0).then_some(out)
}

/// The card work of one move; `None`: nothing sent (an object could not be
/// made). Returns the fence value, the objects to keep until it, and
/// (verify) the two read-back buffers.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn one(
    device: &ID3D12Device,
    host: &crate::host::Host,
    fence: &ID3D12Fence,
    value: &mut u64,
    slot: &mut Slot,
    _raw: usize,
    texture: &ID3D12Resource,
    tiling: &super::Tiling,
    desc: &D3D12_RESOURCE_DESC,
    moved: &[(u32, usize, u32, usize, u32)],
    verify: bool,
) -> Option<(u64, Vec<ID3D12Resource>, Option<(ID3D12Resource, ID3D12Resource)>)> {
    let reserved = |state: D3D12_RESOURCE_STATES| -> Option<ID3D12Resource> {
        let mut made: Option<ID3D12Resource> = None;
        // SAFETY: the copy's own description (tiles' layout).
        unsafe { device.CreateReservedResource(desc, state, None, &mut made) }.ok()?;
        made
    };
    let source = reserved(D3D12_RESOURCE_STATE_COPY_SOURCE)?;
    let dest = reserved(D3D12_RESOURCE_STATE_COPY_DEST)?;
    let bytes = moved.len() as u64 * PAGE;
    let readback = |_: ()| -> Option<ID3D12Resource> {
        let mut made: Option<ID3D12Resource> = None;
        let d = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Width: bytes,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            ..Default::default()
        };
        // SAFETY: plain creation.
        unsafe {
            device.CreateCommittedResource(
                &D3D12_HEAP_PROPERTIES { Type: D3D12_HEAP_TYPE_READBACK, ..Default::default() },
                D3D12_HEAP_FLAG_NONE,
                &d,
                D3D12_RESOURCE_STATE_COPY_DEST,
                None,
                &mut made,
            )
        }
        .ok()?;
        made
    };
    let checks = if verify { Some((readback(())?, readback(())?)) } else { None };
    let scratch = scratch_buffer(device, bytes)?;
    let old: Vec<(u32, usize, u32)> = moved.iter().map(|m| (m.0, m.1, m.2)).collect();
    let new: Vec<(u32, usize, u32)> = moved.iter().map(|m| (m.0, m.3, m.4)).collect();
    let heaps = heaps_of(moved.iter().flat_map(|m| [m.1, m.3]));
    let queue = host.queue();
    // The two of ours over the old and the new pages, ahead on the queue.
    map(queue, &source, tiling, &old, &heaps);
    map(queue, &dest, tiling, &new, &heaps);
    // SAFETY: the slot's allocator and list, whose last work has run (the
    // slot is free); objects that live until the fence value is reached.
    unsafe {
        slot.allocator.Reset().ok()?;
        slot.list.Reset(&slot.allocator, None).ok()?;
        // (V10) Tile for tile, raw: out of the old pages into a buffer of
        // ours, back into the new ones (`CopyResource` copies levels, and a
        // packed tail with tiles of no memory is undefined: run-178).
        for (k, m) in moved.iter().enumerate() {
            let (at, size) = regions(tiling, m.0, m.0 + 1);
            let (Some(at), Some(size)) = (at.first(), size.first()) else { continue };
            slot.list.CopyTiles(&source, at, size, &scratch, k as u64 * PAGE, D3D12_TILE_COPY_FLAG_SWIZZLED_TILED_RESOURCE_TO_LINEAR_BUFFER);
        }
        slot.list.ResourceBarrier(&[crate::host::transition(&scratch, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_COPY_SOURCE)]);
        for (k, m) in moved.iter().enumerate() {
            let (at, size) = regions(tiling, m.0, m.0 + 1);
            let (Some(at), Some(size)) = (at.first(), size.first()) else { continue };
            slot.list.CopyTiles(&dest, at, size, &scratch, k as u64 * PAGE, D3D12_TILE_COPY_FLAG_LINEAR_BUFFER_TO_SWIZZLED_TILED_RESOURCE);
        }
        let alias = D3D12_RESOURCE_BARRIER {
            Type: D3D12_RESOURCE_BARRIER_TYPE_ALIASING,
            Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
            Anonymous: D3D12_RESOURCE_BARRIER_0 {
                Aliasing: std::mem::ManuallyDrop::new(D3D12_RESOURCE_ALIASING_BARRIER {
                    pResourceBefore: std::mem::ManuallyDrop::new(None),
                    pResourceAfter: std::mem::ManuallyDrop::new(None),
                }),
            },
        };
        slot.list.ResourceBarrier(&[alias]);
        if let Some((a, b)) = &checks {
            // The new pages through ours (after the copy) and the old ones.
            slot.list.ResourceBarrier(&[crate::host::transition(&dest, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_COPY_SOURCE)]);
            for (k, m) in moved.iter().enumerate() {
                let (at, size) = regions(tiling, m.0, m.0 + 1);
                let (Some(at), Some(size)) = (at.first(), size.first()) else { continue };
                let offset = k as u64 * PAGE;
                slot.list.CopyTiles(&dest, at, size, a, offset, D3D12_TILE_COPY_FLAG_SWIZZLED_TILED_RESOURCE_TO_LINEAR_BUFFER);
                slot.list.CopyTiles(&source, at, size, b, offset, D3D12_TILE_COPY_FLAG_SWIZZLED_TILED_RESOURCE_TO_LINEAR_BUFFER);
            }
        }
        slot.list.Close().ok()?;
    }
    let list: ID3D12CommandList = slot.list.cast().ok()?;
    // The copy and the copy's own mapping with nothing of the title's between.
    let asked = std::time::Instant::now();
    let held_us = crate::host::between_submissions(|queue| {
        let held = std::time::Instant::now();
        STATS[S_WAIT_US].fetch_add(held.duration_since(asked).as_micros() as u64, Ordering::Relaxed);
        // SAFETY: a closed list of ours; the queue is the host's.
        unsafe { queue.ExecuteCommandLists(&[Some(list.clone())]) };
        map(queue, texture, tiling, &new, &heaps);
        *value += 1;
        // SAFETY: a live fence of ours.
        let _ = unsafe { queue.Signal(fence, *value) };
        let us = held.elapsed().as_micros() as u64;
        STATS[S_HELD_US].fetch_add(us, Ordering::Relaxed);
        STATS[S_HELD_MOST].fetch_max(us, Ordering::Relaxed);
        us
    })?;
    // (V11) A move whose queue calls took long (the card or the driver held
    // them) stops moving for the run: the title's submissions wait for it.
    if held_us > HELD_MOST_US {
        STOPPED.store(true, Ordering::Relaxed);
        tracing::warn!(held_us, "a page move held the submission lock {held_us} us: no more pages are moved this run (VICTORY_PAGE_MIGRATE)");
    }
    let mut keep = vec![source.clone(), dest, scratch];
    if let Some((a, b)) = &checks {
        keep.push(a.clone());
        keep.push(b.clone());
    }
    Some((*value, keep, checks.map(|(a, b)| (a, b))))
}

/// A buffer on the card of `bytes` for tiles on their way.
fn scratch_buffer(device: &ID3D12Device, bytes: u64) -> Option<ID3D12Resource> {
    let d = D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
        Width: bytes,
        Height: 1,
        DepthOrArraySize: 1,
        MipLevels: 1,
        SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        ..Default::default()
    };
    let mut made: Option<ID3D12Resource> = None;
    // SAFETY: plain creation.
    unsafe {
        device.CreateCommittedResource(
            &D3D12_HEAP_PROPERTIES { Type: D3D12_HEAP_TYPE_DEFAULT, ..Default::default() },
            D3D12_HEAP_FLAG_NONE,
            &d,
            D3D12_RESOURCE_STATE_COMMON,
            None,
            &mut made,
        )
    }
    .ok()?;
    made
}

/// For the pages line (empty when off).
#[cfg(feature = "diagnostics")]
pub(crate) fn line() -> String {
    if mode() == Mode::Off {
        return String::new();
    }
    let s: [u64; 13] = std::array::from_fn(|i| STATS[i].load(Ordering::Relaxed));
    format!(
        " | page moves (VICTORY_PAGE_MIGRATE={}{}): {} copies moved, {} tiles ({} MB), {} MB of copies copied; verified {}, mismatches {}; \
         passed over {} (packed tail split over heaps {}), steps without pages {}; heaps marked emptying {}; submission lock held {} us in all (the longest {} us), waited for {} us; \
         moves not finished as the device went {};",
        if mode() == Mode::On { "1" } else { "verify" },
        if STOPPED.load(Ordering::Relaxed) { ", STOPPED" } else { "" },
        s[S_MOVED],
        s[S_TILES],
        s[S_TILES] * PAGE >> 20,
        s[S_BYTES] >> 20,
        s[S_VERIFIED],
        s[S_MISMATCH],
        s[S_PASSED],
        s[S_SPLIT],
        s[S_NO_PAGES],
        s[S_EMPTYING],
        s[S_HELD_US],
        s[S_HELD_MOST],
        s[S_WAIT_US],
        s[S_UNFINISHED],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC};

    fn wait(fence: &ID3D12Fence, value: u64) {
        // SAFETY: a live fence; without an event this waits.
        unsafe { fence.SetEventOnCompletion(value, windows::Win32::Foundation::HANDLE::default()) }.unwrap();
    }

    fn buffer(device: &ID3D12Device, kind: D3D12_HEAP_TYPE, bytes: u64, state: D3D12_RESOURCE_STATES) -> ID3D12Resource {
        let d = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Width: bytes,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            ..Default::default()
        };
        let mut made: Option<ID3D12Resource> = None;
        // SAFETY: plain creation.
        unsafe {
            device.CreateCommittedResource(
                &D3D12_HEAP_PROPERTIES { Type: kind, ..Default::default() },
                D3D12_HEAP_FLAG_NONE,
                &d,
                state,
                None,
                &mut made,
            )
        }
        .unwrap();
        made.unwrap()
    }

    fn location(resource: &ID3D12Resource, footprint: Option<D3D12_PLACED_SUBRESOURCE_FOOTPRINT>) -> D3D12_TEXTURE_COPY_LOCATION {
        D3D12_TEXTURE_COPY_LOCATION {
            // SAFETY: a borrowed pointer, never released through this.
            pResource: unsafe { std::mem::transmute_copy(resource) },
            Type: if footprint.is_some() { D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT } else { D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX },
            Anonymous: match footprint {
                Some(f) => D3D12_TEXTURE_COPY_LOCATION_0 { PlacedFootprint: f },
                None => D3D12_TEXTURE_COPY_LOCATION_0 { SubresourceIndex: 0 },
            },
        }
    }

    /// A copy on pages filled with a pattern, all its tiles moved to other
    /// pages: the old and new pages agree byte for byte, and the copy reads
    /// the pattern from its new pages.
    #[test]
    fn a_moved_copy_keeps_its_texels() {
        let _card = crate::pool::tests::CARD_TEST.lock().unwrap_or_else(|e| e.into_inner());
        let Some(host) = crate::host::host() else { return };
        let desc = |format, levels| D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Width: 512,
            Height: 512,
            DepthOrArraySize: 1,
            MipLevels: levels,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            ..Default::default()
        };
        // One level; and block-compressed with every level, a packed tail
        // among the tiles moved.
        moved_keeps(host, desc(DXGI_FORMAT_R8G8B8A8_UNORM, 1));
        moved_keeps(host, desc(windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_BC1_UNORM, 10));
    }

    fn moved_keeps(host: &crate::host::Host, desc: D3D12_RESOURCE_DESC) {
        let device = &host.device;
        // SAFETY: plain queries and creation of our own objects; every list
        // is waited for before what it names goes.
        unsafe {
            let placed = device.GetResourceAllocationInfo(0, &[desc]).SizeInBytes;
            let t = match crate::pool::pages::texture(device, &desc, D3D12_RESOURCE_STATE_COPY_DEST, placed) {
                Ok(t) => t,
                Err(why) => panic!("not on pages: {:?}", why.map(|w| super::super::WHY_NAMES[w as usize])),
            };
            let raw = t.as_raw() as usize;
            let fence: ID3D12Fence = device.CreateFence(0, D3D12_FENCE_FLAG_NONE).unwrap();
            let allocator: ID3D12CommandAllocator = device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT).unwrap();
            let list: ID3D12GraphicsCommandList =
                device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None).unwrap();
            // The pattern into level 0.
            let mut foot = D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default();
            let mut total = 0u64;
            device.GetCopyableFootprints(&desc, 0, 1, 0, Some(&raw mut foot), None, None, Some(&raw mut total));
            let bytes = total as usize;
            let pattern: Vec<u8> = (0..bytes).map(|i| (i * 7 + i / 2048) as u8).collect();
            let upload = buffer(device, D3D12_HEAP_TYPE_UPLOAD, total, D3D12_RESOURCE_STATE_GENERIC_READ);
            let mut data = std::ptr::null_mut();
            upload.Map(0, None, Some(&raw mut data)).unwrap();
            std::ptr::copy_nonoverlapping(pattern.as_ptr(), data.cast::<u8>(), bytes);
            upload.Unmap(0, None);
            list.CopyTextureRegion(&location(&t, None), 0, 0, 0, &location(&upload, Some(foot)), None);
            list.ResourceBarrier(&[crate::host::transition(&t, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_COPY_SOURCE)]);
            list.Close().unwrap();
            host.queue().ExecuteCommandLists(&[Some(list.cast().unwrap())]);
            host.queue().Signal(&fence, 1).unwrap();
            wait(&fence, 1);
            // Every tile to pages of another heap.
            let (segs, tiling, tiled_desc) = {
                let all = copies();
                let c = all.as_ref().and_then(|c| c.get(&raw)).expect("in the record of copies");
                (c.segs.clone(), c.tiling.clone(), c.desc)
            };
            let first = segs[0].0;
            let mut old: Vec<(u32, usize, u32)> = Vec::new();
            let mut k = 0u32;
            for &(h, start, n) in &segs {
                for j in 0..n {
                    old.push((k, h, start + j));
                    k += 1;
                }
            }
            let new = {
                let mut pages = lock();
                // A heap of its own for the new pages if no other has room.
                let mut got = take_elsewhere(&mut pages, first, k);
                if got.is_none() {
                    pages.make(device, crate::host::now_ms(), super::super::Ripe::ALL).unwrap();
                    got = take_elsewhere(&mut pages, first, k);
                }
                got.unwrap()
            };
            let moved: Vec<(u32, usize, u32, usize, u32)> =
                old.iter().zip(&new).map(|(o, n)| (o.0, o.1, o.2, n.0, n.1)).collect();
            assert!(moved.iter().all(|m| m.3 != first));
            let s_alloc: ID3D12CommandAllocator = device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT).unwrap();
            let s_list: ID3D12GraphicsCommandList =
                device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &s_alloc, None).unwrap();
            s_list.Close().unwrap();
            let mut slot = Slot { allocator: s_alloc, list: s_list, busy: false };
            let mut value = 1u64;
            let (v, keep, checks) =
                one(device, host, &fence, &mut value, &mut slot, raw, &t, &tiling, &tiled_desc, &moved, true).unwrap();
            wait(&fence, v);
            let (a, b) = checks.unwrap();
            let len = moved.len() * PAGE as usize;
            assert_eq!(read(&a, len), read(&b, len), "old and new pages agree");
            assert!(read(&a, len).unwrap().iter().any(|&x| x != 0), "the pages hold the pattern");
            // The copy reads the pattern through its new pages.
            let back = buffer(device, D3D12_HEAP_TYPE_READBACK, total, D3D12_RESOURCE_STATE_COPY_DEST);
            allocator.Reset().unwrap();
            list.Reset(&allocator, None).unwrap();
            list.CopyTextureRegion(&location(&back, Some(foot)), 0, 0, 0, &location(&t, None), None);
            list.Close().unwrap();
            host.queue().ExecuteCommandLists(&[Some(list.cast().unwrap())]);
            value += 1;
            host.queue().Signal(&fence, value).unwrap();
            wait(&fence, value);
            assert!(read(&back, bytes).unwrap() == pattern, "the moved copy shows what was put in it");
            set_pages(raw, &moved, false);
            drop(keep);
            drop(t);
            assert!(device.GetDeviceRemovedReason().is_ok());
        }
    }
}
