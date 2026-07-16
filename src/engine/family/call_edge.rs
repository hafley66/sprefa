//! `CallEdge` family — the aggregation tier. Reads the owned
//! `_call_resolution` + `_call_raw_site` + `_call_owner` tables, computes the
//! support counts (`GROUP BY caller, callee, kind, rev` with `COUNT`), and
//! emits the public `call_edge(caller, callee, kind)` relation as the distinct
//! projection over support.
//!
//! Rust twin of the support + edge blocks in
//! `reproject_sqlite_call_affected_keys` (`src/storage/call.rs:508`):
//! `_call_edge_support` = `SELECT s.caller_sid, r.callee_sid, r.kind_sid,
//! o.rev_sid, COUNT(*) FROM _call_resolution r JOIN _call_raw_site s
//! USING(site_id) JOIN _call_owner o USING(owner_id) GROUP BY ...`, then
//! `call_edge` = `SELECT DISTINCT caller_sid, callee_sid, kind_sid FROM
//! _call_edge_support`. Step 2 proves the engine owns the GROUP BY/COUNT
//! shape, not just flat projection.

use super::*;
use std::collections::{HashMap, HashSet};

pub(crate) struct CallEdge;

impl Family for CallEdge {
    fn name(&self) -> &'static str {
        "call_edge"
    }

    fn out_cols(&self) -> &'static [&'static str] {
        &["caller", "callee", "kind"]
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

        // resolutions -> support counts keyed by (caller, callee, kind, rev).
        // _call_resolution is WITHOUT ROWID with composite PK (site_id,
        // callee_sid, kind_sid); we dep-key it by site_id (the leading column),
        // so the dep granularity is per-site: a site's resolutions move
        // together, which is the unit a delta replaces.
        let resolutions = ctx.scan("_call_resolution", "site_id", &["callee_sid", "kind_sid"])?;
        let mut support: HashMap<[i64; 4], i64> = HashMap::new();
        for (site_id, row) in &resolutions {
            let callee = as_int(&row[0]);
            let kind = as_int(&row[1]);
            let Some(&(owner_id, caller)) = caller_of_site.get(site_id) else { continue };
            let Some(&rev) = rev_by_owner.get(&owner_id) else { continue };
            *support.entry([caller, callee, kind, rev]).or_default() += 1;
        }

        // call_edge = DISTINCT (caller, callee, kind) over support.
        let mut emitted: HashSet<[i64; 3]> = HashSet::new();
        for &[caller, callee, kind, _rev] in support.keys() {
            if emitted.insert([caller, callee, kind]) {
                out.rows.push(vec![Value::Int(caller), Value::Int(callee), Value::Int(kind)]);
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

register_family!(CallEdge);
