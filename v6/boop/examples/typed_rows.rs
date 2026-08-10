//! Receipt: typed public rows for sessions from the default store. Prints flat
//! scalar fields, the canonical verb vs raw spelling, and grouped usage, so a
//! host links the typed surface without shelling out.

use boop::{FactQuery, GroupBy, UsageQuery};

fn main() -> anyhow::Result<()> {
    let store = boop::open_default()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as u64;

    let sessions = store.session_rows(None, Some(3))?;
    for row in sessions {
        println!(
            "SessionRow {{ session={} harness={} cwd={:?} turns={} last_ts={:?} }}",
            row.session, row.harness, row.cwd, row.turns, row.last_ts
        );
    }

    let status = store.status_rows(6 * 3_600_000, now)?;
    println!("StatusRow count: {}", status.len());

    for row in store.touch_rows(&FactQuery::default())? {
        println!(
            "TouchRow {{ path={} verb={} raw_verb={} turn={} ts={} }}",
            row.path, row.verb, row.raw_verb, row.turn, row.ts
        );
    }

    for row in store.query_cursors(None)? {
        println!(
            "FactCursor {{ session={} transcript={} byte_offset={} }}",
            row.session, row.transcript, row.byte_offset
        );
    }

    let usage = store.usage_report_rows(Some(GroupBy::Harness), &UsageQuery::default())?;
    for row in usage {
        println!(
            "UsageRow {{ bucket={:?} calls={} first_ts={} last_ts={} }}",
            row.bucket, row.calls, row.first_ts, row.last_ts
        );
    }
    Ok(())
}
