//! (Coder H, ledger F23) Hooks on the engine's buffer lock contract
//! (build 1.0.2.1; RVAs from the preferred base 0x140000000), for
//! `victory_gpu::lockmeter` (`VICTORY_LOCK_METER=1`, default off).
//!
//! - The inner lock 0x145121690 (rcx = R, a record): every engine lock ends
//!   there (the generic lock 0x145121550 writes R+0x30 = the object and
//!   R+0x3c = 2 and tail-jumps in; the ring reader 0x14524dab0 writes mode 1
//!   and tail-jumps in; 15 direct sites call it), so the return address seen
//!   at its entry is the caller's. Read at entry: R+0x30 object, R+0x38 u32
//!   subresource, R+0x3c u8 mode. At its return with al = 1: R+0 the CPU
//!   view, R+8 a texture's row pitch, R+0x18 u32 bytes (the whole buffer).
//!   Prologue `push r15; push r14; push r13; push r12` (8 bytes).
//! - The unlock 0x144e8abe0 (rcx = a copy of R; the thunk 0x144e8abd0 jumps
//!   to it): `mov r8, rcx; mov rcx, [rcx+0x10]` (7 bytes), as the read-back
//!   hooks know it. With `VICTORY_READBACK_LOCKS=1` that hook holds the
//!   entry and this one is refused (the bytes differ): said so.
//!
//! Each prologue is checked before it is patched; a site that differs is
//! left alone with a warning. A line thread gives the meter's lines once a
//! second, with the graphics heap's globals (F21).

use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};

use victory_loader::callhook::{self, Regs};

const PREFERRED: usize = 0x1_4000_0000;
const LOCK: usize = 0x0512_1690;
const LOCK_PROLOGUE: [u8; 8] = [0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54];
const UNLOCK: usize = 0x04e8_abe0;
const UNLOCK_PROLOGUE: [u8; 7] = [0x49, 0x89, 0xC8, 0x48, 0x8B, 0x49, 0x10];
/// (F21) The graphics heap's globals (u64 each): the reservation, segment C
/// (chunk pool) start and bytes, D start and bytes, E start and bytes.
const RESERVATION: usize = 0x075d_f188;
const SEG_C: usize = 0x075d_f140;
const SEG_C_BYTES: usize = 0x075d_f168;
const SEG_D: usize = 0x075d_f148;
const SEG_D_BYTES: usize = 0x075d_f170;
const SEG_E: usize = 0x075d_f150;
const SEG_E_BYTES: usize = 0x075d_f178;
/// (F3/F24) The streaming region: CPU view base, bytes, GPU view base (u64
/// each), and the pointer to its page-state table (u64 per 64 KiB page).
const VIEW_CPU: usize = 0x09e1_dac0;
const VIEW_LEN: usize = 0x09e1_dac8;
const VIEW_GPU: usize = 0x09e1_dad0;
const PAGE_TABLE: usize = 0x071b_fb08;

static BASE: AtomicUsize = AtomicUsize::new(0);
static AFTER: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    /// Locks under way on this thread: the real return address, the record,
    /// the object, the subresource, the mode.
    static LOCKS: RefCell<Vec<(u64, u64, u64, u32, u8)>> = const { RefCell::new(Vec::new()) };
}

/// Whether `[at, at + 0x40)` may be read as a record (canonical, aligned).
fn record_ok(at: u64) -> bool {
    at >= 0x1_0000 && at >> 47 == 0 && at & 7 == 0
}

/// # Safety
/// `record_ok(at)` and the record is live (the title's own, during its lock).
unsafe fn read<T: Copy>(at: u64, off: u64) -> T {
    // SAFETY: the caller's promise.
    unsafe { ((at + off) as *const T).read_unaligned() }
}

unsafe extern "C" fn lock_entered(regs: *mut Regs) {
    // SAFETY: the thunk's saved registers; `ret` is the return address,
    // which the stub takes.
    let regs = unsafe { &mut *regs };
    let record = regs.rcx;
    if !record_ok(record) {
        return;
    }
    // SAFETY: the lock's own record, filled by its caller before the call.
    let (object, sub, mode) = unsafe { (read::<u64>(record, 0x30), read::<u32>(record, 0x38), read::<u8>(record, 0x3c)) };
    LOCKS.with(|l| l.borrow_mut().push((regs.ret, record, object, sub, mode)));
    regs.ret = AFTER.load(Ordering::Relaxed) as u64;
}

unsafe extern "C" fn lock_returned(rax: u64, slot: *mut u64) {
    let Some((ret, record, object, sub, mode)) = LOCKS.with(|l| l.borrow_mut().pop()) else {
        // Nowhere to return to: cannot happen (each stub return was pushed).
        std::process::abort();
    };
    // SAFETY: the stub's slot for the return address.
    unsafe { *slot = ret };
    let ok = rax & 0xff != 0;
    let caller = (ret as usize).wrapping_sub(BASE.load(Ordering::Relaxed)).wrapping_add(PREFERRED) as u64;
    // SAFETY: the record the lock just filled (its caller's, still live).
    let (ptr, pitch, bytes) = if ok { unsafe { (read::<u64>(record, 0), read::<u64>(record, 8), u64::from(read::<u32>(record, 0x18))) } } else { (0, 0, 0) };
    let _ = std::panic::catch_unwind(|| victory_gpu::lockmeter::locked(object, sub, mode, caller, ok, ptr, pitch, bytes));
}

unsafe extern "C" fn unlock_entered(regs: *mut Regs) {
    // SAFETY: the thunk's saved registers.
    let record = unsafe { (*regs).rcx };
    if !record_ok(record) {
        return;
    }
    // SAFETY: the copy of the lock record the caller hands in.
    let (object, ptr) = unsafe { (read::<u64>(record, 0x30), read::<u64>(record, 0)) };
    let _ = std::panic::catch_unwind(|| victory_gpu::lockmeter::unlocked(object, ptr));
}

fn global_u64(rva: usize) -> u64 {
    let at = BASE.load(Ordering::Relaxed) + rva;
    // SAFETY: inside the title's mapped image (.data).
    unsafe { (at as *const u64).read_unaligned() }
}

fn line_thread() {
    let _ = std::thread::Builder::new().name("victory-lock-meter".into()).spawn(|| {
        let mut frame = victory_gpu::frame::current();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            let now = victory_gpu::frame::current();
            let frames = now.saturating_sub(frame).max(1) as f64;
            frame = now;
            victory_gpu::lockmeter::heap(
                global_u64(RESERVATION),
                (global_u64(SEG_C), global_u64(SEG_C_BYTES)),
                (global_u64(SEG_D), global_u64(SEG_D_BYTES)),
                (global_u64(SEG_E), global_u64(SEG_E_BYTES)),
            );
            victory_gpu::lockmeter::views(
                global_u64(VIEW_CPU),
                global_u64(VIEW_LEN),
                global_u64(VIEW_GPU),
                global_u64(PAGE_TABLE),
                BASE.load(Ordering::Relaxed) as u64,
            );
            if victory_gpu::lockmeter::on() {
                let _ = std::panic::catch_unwind(|| victory_gpu::lockmeter::line(frames));
            }
            // (Coder J) `VICTORY_KNOWN_WRITES`: the ledger's lines.
            let _ = std::panic::catch_unwind(|| victory_gpu::knownwrites::line(frames));
            if victory_gpu::lockmeter::on() {
                let _ = std::panic::catch_unwind(victory_gpu::lockmeter::probe_round);
            }
        }
    });
}

fn bytes(base: usize, rva: usize, n: usize) -> Vec<u8> {
    // SAFETY: inside the mapped image (the sites are in its .text).
    unsafe { std::slice::from_raw_parts((base + rva) as *const u8, n) }.to_vec()
}

/// Installs the hooks `VICTORY_LOCK_METER=1` asks for.
///
/// # Safety
/// `base` is the title's mapped image, and no title code runs yet.
pub unsafe fn install(base: usize) {
    // (Coder J) `VICTORY_KNOWN_WRITES` needs the lock windows too (ledger class (e)).
    if !victory_gpu::lockmeter::on() && !victory_gpu::knownwrites::on() {
        return;
    }
    BASE.store(base, Ordering::Relaxed);
    let stub = match callhook::after_stub(lock_returned) {
        Ok(stub) => stub,
        Err(e) => {
            tracing::warn!(error = %e, "F23: inner lock hook not installed: no stub; VICTORY_LOCK_METER does nothing");
            return;
        }
    };
    AFTER.store(stub, Ordering::Relaxed);
    // SAFETY: the caller's promise; `hook_entry` checks the prologue.
    match unsafe { callhook::hook_entry(base + LOCK, &LOCK_PROLOGUE, lock_entered) } {
        Ok(()) => tracing::info!(at = format_args!("{:#x}", PREFERRED + LOCK), "F23: inner buffer lock hooked (VICTORY_LOCK_METER=1)"),
        Err(e) => {
            tracing::warn!(error = %e, found = ?bytes(base, LOCK, 8), "F23: inner lock 0x145121690 not hooked: the bytes differ (another build?); VICTORY_LOCK_METER does nothing");
            return;
        }
    }
    // SAFETY: as above.
    match unsafe { callhook::hook_entry(base + UNLOCK, &UNLOCK_PROLOGUE, unlock_entered) } {
        Ok(()) => tracing::info!(at = format_args!("{:#x}", PREFERRED + UNLOCK), "F23: buffer unlock hooked (VICTORY_LOCK_METER=1)"),
        Err(e) => tracing::warn!(
            error = %e,
            found = ?bytes(base, UNLOCK, 7),
            readback_locks = victory_gpu::hlehooks::readback::locks_on(),
            "F23: unlock 0x144e8abe0 not hooked (the bytes differ: another build, or VICTORY_READBACK_LOCKS=1 holds it); windows never close in the meter"
        ),
    }
    line_thread();
}
