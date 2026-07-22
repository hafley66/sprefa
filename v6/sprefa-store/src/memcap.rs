//! OS-protective self-cap for the head-to-head examples. The point is narrow:
//! a runaway scale (fat CLI arg, an accidental extra zero) must make the PROCESS
//! die with an allocation error, never drive the whole machine into swap.
//!
//! macOS reality check (proved with examples/memcap_probe): `setrlimit` does NOT
//! bite here. `RLIMIT_AS` is a documented no-op on Darwin and `RLIMIT_DATA` only
//! governs the `sbrk` segment, but system malloc services large allocations via
//! `mmap`, which neither limit touches. A 128 MB cap let a 512 MB Vec through.
//!
//! So the real enforcement is [`CappedAlloc`], a counting `#[global_allocator]`
//! wrapper: it tracks live bytes and returns null past the cap, which makes Rust
//! abort the process cleanly (SIGABRT) instead of the OS swapping. That works
//! identically on every platform because it intercepts every allocation in the
//! process. `setrlimit` is kept only as a belt-and-suspenders on Linux, where it
//! does bite; it is a no-op safety net on mac, never the guarantee.
//!
//! Each binary opts in by declaring the allocator:
//! ```ignore
//! #[global_allocator]
//! static GLOBAL: sprefa_store::memcap::CappedAlloc = sprefa_store::memcap::CappedAlloc;
//! ```
//! then calling [`cap_address_space_mb`] at the top of `main`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Live bytes currently handed out through [`CappedAlloc`]. Always tracked (even
/// when the cap is unset) so dealloc accounting can never underflow after a cap
/// is installed mid-run.
static LIVE: AtomicUsize = AtomicUsize::new(0);
/// Hard ceiling in bytes; 0 means unlimited (no enforcement).
static CAP: AtomicUsize = AtomicUsize::new(0);
/// High-water mark of [`LIVE`] since the last [`reset_peak`]. This is the honest
/// answer to "did the measured op ever transiently hold a lot of Rust heap?" —
/// reading LIVE after an op only shows what survives, not the peak during it.
static PEAK: AtomicUsize = AtomicUsize::new(0);

/// Bump PEAK to at least `now` (relaxed CAS loop; only runs on the alloc path).
#[inline]
fn bump_peak(now: usize) {
    let mut cur = PEAK.load(Ordering::Relaxed);
    while now > cur {
        match PEAK.compare_exchange_weak(cur, now, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(x) => cur = x,
        }
    }
}

/// A `#[global_allocator]` that refuses to exceed [`cap_address_space_mb`].
/// Delegates every real allocation to the System allocator and only adds a pair
/// of relaxed atomics per call, so the un-capped path stays cheap.
pub struct CappedAlloc;

unsafe impl GlobalAlloc for CappedAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let cap = CAP.load(Ordering::Relaxed);
        // Reserve first, so concurrent allocs can't jointly overshoot the cap.
        let prev = LIVE.fetch_add(size, Ordering::Relaxed);
        if cap != 0 && prev + size > cap {
            LIVE.fetch_sub(size, Ordering::Relaxed);
            return std::ptr::null_mut(); // -> handle_alloc_error -> abort
        }
        let ptr = System.alloc(layout);
        if ptr.is_null() {
            LIVE.fetch_sub(size, Ordering::Relaxed);
        } else {
            bump_peak(prev + size);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let cap = CAP.load(Ordering::Relaxed);
        let prev = LIVE.fetch_add(size, Ordering::Relaxed);
        if cap != 0 && prev + size > cap {
            LIVE.fetch_sub(size, Ordering::Relaxed);
            return std::ptr::null_mut();
        }
        let ptr = System.alloc_zeroed(layout);
        if ptr.is_null() {
            LIVE.fetch_sub(size, Ordering::Relaxed);
        } else {
            bump_peak(prev + size);
        }
        ptr
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let old = layout.size();
        let cap = CAP.load(Ordering::Relaxed);
        if new_size > old {
            let grow = new_size - old;
            let prev = LIVE.fetch_add(grow, Ordering::Relaxed);
            if cap != 0 && prev + grow > cap {
                LIVE.fetch_sub(grow, Ordering::Relaxed);
                return std::ptr::null_mut();
            }
            let new_ptr = System.realloc(ptr, layout, new_size);
            if new_ptr.is_null() {
                LIVE.fetch_sub(grow, Ordering::Relaxed);
            } else {
                bump_peak(prev + grow);
            }
            new_ptr
        } else {
            LIVE.fetch_sub(old - new_size, Ordering::Relaxed);
            System.realloc(ptr, layout, new_size)
        }
    }
}

/// Cap this process's heap to `mb` megabytes. The [`CappedAlloc`] global
/// allocator is the real enforcer (aborts the process past the cap on every
/// platform); `setrlimit` is also set as a Linux-only belt-and-suspenders and is
/// a harmless no-op on macOS. Best-effort and idempotent: only tightens.
pub fn cap_address_space_mb(mb: u64) {
    let want = (mb as usize).saturating_mul(1024 * 1024);
    // Real enforcement: only lower an existing cap, never raise it.
    let cur = CAP.load(Ordering::Relaxed);
    if cur == 0 || want < cur {
        CAP.store(want, Ordering::Relaxed);
    }
    // Bonus on Linux (bites there); no-op safety net on macOS.
    set_soft(libc::RLIMIT_AS, want as u64);
    set_soft(libc::RLIMIT_DATA, want as u64);
}

/// Live bytes currently allocated through [`CappedAlloc`]. Test/introspection
/// hook; also lets a caller prove the accounting is wired.
pub fn live_bytes() -> usize {
    LIVE.load(Ordering::Relaxed)
}

/// High-water mark of live Rust heap since the last [`reset_peak`]. This is the
/// honest "peak Rust heap DURING the op" number: `live_bytes()` after an op only
/// shows what survives it, so a transient spike is invisible without this.
pub fn peak_bytes() -> usize {
    PEAK.load(Ordering::Relaxed)
}

/// Reset the high-water to the current live value, so the next [`peak_bytes`]
/// measures only allocations after this call (e.g. bracket the measured op).
pub fn reset_peak() {
    PEAK.store(LIVE.load(Ordering::Relaxed), Ordering::Relaxed);
}

/// The current hard cap in bytes; 0 = unlimited. Deterministic introspection for
/// tests (the enforcement itself can only be observed by aborting a subprocess).
pub fn cap_bytes() -> usize {
    CAP.load(Ordering::Relaxed)
}

fn set_soft(resource: libc::c_int, want: u64) {
    unsafe {
        let mut lim: libc::rlimit = std::mem::zeroed();
        if libc::getrlimit(resource, &mut lim) != 0 {
            return; // cannot read the current limit; leave it alone
        }
        let want = want as libc::rlim_t;
        // Never raise an existing lower cap; only tighten. RLIM_INFINITY means
        // "unlimited", which is always looser than our finite request.
        if lim.rlim_cur != libc::RLIM_INFINITY && lim.rlim_cur <= want {
            return;
        }
        let target = if lim.rlim_max != libc::RLIM_INFINITY && lim.rlim_max < want {
            lim.rlim_max
        } else {
            want
        };
        lim.rlim_cur = target;
        let _ = libc::setrlimit(resource, &lim); // best-effort; ignore refusal
    }
}
