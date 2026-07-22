//! SQLite's OWN heap — the C allocator the Rust gun cannot see. `sqlite3_memory_used`
//! and `sqlite3_memory_highwater` are process-global (across every connection), so this
//! is the honest "how much RAM is SQLite itself holding" number. Pair it with:
//!   - gun::peak_mb()     — the RUST heap high-water (what the 5 GB gun caps)
//!   - gun::peak_rss_mb() — the whole PROCESS RSS (getrusage: Rust + SQLite C + page cache)
//! For an in-memory db, used() ≈ the db size; for file-backed, used() ≈ the page cache
//! SQLite is holding, and the OS file-cache shows up only in RSS.

pub fn used_mb() -> f64 {
    unsafe { rusqlite::ffi::sqlite3_memory_used() as f64 / (1024.0 * 1024.0) }
}

/// Peak SQLite heap since the last reset (resetFlag = 0 → read without reset).
pub fn peak_mb() -> f64 {
    unsafe { rusqlite::ffi::sqlite3_memory_highwater(0) as f64 / (1024.0 * 1024.0) }
}

/// Reset the SQLite heap high-water mark (resetFlag = 1).
pub fn reset_peak() {
    unsafe {
        let _ = rusqlite::ffi::sqlite3_memory_highwater(1);
    }
}
