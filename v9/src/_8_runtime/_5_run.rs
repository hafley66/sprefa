//! The author's entry: a `Program` from any Rust file, its services, a db, a port. Recipe from
//! `V9_SYSTEM`, bounds `MAX_ROUNDS`; `v9 serve` and `examples/` take this path.

use crate::_1_effects;
use crate::_2_ir::Program;
use crate::_4_recipe::Recipe;
use crate::_5_ends::{Bounds, Cell, Diag, IService};
use crate::_6_back::_1_dd::MAX_ROUNDS;
use crate::_7_store::_0_sql::quote;
use crate::_8_runtime::_3_core::{build, Core};
use crate::_8_runtime::_4_handle::start;
use crate::_9_wire::http_auto::router;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

pub const BOUNDS: Bounds = Bounds {
    max_rounds: MAX_ROUNDS,
};

/// `V9_SYSTEM` parsed; unset is the default recipe.
pub fn system() -> Result<Recipe, Diag> {
    Recipe::parse(&_1_effects::v9_system().unwrap_or_default())
}

/// `system()` + `build`: a Core in this thread, for scripts and tests.
pub fn run(
    program: Program,
    services: Vec<Box<dyn IService>>,
    db: &Path,
) -> Result<Core, Vec<Diag>> {
    let recipe = system().map_err(|diag| vec![diag])?;
    build(recipe, program, db, services, BOUNDS)
}

/// Builds on the Core thread (a service need not be Send), prints `serving NAME on http://ADDR
/// recipe CANONICAL`, serves `127.0.0.1:port` until the process ends.
pub fn serve(
    name: &str,
    program: Program,
    services: impl FnOnce() -> Vec<Box<dyn IService>> + Send + 'static,
    db: PathBuf,
    port: u16,
) -> Result<(), String> {
    let recipe = system().map_err(|diag| format!("V9_SYSTEM: {diag:?}"))?;
    let handle = start(move || build(recipe, program, &db, services(), BOUNDS))
        .map_err(|diags| format!("{diags:?}"))?;
    let canonical = handle.recipe.canonical.clone();
    let listener = _1_effects::bind(port).map_err(|error| format!("bind: {error}"))?;
    let addr = listener.local_addr().map_err(|error| error.to_string())?;
    println!("serving {name} on http://{addr} recipe {canonical}");
    _1_effects::serve_http(listener, router(Arc::new(handle))).map_err(|error| error.to_string())
}

/// Every `<rel>_text` view, rel order then row order, one `rel(cell, ..)` line per row.
pub fn views(core: &Core) -> Result<Vec<String>, Diag> {
    let mut lines = Vec::new();
    for rel in &core.plan.rels {
        let order: Vec<String> = (1..=rel.cols.len()).map(|n| n.to_string()).collect();
        let order = match order.is_empty() {
            true => String::new(),
            false => format!(" ORDER BY {}", order.join(", ")),
        };
        let table = core.query(&format!(
            "SELECT * FROM {}{order}",
            quote(&format!("{}_text", rel.name))
        ))?;
        for row in table.rows {
            let cells: Vec<String> = row
                .iter()
                .map(|cell| match cell {
                    Cell::Text(text) => text.clone(),
                    Cell::Int(int) => int.to_string(),
                    other => format!("{other:?}"),
                })
                .collect();
            lines.push(format!("{}({})", rel.name, cells.join(", ")));
        }
    }
    Ok(lines)
}

const USAGE: &str = "usage: <DB> [--port N]";

/// `<DB> [--port N]` from the process args. With `--port`: `serve`. Without: `run`, print
/// `views`, exit. An example's `main` is this one call.
pub fn main(
    name: &str,
    program: fn() -> Program,
    services: fn() -> Vec<Box<dyn IService>>,
) -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [db] => run(program(), services(), Path::new(db))
            .map_err(|diags| format!("{diags:?}"))
            .and_then(|core| views(&core).map_err(|diag| format!("{diag:?}")))
            .map(|lines| lines.iter().for_each(|line| println!("{line}"))),
        [db, flag, port] if flag == "--port" => port
            .parse()
            .map_err(|error| format!("--port: {error}"))
            .and_then(|port| serve(name, program(), services, PathBuf::from(db), port)),
        _ => Err(USAGE.to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            println!("{name}: {error}");
            ExitCode::from(1)
        }
    }
}
