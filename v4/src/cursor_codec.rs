// Canonical cursor encoding. Used by:
//   - SqliteQueue to serialize parked cursors to BLOB
//   - blake3 hashing for mount keys and lineage hashes
//   - future cross-process cursor transport
//
// Format, little-endian:
//
//   [u32 n_terms]
//   for each term:
//     [u32 name_len][name_bytes]
//     [u32 value_len][value_bytes]
//     [u8 cursor_value_tag][u64 payload]
//     [u64 value_id]
//     [u64 at_ref]
//
// The focal value is the term named `&`. The Weak<SprfStore> handle on
// Cursor is process-local and is not encoded.

use std::sync::Arc;

use crate::{Cursor, CursorValue, Ref, StringId, Term, WhereBytesId, FOCAL_TERM};

fn encode_cursor_value(value: CursorValue) -> (u8, u64) {
    match value {
        CursorValue::Null => (0, 0),
        CursorValue::Bool(v) => (1, u64::from(v)),
        CursorValue::Int(v) => (2, u64::from_le_bytes(v.to_le_bytes())),
        CursorValue::Float(v) => (3, v),
        CursorValue::String(v) => (4, v.0),
        CursorValue::WhereBytes(v) => (5, v.0),
        CursorValue::Blob(v) => (6, v),
    }
}

fn decode_cursor_value(tag: u8, payload: u64) -> Result<CursorValue, &'static str> {
    Ok(match tag {
        0 => CursorValue::Null,
        1 => CursorValue::Bool(payload != 0),
        2 => CursorValue::Int(i64::from_le_bytes(payload.to_le_bytes())),
        3 => CursorValue::Float(payload),
        4 => CursorValue::String(StringId(payload)),
        5 => CursorValue::WhereBytes(WhereBytesId(payload)),
        6 => CursorValue::Blob(payload),
        _ => return Err("unknown cursor_value tag"),
    })
}

fn encode_text(buf: &mut Vec<u8>, text: &str) {
    buf.extend_from_slice(&(text.len() as u32).to_le_bytes());
    buf.extend_from_slice(text.as_bytes());
}

fn read_u32(buf: &[u8], p: &mut usize, err: &'static str) -> Result<u32, &'static str> {
    if *p + 4 > buf.len() {
        return Err(err);
    }
    let value = u32::from_le_bytes(buf[*p..*p + 4].try_into().unwrap());
    *p += 4;
    Ok(value)
}

fn read_u64(buf: &[u8], p: &mut usize, err: &'static str) -> Result<u64, &'static str> {
    if *p + 8 > buf.len() {
        return Err(err);
    }
    let value = u64::from_le_bytes(buf[*p..*p + 8].try_into().unwrap());
    *p += 8;
    Ok(value)
}

fn read_text(buf: &[u8], p: &mut usize, len_err: &'static str) -> Result<Arc<str>, &'static str> {
    let len = read_u32(buf, p, len_err)? as usize;
    if *p + len > buf.len() {
        return Err("truncated text");
    }
    let text = std::str::from_utf8(&buf[*p..*p + len]).map_err(|_| "text not utf8")?;
    *p += len;
    Ok(Arc::<str>::from(text))
}

pub fn encode(c: &Cursor) -> Vec<u8> {
    let mut sz = 4;
    for term in &c.terms {
        sz += 4 + term.name.len();
        sz += 4 + term.value.len();
        sz += 1 + 8 + 8 + 8;
    }

    let mut buf = Vec::with_capacity(sz);
    buf.extend_from_slice(&(c.terms.len() as u32).to_le_bytes());
    for term in &c.terms {
        encode_text(&mut buf, term.name.as_ref());
        encode_text(&mut buf, term.value.as_ref());
        let (tag, payload) = encode_cursor_value(term.cursor_value);
        buf.push(tag);
        buf.extend_from_slice(&payload.to_le_bytes());
        buf.extend_from_slice(&term.value_id.0.to_le_bytes());
        buf.extend_from_slice(&term.at.0.to_le_bytes());
    }
    buf
}

pub fn decode(buf: &[u8]) -> Result<Cursor, &'static str> {
    let mut p = 0;
    let n_terms = read_u32(buf, &mut p, "buf too short for n_terms")? as usize;
    let mut terms = Vec::with_capacity(n_terms.max(1));

    for _ in 0..n_terms {
        let name = read_text(buf, &mut p, "truncated name_len")?;
        let value = read_text(buf, &mut p, "truncated value_len")?;
        if p + 9 > buf.len() {
            return Err("buf too short for cursor_value");
        }
        let cursor_value = decode_cursor_value(
            buf[p],
            u64::from_le_bytes(buf[p + 1..p + 9].try_into().unwrap()),
        )?;
        p += 9;
        let value_id = StringId(read_u64(buf, &mut p, "buf too short for value_id")?);
        let at = Ref(read_u64(buf, &mut p, "buf too short for at_ref")?);
        terms.push(Term {
            name,
            value,
            cursor_value,
            value_id,
            at,
        });
    }

    if !terms.iter().any(|term| term.name.as_ref() == FOCAL_TERM) {
        return Err("focal term missing");
    }

    Ok(Cursor { terms, store: None })
}

/// Stable u64 hash of a cursor, used for mount keys and lineage IDs.
pub fn hash_u64(c: &Cursor) -> u64 {
    let bytes = encode(c);
    let h = blake3::hash(&bytes);
    let b = h.as_bytes();
    u64::from_le_bytes(b[0..8].try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cursor(value: &str, kvs: &[(&str, &str)]) -> Cursor {
        let mut c = Cursor::default();
        c.set_value(value);
        for (k, v) in kvs {
            c.set(k, *v);
        }
        c
    }

    #[test]
    fn roundtrip_empty() {
        let c = Cursor::default();
        assert_eq!(decode(&encode(&c)).unwrap(), c);
    }

    #[test]
    fn roundtrip_simple() {
        let c = cursor("", &[(":FS", "/tmp/a.rs"), (":REPO", "myrepo")]);
        assert_eq!(decode(&encode(&c)).unwrap(), c);
    }

    #[test]
    fn roundtrip_unicode() {
        let c = cursor("", &[(":k", "héllo 🌊")]);
        assert_eq!(decode(&encode(&c)).unwrap(), c);
    }

    #[test]
    fn hash_is_stable_across_calls() {
        let c = cursor("", &[(":a", "1"), (":b", "2")]);
        assert_eq!(hash_u64(&c), hash_u64(&c));
    }

    #[test]
    fn hash_differs_on_value_change() {
        let a = cursor("", &[(":k", "1")]);
        let b = cursor("", &[(":k", "2")]);
        assert_ne!(hash_u64(&a), hash_u64(&b));
    }

    #[test]
    fn rejects_truncated() {
        assert!(decode(&[]).is_err());
        assert!(decode(&[1, 0, 0, 0]).is_err());
    }

    #[test]
    fn roundtrip_coord_space_fields() {
        let mut c = Cursor::default();
        c.set_focal_at(
            "hi",
            Ref(0x1234_5678),
            crate::CursorValue::WhereBytes(crate::WhereBytesId(0x1234_5678)),
        );
        c.focal_mut().value_id = StringId(0xDEADBEEF);
        c.terms.push(Term {
            name: Arc::<str>::from("MATCH"),
            value: Arc::<str>::from("hit"),
            cursor_value: crate::CursorValue::String(StringId(2)),
            value_id: StringId(2),
            at: Ref(3),
        });
        c.set("LO", "100");

        let got = decode(&encode(&c)).unwrap();
        assert_eq!(got, c);
    }

    #[test]
    fn roundtrip_cursor_value_scalars() {
        for cursor_value in [
            crate::CursorValue::Null,
            crate::CursorValue::Bool(true),
            crate::CursorValue::Int(-42),
            crate::CursorValue::Float(42.5f64.to_bits()),
            crate::CursorValue::String(StringId(7)),
            crate::CursorValue::WhereBytes(crate::WhereBytesId(8)),
            crate::CursorValue::Blob(9),
        ] {
            let mut c = Cursor::default();
            c.focal_mut().cursor_value = cursor_value;
            assert_eq!(decode(&encode(&c)).unwrap(), c);
        }
    }

    #[test]
    fn source_handle_encoding_does_not_include_source_body() {
        let mut c = Cursor::default();
        c.focal_mut().cursor_value = crate::CursorValue::WhereBytes(crate::WhereBytesId(99));
        c.set("FS", "/tmp/huge.rs");

        let encoded = encode(&c);
        assert!(
            !encoded
                .windows(b"fn huge_body".len())
                .any(|w| w == b"fn huge_body"),
            "source cursors encode handles, not file bodies",
        );
    }
}
