//! `v9 reach <ROOT> <DB> [--check]`: the `reach` program on Core. ROOT is committed as `root`, the
//! ryi service answers its call edges, `reach` is derived and kept in DB. A rerun replays the
//! stored rows, asks ryi again and writes only what changed.
//! `v9 serve [PROGRAM] <DB> --port N`: a built-in program on Core over HTTP; the system is
//! `V9_SYSTEM` (unset: the default recipe).

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;
use v9::_0_id::id_of;
use v9::_1_effects;
use v9::_3_plan::Row;
use v9::_4_recipe::Recipe;
use v9::_5_ends::{Cell, IService};
use v9::_8_runtime::_0_programs::{program, reach, PROGRAMS};
use v9::_8_runtime::_2_ryi::Ryi;
use v9::_8_runtime::_3_core::{build, Core};
use v9::_8_runtime::_5_run;

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
            run(root, PathBuf::from(db), check)
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
    _5_run::system().map_err(|diag| format!("V9_SYSTEM: {diag:?}"))?;
    let chosen = program(name).ok_or_else(|| {
        let known: Vec<&str> = PROGRAMS.iter().map(|(known, _)| *known).collect();
        format!("unknown program {name:?}, known: {known:?}")
    })?;
    let ryi = _1_effects::v9_ryi();
    let services = move || -> Vec<Box<dyn IService>> { vec![Box::new(Ryi::new(ryi))] };
    _5_run::serve(name, chosen, services, PathBuf::from(db), port)
}

/// Plain Rust transitive closure, by search from each caller: the `--check` reference.
fn closure(edges: &BTreeSet<(String, String)>) -> BTreeSet<(String, String)> {
    let mut out: HashMap<&str, Vec<&str>> = HashMap::new();
    for (from, to) in edges {
        out.entry(from).or_default().push(to);
    }
    let mut reach = BTreeSet::new();
    for (&start, next) in &out {
        let mut seen = BTreeSet::new();
        let mut stack = next.clone();
        while let Some(node) = stack.pop() {
            if seen.insert(node) {
                stack.extend(out.get(node).into_iter().flatten().copied());
            }
        }
        reach.extend(
            seen.into_iter()
                .map(|node| (start.to_string(), node.to_string())),
        );
    }
    reach
}

fn ints(core: &Core, sql: &str) -> Result<Vec<Row>, String> {
    let table = core.query(sql).map_err(|diag| format!("{diag:?}"))?;
    table
        .rows
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|cell| match cell {
                    Cell::Int(value) => Ok(value as u64),
                    other => Err(format!("{sql}: {other:?}")),
                })
                .collect()
        })
        .collect()
}

fn pairs(core: &Core, sql: &str) -> Result<BTreeSet<(String, String)>, String> {
    let table = core.query(sql).map_err(|diag| format!("{diag:?}"))?;
    table
        .rows
        .into_iter()
        .map(|row| match &row[..] {
            [Cell::Text(a), Cell::Text(b)] => Ok((a.clone(), b.clone())),
            other => Err(format!("{sql}: {other:?}")),
        })
        .collect()
}

fn count(core: &Core, table: &str) -> Result<u64, String> {
    Ok(ints(core, &format!("SELECT count(*) FROM {table}"))?[0][0])
}

/// Commits ROOT and retracts every other stored root. A stored ROOT has its ask in the replayed
/// engine and no request change to route, so `refresh` re-asks ryi for it.
fn run(root: &str, db_path: PathBuf, check: bool) -> Result<(), String> {
    let services: Vec<Box<dyn IService>> = vec![Box::new(Ryi::new(_1_effects::v9_ryi()))];
    let started = Instant::now();
    let mut core = build(
        Recipe::default(),
        reach(),
        &db_path,
        services,
        _5_run::BOUNDS,
    )
    .map_err(|diags| format!("{diags:?}"))?;
    let open = started.elapsed();

    let root_id = id_of(root.as_bytes());
    let stored_roots: Vec<u64> = ints(&core, "SELECT root FROM root")?
        .into_iter()
        .map(|row| row[0])
        .collect();
    let dict_before = count(&core, "dict")?;

    let started = Instant::now();
    let stored = stored_roots.contains(&root_id);
    let mut changes = Vec::new();
    if !stored {
        changes.push(("root".to_string(), vec![Cell::Text(root.to_string())], 1));
    }
    for other in stored_roots.iter().filter(|id| **id != root_id) {
        let text = String::from_utf8_lossy(&core.dict.bytes[other]).into_owned();
        changes.push(("root".to_string(), vec![Cell::Text(text)], -1));
    }
    let mut changed = Vec::new();
    if !changes.is_empty() {
        changed = core.commit(changes).map_err(|diag| format!("{diag:?}"))?.1;
    }
    let commit = started.elapsed();
    let started = Instant::now();
    if stored {
        let refreshed = core.refresh("ryi").map_err(|diag| format!("{diag:?}"))?;
        changed.extend(refreshed.into_iter().flat_map(|(_, named)| named));
    }
    let refresh = started.elapsed();

    let signed = |rel: &str, sign: isize| {
        changed
            .iter()
            .filter(|(name, _, diff)| name == rel && diff.signum() == sign)
            .count()
    };
    let written = changed.len() as u64 + count(&core, "dict")? - dict_before;
    println!(
        "edges {} (+{} -{}) reach {} (+{} -{}) rows_written {}",
        count(&core, "edge")?,
        signed("edge", 1),
        signed("edge", -1),
        count(&core, "reach")?,
        signed("reach", 1),
        signed("reach", -1),
        written,
    );
    println!(
        "ms open {} commit {} refresh {}",
        open.as_millis(),
        commit.as_millis(),
        refresh.as_millis(),
    );
    if check {
        let edges = pairs(&core, "SELECT caller, callee FROM edge_text")?;
        let stored = pairs(&core, "SELECT caller, callee FROM reach_text")?;
        let oracle = closure(&edges);
        if oracle != stored {
            return Err(format!(
                "reach differs from the oracle: stored {} oracle {}",
                stored.len(),
                oracle.len()
            ));
        }
        println!("check: reach equals the oracle ({} rows)", oracle.len());
    }
    Ok(())
}
