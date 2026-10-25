//! `v9 reach <ROOT> <DB> [--check]`: the `reach` program on Core. ROOT is committed as `root`, the
//! ryi service answers its call edges, `reach` is derived and kept in DB. A rerun replays the
//! stored rows, asks ryi again and writes only what changed.
//! `v9 serve [PROGRAM] <DB> --port N`: a built-in program on Core over HTTP; the system is
//! `V9_SYSTEM` (unset: the default recipe).

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use v9::_0_id::{id_of, Dict};
use v9::_1_effects;
use v9::_3_plan::Row;
use v9::_4_recipe::Recipe;
use v9::_5_ends::{Bounds, Cadence, Cell, Change, IService};
use v9::_8_runtime::_0_programs::{program, reach, PROGRAMS};
use v9::_8_runtime::_2_ryi::Ryi;
use v9::_8_runtime::_3_core::build;
use v9::_8_runtime::_3_core::Core;
use v9::_8_runtime::_4_handle::start;
use v9::_9_wire::http_auto::router;

const USAGE: &str = "usage: v9 reach <ROOT> <DB> [--check] | v9 serve [PROGRAM] <DB> --port N";

/// Rounds per recursive stratum per step under `serve` and `reach`.
const SERVE_MAX_ROUNDS: u32 = 1 << 20;

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
            max_rounds: SERVE_MAX_ROUNDS,
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

/// The ryi service shared with `run`: it seeds `answered` from the store, asks again on a rerun,
/// reads `errors`, and times every `call`.
struct Shared {
    ryi: Rc<RefCell<Ryi>>,
    called: Rc<RefCell<Duration>>,
}

impl IService for Shared {
    fn service(&self) -> &str {
        "ryi"
    }

    fn cadence(&self) -> Cadence {
        Cadence::Once
    }

    fn call(&mut self, requests: &[Change], dict: &Dict) {
        let started = Instant::now();
        self.ryi.borrow_mut().call(requests, dict);
        *self.called.borrow_mut() += started.elapsed();
    }

    fn poll(&mut self, dict: &mut Dict, out: &mut Vec<Change>) {
        self.ryi.borrow_mut().poll(dict, out);
    }
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
/// engine and no request change to route, so its ryi call happens here, before the commit polls.
fn run(root: &str, db_path: PathBuf, check: bool) -> Result<(), String> {
    let ryi = Rc::new(RefCell::new(Ryi::new(_1_effects::v9_ryi())));
    let called = Rc::new(RefCell::new(Duration::ZERO));
    let services: Vec<Box<dyn IService>> = vec![Box::new(Shared {
        ryi: Rc::clone(&ryi),
        called: Rc::clone(&called),
    })];
    let bounds = Bounds {
        max_rounds: SERVE_MAX_ROUNDS,
    };

    let started = Instant::now();
    let mut core = build(Recipe::default(), reach(), &db_path, services, bounds)
        .map_err(|diags| format!("{diags:?}"))?;
    let open = started.elapsed();

    let root_id = id_of(root.as_bytes());
    let stored_roots: Vec<u64> = ints(&core, "SELECT root FROM root")?
        .into_iter()
        .map(|row| row[0])
        .collect();
    let mut answered: HashMap<u64, BTreeSet<Row>> = HashMap::new();
    for row in ints(&core, "SELECT root, caller, callee FROM ryi_edge")? {
        answered.entry(row[0]).or_default().insert(row);
    }
    ryi.borrow_mut().answered = answered;
    let dict_before = count(&core, "dict")?;

    let started = Instant::now();
    if stored_roots.contains(&root_id) {
        let ask = Change {
            rel: core.ids["ryi_ask"],
            row: vec![root_id],
            diff: 1,
        };
        Shared {
            ryi: Rc::clone(&ryi),
            called: Rc::clone(&called),
        }
        .call(&[ask], &core.dict);
    }
    let mut changes = vec![("root".to_string(), vec![Cell::Text(root.to_string())], 1)];
    for other in stored_roots.iter().filter(|id| **id != root_id) {
        let text = String::from_utf8_lossy(&core.dict.bytes[other]).into_owned();
        changes.push(("root".to_string(), vec![Cell::Text(text)], -1));
    }
    let (_, changed) = core.commit(changes).map_err(|diag| format!("{diag:?}"))?;
    let commit = started.elapsed();
    if let Some(error) = ryi.borrow().errors.first() {
        return Err(format!("ryi: {error}"));
    }
    let ryi_time = *called.borrow();

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
        "ms open {} ryi {} step+write {}",
        open.as_millis(),
        ryi_time.as_millis(),
        commit.saturating_sub(ryi_time).as_millis(),
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
