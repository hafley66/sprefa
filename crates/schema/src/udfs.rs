use libsqlite3_sys as ffi;
use std::os::raw::{c_char, c_int, c_void};

/// Register all custom SQLite UDFs on a raw `sqlite3*` handle.
///
/// Call this from `after_connect` on each new connection.
///
/// Safety: `db` must be a valid, open sqlite3 connection pointer.
pub unsafe fn register_all(db: *mut ffi::sqlite3) {
    register_re_extract(db);
    register_split_part(db);
    register_repo_name(db);
    register_file_path(db);
    register_fzy_score(db);
}

// ---------------------------------------------------------------------------
// re_extract(text, pattern, group) -> TEXT|NULL
// ---------------------------------------------------------------------------

unsafe extern "C" fn xfunc_re_extract(
    ctx: *mut ffi::sqlite3_context,
    argc: c_int,
    argv: *mut *mut ffi::sqlite3_value,
) {
    if argc != 3 {
        ffi::sqlite3_result_null(ctx);
        return;
    }

    let text = match sqlite_text(argv, 0) {
        Some(t) => t,
        None => { ffi::sqlite3_result_null(ctx); return; }
    };
    let pattern = match sqlite_text(argv, 1) {
        Some(p) => p,
        None => { ffi::sqlite3_result_null(ctx); return; }
    };
    let group_idx = ffi::sqlite3_value_int(*argv.offset(2)) as usize;

    // Cache the compiled Regex in auxdata slot 1 (keyed on the pattern arg).
    let re: &regex::Regex = {
        let ptr = ffi::sqlite3_get_auxdata(ctx, 1) as *const regex::Regex;
        if !ptr.is_null() {
            &*ptr
        } else {
            match regex::Regex::new(pattern) {
                Ok(r) => {
                    let boxed = Box::into_raw(Box::new(r));
                    ffi::sqlite3_set_auxdata(ctx, 1, boxed as *mut c_void, Some(drop_regex));
                    &*boxed
                }
                Err(_) => { ffi::sqlite3_result_null(ctx); return; }
            }
        }
    };

    match re.captures(text) {
        Some(caps) => match caps.get(group_idx) {
            Some(m) => {
                let s = m.as_str();
                ffi::sqlite3_result_text(
                    ctx,
                    s.as_ptr() as *const c_char,
                    s.len() as c_int,
                    ffi::SQLITE_TRANSIENT(),
                );
            }
            None => ffi::sqlite3_result_null(ctx),
        },
        None => ffi::sqlite3_result_null(ctx),
    }
}

unsafe extern "C" fn drop_regex(ptr: *mut c_void) {
    drop(Box::from_raw(ptr as *mut regex::Regex));
}

unsafe fn register_re_extract(db: *mut ffi::sqlite3) {
    let name = b"re_extract\0";
    ffi::sqlite3_create_function_v2(
        db,
        name.as_ptr() as *const c_char,
        3,
        ffi::SQLITE_UTF8 | ffi::SQLITE_DETERMINISTIC,
        std::ptr::null_mut(),
        Some(xfunc_re_extract),
        None,
        None,
        None,
    );
}

// ---------------------------------------------------------------------------
// split_part(text, delim, index) -> TEXT|NULL  (1-indexed)
// ---------------------------------------------------------------------------

unsafe extern "C" fn xfunc_split_part(
    ctx: *mut ffi::sqlite3_context,
    argc: c_int,
    argv: *mut *mut ffi::sqlite3_value,
) {
    if argc != 3 {
        ffi::sqlite3_result_null(ctx);
        return;
    }

    let text = match sqlite_text(argv, 0) {
        Some(t) => t,
        None => { ffi::sqlite3_result_null(ctx); return; }
    };
    let delim = match sqlite_text(argv, 1) {
        Some(d) => d,
        None => { ffi::sqlite3_result_null(ctx); return; }
    };
    let idx = ffi::sqlite3_value_int(*argv.offset(2));
    if idx < 1 {
        ffi::sqlite3_result_null(ctx);
        return;
    }

    let parts: Vec<&str> = text.split(delim).collect();
    match parts.get((idx - 1) as usize) {
        Some(part) => {
            ffi::sqlite3_result_text(
                ctx,
                part.as_ptr() as *const c_char,
                part.len() as c_int,
                ffi::SQLITE_TRANSIENT(),
            );
        }
        None => ffi::sqlite3_result_null(ctx),
    }
}

unsafe fn register_split_part(db: *mut ffi::sqlite3) {
    let name = b"split_part\0";
    ffi::sqlite3_create_function_v2(
        db,
        name.as_ptr() as *const c_char,
        3,
        ffi::SQLITE_UTF8 | ffi::SQLITE_DETERMINISTIC,
        std::ptr::null_mut(),
        Some(xfunc_split_part),
        None,
        None,
        None,
    );
}

// ---------------------------------------------------------------------------
// repo_name(repo_id) -> TEXT|NULL
// Runs: SELECT name FROM repos WHERE id = ?
// ---------------------------------------------------------------------------

unsafe extern "C" fn xfunc_repo_name(
    ctx: *mut ffi::sqlite3_context,
    argc: c_int,
    argv: *mut *mut ffi::sqlite3_value,
) {
    if argc != 1 {
        ffi::sqlite3_result_null(ctx);
        return;
    }

    let id = ffi::sqlite3_value_int64(*argv.offset(0));
    let db = ffi::sqlite3_context_db_handle(ctx);
    let sql = b"SELECT name FROM repos WHERE id = ?\0";

    let mut stmt: *mut ffi::sqlite3_stmt = std::ptr::null_mut();
    let rc = ffi::sqlite3_prepare_v2(
        db,
        sql.as_ptr() as *const c_char,
        -1,
        &mut stmt,
        std::ptr::null_mut(),
    );
    if rc != ffi::SQLITE_OK {
        ffi::sqlite3_result_null(ctx);
        return;
    }
    ffi::sqlite3_bind_int64(stmt, 1, id);

    if ffi::sqlite3_step(stmt) == ffi::SQLITE_ROW {
        let ptr = ffi::sqlite3_column_text(stmt, 0);
        let len = ffi::sqlite3_column_bytes(stmt, 0);
        if !ptr.is_null() {
            ffi::sqlite3_result_text(ctx, ptr as *const c_char, len, ffi::SQLITE_TRANSIENT());
        } else {
            ffi::sqlite3_result_null(ctx);
        }
    } else {
        ffi::sqlite3_result_null(ctx);
    }

    ffi::sqlite3_finalize(stmt);
}

unsafe fn register_repo_name(db: *mut ffi::sqlite3) {
    let name = b"repo_name\0";
    ffi::sqlite3_create_function_v2(
        db,
        name.as_ptr() as *const c_char,
        1,
        ffi::SQLITE_UTF8,
        std::ptr::null_mut(),
        Some(xfunc_repo_name),
        None,
        None,
        None,
    );
}

// ---------------------------------------------------------------------------
// file_path(file_id) -> TEXT|NULL
// Runs: SELECT path FROM files WHERE id = ?
// ---------------------------------------------------------------------------

unsafe extern "C" fn xfunc_file_path(
    ctx: *mut ffi::sqlite3_context,
    argc: c_int,
    argv: *mut *mut ffi::sqlite3_value,
) {
    if argc != 1 {
        ffi::sqlite3_result_null(ctx);
        return;
    }

    let id = ffi::sqlite3_value_int64(*argv.offset(0));
    let db = ffi::sqlite3_context_db_handle(ctx);
    let sql = b"SELECT path FROM files WHERE id = ?\0";

    let mut stmt: *mut ffi::sqlite3_stmt = std::ptr::null_mut();
    let rc = ffi::sqlite3_prepare_v2(
        db,
        sql.as_ptr() as *const c_char,
        -1,
        &mut stmt,
        std::ptr::null_mut(),
    );
    if rc != ffi::SQLITE_OK {
        ffi::sqlite3_result_null(ctx);
        return;
    }
    ffi::sqlite3_bind_int64(stmt, 1, id);

    if ffi::sqlite3_step(stmt) == ffi::SQLITE_ROW {
        let ptr = ffi::sqlite3_column_text(stmt, 0);
        let len = ffi::sqlite3_column_bytes(stmt, 0);
        if !ptr.is_null() {
            ffi::sqlite3_result_text(ctx, ptr as *const c_char, len, ffi::SQLITE_TRANSIENT());
        } else {
            ffi::sqlite3_result_null(ctx);
        }
    } else {
        ffi::sqlite3_result_null(ctx);
    }

    ffi::sqlite3_finalize(stmt);
}

unsafe fn register_file_path(db: *mut ffi::sqlite3) {
    let name = b"file_path\0";
    ffi::sqlite3_create_function_v2(
        db,
        name.as_ptr() as *const c_char,
        1,
        ffi::SQLITE_UTF8,
        std::ptr::null_mut(),
        Some(xfunc_file_path),
        None,
        None,
        None,
    );
}

// ---------------------------------------------------------------------------
// fzy_score(a, b) -> REAL
// Simple subsequence scoring: fraction of b's chars found in a as a subsequence.
// Returns 0.0 if either arg is null/empty.
// ---------------------------------------------------------------------------

fn fzy_score_impl(haystack: &str, needle: &str) -> f64 {
    if needle.is_empty() || haystack.is_empty() {
        return 0.0;
    }

    // Walk haystack finding needle chars in order.
    let h: Vec<char> = haystack.to_lowercase().chars().collect();
    let n: Vec<char> = needle.to_lowercase().chars().collect();

    let mut hi = 0usize;
    let mut ni = 0usize;
    let mut matched = 0usize;

    while hi < h.len() && ni < n.len() {
        if h[hi] == n[ni] {
            matched += 1;
            ni += 1;
        }
        hi += 1;
    }

    if matched < n.len() {
        return 0.0; // not a subsequence at all
    }

    // Score: ratio of matched chars to needle length, weighted by contiguity.
    // Bonus for consecutive matches.
    let base_score = matched as f64 / n.len() as f64;

    // Contiguity bonus: count consecutive pairs in the match positions.
    let mut positions: Vec<usize> = Vec::with_capacity(n.len());
    let mut hi2 = 0usize;
    let mut ni2 = 0usize;
    while hi2 < h.len() && ni2 < n.len() {
        if h[hi2] == n[ni2] {
            positions.push(hi2);
            ni2 += 1;
        }
        hi2 += 1;
    }

    let consecutive = positions.windows(2).filter(|w| w[1] == w[0] + 1).count();
    let contiguity_bonus = if n.len() > 1 {
        consecutive as f64 / (n.len() - 1) as f64 * 0.5
    } else {
        0.5
    };

    (base_score * 0.5 + contiguity_bonus).min(1.0)
}

unsafe extern "C" fn xfunc_fzy_score(
    ctx: *mut ffi::sqlite3_context,
    argc: c_int,
    argv: *mut *mut ffi::sqlite3_value,
) {
    if argc != 2 {
        ffi::sqlite3_result_double(ctx, 0.0);
        return;
    }

    let a = match sqlite_text(argv, 0) {
        Some(t) => t,
        None => { ffi::sqlite3_result_double(ctx, 0.0); return; }
    };
    let b = match sqlite_text(argv, 1) {
        Some(t) => t,
        None => { ffi::sqlite3_result_double(ctx, 0.0); return; }
    };

    ffi::sqlite3_result_double(ctx, fzy_score_impl(a, b));
}

unsafe fn register_fzy_score(db: *mut ffi::sqlite3) {
    let name = b"fzy_score\0";
    ffi::sqlite3_create_function_v2(
        db,
        name.as_ptr() as *const c_char,
        2,
        ffi::SQLITE_UTF8 | ffi::SQLITE_DETERMINISTIC,
        std::ptr::null_mut(),
        Some(xfunc_fzy_score),
        None,
        None,
        None,
    );
}

// ---------------------------------------------------------------------------
// Views: repo_tags, repo_branches
// ---------------------------------------------------------------------------

/// Create convenience views for semver-tagged revisions and branches.
/// These are idempotent (CREATE VIEW IF NOT EXISTS).
pub async fn create_views(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE VIEW IF NOT EXISTS repo_tags AS \
         SELECT * FROM repo_revs WHERE is_semver = 1",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE VIEW IF NOT EXISTS repo_branches AS \
         SELECT * FROM repo_revs WHERE is_semver = 0",
    )
    .execute(pool)
    .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract a UTF-8 string from a sqlite3_value at position `offset` in `argv`.
/// Returns None if the value is NULL or not text.
unsafe fn sqlite_text<'a>(argv: *mut *mut ffi::sqlite3_value, offset: isize) -> Option<&'a str> {
    let val = *argv.offset(offset);
    let ty = ffi::sqlite3_value_type(val);
    if ty == ffi::SQLITE_TEXT {
        let ptr = ffi::sqlite3_value_text(val);
        let len = ffi::sqlite3_value_bytes(val);
        if ptr.is_null() || len == 0 {
            return Some("");
        }
        let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
        std::str::from_utf8(slice).ok()
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::fzy_score_impl;

    #[test]
    fn fzy_exact_match() {
        assert!((fzy_score_impl("hello", "hello") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn fzy_no_match() {
        assert_eq!(fzy_score_impl("abc", "xyz"), 0.0);
    }

    #[test]
    fn fzy_subsequence() {
        let score = fzy_score_impl("hello world", "hwd");
        assert!(score > 0.0);
        assert!(score < 1.0);
    }

    #[test]
    fn fzy_empty_needle() {
        assert_eq!(fzy_score_impl("hello", ""), 0.0);
    }

    #[test]
    fn fzy_empty_haystack() {
        assert_eq!(fzy_score_impl("", "hi"), 0.0);
    }

    #[test]
    fn fzy_contiguous_higher_than_scattered() {
        let contiguous = fzy_score_impl("foobar", "foo");
        let scattered = fzy_score_impl("fxoxo", "foo");
        assert!(contiguous > scattered, "contiguous={contiguous} scattered={scattered}");
    }
}
