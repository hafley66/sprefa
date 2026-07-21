//! Pure pre-storage transforms, done RIGHT. No DB, no state — deterministic
//! functions the Store calls before writing. Each entry names the v5 hack it
//! replaces so the hacks stay dead.

/// Strip to ASCII alphanumeric, lowercase. The ONE v5 transform that was already
/// correct (spine.rs:297). Used for case/punct-insensitive name matching,
/// computed on read — never stored as a duplicate normalized column.
pub fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Content hash for a file: blake3 truncated to 16 bytes, the UNIQUE natural key
/// on `files` that dedups byte-identical content to ONE row.
///
/// v5 hack replaced: `FileId = hash64(path+content)` used AS the id (spine.rs:23)
/// — an 8-byte hash that both defeated varint AND could not dedup identical
/// bytes across two paths. Here the surrogate is the dense DB rowid and the hash
/// is only the natural key.
pub fn content_hash(bytes: &[u8]) -> [u8; 16] {
    let h = blake3::hash(bytes);
    let mut out = [0u8; 16];
    out.copy_from_slice(&h.as_bytes()[..16]);
    out
}

/// A 40-char git sha hex string -> 20 raw bytes for `repo_revs.git_sha`.
///
/// v5 hack replaced: the 40-char hex was stored as TEXT and, worse, smuggled
/// into interned id strings via `salt_rev` (`format!("{rev}\u{1}{id}")`). Here
/// it is 20 raw bytes in exactly one column and nowhere else.
pub fn git_sha_bytes(hex: &str) -> Option<[u8; 20]> {
    if hex.len() != 40 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = [0u8; 20];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

/// Byte offset -> (line, col), 0-based, derived from content on demand.
///
/// v5 hack replaced: `file:line:col` was baked INTO dictionary strings (10.2% of
/// the dict) and even hashed into node ids (`StringId::of(format!("{file}:{line}:
/// {col}:{kind}"))`). Here a node is `(file_id, byte_start)` and line/col are
/// DERIVED here when a human needs to read them — never stored.
pub fn byte_to_linecol(content: &[u8], offset: usize) -> (u32, u32) {
    let end = offset.min(content.len());
    let mut line = 0u32;
    let mut col = 0u32;
    for &b in &content[..end] {
        if b == b'\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}
