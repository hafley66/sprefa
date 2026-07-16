//! `CallSite` family — projects the owned `_call_raw_site` + `_call_owner`
//! tables to the public `call_site(repo, caller, callee, file, line)` relation.
//!
//! This is the Rust twin of `reproject_sqlite_call_affected_keys`' `call_site`
//! block (`src/storage/call.rs:478`): `SELECT DISTINCT o.repo_sid, s.caller_sid,
//! s.callee_sid, s.file_sid, s.line FROM _call_raw_site s JOIN _call_owner o
//! USING(owner_id)`. Step 0 hosts it on the family engine to prove the
//! projection tier; the aggregation tier (`call_edge` over support counts) is
//! step 2.

use super::*;

pub(crate) struct CallSite;

impl Family for CallSite {
    fn name(&self) -> &'static str {
        "call_site"
    }

    fn derive(&self, ctx: &mut Ctx, out: &mut RowSink) -> Result<()> {
        let owners = ctx.scan("_call_owner", "owner_id", &["repo_sid"])?;
        let repo_by_owner: std::collections::HashMap<i64, i64> = owners
            .into_iter()
            .map(|(owner_id, row)| (owner_id, as_int(&row[0])))
            .collect();

        let sites = ctx.scan(
            "_call_raw_site",
            "site_id",
            &["owner_id", "caller_sid", "callee_sid", "file_sid", "line"],
        )?;

        let mut seen: HashSet<[i64; 5]> = HashSet::new();
        for (_site_id, row) in sites {
            let owner_id = as_int(&row[0]);
            let caller = as_int(&row[1]);
            let callee = as_int(&row[2]);
            let file = as_int(&row[3]);
            let line = as_int(&row[4]);
            let Some(&repo) = repo_by_owner.get(&owner_id) else { continue; };
            if seen.insert([repo, caller, callee, file, line]) {
                out.rows.push(vec![
                    Value::Int(repo),
                    Value::Int(caller),
                    Value::Int(callee),
                    Value::Int(file),
                    Value::Int(line),
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
