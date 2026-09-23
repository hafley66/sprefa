//! Replay a captured SQLite evaluator view without invoking the compiler or evaluator.
//! The arena supplies the same dl_* scalar functions used by the captured SQL.

use dl8::_6_eval::{functions, Universe};
use dl8::_9_runtime::{open, IRowStore, SqliteRowStore};
use rusqlite::{types::Value, Connection};
use serde_json::json;
use std::{
    env, fs,
    path::Path,
    sync::{Arc, Mutex},
    time::Instant,
};

fn file_bytes(path: &Path) -> u64 {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

fn sample(
    connection: &Connection,
    database: &Path,
    phase: &str,
    elapsed_ms: f64,
) -> rusqlite::Result<()> {
    hafley_observe::sqlite_memory::record_memory(connection, phase);
    let sqlite = hafley_observe::sqlite_memory::sample(connection);
    let process = hafley_observe::process_sample();
    let main_pages: i64 = connection.query_row("PRAGMA main.page_count", [], |row| row.get(0))?;
    let temp_pages: i64 = connection.query_row("PRAGMA temp.page_count", [], |row| row.get(0))?;
    let page_size: i64 = connection.query_row("PRAGMA page_size", [], |row| row.get(0))?;
    let database_name = database.to_string_lossy();
    println!(
        "{}",
        json!({
            "phase": phase,
            "elapsed_ms": elapsed_ms,
            "process_peak_rss_bytes": process.peak_rss_bytes,
            "sqlite_allocator_current_bytes": sqlite.map(|memory| memory.allocator_current_bytes),
            "sqlite_allocator_peak_bytes": sqlite.map(|memory| memory.allocator_peak_bytes),
            "sqlite_connection_cache_bytes": sqlite.map(|memory| memory.connection_cache_bytes),
            "sqlite_connection_schema_bytes": sqlite.map(|memory| memory.connection_schema_bytes),
            "sqlite_connection_statement_bytes": sqlite.map(|memory| memory.connection_statement_bytes),
            "main_pages": main_pages,
            "temp_pages": temp_pages,
            "page_size": page_size,
            "database_bytes": file_bytes(database),
            "wal_bytes": file_bytes(Path::new(&format!("{database_name}-wal"))),
            "shm_bytes": file_bytes(Path::new(&format!("{database_name}-shm"))),
        })
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let [source, arena_file, program_name, view_file, rounds_file, database_file, output_file] =
        arguments.as_slice()
    else {
        return Err("usage: 0_c2_sqlite_replay SOURCE_DB ARENA_DB PROGRAM_NAME VIEW_SQL ROUNDS_JSON WORK_DB OUTPUT_ROWS".into());
    };
    let database = Path::new(database_file);
    fs::copy(source, database)?;

    let config = hafley_observe::Config::from_env("sqlite-ivm-c2-replay", "0", "warn", false)?;
    hafley_observe::init(config)?;
    let mut store = SqliteRowStore::at(Path::new(arena_file))?;
    store.open(program_name)?;
    let mut universe = Universe::new();
    store.load_arena(&mut universe)?;

    let connection = open(database)?;
    functions::register(&connection, Arc::new(Mutex::new(universe)))?;
    sample(&connection, database, "source_ready", 0.0)?;

    let view = fs::read_to_string(view_file)?;
    let start = Instant::now();
    connection.execute_batch(&view)?;
    sample(
        &connection,
        database,
        "view_installed",
        start.elapsed().as_secs_f64() * 1000.0,
    )?;

    let rounds: Vec<String> = serde_json::from_slice(&fs::read(rounds_file)?)?;
    let start = Instant::now();
    let mut rounds_run = 0;
    for _ in 0..64 {
        rounds_run += 1;
        let mut changed = 0;
        for statement in &rounds {
            changed += connection.execute(statement, [])?;
        }
        if changed == 0 {
            break;
        }
    }
    sample(
        &connection,
        database,
        "rounds_complete",
        start.elapsed().as_secs_f64() * 1000.0,
    )?;

    let mut statement = connection.prepare("SELECT * FROM program")?;
    let width = statement.column_count();
    let mut rows = statement
        .query_map([], |row| {
            (0..width)
                .map(|index| row.get::<_, Value>(index))
                .collect::<rusqlite::Result<Vec<_>>>()
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.sort_by_key(|row| format!("{row:?}"));
    fs::write(output_file, format!("{rows:?}\n"))?;
    println!(
        "{}",
        json!({"phase":"output", "rows":rows.len(), "rounds_run":rounds_run, "output_file":output_file})
    );
    Ok(())
}
