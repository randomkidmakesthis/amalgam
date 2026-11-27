//! Finding F19 (analyst 8): the title's dynamic per-draw data comes from a
//! small pool of fixed chunks with a per-frame quota. A recording context out
//! of quota blocks in 0x140037a50 (a semaphore per thread); with no reusable
//! chunk it loops flush + Sleep(1) in 0x1400049b0 (the Sleep call at
//! 0x140004b01) and its twin 0x140031b90 (0x140031ce3). A chunk is reusable
//! only once the fence token stored at its list's close has passed, which is
//! our fence latency.
//!
//! `VICTORY_CHUNK_METER=1` (default off): call-site hooks on the two Sleep
//! calls and an entry hook on 0x140037a50 (each timed to its return), and a
//! line a second: per frame, the Sleep(1) calls of each site and their ms,
//! the calls of 0x140037a50, those that took 50 us or more (waits) and their
//! ms; and the globals [0x1475df1c8] (chunk count), [0x1475df1cc] (chunk
//! size), [0x149c3d7ec] (reserve), [0x14a943914] (chunk count asked by
//! config; 0: the code's 32).
//!
//! (F20, same switch) The render thread's walk of draw-list entries: an
//! entry run inline on the render thread after all jobs were drained (the
//! call to 0x1443c3890 at 0x1443c3f6e: SERIAL) against one run as a job by a
//! Subrender thread (the call at 0x1443c458c): per frame, serial entries and
//! the ms they took on the render thread, jobs and their ms summed over the
//! threads, and jobs per serial entry (a batch of fewer than 5 jobs between
//! two drains cannot keep the 5 threads busy).
//!
//! (F21 meter, same switch) The list close's hand-over of arena chunks
//! (0x1424749d0: rdx open chunks linked by +0x30, r8 retired ones linked by
//! +0x38, r9 the fence token; a chunk's address at desc+0x10): chunks and
//! bytes retired per frame against the bytes our refreshes take from the
//! segment, and chunks retired again with no refresh of ours over them since
//! (`victory_gpu::chunks`).
//!
//! `VICTORY_CHUNK_COUNT=N` (unset: nothing; an experiment; 5-120, 64 to try):
//! before 0x145c5ff00 runs, [0x14a943914] is raised to N when it is lower.
//! The analyst: safe from 5 to 255, nothing sized for 32; but the title's
//! view heap spans the segment (8,192 slots per 256 KiB chunk), so the
//! layer's dynamic slots follow (`VICTORY_DYNAMIC_SLOTS` = N x 8,192 unless
//! set; held under the host heap's limit, hence at most 120). Verified in the decompilation of 0x145c5ff00:
//! the count is read there (0 means 32), the chunk size from [0x14a943910]
//! (0 means 256 KiB, rounded up to 64 KiB), and the arena's segment for them
//! is made for count x size bytes (0x141e86440); the count is then kept at
//! [0x1475df158]. NOT verified: whether a table elsewhere has room for only
//! 32 chunks (fence tokens, free lists): the analyst's notes do not say, so
//! this stays an experiment.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use victory_loader::callhook::{self, Regs};

/// Where the title's image was meant to load.
const PREFERRED: usize = 0x1_4000_0000;
/// The Sleep(1) call sites (`mov ecx, 1; call 0x14092eca0`).
const SLEEP_SITES: [usize; 2] = [0x0000_4b01, 0x0003_1ce3];
const SLEEP_TARGET: usize = 0x0092_eca0;
/// The quota wait: `push rsi; mov rax, gs:[0x58]`.
const QUOTA: usize = 0x0003_7a50;
const QUOTA_PROLOGUE: [u8; 10] = [0x56, 0x65, 0x48, 0x8B, 0x04, 0x25, 0x58, 0x00, 0x00, 0x00];
/// The arena's setup: `push r15; push r14; push rsi; push rdi; push rbx`.
const SETUP: usize = 0x05c5_ff00;
const SETUP_PROLOGUE: [u8; 7] = [0x41, 0x57, 0x41, 0x56, 0x56, 0x57, 0x53];
/// The globals read (RVAs).
const COUNT: usize = 0x075d_f1c8;
const SIZE: usize = 0x075d_f1cc;
const RESERVE: usize = 0x09c3_d7ec;
const ASKED: usize = 0x0a94_3914;
/// (F20) The entry runner (`mov edx,[rcx+0x18]; mov rax,[rcx+8]; mov rcx,[rcx+0x10]; jmp rax`)
/// and its two call sites: serial (render thread), job (Subrender threads).
const ENTRY_RUN: usize = 0x043c_3890;
const ENTRY_SITES: [usize; 2] = [0x043c_3f6e, 0x043c_458c];
/// (F21) The list close's chunk hand-over: `push r14; push rsi; push rdi; push rbp; push rbx`.
const HAND_OVER: usize = 0x0247_49d0;
const HAND_OVER_PROLOGUE: [u8; 6] = [0x41, 0x56, 0x56, 0x57, 0x55, 0x53];
/// (F21) The segment: CPU base, chunk bytes, segment bytes (u64 each).
const SEG_BASE: usize = 0x075d_f140;
const SEG_CHUNK: usize = 0x075d_f160;
const SEG_BYTES: usize = 0x075d_f168;

static BASE: AtomicUsize = AtomicUsize::new(0);
/// The after stubs: Sleep sites, quota.
static AFTER: [AtomicUsize; 2] = [const { AtomicUsize::new(0) }; 2];
/// Since the last line: Sleep calls and microseconds per site (0, 1); quota
/// calls, quota calls of 50 us or more, their microseconds.
static COUNTS: [AtomicU64; 7] = [const { AtomicU64::new(0) }; 7];
/// (F20) Since the last line: serial entries, their us, jobs, their us.
static ENTRIES: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
/// (F21) Since the last line: hand-overs, open chunks named, odd lists.
static HANDED: [AtomicU64; 3] = [const { AtomicU64::new(0) }; 3];

thread_local! {
    /// Return addresses taken, with the site (0, 1; 2 the quota) and when.
    static TAKEN: std::cell::RefCell<Vec<(u64, u8, std::time::Instant)>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn meter_on() -> bool {
    std::env::var_os("VICTORY_CHUNK_METER").is_some_and(|v| v == "1")
}

fn count_asked() -> Option<u32> {
    std::env::var("VICTORY_CHUNK_COUNT").ok().and_then(|v| v.trim().parse::<u32>().ok()).map(|n| n.clamp(5, 120))
}

unsafe extern "C" fn sleep_called(regs: *mut Regs) {
    // SAFETY: the thunk's saved registers; `ret` is the return address.
    let regs = unsafe { &mut *regs };
    let site = (regs.ret as usize).wrapping_sub(BASE.load(Ordering::Relaxed)).wrapping_sub(5);
    let which = u8::from(site == SLEEP_SITES[1]);
    TAKEN.with(|t| t.borrow_mut().push((regs.ret, which, std::time::Instant::now())));
    regs.ret = AFTER[0].load(Ordering::Relaxed) as u64;
}

unsafe extern "C" fn entry_called(regs: *mut Regs) {
    // SAFETY: the thunk's saved registers; `ret` is the return address.
    let regs = unsafe { &mut *regs };
    let site = (regs.ret as usize).wrapping_sub(BASE.load(Ordering::Relaxed)).wrapping_sub(5);
    let which = if site == ENTRY_SITES[0] { 3 } else { 4 };
    TAKEN.with(|t| t.borrow_mut().push((regs.ret, which, std::time::Instant::now())));
    regs.ret = AFTER[0].load(Ordering::Relaxed) as u64;
}

fn read_u64(at: u64) -> Option<u64> {
    if at < 0x1_0000 || at >> 47 != 0 || at & 7 != 0 {
        return None;
    }
    // SAFETY: the closing list's own records, read on the thread closing it,
    // before the hooked code links them.
    Some(unsafe { (at as *const u64).read_volatile() })
}

unsafe extern "C" fn hand_over_entered(regs: *mut Regs) {
    // SAFETY: the thunk's saved registers.
    let (open, retired) = unsafe { ((*regs).rdx, (*regs).r8) };
    let _ = std::panic::catch_unwind(|| {
        HANDED[0].fetch_add(1, Ordering::Relaxed);
        // (Fox port C2, `VICTORY_CHUNK_FINAL`) The segment is read here too:
        // the gpu side needs it before the first line of the meter.
        if victory_gpu::chunks::final_on() {
            victory_gpu::chunks::segment(global_u64(SEG_BASE), global_u64(SEG_BYTES), global_u64(SEG_CHUNK));
        }
        let (mut at, mut n) = (open, 0u64);
        let mut opened = Vec::new();
        while at != 0 && n < 4096 {
            n += 1;
            if let Some(address) = read_u64(at + 0x10) {
                opened.push(address);
            }
            match read_u64(at + 0x30) {
                Some(next) => at = next,
                None => {
                    HANDED[2].fetch_add(1, Ordering::Relaxed);
                    break;
                }
            }
        }
        HANDED[1].fetch_add(n, Ordering::Relaxed);
        victory_gpu::chunks::still_open(&opened);
        let mut chunks = Vec::new();
        let mut at = retired;
        while at != 0 && chunks.len() < 4096 {
            let (Some(address), Some(next)) = (read_u64(at + 0x10), read_u64(at + 0x38)) else {
                HANDED[2].fetch_add(1, Ordering::Relaxed);
                break;
            };
            chunks.push(address);
            at = next;
        }
        victory_gpu::chunks::retired(&chunks);
    });
}

unsafe extern "C" fn quota_entered(regs: *mut Regs) {
    // SAFETY: as above.
    let regs = unsafe { &mut *regs };
    TAKEN.with(|t| t.borrow_mut().push((regs.ret, 2, std::time::Instant::now())));
    regs.ret = AFTER[1].load(Ordering::Relaxed) as u64;
}

unsafe extern "C" fn returned(_rax: u64, slot: *mut u64) {
    let Some((ret, which, began)) = TAKEN.with(|t| t.borrow_mut().pop()) else {
        // Nowhere to return to: cannot happen (each stub return was pushed).
        std::process::abort();
    };
    // SAFETY: the stub's slot for the return address.
    unsafe { *slot = ret };
    let us = began.elapsed().as_micros() as u64;
    match which {
        0 | 1 => {
            COUNTS[usize::from(which) * 2].fetch_add(1, Ordering::Relaxed);
            COUNTS[usize::from(which) * 2 + 1].fetch_add(us, Ordering::Relaxed);
        }
        3 | 4 => {
            let k = usize::from(which - 3) * 2;
            ENTRIES[k].fetch_add(1, Ordering::Relaxed);
            ENTRIES[k + 1].fetch_add(us, Ordering::Relaxed);
        }
        _ => {
            COUNTS[4].fetch_add(1, Ordering::Relaxed);
            if us >= 50 {
                COUNTS[5].fetch_add(1, Ordering::Relaxed);
                COUNTS[6].fetch_add(us, Ordering::Relaxed);
            }
        }
    }
}

unsafe extern "C" fn setup_entered(_regs: *mut Regs) {
    let Some(n) = count_asked() else {
        return;
    };
    let at = (BASE.load(Ordering::Relaxed) + ASKED) as *mut u32;
    // SAFETY: the title's .data (writable), a u32 of the image.
    let was = unsafe { at.read_volatile() };
    let now = if was == 0 { 32 } else { was };
    if n > now {
        // SAFETY: as above.
        unsafe { at.write_volatile(n) };
    }
    tracing::info!(was, asked = n, "F19: chunk count asked of the arena's setup (VICTORY_CHUNK_COUNT; 0 = the code's 32)");
}

fn global(rva: usize) -> u32 {
    let mut b = [0u8; 4];
    let at = BASE.load(Ordering::Relaxed) + rva;
    // SAFETY: inside the title's mapped image (.data).
    unsafe { std::ptr::copy_nonoverlapping(at as *const u8, b.as_mut_ptr(), 4) };
    u32::from_le_bytes(b)
}

fn global_u64(rva: usize) -> u64 {
    let mut b = [0u8; 8];
    let at = BASE.load(Ordering::Relaxed) + rva;
    // SAFETY: inside the title's mapped image (.data).
    unsafe { std::ptr::copy_nonoverlapping(at as *const u8, b.as_mut_ptr(), 8) };
    u64::from_le_bytes(b)
}

fn line_thread() {
    let _ = std::thread::Builder::new().name("victory-chunk-meter".into()).spawn(|| {
        let mut frame = victory_gpu::frame::current();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            let now = victory_gpu::frame::current();
            let frames = now.saturating_sub(frame).max(1) as f64;
            frame = now;
            let c: Vec<f64> = COUNTS.iter().map(|c| c.swap(0, Ordering::Relaxed) as f64).collect();
            tracing::info!(
                "F19 chunk pool per frame over {frames:.0} frames (VICTORY_CHUNK_METER=1): Sleep(1) for want of a chunk at 0x140004b01 {:.2} ({:.2} ms), \
                 at 0x140031ce3 {:.2} ({:.2} ms); quota waits 0x140037a50: {:.1} calls, {:.2} of 50 us or more ({:.2} ms); chunk count [0x1475df1c8] {}, \
                 chunk size [0x1475df1cc] {}, reserve [0x149c3d7ec] {}, count asked [0x14a943914] {}",
                c[0] / frames,
                c[1] / 1e3 / frames,
                c[2] / frames,
                c[3] / 1e3 / frames,
                c[4] / frames,
                c[5] / frames,
                c[6] / 1e3 / frames,
                global(COUNT),
                global(SIZE),
                global(RESERVE),
                global(ASKED),
            );
            let e: Vec<f64> = ENTRIES.iter().map(|c| c.swap(0, Ordering::Relaxed) as f64).collect();
            tracing::info!(
                "F20 draw-list entries per frame (VICTORY_CHUNK_METER=1): serial {:.1} ({:.2} ms on the render thread, after a drain each), jobs {:.1} \
                 ({:.2} ms summed over the Subrender threads), jobs per serial entry {:.2}",
                e[0] / frames,
                e[1] / 1e3 / frames,
                e[2] / frames,
                e[3] / 1e3 / frames,
                e[2] / e[0].max(1.0)
            );
            victory_gpu::chunks::segment(global_u64(SEG_BASE), global_u64(SEG_BYTES), global_u64(SEG_CHUNK));
            let h: Vec<f64> = HANDED.iter().map(|c| c.swap(0, Ordering::Relaxed) as f64).collect();
            let k = victory_gpu::chunks::take().map(|v| v as f64);
            tracing::info!(
                "F21 arena chunks per frame (VICTORY_CHUNK_METER=1): list closes {:.1}, chunks still open named {:.1}, chunks retired {:.1} ({:.2} MB final at \
                 close), outside the segment {:.1}, odd lists {:.1}; our refreshes from the segment {:.2} MB in {:.1} refreshes; chunk reused before read \
                 (retired again with no refresh of ours over it since) {:.2}; segment {:#x} + {} MB, chunk {} KiB",
                h[0] / frames,
                h[1] / frames,
                k[0] / frames,
                k[1] / 1048576.0 / frames,
                k[2] / frames,
                h[2] / frames,
                k[4] / 1048576.0 / frames,
                k[5] / frames,
                k[3] / frames,
                global_u64(SEG_BASE),
                global_u64(SEG_BYTES) >> 20,
                global_u64(SEG_CHUNK) >> 10
            );
        }
    });
}

/// Installs what `VICTORY_CHUNK_METER` / `VICTORY_CHUNK_COUNT` ask for.
///
/// # Safety
/// `base` is the title's mapped image, and no title code runs yet.
pub unsafe fn install(base: usize) {
    BASE.store(base, Ordering::Relaxed);
    let _ = PREFERRED;
    if count_asked().is_some() {
        // SAFETY: the caller's promise; `hook_entry` checks the prologue.
        match unsafe { callhook::hook_entry(base + SETUP, &SETUP_PROLOGUE, setup_entered) } {
            Ok(()) => tracing::info!("F19: arena setup hook installed (VICTORY_CHUNK_COUNT)"),
            Err(e) => tracing::warn!(error = %e, "F19: arena setup hook not installed: the bytes differ (another build?)"),
        }
    }
    if !meter_on() {
        // (Fox port C2) `VICTORY_CHUNK_FINAL=1` needs the hand-over alone.
        if victory_gpu::chunks::final_on() {
            // SAFETY: as above.
            match unsafe { callhook::hook_entry(base + HAND_OVER, &HAND_OVER_PROLOGUE, hand_over_entered) } {
                Ok(()) => tracing::info!("F21: chunk hand-over 0x1424749d0 hooked (VICTORY_CHUNK_FINAL=1)"),
                Err(e) => tracing::warn!(error = %e, "F21: chunk hand-over hook not installed: the bytes differ (another build?); VICTORY_CHUNK_FINAL does nothing"),
            }
        }
        return;
    }
    let stubs = (callhook::after_stub(returned), callhook::after_stub(returned));
    let (Ok(a), Ok(b)) = stubs else {
        tracing::warn!("F19 chunk meter not installed: no stub");
        return;
    };
    AFTER[0].store(a, Ordering::Relaxed);
    AFTER[1].store(b, Ordering::Relaxed);
    for site in SLEEP_SITES {
        // SAFETY: the caller's promise; `redirect_call` checks the call.
        match unsafe { callhook::redirect_call(base + site, base + SLEEP_TARGET, sleep_called) } {
            Ok(()) => tracing::info!(at = format_args!("{:#x}", PREFERRED + site), "F19: Sleep(1) site hooked (VICTORY_CHUNK_METER=1)"),
            Err(e) => tracing::warn!(error = %e, "F19: Sleep(1) site not hooked: the bytes differ (another build?)"),
        }
    }
    for site in ENTRY_SITES {
        // SAFETY: the caller's promise; `redirect_call` checks the call.
        match unsafe { callhook::redirect_call(base + site, base + ENTRY_RUN, entry_called) } {
            Ok(()) => tracing::info!(at = format_args!("{:#x}", PREFERRED + site), "F20: draw-list entry call hooked (VICTORY_CHUNK_METER=1)"),
            Err(e) => tracing::warn!(error = %e, "F20: draw-list entry call not hooked: the bytes differ (another build?)"),
        }
    }
    // SAFETY: as above.
    match unsafe { callhook::hook_entry(base + HAND_OVER, &HAND_OVER_PROLOGUE, hand_over_entered) } {
        Ok(()) => tracing::info!("F21: chunk hand-over 0x1424749d0 hooked (VICTORY_CHUNK_METER=1)"),
        Err(e) => tracing::warn!(error = %e, "F21: chunk hand-over hook not installed: the bytes differ (another build?)"),
    }
    // SAFETY: as above.
    match unsafe { callhook::hook_entry(base + QUOTA, &QUOTA_PROLOGUE, quota_entered) } {
        Ok(()) => tracing::info!("F19: quota wait 0x140037a50 hooked (VICTORY_CHUNK_METER=1)"),
        Err(e) => tracing::warn!(error = %e, "F19: quota wait hook not installed: the bytes differ (another build?)"),
    }
    line_thread();
}
