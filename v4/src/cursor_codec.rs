// Canonical cursor encoding. Used by:
//   - SqliteQueue (Phase 7) to serialize cursors to BLOB
//   - blake3 hashing for mount keys + lineage hashes
//   - any future cross-process cursor transport
//
// Format (little-endian throughout). Layer 0c.1 widens the wire to
// include the coord-space fields next to the legacy bare-string bag:
//
//   [u32 value_len][value_bytes]              ← &.value (legacy Arc<str>)
//   [u64 value_id]                            ← StringId FK
//   [u64 at_ref]                              ← Ref FK
//   [u32 n_terms]                             ← coord-space terms
//   for each term:
//     [u64 name][u64 value][u64 at]
//   [u32 n_raw]                               ← bare-string raw_terms
//   for each raw term (already sorted):
//     [u32 name_len][name_bytes]
//     [u32 value_len][value_bytes]
//
// The Weak<SprfStore> handle on Cursor is process-local and NOT
// encoded; decode produces store=None and callers re-attach via
// `Cursor::with_store(...)` when they have the store in scope.
//
// No Arc identity, no pointers, no mtimes. Bytes-only. Deterministic
// across processes. Hashes of the encoding are stable.

use crate::{Cursor, Ref, StringId, Term};

pub fn encode(c: &Cursor) -> Vec<u8> {
    let mut sz = 4 + c.value.len()        // value
        + 8 + 8                            // value_id + at
        + 4 + c.terms.len() * (8 * 3)      // coord-space terms
        + 4;                               // n_raw header
    for (n, v) in &c.raw_terms {
        sz += 4 + n.len() + 4 + v.len();
    }
    let mut buf = Vec::with_capacity(sz);
    // legacy &.value
    buf.extend_from_slice(&(c.value.len() as u32).to_le_bytes());
    buf.extend_from_slice(c.value.as_bytes());
    // coord-space scalars
    buf.extend_from_slice(&c.value_id.0.to_le_bytes());
    buf.extend_from_slice(&c.at.0.to_le_bytes());
    // coord-space terms
    buf.extend_from_slice(&(c.terms.len() as u32).to_le_bytes());
    for t in &c.terms {
        buf.extend_from_slice(&t.name.0.to_le_bytes());
        buf.extend_from_slice(&t.value.0.to_le_bytes());
        buf.extend_from_slice(&t.at.0.to_le_bytes());
    }
    // bare-string raw_terms
    buf.extend_from_slice(&(c.raw_terms.len() as u32).to_le_bytes());
    for (n, v) in &c.raw_terms {
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

    if p + 16 > buf.len() { return Err("buf too short for value_id+at"); }
    let value_id = StringId(u64::from_le_bytes(buf[p..p+8].try_into().unwrap()));
    p += 8;
    let at = Ref(u64::from_le_bytes(buf[p..p+8].try_into().unwrap()));
    p += 8;

    if p + 4 > buf.len() { return Err("buf too short for n_terms"); }
    let n_terms = u32::from_le_bytes(buf[p..p+4].try_into().unwrap()) as usize;
    p += 4;
    let mut terms = Vec::with_capacity(n_terms);
    for _ in 0..n_terms {
        if p + 24 > buf.len() { return Err("truncated coord term"); }
        let name  = StringId(u64::from_le_bytes(buf[p..p+8].try_into().unwrap()));
        let value = StringId(u64::from_le_bytes(buf[p+8..p+16].try_into().unwrap()));
        let at_r  = Ref(u64::from_le_bytes(buf[p+16..p+24].try_into().unwrap()));
        terms.push(Term { name, value, at: at_r });
        p += 24;
    }

    if p + 4 > buf.len() { return Err("buf too short for n_raw"); }
    let n_raw = u32::from_le_bytes(buf[p..p+4].try_into().unwrap()) as usize;
    p += 4;
    let mut raw_terms = Vec::with_capacity(n_raw);
    for _ in 0..n_raw {
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
        raw_terms.push((name.into(), val.into()));
    }
    Ok(Cursor {
        value,
        value_id,
        at,
        terms,
        raw_terms,
        store: None,
    })
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
        let c = Cursor { value: "".into(), raw_terms: vec![], ..Default::default() };
        assert_eq!(decode(&encode(&c)).unwrap(), c);
    }

    #[test]
    fn roundtrip_simple() {
        let c = Cursor {
            value: "".into(),
            raw_terms: vec![
                term(":FS", "/tmp/a.rs"),
                term(":REPO", "myrepo"),
            ],
            ..Default::default()
        };
        assert_eq!(decode(&encode(&c)).unwrap(), c);
    }

    #[test]
    fn roundtrip_unicode() {
        let c = Cursor {
            value: "".into(),
            raw_terms: vec![term(":k", "héllo 🌊")],
            ..Default::default()
        };
        assert_eq!(decode(&encode(&c)).unwrap(), c);
    }

    #[test]
    fn hash_is_stable_across_calls() {
        let c = Cursor {
            value: "".into(),
            raw_terms: vec![term(":a", "1"), term(":b", "2")],
            ..Default::default()
        };
        assert_eq!(hash_u64(&c), hash_u64(&c));
    }

    #[test]
    fn hash_differs_on_value_change() {
        let a = Cursor {
            value: "".into(),
            raw_terms: vec![term(":k", "1")],
            ..Default::default()
        };
        let b = Cursor {
            value: "".into(),
            raw_terms: vec![term(":k", "2")],
            ..Default::default()
        };
        assert_ne!(hash_u64(&a), hash_u64(&b));
    }

    #[test]
    fn rejects_truncated() {
        assert!(decode(&[1,0,0,0]).is_err()); // claims 1-byte value, no body
        assert!(decode(&[]).is_err());
    }

    #[test]
    fn roundtrip_coord_space_fields() {
        let c = Cursor {
            value: "hi".into(),
            value_id: StringId(0xDEADBEEF),
            at: Ref(0x1234_5678),
            terms: vec![
                Term { name: StringId(1), value: StringId(2), at: Ref(3) },
                Term { name: StringId(4), value: StringId(5), at: Ref(0) },
            ],
            raw_terms: vec![term("LO", "100")],
            ..Default::default()
        };
        let got = decode(&encode(&c)).unwrap();
        assert_eq!(got, c);
    }
}
