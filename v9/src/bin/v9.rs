//! `v9 reach <ROOT> <DB> [--check]`: ryi call edges of ROOT, transitive reach kept in DB. A rerun
//! replays the stored edges and writes only what changed.
//! `v9 serve [PROGRAM] <DB> --port N`: a built-in program on Core over HTTP; the system is
//! `V9_SYSTEM` (unset: the default recipe).

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;
use v9::_0_id::Dict;
use v9::_1_effects;
use v9::_4_recipe::Recipe;
use v9::_5_ends::{Bounds, IService};
use v9::_6_back::_1_dd::MAX_ROUNDS;
use v9::_8_runtime::_0_programs::{program, PROGRAMS};
use v9::_8_runtime::_2_ryi::Ryi;
use v9::_8_runtime::_3_core::build;
use v9::_8_runtime::_4_handle::start;
use v9::_9_wire::http_auto::router;
use v9::skeleton::{_0_store as _2_store, _1_reach as _3_reach, _2_oracle as _4_oracle};

const USAGE: &str = "usage: v9 reach <ROOT> <DB> [--check] | v9 serve [PROGRAM] <DB> --port N";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let result = match args.get(1).map(String::as_str) {
        Some("reach") => {
            let (Some(root), Some(db)) = (args.get(2), args.get(3)) else {
                println!("{USAGE}");
                return ExitCode::from(2);
            };
            let check = args.iter().any(|arg| arg == "--check");
            run(PathBuf::from(root), PathBuf::from(db), check)
        }
        Some("serve") => serve(&args[2..]),
        _ => {
            println!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            println!("v9: {error}");
            ExitCode::from(1)
        }
    }
}

fn serve(args: &[String]) -> Result<(), String> {
    let flag = args.iter().position(|arg| arg == "--port");
    let port = flag.and_then(|index| args.get(index + 1));
    let positional: Vec<&str> = args
        .iter()
        .enumerate()
        .filter(|(index, _)| flag.is_none_or(|flag| *index != flag && *index != flag + 1))
        .map(|(_, arg)| arg.as_str())
        .collect();
    let (name, db) = match positional.as_slice() {
        [db] => ("reach", *db),
        [name, db] => (*name, *db),
        _ => return Err(USAGE.to_string()),
    };
    let port: u16 = port
        .ok_or(USAGE)?
        .parse()
        .map_err(|error| format!("--port: {error}"))?;
    let recipe = Recipe::parse(&_1_effects::v9_system().unwrap_or_default())
        .map_err(|diag| format!("V9_SYSTEM: {diag:?}"))?;
    let chosen = program(name).ok_or_else(|| {
        let known: Vec<&str> = PROGRAMS.iter().map(|(known, _)| *known).collect();
        format!("unknown program {name:?}, known: {known:?}")
    })?;
    let ryi = _1_effects::v9_ryi();
    let db = PathBuf::from(db);
    let handle = start(move || {
        let services: Vec<Box<dyn IService>> = vec![Box::new(Ryi::new(ryi))];
        let bounds = Bounds {
            max_rounds: MAX_ROUNDS,
        };
        build(recipe, chosen, &db, services, bounds)
    })
    .map_err(|diags| format!("{diags:?}"))?;
    let canonical = handle.recipe.canonical.clone();
    let listener = _1_effects::bind(port).map_err(|error| format!("bind: {error}"))?;
    let addr = listener.local_addr().map_err(|error| error.to_string())?;
    println!("serving {name} on http://{addr} recipe {canonical}");
    _1_effects::serve_http(listener, router(Arc::new(handle))).map_err(|error| error.to_string())
}

fn run(root: PathBuf, db_path: PathBuf, check: bool) -> Result<(), String> {
    let ryi = _1_effects::v9_ryi();
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
