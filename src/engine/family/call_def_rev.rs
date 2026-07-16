//! `CallDefRev` family — projects the owned `_call_def` def-site store to the
//! public `call_def_rev(repo, sym, kind, file, line, end, rev)` relation, one
//! row per stored (sym, rev) definition with the rev kept in the tuple.
//!
//! Reads only `_call_def` (same footprint as `CallName`/`CallDef`): a site-only
//! owner delta never touches `_call_def`, so the router skips this family. The
//! `_call_def` PK is (sym_sid, rev_sid), so rows are already distinct on
//! (sym, rev); the HashSet keeps set semantics for the full tuple regardless.
//!
//! `end` is a SQL keyword, so it is passed to `Ctx::scan` pre-quoted (`"end"`);
//! Ctx::scan interpolates column names verbatim into the SELECT.

use super::*;

pub(crate) struct CallDefRev;

impl Family for CallDefRev {
    fn name(&self) -> &'static str {
        "call_def_rev"
    }

    fn out_cols(&self) -> &'static [&'static str] {
        &["repo", "sym", "kind", "file", "line", "end", "rev"]
    }

    fn input_rels(&self) -> &'static [&'static str] {
        &["_call_def"]
    }

    fn derive(&self, ctx: &mut Ctx, out: &mut RowSink) -> Result<()> {
        // Dep-keyed by the leading PK column (`sym_sid`); footprint is
        // `{_call_def}` regardless.
        let defs = ctx.scan(
            "_call_def",
            "sym_sid",
            &["repo_sid", "kind_sid", "file_sid", "line", "\"end\"", "rev_sid"],
        )?;
        let mut seen: HashSet<[i64; 7]> = HashSet::new();
        for (sym_sid, row) in defs {
            let repo = as_int(&row[0]);
            let kind = as_int(&row[1]);
            let file = as_int(&row[2]);
            let line = as_int(&row[3]);
            let end = as_int(&row[4]);
            let rev = as_int(&row[5]);
            // out_cols order: repo, sym, kind, file, line, end, rev.
            if seen.insert([repo, sym_sid, kind, file, line, end, rev]) {
                out.rows.push(vec![
                    Value::Int(repo),
                    Value::Int(sym_sid),
                    Value::Int(kind),
                    Value::Int(file),
                    Value::Int(line),
                    Value::Int(end),
                    Value::Int(rev),
                ]);
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

register_family!(CallDefRev);
