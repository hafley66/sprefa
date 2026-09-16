//! `CallDef` family — the rev-collapsed twin of `CallDefRev`. Projects the
//! owned `_call_def` def-site store to the public
//! `call_def(repo, sym, kind, file, line, end)` relation, dropping `rev` so two
//! revs of one definition collapse to a single row (the legacy `call_def` was
//! the rev-deduped union of `call_def_rev`).
//!
//! Reads only `_call_def` (same footprint as `CallName`/`CallDefRev`), so a
//! site-only owner delta leaves it untouched and the router skips it. Dedup via
//! a HashSet over the 6-tuple (rev excluded) — that collapse is the whole
//! difference from `CallDefRev`.

use super::*;

pub(crate) struct CallDef;

impl Family for CallDef {
    fn name(&self) -> &'static str {
        "call_def"
    }

    fn out_cols(&self) -> &'static [&'static str] {
        &["repo", "sym", "kind", "file", "line", "end"]
    }

    fn input_rels(&self) -> &'static [&'static str] {
        &["_call_def"]
    }

    fn derive(&self, ctx: &mut Ctx, out: &mut RowSink) -> Result<()> {
        // `end` is a SQL keyword — pre-quote it for Ctx::scan's verbatim
        // interpolation. rev_sid is intentionally not scanned: this is the
        // rev-collapsed projection.
        let defs = ctx.scan(
            "_call_def",
            "sym_sid",
            &["repo_sid", "kind_sid", "file_sid", "line", "\"end\""],
        )?;
        let mut seen: HashSet<[i64; 6]> = HashSet::new();
        for (sym_sid, row) in defs {
            let repo = as_int(&row[0]);
            let kind = as_int(&row[1]);
            let file = as_int(&row[2]);
            let line = as_int(&row[3]);
            let end = as_int(&row[4]);
            // out_cols order: repo, sym, kind, file, line, end. Two revs of one
            // def share this tuple and collapse to one row.
            if seen.insert([repo, sym_sid, kind, file, line, end]) {
                out.rows.push(vec![
                    Value::Int(repo),
                    Value::Int(sym_sid),
                    Value::Int(kind),
                    Value::Int(file),
                    Value::Int(line),
                    Value::Int(end),
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

register_family!(CallDef);
