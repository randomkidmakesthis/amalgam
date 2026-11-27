//! The hooks of finding F1 in the title's code (build 1.0.2.1; addresses
//! as RVAs from the preferred base 0x140000000): the ring of read-back
//! buffers (`victory_gpu::readback`), and with `VICTORY_READBACK_LOCKS=1`
//! the engine's generic buffer lock. Each site's bytes are checked first;
//! a site that differs is left alone with a warning.

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use victory_loader::callhook::{self, Regs};

/// The `call` (5 bytes) in the read-back ring's constructor that runs once
/// its buffers exist, with rcx = rsi = the ring (`mov rcx, rsi` before it).
const RING_SITE: usize = 0x1adc33b;
/// What it calls.
const RING_TARGET: usize = 0x1adc380;
/// The title's alias of CPU memory for the GPU: start, length, GPU base.
const ALIAS: u64 = 0x9e1dac0;
/// The generic buffer lock (rcx = buffer handle, rdx = lock record), and
/// its first instructions: `push rsi; push rdi; push rbx; sub rsp, 0x20`.
const LOCK: usize = 0x5121550;
const LOCK_PROLOGUE: [u8; 7] = [0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x20];
/// Its unlock (rcx = the same record): `mov r8, rcx; mov rcx, [rcx+0x10]`.
const UNLOCK: usize = 0x4e8abe0;
const UNLOCK_PROLOGUE: [u8; 7] = [0x49, 0x89, 0xC8, 0x48, 0x8B, 0x49, 0x10];
/// The return address of the constructor's one call, in the ring wrapper
/// (0x141d40f90: eight pushes and 0x68 bytes, then the call).
const CTOR_RETURN: usize = 0x1d4107d;
/// From the hook's return slot to the constructor's return address (its
/// five pushes and 0x50 bytes, the call's own slot), and from there to the
/// wrapper's return address.
const TO_CTOR_RETURN: usize = 0x80;
const TO_WRAPPER_RETURN: usize = 0xb0;
/// Where the wrapper returns to, for the rings named so far (the 22 call
/// sites are all in the log by address).
const NAMED: [(usize, &str); 3] =
    [(0xcc9067, "imposter"), (0x1dfee58, "visibility_buffer"), (0x3286358, "terrain_virtual_texture")];
/// The generic ring reader (finding F4; rcx = ring, rdx = record, returns
/// a bool; tail-jumps into the inner lock 0x145121690 on success):
/// `push r15; push r14; push rsi; push rdi; push rbp; push rbx`.
const READ: usize = 0x524dab0;
const READ_PROLOGUE: [u8; 8] = [0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x55, 0x53];
/// "Mark written" (rcx = ring): `push r14; push rsi; push rdi; push rbx`.
const MARK: usize = 0x53f1950;
const MARK_PROLOGUE: [u8; 5] = [0x41, 0x56, 0x56, 0x57, 0x53];
/// The preferred base, for addresses printed as the analyst's.
const PREFERRED: u64 = 0x1_4000_0000;

static BASE: AtomicU64 = AtomicU64::new(0);
static AFTER: AtomicUsize = AtomicUsize::new(0);
static READ_AFTER: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    /// Locks under way on this thread: the real return address, the
    /// record, the handle.
    static LOCKS: RefCell<Vec<(u64, u64, u64)>> = const { RefCell::new(Vec::new()) };
    /// Ring reads under way on this thread: the real return address, the
    /// ring, the record.
    static READS: RefCell<Vec<(u64, u64, u64)>> = const { RefCell::new(Vec::new()) };
}

unsafe extern "C" fn ring_made(regs: *mut Regs) {
    // SAFETY: the thunk's saved registers.
    let ring = unsafe { (*regs).rcx } as usize;
    let base = BASE.load(Ordering::Relaxed) as usize;
    // The stack above the call, as the constructor and the wrapper laid it
    // out (checked by the constructor's return address).
    // SAFETY: `ret` is the call's slot on this thread's stack; the frames
    // above it are the constructor's and the wrapper's, still live.
    let slot = unsafe { std::ptr::addr_of!((*regs).ret) } as usize;
    let ctor_return = unsafe { ((slot + TO_CTOR_RETURN) as *const usize).read() };
    let site = if ctor_return == base + CTOR_RETURN {
        // SAFETY: as above.
        unsafe { ((slot + TO_CTOR_RETURN + TO_WRAPPER_RETURN) as *const usize).read() }.wrapping_sub(base)
    } else {
        0
    };
    let name = NAMED.iter().find(|n| n.0 == site).map_or("", |n| n.1);
    let site = if site == 0 { 0 } else { site as u64 + PREFERRED };
    let alias = base as u64 + ALIAS;
    let _ = std::panic::catch_unwind(|| victory_gpu::hlehooks::readback::ring_created(ring, alias, site, name));
}

unsafe extern "C" fn lock_entered(regs: *mut Regs) {
    // SAFETY: the thunk's saved registers; `ret` is the return address on
    // the stack, which the stub takes.
    let regs = unsafe { &mut *regs };
    if regs.rdx == 0 {
        return;
    }
    let caller = regs.ret.wrapping_sub(BASE.load(Ordering::Relaxed)).wrapping_add(PREFERRED);
    let (handle, record, r8, r9) = (regs.rcx, regs.rdx, regs.r8, regs.r9);
    let _ = std::panic::catch_unwind(|| victory_gpu::hlehooks::readback::lock_entered(handle, record, r8, r9, caller));
    LOCKS.with(|l| l.borrow_mut().push((regs.ret, record, handle)));
    regs.ret = AFTER.load(Ordering::Relaxed) as u64;
}

unsafe extern "C" fn unlock_entered(regs: *mut Regs) {
    // SAFETY: the thunk's saved registers.
    let record = unsafe { (*regs).rcx };
    let _ = std::panic::catch_unwind(|| victory_gpu::hlehooks::readback::unlocked(record));
}

unsafe extern "C" fn lock_returned(rax: u64, slot: *mut u64) {
    let Some((ret, record, handle)) = LOCKS.with(|l| l.borrow_mut().pop()) else {
        // Nowhere to return to: cannot happen (each stub return was pushed).
        std::process::abort();
    };
    // SAFETY: the stub's slot for the return address.
    unsafe { *slot = ret };
    let caller = ret.wrapping_sub(BASE.load(Ordering::Relaxed)).wrapping_add(PREFERRED);
    let _ = std::panic::catch_unwind(|| victory_gpu::hlehooks::readback::lock_returned(handle, record, caller, rax & 0xff != 0));
}

unsafe extern "C" fn read_entered(regs: *mut Regs) {
    // SAFETY: the thunk's saved registers; `ret` is the return address.
    let regs = unsafe { &mut *regs };
    READS.with(|l| l.borrow_mut().push((regs.ret, regs.rcx, regs.rdx)));
    regs.ret = READ_AFTER.load(Ordering::Relaxed) as u64;
}

unsafe extern "C" fn read_returned(rax: u64, slot: *mut u64) {
    let Some((ret, ring, record)) = READS.with(|l| l.borrow_mut().pop()) else {
        std::process::abort();
    };
    // SAFETY: the stub's slot for the return address.
    unsafe { *slot = ret };
    let _ = std::panic::catch_unwind(|| victory_gpu::hlehooks::readback::ring_read(ring, record, rax & 0xff != 0));
}

unsafe extern "C" fn mark_entered(regs: *mut Regs) {
    // SAFETY: the thunk's saved registers.
    let ring = unsafe { (*regs).rcx };
    let _ = std::panic::catch_unwind(|| victory_gpu::hlehooks::readback::ring_marked(ring));
}

/// Finding F9: the streamer's group drop 0x1420cf7b0 (`push rsi; push rdi;
/// sub rsp, 0x28`), entered when the enqueue 0x1443c50a0 refused a group at
/// the slot ceiling; the builder rebuilds the same group next pass.
const GROUP_DROP: usize = 0x20cf7b0;
const GROUP_DROP_PROLOGUE: [u8; 6] = [0x56, 0x57, 0x48, 0x83, 0xec, 0x28];

unsafe extern "C" fn group_drop_entered(regs: *mut Regs) {
    // SAFETY: the thunk's saved registers; rcx is the group (read safely there).
    let group = unsafe { (*regs).rcx };
    victory_gpu::hlehooks::readback::group_dropped(group);
}

/// (F9) Counts the streamer's dropped groups (the "streamer:" line).
///
/// # Safety
/// As [`install`].
unsafe fn group_drop_hook(base: usize) {
    // SAFETY: the caller's promise; `hook_entry` checks the prologue.
    match unsafe { callhook::hook_entry(base + GROUP_DROP, &GROUP_DROP_PROLOGUE, group_drop_entered) } {
        Ok(()) => tracing::info!(at = format_args!("{:#x}", base + GROUP_DROP), "streamer group-drop hook installed (0x1420cf7b0, F9 meter)"),
        Err(e) => tracing::warn!(error = %e, found = ?bytes(base, GROUP_DROP, 8), "streamer group-drop hook not installed: the bytes differ (another build?)"),
    }
}

/// Finding F14: the title's single drawable-removal function 0x140151bb0
/// (`push rsi; sub rsp, 0x20`; rcx = entity, edx = flags; removes the
/// drawable at entity+0x28 when flags bit 0 is set).
const DRAWABLE_REMOVE: usize = 0x151bb0;
const DRAWABLE_REMOVE_PROLOGUE: [u8; 5] = [0x56, 0x48, 0x83, 0xec, 0x20];

unsafe extern "C" fn drawable_remove_entered(regs: *mut Regs) {
    // SAFETY: the thunk's saved registers; `ret` is the return address
    // ([rsp] at entry). Everything behind rcx is read safely there.
    let (entity, flags, ret) = unsafe { ((*regs).rcx, (*regs).rdx, (*regs).ret) };
    // (F17) Where the return address lies on the stack (as in `ring_made`),
    // for the two callers above it (read safely there).
    // SAFETY: only the address of the saved slot is taken.
    let slot = unsafe { std::ptr::addr_of!((*regs).ret) } as u64;
    let caller = ret.wrapping_sub(BASE.load(Ordering::Relaxed)).wrapping_add(PREFERRED);
    let _ = std::panic::catch_unwind(|| victory_gpu::hlehooks::readback::drawable_removed(entity, flags, caller, slot));
}

/// (F14) Counts drawables removed, by caller and kind (the "drawables
/// removed" line, with `VICTORY_STREAM_METER`).
///
/// # Safety
/// As [`install`].
unsafe fn drawable_remove_hook(base: usize) {
    // SAFETY: the caller's promise; `hook_entry` checks the prologue.
    match unsafe { callhook::hook_entry(base + DRAWABLE_REMOVE, &DRAWABLE_REMOVE_PROLOGUE, drawable_remove_entered) } {
        Ok(()) => tracing::info!(at = format_args!("{:#x}", base + DRAWABLE_REMOVE), "drawable-removal hook installed (0x140151bb0, F14 meter)"),
        Err(e) => tracing::warn!(error = %e, found = ?bytes(base, DRAWABLE_REMOVE, 8), "drawable-removal hook not installed: the bytes differ (another build?)"),
    }
}

/// The bytes at `rva` of the image at `base`.
fn bytes(base: usize, rva: usize, n: usize) -> Vec<u8> {
    // SAFETY: inside the mapped image (the sites are in its .text).
    unsafe { std::slice::from_raw_parts((base + rva) as *const u8, n) }.to_vec()
}

/// Installs the hooks the switches ask for into the image at `base`.
///
/// # Safety
/// `base` is the title's mapped image (build 1.0.2.1 or one whose bytes the
/// checks refuse), and no title code runs yet.
pub unsafe fn install(base: usize) {
    victory_gpu::hlehooks::readback::image_base(base as u64);
    // (F19) `VICTORY_CHUNK_METER` / `VICTORY_CHUNK_COUNT`.
    // SAFETY: the caller's promise.
    unsafe { crate::chunkmeter::install(base) };
    // (coder G, F25) `VICTORY_PAGE_KINDS`: the texture-level allocator.
    // SAFETY: the caller's promise.
    unsafe { crate::modules::pagekinds::install(base) };
    // (F5: `VICTORY_STREAM_ISSUE_WAIT_FRAMES` / `_FREE_WAIT_FRAMES` not
    // ported: measured and rejected, the waits stay 2 frames.)
    BASE.store(base as u64, Ordering::Relaxed);
    // (Fox port) The streamer's hooks only with `VICTORY_STREAM_METER=1`
    // (meters off by default; the old tree hooked them always).
    if victory_gpu::hlehooks::readback::stream_on() {
        // SAFETY: the caller's promise.
        unsafe { group_drop_hook(base) };
        // SAFETY: the caller's promise.
        unsafe { drawable_remove_hook(base) };
    }
    if !victory_gpu::hlehooks::readback::set_on() {
        return;
    }
    BASE.store(base as u64, Ordering::Relaxed);
    // The call and the `mov rcx, rsi` before it.
    let found = bytes(base, RING_SITE - 3, 8);
    if found[..3] != [0x48, 0x89, 0xF1] {
        tracing::warn!(at = format_args!("{:#x}", base + RING_SITE), found = ?found, "read-back ring hook not installed: the bytes differ (another build?)");
        return;
    }
    // SAFETY: the caller's promise; `redirect_call` checks the call.
    match unsafe { callhook::redirect_call(base + RING_SITE, base + RING_TARGET, ring_made) } {
        Ok(()) => tracing::info!(at = format_args!("{:#x}", base + RING_SITE), "read-back ring hook installed (VICTORY_READBACK_SET=1)"),
        Err(e) => {
            tracing::warn!(error = %e, "read-back ring hook not installed: the bytes differ (another build?)");
            return;
        }
    }
    if victory_gpu::hlehooks::readback::reads_on() {
        // SAFETY: as above.
        unsafe { read_hooks(base) };
    }
    if !victory_gpu::hlehooks::readback::locks_on() {
        return;
    }
    let stub = match callhook::after_stub(lock_returned) {
        Ok(stub) => stub,
        Err(e) => {
            tracing::warn!(error = %e, "buffer lock hook not installed");
            return;
        }
    };
    AFTER.store(stub, Ordering::Relaxed);
    // SAFETY: the caller's promise; `hook_entry` checks the prologue.
    match unsafe { callhook::hook_entry(base + LOCK, &LOCK_PROLOGUE, lock_entered) } {
        Ok(()) => tracing::info!(at = format_args!("{:#x}", base + LOCK), "buffer lock hook installed (VICTORY_READBACK_LOCKS=1)"),
        Err(e) => {
            tracing::warn!(error = %e, "buffer lock hook not installed: the bytes differ (another build?)");
            return;
        }
    }
    // SAFETY: as above.
    match unsafe { callhook::hook_entry(base + UNLOCK, &UNLOCK_PROLOGUE, unlock_entered) } {
        Ok(()) => tracing::info!(at = format_args!("{:#x}", base + UNLOCK), "buffer unlock hook installed (VICTORY_READBACK_LOCKS=1)"),
        Err(e) => tracing::warn!(error = %e, "buffer unlock hook not installed: the bytes differ (another build?)"),
    }
}

/// The ring reader (entry and return) and "mark written" (entry) hooks.
///
/// # Safety
/// As [`install`].
unsafe fn read_hooks(base: usize) {
    let stub = match callhook::after_stub(read_returned) {
        Ok(stub) => stub,
        Err(e) => {
            tracing::warn!(error = %e, "ring read hook not installed");
            return;
        }
    };
    READ_AFTER.store(stub, Ordering::Relaxed);
    // SAFETY: the caller's promise; `hook_entry` checks the prologue.
    match unsafe { callhook::hook_entry(base + READ, &READ_PROLOGUE, read_entered) } {
        Ok(()) => tracing::info!(at = format_args!("{:#x}", base + READ), "ring read hook installed (0x14524dab0)"),
        Err(e) => tracing::warn!(error = %e, found = ?bytes(base, READ, 8), "ring read hook not installed: the bytes differ (another build?)"),
    }
    // SAFETY: as above.
    match unsafe { callhook::hook_entry(base + MARK, &MARK_PROLOGUE, mark_entered) } {
        Ok(()) => tracing::info!(at = format_args!("{:#x}", base + MARK), "ring mark-written hook installed (0x1453f1950)"),
        Err(e) => tracing::warn!(error = %e, found = ?bytes(base, MARK, 8), "ring mark-written hook not installed: the bytes differ (another build?)"),
    }
}
