//! Proof that a Rust host links boop and runs the four instant v2 views in
//! process. This file uses only the public crate surface, no bin internals.

use boop::{FactKind, FactQuery, GroupBy, UsageQuery};

fn main() -> anyhow::Result<()> {
    let store = boop::open_default()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as u64;

    let subagents = store.query_status(6 * 3_600_000, now)?;
    println!("view 1 subagent readout: {} sessions", subagents.len());

    let shells = store.query_sessions(None, Some(500))?;
    println!("view 2 external shells: {} sessions", shells.len());

    let network = store.query_facts(
        FactKind::Fetch,
        &FactQuery {
            limit: Some(50),
            ..Default::default()
        },
    )?;
    println!("view 3 network: {} rows", network.len());

    let touched = store.query_facts(
        FactKind::Touch,
        &FactQuery {
            limit: Some(50),
            ..Default::default()
        },
    )?;
    println!("view 4 file sidebar: {} rows", touched.len());

    let usage = store.usage_report(Some(GroupBy::Harness), &UsageQuery::default())?;
    for row in &usage {
        println!(
            "  usage by harness: {} calls={}",
            row["bucket"], row["calls"]
        );
    }
    Ok(())
}
