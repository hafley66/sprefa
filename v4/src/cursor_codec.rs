// Canonical cursor encoding. Used by:
//   - SqliteQueue (Phase 7) to serialize cursors to BLOB
//   - blake3 hashing for mount keys + lineage hashes
//   - any future cross-process cursor transport
//
// Format (little-endian throughout):
//   [u32 value_len][value_bytes]              ← &.value
//   [u32 n_terms]
//   for each term (already sorted by Cursor's invariant):
//     [u32 name_len][name_bytes]
//     [u32 value_len][value_bytes]
//
// No Arc identity, no pointers, no mtimes. Bytes-only. Deterministic
// across processes. Hashes of the encoding are stable.

use crate::Cursor;

pub fn encode(c: &Cursor) -> Vec<u8> {
    let mut sz = 4 + c.value.len() + 4;
    for (n, v) in &c.terms {
        sz += 4 + n.len() + 4 + v.len();
    }
    let mut buf = Vec::with_capacity(sz);
    buf.extend_from_slice(&(c.value.len() as u32).to_le_bytes());
    buf.extend_from_slice(c.value.as_bytes());
    buf.extend_from_slice(&(c.terms.len() as u32).to_le_bytes());
    for (n, v) in &c.terms {
        buf.extend_from_slice(&(n.len() as u32).to_le_bytes());
        buf.extend_from_slice(n.as_bytes());
        buf.extend_from_slice(&(v.len() as u32).to_le_bytes());
        buf.extend_from_slice(v.as_bytes());
    }
    buf
}

pub fn decode(buf: &[u8]) -> Result<Cursor, &'static str> {
    if buf.len() < 4 { return Err("buf too short for value_len"); }
    let vl = u32::from_le_bytes(buf[0..4].try_into().unwrap()) as usize;
    let mut p = 4;
    if p + vl > buf.len() { return Err("truncated value"); }
    let value: std::sync::Arc<str> =
        std::str::from_utf8(&buf[p..p+vl]).map_err(|_| "value not utf8")?.into();
    p += vl;
    if p + 4 > buf.len() { return Err("buf too short for n_terms"); }
    let n_terms = u32::from_le_bytes(buf[p..p+4].try_into().unwrap()) as usize;
    p += 4;
    let mut terms = Vec::with_capacity(n_terms);
    for _ in 0..n_terms {
        if p + 4 > buf.len() { return Err("truncated name_len"); }
        let nl = u32::from_le_bytes(buf[p..p+4].try_into().unwrap()) as usize;
        p += 4;
        if p + nl > buf.len() { return Err("truncated name"); }
        let name = std::str::from_utf8(&buf[p..p+nl]).map_err(|_| "name not utf8")?;
        p += nl;
        if p + 4 > buf.len() { return Err("truncated value_len"); }
        let vl = u32::from_le_bytes(buf[p..p+4].try_into().unwrap()) as usize;
        p += 4;
        if p + vl > buf.len() { return Err("truncated value"); }
        let val = std::str::from_utf8(&buf[p..p+vl]).map_err(|_| "value not utf8")?;
        p += vl;
        terms.push((name.into(), val.into()));
    }
    Ok(Cursor { value, terms })
}

/// Stable u64 hash of a cursor — used for mount keys and lineage IDs.
pub fn hash_u64(c: &Cursor) -> u64 {
    let bytes = encode(c);
    let h = blake3::hash(&bytes);
    let b = h.as_bytes();
    u64::from_le_bytes(b[0..8].try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn term(n: &str, v: &str) -> (Arc<str>, Arc<str>) { (n.into(), v.into()) }

    #[test]
    fn roundtrip_empty() {
        let c = Cursor { value: "".into(), terms: vec![] };
        assert_eq!(decode(&encode(&c)).unwrap(), c);
    }

    #[test]
    fn roundtrip_simple() {
        let c = Cursor { value: "".into(), terms: vec![
            term(":FS", "/tmp/a.rs"),
            term(":REPO", "myrepo"),
        ]};
        assert_eq!(decode(&encode(&c)).unwrap(), c);
    }

    #[test]
    fn roundtrip_unicode() {
        let c = Cursor { value: "".into(), terms: vec![term(":k", "héllo 🌊")]};
        assert_eq!(decode(&encode(&c)).unwrap(), c);
    }

    #[test]
    fn hash_is_stable_across_calls() {
        let c = Cursor { value: "".into(), terms: vec![term(":a", "1"), term(":b", "2")]};
        assert_eq!(hash_u64(&c), hash_u64(&c));
    }

    #[test]
    fn hash_differs_on_value_change() {
        let a = Cursor { value: "".into(), terms: vec![term(":k", "1")]};
        let b = Cursor { value: "".into(), terms: vec![term(":k", "2")]};
        assert_ne!(hash_u64(&a), hash_u64(&b));
    }

    #[test]
    fn rejects_truncated() {
        assert!(decode(&[1,0,0,0]).is_err()); // claims 1 term, no body
        assert!(decode(&[]).is_err());
    }
}
