//! `v9 reach <ROOT> <DB> [--check]`: ryi call edges of ROOT, transitive reach kept in DB. A rerun
//! replays the stored edges and writes only what changed.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;
use v9::_0_id::Dict;
use v9::{_1_effects, _2_store, _3_reach, _4_oracle};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let (Some("reach"), Some(root), Some(db)) =
        (args.get(1).map(String::as_str), args.get(2), args.get(3))
    else {
        println!("usage: v9 reach <ROOT> <DB> [--check]");
        return ExitCode::from(2);
    };
    let check = args.iter().any(|arg| arg == "--check");
    match run(PathBuf::from(root), PathBuf::from(db), check) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            println!("v9: {error}");
            ExitCode::from(1)
        }
    }
}

fn run(root: PathBuf, db_path: PathBuf, check: bool) -> Result<(), String> {
    let ryi = std::env::var("V9_RYI").unwrap_or_else(|_| "ryi".to_string());
    let mut db = _1_effects::open_db(&db_path).map_err(|e| e.to_string())?;
    _2_store::open(&db).map_err(|e| e.to_string())?;

    let started = Instant::now();
    let mut dict = Dict::default();
    _2_store::load_dict(&db, &mut dict).map_err(|e| e.to_string())?;
    let stored = _2_store::load_pairs(&db, "edge").map_err(|e| e.to_string())?;
    let load = started.elapsed();

    let started = Instant::now();
    let found = _1_effects::ryi_call_edges(&ryi, &root)?;
    let ryi_time = started.elapsed();
    let current: BTreeSet<_2_store::Edge> = found
        .iter()
        .map(|edge| (dict.intern(edge.caller.as_bytes()), dict.intern(edge.callee.as_bytes())))
        .collect();

    let mut edge_changes: Vec<(_2_store::Edge, isize)> =
        current.difference(&stored).map(|edge| (*edge, 1)).collect();
    edge_changes.extend(stored.difference(&current).map(|edge| (*edge, -1)));

    let settled = _3_reach::settle(stored.iter().copied().collect(), edge_changes.clone());

    let started = Instant::now();
    let written = _2_store::write(
        &mut db,
        _2_store::Frontier {
            dict: dict.take_new(),
            edge: &edge_changes,
            reach: &settled.reach,
        },
    )
    .map_err(|e| e.to_string())?;
    let write = started.elapsed();

    let reach_rows = _2_store::load_pairs(&db, "reach").map_err(|e| e.to_string())?;
    println!(
        "edges {} (+{} -{}) reach {} (+{} -{}) rows_written {}",
        current.len(),
        edge_changes.iter().filter(|(_, d)| *d > 0).count(),
        edge_changes.iter().filter(|(_, d)| *d < 0).count(),
        reach_rows.len(),
        settled.reach.iter().filter(|(_, d)| *d > 0).count(),
        settled.reach.iter().filter(|(_, d)| *d < 0).count(),
        written,
    );
    println!(
        "ms load {} ryi {} replay {} delta {} write {}",
        load.as_millis(),
        ryi_time.as_millis(),
        settled.replay.as_millis(),
        settled.delta.as_millis(),
        write.as_millis(),
    );
    if check {
        let oracle = _4_oracle::closure(&current);
        if oracle != reach_rows {
            return Err(format!(
                "reach differs from the oracle: stored {} oracle {}",
                reach_rows.len(),
                oracle.len()
            ));
        }
        println!("check: reach equals the oracle ({} rows)", oracle.len());
    }
    Ok(())
}
