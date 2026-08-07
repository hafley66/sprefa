// Prices the emitted engine's key choice: the same rows inserted into a
// 4-column TEXT WITHOUT ROWID table and into its interned INTEGER twin.

use intern_bench::intern::Interner;
use intern_bench::keys::{node_name, node_path};
use intern_bench::textinput::{parse_edge, parse_header};
use rusqlite::{params, Connection};
use std::time::Instant;

const PRAGMAS: &str = "PRAGMA page_size=16384; PRAGMA temp_store=MEMORY;";

const TEXT_DDL: &str = "CREATE TABLE \"flow_edge\" (\"from_path\" TEXT NOT NULL, \"from_name\" TEXT NOT NULL, \"to_path\" TEXT NOT NULL, \"to_name\" TEXT NOT NULL, \"__refcount\" INTEGER NOT NULL DEFAULT 1, PRIMARY KEY (\"from_path\", \"from_name\", \"to_path\", \"to_name\")) WITHOUT ROWID";

const INTEGER_DDL: &str = "CREATE TABLE \"flow_edge\" (\"from_path\" INTEGER NOT NULL, \"from_name\" INTEGER NOT NULL, \"to_path\" INTEGER NOT NULL, \"to_name\" INTEGER NOT NULL, \"__refcount\" INTEGER NOT NULL DEFAULT 1, PRIMARY KEY (\"from_path\", \"from_name\", \"to_path\", \"to_name\")) WITHOUT ROWID";

const INSERT_SQL: &str = "INSERT OR IGNORE INTO \"flow_edge\" (\"from_path\", \"from_name\", \"to_path\", \"to_name\", \"__refcount\") VALUES (?, ?, ?, ?, 1)";

struct TextRows {
    columns: Vec<[String; 4]>,
}

struct IntegerRows {
    columns: Vec<[i64; 4]>,
}

fn read_text_rows(path: &str) -> TextRows {
    let contents = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("cannot read {path}: {error}"));
    let mut lines = contents.lines();
    let header = lines.next().expect("input file is empty");
    parse_header(header).expect("header");
    let mut columns = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let edge = parse_edge(line).expect("edge line");
        columns.push([
            edge.from_path.to_string(),
            edge.from_name.to_string(),
            edge.to_path.to_string(),
            edge.to_name.to_string(),
        ]);
    }
    TextRows { columns }
}

// A distinct-per-row chain over the same key generator, used to read the insert
// rate at volumes where the WITHOUT ROWID btree is deeper than the edge set.
fn synth_text_rows(count: u32) -> TextRows {
    let mut columns = Vec::with_capacity(count as usize);
    for node in 0..count {
        columns.push([
            node_path(node),
            node_name(node),
            node_path(node + 1),
            node_name(node + 1),
        ]);
    }
    TextRows { columns }
}

fn intern_rows(rows: &TextRows) -> IntegerRows {
    let mut interner = Interner::default();
    let columns = rows
        .columns
        .iter()
        .map(|row| {
            [
                interner.intern(&row[0]) as i64,
                interner.intern(&row[1]) as i64,
                interner.intern(&row[2]) as i64,
                interner.intern(&row[3]) as i64,
            ]
        })
        .collect();
    IntegerRows { columns }
}

fn open_fresh(ddl: &str) -> Connection {
    let connection = Connection::open_in_memory().expect("open in-memory database");
    connection.execute_batch(PRAGMAS).expect("pragmas");
    connection.execute_batch(ddl).expect("ddl");
    connection
}

fn insert_text(rows: &TextRows) -> (u128, i64) {
    let connection = open_fresh(TEXT_DDL);
    let clock = Instant::now();
    connection.execute_batch("BEGIN").expect("begin");
    {
        let mut statement = connection.prepare(INSERT_SQL).expect("prepare");
        for row in &rows.columns {
            statement
                .execute(params![row[0], row[1], row[2], row[3]])
                .expect("insert");
        }
    }
    connection.execute_batch("COMMIT").expect("commit");
    let elapsed = clock.elapsed().as_micros();
    let stored: i64 = connection
        .query_row("SELECT COUNT(*) FROM \"flow_edge\"", [], |row| row.get(0))
        .expect("count");
    (elapsed, stored)
}

fn insert_integer(rows: &IntegerRows) -> (u128, i64) {
    let connection = open_fresh(INTEGER_DDL);
    let clock = Instant::now();
    connection.execute_batch("BEGIN").expect("begin");
    {
        let mut statement = connection.prepare(INSERT_SQL).expect("prepare");
        for row in &rows.columns {
            statement
                .execute(params![row[0], row[1], row[2], row[3]])
                .expect("insert");
        }
    }
    connection.execute_batch("COMMIT").expect("commit");
    let elapsed = clock.elapsed().as_micros();
    let stored: i64 = connection
        .query_row("SELECT COUNT(*) FROM \"flow_edge\"", [], |row| row.get(0))
        .expect("count");
    (elapsed, stored)
}

fn report(source: &str, variant: &str, rows: usize, best_us: u128, stored: i64) {
    let rows_per_sec = if best_us == 0 {
        0
    } else {
        (rows as u128 * 1_000_000 / best_us) as u64
    };
    println!(
        "{{\"event\":\"insert\",\"source\":\"{source}\",\"variant\":\"{variant}\",\"rows\":{rows},\"stored\":{stored},\"us\":{best_us},\"ms\":{},\"rows_per_sec\":{rows_per_sec}}}",
        best_us / 1000
    );
}

fn race(source: &str, rows: &TextRows, runs: u32) {
    let integers = intern_rows(rows);
    let mut best_text = u128::MAX;
    let mut best_integer = u128::MAX;
    let mut stored_text = 0i64;
    let mut stored_integer = 0i64;
    for _ in 0..runs {
        let (elapsed, stored) = insert_text(rows);
        if elapsed < best_text {
            best_text = elapsed;
            stored_text = stored;
        }
        let (elapsed, stored) = insert_integer(&integers);
        if elapsed < best_integer {
            best_integer = elapsed;
            stored_integer = stored;
        }
    }
    report(source, "text", rows.columns.len(), best_text, stored_text);
    report(
        source,
        "integer",
        integers.columns.len(),
        best_integer,
        stored_integer,
    );
}

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    let mut input_path: Option<String> = None;
    let mut synth: Vec<u32> = Vec::new();
    let mut runs: u32 = 5;

    let mut index = 1;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--input" => {
                index += 1;
                input_path = arguments.get(index).cloned();
            }
            "--synth" => {
                index += 1;
                synth = arguments[index]
                    .split(',')
                    .map(|token| token.trim().parse().expect("synth count not an int"))
                    .collect();
            }
            "--runs" => {
                index += 1;
                runs = arguments[index].parse().expect("runs not an int");
            }
            "--help" | "-h" => {
                eprintln!("sqlite_keys [--input <path.tin>] [--synth N[,N...]] [--runs K]");
                std::process::exit(0);
            }
            other => {
                eprintln!("sqlite_keys: unknown argument '{other}'");
                std::process::exit(1);
            }
        }
        index += 1;
    }

    if let Some(path) = &input_path {
        let rows = read_text_rows(path);
        let label = std::path::Path::new(path)
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_else(|| "edges".to_string());
        race(&label, &rows, runs);
    }
    for count in synth {
        let rows = synth_text_rows(count);
        race(&format!("synth_{count}"), &rows, runs);
    }
}
