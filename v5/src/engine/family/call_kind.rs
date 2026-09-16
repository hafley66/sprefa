//! `CallKind` family — projects the owned `_call_raw_site` table to the public
//! `call_kind(fn, kind)` relation as the distinct `(caller_sid,
//! classification_sid)` set over sites whose caller resolved and whose callee
//! carried a read/write classification.
//!
//! Rust twin of the `call_kind` block in `reproject_sqlite_call_affected_keys`
//! (`src/storage/call.rs`): `SELECT DISTINCT caller_sid, classification_sid FROM
//! _call_raw_site WHERE caller_sid != 0 AND classification_sid IS NOT NULL`.
//! `classification_sid` is the one nullable column here, so a NULL surfaces as
//! `Value::Null` from `Ctx::scan` and the row is dropped; an unresolved caller
//! (`caller_sid == 0`) is dropped too.

use super::*;

pub(crate) struct CallKind;

impl Family for CallKind {
    fn name(&self) -> &'static str {
        "call_kind"
    }

    fn out_cols(&self) -> &'static [&'static str] {
        &["fn", "kind"]
    }

    fn input_rels(&self) -> &'static [&'static str] {
        &["_call_raw_site"]
    }

    fn derive(&self, ctx: &mut Ctx, out: &mut RowSink) -> Result<()> {
        let sites = ctx.scan(
            "_call_raw_site",
            "site_id",
            &["caller_sid", "classification_sid"],
        )?;
        let mut seen: HashSet<[i64; 2]> = HashSet::new();
        for (_site_id, row) in sites {
            let caller = as_int(&row[0]);
            // classification_sid is nullable: a NULL becomes Value::Null, which
            // the source SQL drops (`classification_sid IS NOT NULL`).
            let Value::Int(kind) = row[1] else { continue };
            if caller == 0 {
                continue;
            }
            if seen.insert([caller, kind]) {
                out.rows.push(vec![Value::Int(caller), Value::Int(kind)]);
            }
        }
        Ok(())
    }
}

fn as_int(v: &Value) -> i64 {
    match v {
        Value::Int(n) => *n,
        _ => 0,
    }
}

register_family!(CallKind);
