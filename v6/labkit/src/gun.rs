//! Gun to the head of every Rust allocation. Ported from sprefa-store::memcap
//! (proved with memcap_probe): a counting `#[global_allocator]` that returns null
//! past the cap -> Rust `handle_alloc_error` -> clean SIGABRT, on every platform.
//! Adds PEAK tracking so the harness can report the high-water mark per experiment.
//!
//! CAVEAT (from the store's own RAM audit): this sees only the RUST heap. SQLite's
//! C allocator is invisible to it — cap that separately with PRAGMA soft_heap_limit
//! and (on Linux) setrlimit. Both are wired by `install(mb)`.
//!
//! A binary opts in:
//! ```ignore
//! #[global_allocator]
//! static GLOBAL: labkit::gun::Gun = labkit::gun::Gun;
//! ```
//! then calls `labkit::gun::install(5120)` at the top of main.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static CAP: AtomicUsize = AtomicUsize::new(0); // 0 = unlimited

pub struct Gun;

#[inline]
fn reserve(size: usize) -> bool {
    let cap = CAP.load(Ordering::Relaxed);
    let prev = LIVE.fetch_add(size, Ordering::Relaxed);
    let now = prev + size;
    if cap != 0 && now > cap {
        LIVE.fetch_sub(size, Ordering::Relaxed);
        return false;
    }
    // record peak (best-effort CAS loop)
    let mut peak = PEAK.load(Ordering::Relaxed);
    while now > peak {
        match PEAK.compare_exchange_weak(peak, now, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(p) => peak = p,
        }
    }
    true
}

unsafe impl GlobalAlloc for Gun {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if !reserve(layout.size()) {
            return std::ptr::null_mut();
        }
        let ptr = System.alloc(layout);
        if ptr.is_null() {
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        }
        ptr
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if !reserve(layout.size()) {
            return std::ptr::null_mut();
        }
        let ptr = System.alloc_zeroed(layout);
        if ptr.is_null() {
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let old = layout.size();
        if new_size > old {
            if !reserve(new_size - old) {
                return std::ptr::null_mut();
            }
            let np = System.realloc(ptr, layout, new_size);
            if np.is_null() {
                LIVE.fetch_sub(new_size - old, Ordering::Relaxed);
            }
            np
        } else {
            LIVE.fetch_sub(old - new_size, Ordering::Relaxed);
            System.realloc(ptr, layout, new_size)
        }
    }
}

/// Install the cap: `mb` megabytes of RUST heap (the gun), plus a Linux setrlimit
/// belt (no-op on macOS). Only ever tightens. SQLite C heap is capped by the
/// experiments via PRAGMA soft_heap_limit.
pub fn install(mb: u64) {
    let want = (mb as usize).saturating_mul(1024 * 1024);
    let cur = CAP.load(Ordering::Relaxed);
    if cur == 0 || want < cur {
        CAP.store(want, Ordering::Relaxed);
    }
    set_soft(libc::RLIMIT_AS, want as u64);
    set_soft(libc::RLIMIT_DATA, want as u64);
}

pub fn live_mb() -> f64 {
    LIVE.load(Ordering::Relaxed) as f64 / (1024.0 * 1024.0)
}
pub fn peak_mb() -> f64 {
    PEAK.load(Ordering::Relaxed) as f64 / (1024.0 * 1024.0)
}
/// Reset the peak between experiments so each gets a clean high-water reading.
pub fn reset_peak() {
    PEAK.store(LIVE.load(Ordering::Relaxed), Ordering::Relaxed);
}
pub fn cap_mb() -> f64 {
    CAP.load(Ordering::Relaxed) as f64 / (1024.0 * 1024.0)
}

fn set_soft(resource: libc::c_int, want: u64) {
    unsafe {
        let mut lim: libc::rlimit = std::mem::zeroed();
        if libc::getrlimit(resource, &mut lim) != 0 {
            return;
        }
        let want = want as libc::rlim_t;
        if lim.rlim_cur != libc::RLIM_INFINITY && lim.rlim_cur <= want {
            return;
        }
        let target = if lim.rlim_max != libc::RLIM_INFINITY && lim.rlim_max < want {
            lim.rlim_max
        } else {
            want
        };
        lim.rlim_cur = target;
        let _ = libc::setrlimit(resource, &lim);
    }
}

pub fn peak_rss_mb() -> f64 {
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) };
    ru.ru_maxrss as f64 / (1024.0 * 1024.0) // darwin bytes
}
