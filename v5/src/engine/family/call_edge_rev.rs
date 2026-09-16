//! `CallEdgeRev` family — the rev-carrying twin of `CallEdge`. Reads the owned
//! `_call_resolution` + `_call_raw_site` + `_call_owner` tables, reconstructs
//! the same support keys (`caller, callee, kind, rev`), and emits the public
//! `call_edge_rev(caller, callee, kind, rev)` relation as the distinct set of
//! support KEYS — one row per key, keeping `rev` in the tuple instead of
//! collapsing to `(caller, callee, kind)` the way `CallEdge` does.
//!
//! Rust twin of the `call_edge_rev` block in
//! `reproject_sqlite_call_affected_keys` (`src/storage/call.rs`):
//! `_call_edge_support` = `SELECT s.caller_sid, r.callee_sid, r.kind_sid,
//! o.rev_sid, COUNT(*) FROM _call_resolution r JOIN _call_raw_site s
//! USING(site_id) JOIN _call_owner o USING(owner_id) GROUP BY ...`, then
//! `call_edge_rev` = `SELECT caller_sid, callee_sid, kind_sid, rev_sid FROM
//! _call_edge_support` (distinct = one row per support key).

use super::*;
use std::collections::HashMap;

pub(crate) struct CallEdgeRev;

impl Family for CallEdgeRev {
    fn name(&self) -> &'static str {
        "call_edge_rev"
    }

    fn out_cols(&self) -> &'static [&'static str] {
        &["caller", "callee", "kind", "rev"]
    }

    fn input_rels(&self) -> &'static [&'static str] {
        &["_call_owner", "_call_raw_site", "_call_resolution"]
    }

    fn derive(&self, ctx: &mut Ctx, out: &mut RowSink) -> Result<()> {
        // owner_id -> rev_sid
        let owners = ctx.scan("_call_owner", "owner_id", &["rev_sid"])?;
        let rev_by_owner: HashMap<i64, i64> = owners
            .into_iter()
            .map(|(owner_id, row)| (owner_id, as_int(&row[0])))
            .collect();

        // site_id -> (owner_id, caller_sid)
        let sites = ctx.scan("_call_raw_site", "site_id", &["owner_id", "caller_sid"])?;
        let caller_of_site: HashMap<i64, (i64, i64)> = sites
            .into_iter()
            .map(|(site_id, row)| (site_id, (as_int(&row[0]), as_int(&row[1]))))
            .collect();

        // resolutions -> support keys (caller, callee, kind, rev), same as
        // CallEdge. Dep-keyed by site_id (the leading column of _call_resolution).
        let resolutions = ctx.scan("_call_resolution", "site_id", &["callee_sid", "kind_sid"])?;
        let mut support: HashMap<[i64; 4], i64> = HashMap::new();
        for (site_id, row) in &resolutions {
            let callee = as_int(&row[0]);
            let kind = as_int(&row[1]);
            let Some(&(owner_id, caller)) = caller_of_site.get(site_id) else {
                continue;
            };
            let Some(&rev) = rev_by_owner.get(&owner_id) else {
                continue;
            };
            *support.entry([caller, callee, kind, rev]).or_default() += 1;
        }

        // call_edge_rev = the support keys. A HashMap's keys are already
        // distinct, so one row per key — rev retained, unlike CallEdge.
        for &[caller, callee, kind, rev] in support.keys() {
            out.rows.push(vec![
                Value::Int(caller),
                Value::Int(callee),
                Value::Int(kind),
                Value::Int(rev),
            ]);
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

register_family!(CallEdgeRev);
