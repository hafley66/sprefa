use rusqlite::{params, params_from_iter, types::Value as SqlValue, Connection, ToSql};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, error, fs,
    io::{BufRead, BufReader},
    process,
};

use dd_runner::{kernel, Plan, Row, Rule, SignedRow};

/// One `edgestmt/9` arm as the JSON twin carries it. `project_sql` binds the
/// trigger row positionally; `write_sql` binds the projected head row.
#[derive(Deserialize)]
struct EdgeArm {
    head: String,
    trigger: String,
    trigger_kind: String,
    head_columns: Vec<String>,
    project_sql: String,
    write_sql: String,
}

/// A row presented to the edge arms as a firing, in STORAGE values.
#[derive(Clone, PartialEq)]
struct Occurrence {
    rel: String,
    values: Vec<Value>,
}

type Rows = BTreeMap<String, Vec<Value>>;
type StorageRows = BTreeMap<String, Vec<Vec<Value>>>;
type Outcome<T> = Result<T, Box<dyn error::Error>>;

/// engine.pl:92 `drain_cap(100)`.
const DRAIN_CAP: usize = 100;
/// Matches kernel.rs's fixed-point bound.
const LEVEL_ROUND_CAP: usize = 10_000;

/// A tick phase whose plan term the dd emitter does not put in the JSON twin,
/// with the term it wants. A named gap, never a silent skip.
const PHASE_GAPS: &[(&str, &str)] = &[
    (
        "index_delta",
        "deltastmt/5 DeltaTable; 6_emit_dd_plan.pl:86 keeps SelectAllSql only",
    ),
    (
        "consolidate",
        "rel frontier + next_frontier table names; rels[] carries name/columns/select_all only",
    ),
    (
        "retain",
        "retentionstmt/3; 6_emit_dd_plan.pl:609 filters LevelStatements to levelstmt/7",
    ),
];

enum Arm {
    Sqlite,
    Kernel,
    /// The differential-dataflow crate arm: a reserved slot, built in a
    /// separate arc, not present in this binary.
    RustDd,
}

fn main() {
    let arguments = env::args().collect::<Vec<_>>();
    if arguments.get(1).map(String::as_str) == Some("--shootout") {
        let graph_case = arguments.get(2).map(String::as_str).unwrap_or("");
        let n = arguments
            .get(3)
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        let result = kernel::shootout(graph_case, n).unwrap_or_else(|error| fail(error));
        println!("{result}");
        return;
    }
    if arguments.get(1).map(String::as_str) == Some("--shootout-generated") {
        let graph_case = arguments.get(2).map(String::as_str).unwrap_or("");
        let n = arguments
            .get(3)
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        let result = dd_runner::generated_reachability::shootout(graph_case, n)
            .unwrap_or_else(|error| fail(error));
        println!("{result}");
        return;
    }
    if arguments.get(1).map(String::as_str) == Some("--shootout-sqlite") {
        let graph_case = arguments.get(2).map(String::as_str).unwrap_or("");
        let n = arguments
            .get(3)
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        let result = sqlite_shootout(graph_case, n).unwrap_or_else(|error| fail(error));
        println!("{result}");
        return;
    }
    let mut path = None;
    let mut arm = Arm::Sqlite;
    let mut phases_only = false;
    let mut watch_stdin = false;
    let mut sqlite_state = None;
    let mut index = 1;
    while index < arguments.len() {
        let argument = &arguments[index];
        match argument.as_str() {
            "--dd-diet-rust-sqlite" => arm = Arm::Sqlite,
            "--dd-diet-rust-rust" => arm = Arm::Kernel,
            "--dd-rust-dd" => arm = Arm::RustDd,
            "--phases" => phases_only = true,
            "--watch-stdin" => watch_stdin = true,
            "--sqlite-state" => {
                index += 1;
                sqlite_state = arguments.get(index).cloned();
                if sqlite_state.is_none() {
                    fail("--sqlite-state requires a path");
                }
            }
            other => path = Some(other.to_owned()),
        }
        index += 1;
    }
    let Some(path) = path else {
        println!("usage: dd-runner PLAN.json [--dd-diet-rust-sqlite|--dd-diet-rust-rust|--dd-rust-dd] [--sqlite-state PATH] [--phases] [--watch-stdin]");
        process::exit(2);
    };
    if matches!(arm, Arm::RustDd) {
        println!("dd-runner: arm dd-rust-dd is not built yet (the differential-dataflow crate arc is a separate lane)");
        process::exit(2);
    }
    let input = fs::read_to_string(path).unwrap_or_else(|error| fail(error));
    let plan: Plan = serde_json::from_str(&input).unwrap_or_else(|error| fail(error));
    if phases_only {
        for line in phase_report(&plan) {
            println!("{line}");
        }
        return;
    }
    if watch_stdin {
        match arm {
            Arm::Sqlite => {
                let conn = open_sqlite(sqlite_state.as_deref()).unwrap_or_else(|error| fail(error));
                watch_sqlite(&conn, &plan).unwrap_or_else(|error| fail(error));
            }
            Arm::Kernel => {
                let operators = kernel_operators(&plan).unwrap_or_else(|error| fail(error));
                watch_kernel(&plan, operators).unwrap_or_else(|error| fail(error));
            }
            Arm::RustDd => unreachable!("guarded before dispatch"),
        }
        return;
    }
    match arm {
        Arm::Sqlite => {
            let conn = open_sqlite(sqlite_state.as_deref()).unwrap_or_else(|error| fail(error));
            run(&conn, &plan).unwrap_or_else(|error| fail(error));
        }
        Arm::Kernel => {
            let operators = kernel_operators(&plan).unwrap_or_else(|error| fail(error));
            kernel::run(&plan.rels, &plan.initial, &plan.schedule, &operators)
                .unwrap_or_else(|error| fail(error));
        }
        Arm::RustDd => unreachable!("guarded before dispatch"),
    }
}

fn open_sqlite(path: Option<&str>) -> rusqlite::Result<Connection> {
    match path {
        Some(path) => Connection::open(path),
        None => Connection::open_in_memory(),
    }
}

#[derive(Deserialize)]
struct WatchSource {
    content: String,
}

#[derive(Deserialize)]
struct WatchRecord {
    record: String,
    #[serde(default)]
    generation: usize,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    sign: i8,
    #[serde(default)]
    relation: String,
    #[serde(default)]
    args: Vec<Value>,
    #[serde(default)]
    source: Option<WatchSource>,
}

fn watch_kernel(plan: &Plan, operators: Vec<kernel::Operator>) -> Result<(), String> {
    let mut runtime = kernel::Runtime::open(&plan.rels, &plan.initial, operators)?;
    let mut arrivals = Vec::new();
    let mut generation = 0usize;
    for line in BufReader::new(std::io::stdin().lock()).lines() {
        let line = line.map_err(|error| error.to_string())?;
        let record: WatchRecord =
            serde_json::from_str(&line).map_err(|error| format!("watch stream: {error}"))?;
        match record.record.as_str() {
            "batch_start" => {
                generation = record.generation;
                arrivals.clear();
                if record.mode == "snapshot" {
                    runtime.reset()?;
                }
            }
            "change" if runtime.accepts(&record.relation) => {
                let content = record
                    .source
                    .as_ref()
                    .map(|source| source.content.as_str())
                    .unwrap_or("");
                arrivals.push(SignedRow {
                    sign: record.sign,
                    row: Row {
                        rel: record.relation,
                        values: record
                            .args
                            .iter()
                            .map(|argument| tsi_value(argument, content))
                            .collect(),
                    },
                });
            }
            "change" => {}
            "batch_end" => println!("{}", runtime.tick(generation, &arrivals)?),
            other => return Err(format!("watch stream record {other} is unsupported")),
        }
    }
    Ok(())
}

fn watch_sqlite(conn: &Connection, plan: &Plan) -> Outcome<()> {
    setup_sqlite(conn, plan)?;
    if !edge_arms(plan)?.is_empty() {
        return Err("SQLite watch currently accepts level operators only".into());
    }
    let mut arrivals = Vec::new();
    let mut generation = 0usize;
    let mut reset = false;
    for line in BufReader::new(std::io::stdin().lock()).lines() {
        let line = line?;
        let record: WatchRecord = serde_json::from_str(&line)?;
        match record.record.as_str() {
            "batch_start" => {
                generation = record.generation;
                arrivals.clear();
                reset = record.mode == "snapshot";
            }
            "change" if plan.rels.iter().any(|rel| rel.name == record.relation) => {
                let content = record
                    .source
                    .as_ref()
                    .map(|source| source.content.as_str())
                    .unwrap_or("");
                arrivals.push(SignedRow {
                    sign: record.sign,
                    row: Row {
                        rel: record.relation,
                        values: record
                            .args
                            .iter()
                            .map(|argument| tsi_value(argument, content))
                            .collect(),
                    },
                });
            }
            "change" => {}
            "batch_end" => {
                sqlite_watch_tick(conn, plan, generation, &arrivals, reset)?;
                reset = false;
            }
            other => return Err(format!("watch stream record {other} is unsupported").into()),
        }
    }
    Ok(())
}

fn reset_sqlite(conn: &Connection, plan: &Plan) -> Outcome<()> {
    for rel in &plan.rels {
        let table = rel.name.split('/').next().expect("relation name");
        let table = table.replace('"', "\"\"");
        conn.execute_batch(&format!("DELETE FROM \"{table}\""))?;
    }
    for row in &plan.initial {
        write_row(conn, plan, row, 1)?;
    }
    close_levels(conn, plan)?;
    Ok(())
}

fn sqlite_watch_tick(
    conn: &Connection,
    plan: &Plan,
    generation: usize,
    arrivals: &[SignedRow],
    reset: bool,
) -> Outcome<()> {
    let transaction = conn.unchecked_transaction()?;
    if reset {
        reset_sqlite(&transaction, plan)?;
    }
    let before = snapshot(&transaction, plan)?;
    absorb_arrivals(&transaction, plan, arrivals)?;
    close_levels(&transaction, plan)?;
    let after = snapshot(&transaction, plan)?;
    let output = tick_json(generation, &before, &after);
    transaction.commit()?;
    println!("{output}");
    Ok(())
}

fn tsi_value(argument: &Value, content: &str) -> Value {
    let Some(object) = argument.as_object() else {
        return argument.clone();
    };
    if let Some(id) = object.get("id") {
        return json!({"tsi":{"content":content,"id":id}});
    }
    for tag in ["text", "atom", "int"] {
        if let Some(value) = object.get(tag) {
            return value.clone();
        }
    }
    if let Some(span) = object.get("span") {
        return json!({"span":span});
    }
    argument.clone()
}

#[cfg(test)]
mod watch_tests {
    use super::*;

    #[test]
    fn tsi_ids_are_scoped_by_source_content() {
        assert_eq!(
            tsi_value(&json!({"id":7}), "blake3:abc"),
            json!({"tsi":{"content":"blake3:abc","id":7}})
        );
        assert_eq!(tsi_value(&json!({"text":"Render"}), ""), json!("Render"));
        assert_eq!(
            tsi_value(&json!({"span":["blake3:abc", 3, 9]}), ""),
            json!({"span":["blake3:abc", 3, 9]})
        );
    }
}

fn fail(error: impl std::fmt::Display) -> ! {
    println!("dd-runner: {error}");
    process::exit(1);
}

fn kernel_operators(plan: &Plan) -> Result<Vec<kernel::Operator>, serde_json::Error> {
    serde_json::from_value(Value::Array(plan.operators.clone()))
}

/// `operator_payload/3` scopes edgestmt lookup to the HEAD, so every map
/// operator of a head reports that head's FIRST arm; the repeats are dropped.
fn edge_arms(plan: &Plan) -> Result<Vec<EdgeArm>, serde_json::Error> {
    let mut arms: Vec<EdgeArm> = Vec::new();
    for operator in &plan.operators {
        if operator.get("classification") != Some(&json!("edge"))
            || operator.get("kind") != Some(&json!("map"))
        {
            continue;
        }
        let arm: EdgeArm = serde_json::from_value(operator.clone())?;
        if !arms.iter().any(|kept| {
            kept.trigger == arm.trigger
                && kept.project_sql == arm.project_sql
                && kept.write_sql == arm.write_sql
        }) {
            arms.push(arm);
        }
    }
    Ok(arms)
}

fn phase_report(plan: &Plan) -> Vec<String> {
    let arms = edge_arms(plan).unwrap_or_default();
    let mut lines = Vec::new();
    for phase in &plan.tick_order {
        let (status, detail) = match PHASE_GAPS.iter().find(|(name, _)| name == phase) {
            Some((_, wanted)) => ("no-term", (*wanted).to_owned()),
            None => match phase.as_str() {
                "edge_arrivals" | "edge_departures" => {
                    let kind = if phase == "edge_arrivals" {
                        "arrival"
                    } else {
                        "departure"
                    };
                    let count = arms.iter().filter(|arm| arm.trigger_kind == kind).count();
                    ("ran", format!("{count} {kind} arms"))
                }
                "level_before_edges" | "level_after_edges" | "iterate" => (
                    "ran",
                    format!("{} level bundles", level_bundles(plan).len()),
                ),
                _ => ("ran", String::new()),
            },
        };
        lines.push(format!("{phase}\t{status}\t{detail}"));
    }
    let unordered = arms
        .iter()
        .filter(|arm| arm.trigger_kind == "ordered_arrival")
        .count();
    if unordered > 0 {
        lines.push(format!(
            "ordered_arrival\tno-pipeline\t{unordered} arms need the ordered tick (pre/1 snapshot, seq/1 order)"
        ));
    }
    lines
}

/// `rules[]` repeats a head's bundle once per clause: `operator_payload/3`
/// hands every map operator of a head the same `levelstmt/7` list.
fn level_bundles(plan: &Plan) -> Vec<&Rule> {
    let mut seen: Vec<&Rule> = Vec::new();
    for rule in &plan.rules {
        if !seen.iter().any(|kept| kept.head == rule.head) {
            seen.push(rule);
        }
    }
    seen
}

fn run(conn: &Connection, plan: &Plan) -> Outcome<()> {
    setup_sqlite(conn, plan)?;
    run_sqlite_ticks(conn, plan, true)
}

fn setup_sqlite(conn: &Connection, plan: &Plan) -> Outcome<()> {
    for ddl in &plan.ddl {
        conn.execute_batch(ddl)?;
    }
    for row in &plan.initial {
        write_row(conn, plan, row, 1)?;
    }
    close_levels(conn, plan)?;
    Ok(())
}

fn run_sqlite_ticks(conn: &Connection, plan: &Plan, emit_ticks: bool) -> Outcome<()> {
    let arms = edge_arms(plan)?;
    let mut text_before = snapshot(conn, plan)?;
    let mut level_before = storage_snapshot(conn, plan, &level_heads(plan))?;
    let mut carry: Vec<Occurrence> = Vec::new();
    let mut drains = 0usize;
    let mut tick = 0usize;
    let mut index = 0usize;
    let no_arrivals: Vec<SignedRow> = Vec::new();
    loop {
        let outside = match plan.schedule.get(index) {
            Some(arrivals) => arrivals,
            None if carry.is_empty() => break,
            None => {
                if drains >= DRAIN_CAP {
                    return Err(format!("drain_overflow({DRAIN_CAP})").into());
                }
                drains += 1;
                &no_arrivals
            }
        };
        index += 1;
        tick += 1;
        let storage_before = storage_snapshot(conn, plan, &all_rels(plan))?;
        // engine.pl:472 orders carry ahead of arrivals ahead of level rows.
        let mut occurrences = carry.clone();
        let mut level_mid = level_before.clone();
        let mut text_after = text_before.clone();
        let mut written: Vec<Occurrence> = Vec::new();
        for phase in &plan.tick_order {
            match phase.as_str() {
                "absorb_arrivals" => {
                    occurrences.extend(absorb_arrivals(conn, plan, outside)?);
                }
                "index_delta" => (),
                "level_before_edges" => {
                    close_levels(conn, plan)?;
                    level_mid = storage_snapshot(conn, plan, &level_heads(plan))?;
                    occurrences.extend(new_rows(&level_before, &level_mid));
                }
                "edge_arrivals" => {
                    written.extend(fire_edges(conn, &arms, &occurrences, "arrival")?);
                }
                "edge_departures" => {
                    written.extend(fire_edges(conn, &arms, &occurrences, "departure")?);
                }
                "level_after_edges" => {
                    if !arms.is_empty() {
                        close_levels(conn, plan)?;
                    }
                }
                "iterate" => {
                    close_levels(conn, plan)?;
                }
                "consolidate" => (),
                "retain" => (),
                "boundary" => {
                    text_after = snapshot(conn, plan)?;
                }
                "carry" => {
                    let level_after = storage_snapshot(conn, plan, &level_heads(plan))?;
                    let storage_after = storage_snapshot(conn, plan, &all_rels(plan))?;
                    let mut candidates = written.clone();
                    candidates.extend(new_rows(&level_mid, &level_after));
                    carry = carry_out(&arms, &candidates, &storage_before, &storage_after);
                    level_before = level_after;
                }
                "drain" => (),
                other => return Err(format!("unknown tick phase: {other}").into()),
            }
        }
        let output = tick_json(tick, &text_before, &text_after);
        if emit_ticks {
            println!("{output}");
        }
        text_before = text_after;
    }
    Ok(())
}

fn sqlite_shootout(graph_case: &str, n: usize) -> Outcome<Value> {
    if n == 0 {
        return Err("shootout N must be greater than zero".into());
    }
    let edge_count = match graph_case {
        "chain" => n - 1,
        "ring" => n,
        other => return Err(format!("unknown shootout case {other}").into()),
    };
    let mut plan = Plan {
        ddl: dd_runner::generated_reachability::ddl(),
        rels: dd_runner::generated_reachability::relations(),
        rules: dd_runner::generated_reachability::rules(),
        initial: dd_runner::generated_reachability::initial(),
        schedule: Vec::new(),
        tick_order: dd_runner::generated_reachability::tick_order(),
        operators: Vec::new(),
    };
    let setup_started = std::time::Instant::now();
    let conn = Connection::open_in_memory()?;
    setup_sqlite(&conn, &plan)?;
    let setup_ms = setup_started.elapsed().as_secs_f64() * 1000.0;
    let mut arrivals = (0..n.saturating_sub(1))
        .map(|from| SignedRow {
            sign: 1,
            row: Row {
                rel: "edge".into(),
                values: vec![json!(from), json!(from + 1)],
            },
        })
        .collect::<Vec<_>>();
    if graph_case == "ring" {
        arrivals.push(SignedRow {
            sign: 1,
            row: Row {
                rel: "edge".into(),
                values: vec![json!(n - 1), json!(0)],
            },
        });
    }
    plan.schedule.push(arrivals);
    let closure_started = std::time::Instant::now();
    run_sqlite_ticks(&conn, &plan, false)?;
    let closure_ms = closure_started.elapsed().as_secs_f64() * 1000.0;
    let closure_count = conn.query_row("SELECT count(*) FROM \"path\"", [], |row| {
        row.get::<_, i64>(0)
    })?;
    Ok(json!({
        "runtime":"dbsp-sqlite",
        "version":rusqlite::version(),
        "case":graph_case,
        "n":n,
        "edge_count":edge_count,
        "closure_count":closure_count,
        "setup_ms":setup_ms,
        "closure_ms":closure_ms,
    }))
}

#[cfg(test)]
mod sqlite_shootout_tests {
    #[test]
    fn generated_sql_closes_chain_and_ring_exactly() {
        assert_eq!(
            super::sqlite_shootout("chain", 4).unwrap()["closure_count"],
            6
        );
        assert_eq!(
            super::sqlite_shootout("ring", 4).unwrap()["closure_count"],
            16
        );
    }
}

fn all_rels(plan: &Plan) -> Vec<String> {
    plan.rels.iter().map(|rel| rel.name.clone()).collect()
}

fn level_heads(plan: &Plan) -> Vec<String> {
    level_bundles(plan)
        .iter()
        .map(|rule| rule.head.clone())
        .collect()
}

fn absorb_arrivals(
    conn: &Connection,
    plan: &Plan,
    arrivals: &[SignedRow],
) -> Outcome<Vec<Occurrence>> {
    let mut occurrences = Vec::new();
    for arrival in arrivals {
        write_row(conn, plan, &arrival.row, arrival.sign)?;
        if arrival.sign > 0 {
            occurrences.push(Occurrence {
                rel: arrival.row.rel.clone(),
                values: storage_values(conn, &arrival.row.values)?
                    .iter()
                    .map(sql_to_json)
                    .collect(),
            });
        }
    }
    Ok(occurrences)
}

/// `delete` runs once, then rounds of `insert` only: a recursive head must grow
/// across rounds rather than be rebuilt to the same depth each round.
fn close_levels(conn: &Connection, plan: &Plan) -> Outcome<usize> {
    let bundles = level_bundles(plan);
    if bundles.is_empty() {
        return Ok(0);
    }
    for bundle in &bundles {
        conn.execute_batch(&bundle.delete)?;
    }
    let heads = level_heads(plan);
    let mut previous: Option<StorageRows> = None;
    for round in 1..=LEVEL_ROUND_CAP {
        for bundle in &bundles {
            for insert in &bundle.inserts {
                conn.execute_batch(insert)?;
            }
        }
        let current = storage_snapshot(conn, plan, &heads)?;
        if previous.as_ref() == Some(&current) {
            return Ok(round);
        }
        previous = Some(current);
    }
    Err(format!("level plane did not close in {LEVEL_ROUND_CAP} rounds").into())
}

fn fire_edges(
    conn: &Connection,
    arms: &[EdgeArm],
    occurrences: &[Occurrence],
    kind: &str,
) -> Outcome<Vec<Occurrence>> {
    let mut written = Vec::new();
    for occurrence in occurrences {
        for arm in arms {
            if arm.trigger_kind != kind || arm.trigger != occurrence.rel {
                continue;
            }
            for row in project(conn, arm, occurrence)? {
                let bindings: Vec<SqlValue> = row.iter().map(json_to_sql).collect();
                let arguments: Vec<&dyn ToSql> =
                    bindings.iter().map(|value| value as &dyn ToSql).collect();
                conn.execute(&arm.write_sql, params_from_iter(arguments))?;
                written.push(Occurrence {
                    rel: arm.head.clone(),
                    values: row,
                });
            }
        }
    }
    Ok(written)
}

fn project(conn: &Connection, arm: &EdgeArm, occurrence: &Occurrence) -> Outcome<Vec<Vec<Value>>> {
    let mut statement = conn.prepare(&arm.project_sql)?;
    let wanted = statement.parameter_count().min(occurrence.values.len());
    let bindings: Vec<SqlValue> = occurrence.values[..wanted]
        .iter()
        .map(json_to_sql)
        .collect();
    let arguments: Vec<&dyn ToSql> = bindings.iter().map(|value| value as &dyn ToSql).collect();
    let width = arm.head_columns.len();
    let rows = statement
        .query_map(params_from_iter(arguments), |row| {
            (0..width)
                .map(|column| row.get_ref(column).map(sql_ref_to_json))
                .collect()
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// engine.pl:481-494: a written or post-edge level row carries to T+1 only when
/// it is a boundary `+delta`; a `-delta` of a departure-listened rel carries.
fn carry_out(
    arms: &[EdgeArm],
    candidates: &[Occurrence],
    before: &StorageRows,
    after: &StorageRows,
) -> Vec<Occurrence> {
    let mut out: Vec<Occurrence> = Vec::new();
    for candidate in candidates {
        let added = new_rows_for(before, after, &candidate.rel)
            .iter()
            .any(|row| row == &candidate.values);
        if added && !out.contains(candidate) {
            out.push(candidate.clone());
        }
    }
    let departure_rels: BTreeSet<&str> = arms
        .iter()
        .filter(|arm| arm.trigger_kind == "departure")
        .map(|arm| arm.trigger.as_str())
        .collect();
    for rel in departure_rels {
        for row in new_rows_for(after, before, rel) {
            let occurrence = Occurrence {
                rel: rel.to_owned(),
                values: row,
            };
            if !out.contains(&occurrence) {
                out.push(occurrence);
            }
        }
    }
    out
}

fn new_rows(before: &StorageRows, after: &StorageRows) -> Vec<Occurrence> {
    let mut fresh = Vec::new();
    for (rel, rows) in after {
        let old = before.get(rel);
        for row in rows {
            if !old.is_some_and(|kept| kept.contains(row)) {
                fresh.push(Occurrence {
                    rel: rel.clone(),
                    values: row.clone(),
                });
            }
        }
    }
    fresh
}

fn new_rows_for(before: &StorageRows, after: &StorageRows, rel: &str) -> Vec<Vec<Value>> {
    let Some(rows) = after.get(rel) else {
        return Vec::new();
    };
    let old = before.get(rel);
    rows.iter()
        .filter(|row| !old.is_some_and(|kept| kept.contains(row)))
        .cloned()
        .collect()
}

fn write_row(conn: &Connection, plan: &Plan, row: &Row, sign: i8) -> rusqlite::Result<()> {
    let rel = plan
        .rels
        .iter()
        .find(|rel| rel.name == row.rel)
        .expect("row relation in plan");
    let table = row.rel.split('/').next().expect("relation name");
    let values: Vec<SqlValue> = storage_values(conn, &row.values)?;
    let placeholders = std::iter::repeat_n("?", values.len())
        .collect::<Vec<_>>()
        .join(", ");
    let columns = rel
        .columns
        .iter()
        .map(|column| format!("\"{column}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = if sign > 0 {
        format!("INSERT OR IGNORE INTO \"{table}\" ({columns}) VALUES ({placeholders})")
    } else {
        let where_clause = rel
            .columns
            .iter()
            .map(|column| format!("\"{column}\" = ?"))
            .collect::<Vec<_>>()
            .join(" AND ");
        format!("DELETE FROM \"{table}\" WHERE {where_clause}")
    };
    let bindings: Vec<&dyn ToSql> = values.iter().map(|value| value as &dyn ToSql).collect();
    conn.execute(&sql, params_from_iter(bindings))?;
    Ok(())
}

fn storage_values(conn: &Connection, values: &[Value]) -> rusqlite::Result<Vec<SqlValue>> {
    values
        .iter()
        .map(|value| storage_value(conn, value))
        .collect()
}

fn storage_value(conn: &Connection, value: &Value) -> rusqlite::Result<SqlValue> {
    match value {
        Value::String(text) => {
            conn.execute(
                "INSERT OR IGNORE INTO \"__str\" (\"content\") VALUES (?1)",
                params![text],
            )?;
            let id = conn.query_row(
                "SELECT \"__id\" FROM \"__str\" WHERE \"content\" = ?1",
                params![text],
                |row| row.get::<_, i64>(0),
            )?;
            Ok(SqlValue::Integer(id))
        }
        Value::Null => Ok(SqlValue::Null),
        Value::Bool(value) => Ok(SqlValue::Integer(i64::from(*value))),
        Value::Number(value) if value.is_i64() => Ok(SqlValue::Integer(value.as_i64().unwrap())),
        Value::Number(value) => Ok(SqlValue::Real(value.as_f64().expect("JSON number"))),
        Value::Array(_) | Value::Object(_) => Ok(SqlValue::Text(value.to_string())),
    }
}

fn snapshot(conn: &Connection, plan: &Plan) -> rusqlite::Result<Rows> {
    let mut all = Rows::new();
    for rel in &plan.rels {
        let mut statement = conn.prepare(&rel.select_all)?;
        let count = statement.column_count();
        let rows = statement
            .query_map([], |row| {
                let mut values = Vec::with_capacity(count);
                for column in 0..count {
                    values.push(sql_ref_to_json(row.get_ref(column)?));
                }
                Ok(Value::Array(values))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        all.insert(rel.name.clone(), sorted(rows));
    }
    Ok(all)
}

/// Storage-plane read: interned ids, not the `__txt_` view's decoded text. The
/// edge arms bind these, so a snapshot the carry set compares must match them.
fn storage_snapshot(
    conn: &Connection,
    plan: &Plan,
    wanted: &[String],
) -> rusqlite::Result<StorageRows> {
    let mut all = StorageRows::new();
    for rel in plan.rels.iter().filter(|rel| wanted.contains(&rel.name)) {
        let table = rel.name.split('/').next().expect("relation name");
        let columns = rel
            .columns
            .iter()
            .map(|column| format!("\"{column}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let mut statement = conn.prepare(&format!("SELECT {columns} FROM \"{table}\""))?;
        let width = rel.columns.len();
        let rows = statement
            .query_map([], |row| {
                (0..width)
                    .map(|column| row.get_ref(column).map(sql_ref_to_json))
                    .collect()
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        all.insert(rel.name.clone(), rows);
    }
    Ok(all)
}

fn sql_ref_to_json(value: rusqlite::types::ValueRef<'_>) -> Value {
    use rusqlite::types::ValueRef;
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(value) => json!(value),
        ValueRef::Real(value) => js_number(value),
        ValueRef::Text(value) => Value::String(String::from_utf8_lossy(value).into_owned()),
        ValueRef::Blob(value) => Value::String(String::from_utf8_lossy(value).into_owned()),
    }
}

/// An integral REAL loses its `.0` the way ECMAScript `Number::toString` and
/// 0_type_plane.pl:js_float_text/2 drop it; past 2^53 `as i64` saturates.
fn js_number(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() <= 9_007_199_254_740_992.0 {
        json!(value as i64)
    } else {
        json!(value)
    }
}

fn sql_to_json(value: &SqlValue) -> Value {
    match value {
        SqlValue::Null => Value::Null,
        SqlValue::Integer(number) => json!(number),
        SqlValue::Real(number) => js_number(*number),
        SqlValue::Text(text) => Value::String(text.clone()),
        SqlValue::Blob(bytes) => Value::String(String::from_utf8_lossy(bytes).into_owned()),
    }
}

fn json_to_sql(value: &Value) -> SqlValue {
    match value {
        Value::Null => SqlValue::Null,
        Value::Bool(flag) => SqlValue::Integer(i64::from(*flag)),
        Value::Number(number) if number.is_i64() => SqlValue::Integer(number.as_i64().unwrap()),
        Value::Number(number) => SqlValue::Real(number.as_f64().unwrap_or_default()),
        Value::String(text) => SqlValue::Text(text.clone()),
        other => SqlValue::Text(other.to_string()),
    }
}

fn sorted(mut rows: Vec<Value>) -> Vec<Value> {
    rows.sort_by_key(|row| serde_json::to_string(row).expect("JSON row"));
    rows
}

fn tick_json(tick: usize, before: &Rows, after: &Rows) -> String {
    let mut deltas = Vec::new();
    for name in before.keys().chain(after.keys()).collect::<BTreeSet<_>>() {
        let old = before.get(name).cloned().unwrap_or_default();
        let new = after.get(name).cloned().unwrap_or_default();
        let add: Vec<Value> = new
            .iter()
            .filter(|row| !old.contains(row))
            .cloned()
            .collect();
        let del: Vec<Value> = old
            .iter()
            .filter(|row| !new.contains(row))
            .cloned()
            .collect();
        if !add.is_empty() || !del.is_empty() {
            let relation =
                serde_json::to_string(name.split('/').next().unwrap()).expect("relation JSON");
            let add = serde_json::to_string(&add).expect("add JSON");
            let del = serde_json::to_string(&del).expect("del JSON");
            deltas.push(format!("{relation}:{{\"add\":{add},\"del\":{del}}}"));
        }
    }
    format!("{{\"tick\":{tick},\"deltas\":{{{}}}}}", deltas.join(","))
}
