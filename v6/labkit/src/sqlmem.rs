pub fn used_mb() -> f64 { unsafe { libsqlite3_sys::sqlite3_memory_used() as f64 / (1024.0 * 1024.0) } }
pub fn peak_mb() -> f64 { unsafe { libsqlite3_sys::sqlite3_memory_highwater(0) as f64 / (1024.0 * 1024.0) } }
pub fn reset_peak() { unsafe { let _ = libsqlite3_sys::sqlite3_memory_highwater(1); } }
