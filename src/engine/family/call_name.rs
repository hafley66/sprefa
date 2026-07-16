//! `CallName` family — projects the owned `_call_def` table to the public
//! `call_name(sym, name)` relation as the distinct `(sym_sid, name_sid)` set.
//!
//! Unlike `CallSite`/`CallEdge`, this family reads NEITHER `_call_raw_site` nor
//! `_call_resolution` — only `_call_def`. That disjoint footprint is what makes
//! the router skip pay off on a real change: an owner/site delta rewrites
//! `_call_owner`/`_call_raw_site`/`_call_resolution` but requires the def set
//! unchanged (`apply_call_owner_delta` bails on `call-definition-changed`), so
//! it never touches `_call_def`. The changed-set the delta feeds the router
//! (`call_owner_delta_rels`) therefore misses `_call_def`, and `CallName` keeps
//! its memoized rows instead of rederiving — the first live, non-latent skip.

use super::*;

pub(crate) struct CallName;

impl Family for CallName {
    fn name(&self) -> &'static str {
        "call_name"
    }

    fn out_cols(&self) -> &'static [&'static str] {
        &["sym", "name"]
    }

    fn input_rels(&self) -> &'static [&'static str] {
        &["_call_def"]
    }

    fn derive(&self, ctx: &mut Ctx, out: &mut RowSink) -> Result<()> {
        // Dep-keyed by the leading PK column (`sym_sid`); the rel footprint is
        // `{_call_def}` regardless, which is the sound skip key.
        let defs = ctx.scan("_call_def", "sym_sid", &["name_sid"])?;
        let mut seen: HashSet<[i64; 2]> = HashSet::new();
        for (sym_sid, row) in defs {
            let name_sid = as_int(&row[0]);
            if seen.insert([sym_sid, name_sid]) {
                out.rows.push(vec![Value::Int(sym_sid), Value::Int(name_sid)]);
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

register_family!(CallName);
