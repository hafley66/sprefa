use rusqlite::{params, params_from_iter, types::Value as SqlValue, Connection, ToSql};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::BTreeMap, env, fs, process};

mod kernel;

#[derive(Deserialize)]
struct Plan {
    ddl: Vec<String>,
    rels: Vec<Rel>,
    rules: Vec<Rule>,
    initial: Vec<Row>,
    schedule: Vec<Vec<SignedRow>>,
    tick_order: Vec<String>,
    #[serde(default)]
    operators: Vec<kernel::Operator>,
}

#[derive(Deserialize)]
struct Rel {
    name: String,
    columns: Vec<String>,
    select_all: String,
}

#[derive(Deserialize)]
struct Rule {
    id: String,
    head: String,
    delete: String,
    inserts: Vec<String>,
}

#[derive(Clone, Deserialize)]
struct Row {
    rel: String,
    values: Vec<Value>,
}

#[derive(Deserialize)]
struct SignedRow {
    sign: i8,
    #[serde(flatten)]
    row: Row,
}

type Rows = BTreeMap<String, Vec<Value>>;

fn main() {
    let path = env::args().nth(1).unwrap_or_else(|| {
        println!("usage: dd-runner PLAN.json");
        process::exit(2);
    });
    let input = fs::read_to_string(path).unwrap_or_else(|error| fail(error));
    let plan: Plan = serde_json::from_str(&input).unwrap_or_else(|error| fail(error));
    if plan.operators.is_empty() {
        let conn = Connection::open_in_memory().unwrap_or_else(|error| fail(error));
        run(&conn, &plan).unwrap_or_else(|error| fail(error));
    } else {
        kernel::run(&plan.rels, &plan.initial, &plan.schedule, &plan.operators)
            .unwrap_or_else(|error| fail(error));
    }
}

fn fail(error: impl std::fmt::Display) -> ! {
    println!("dd-runner: {error}");
    process::exit(1);
}

fn run(conn: &Connection, plan: &Plan) -> rusqlite::Result<()> {
    for ddl in &plan.ddl {
        conn.execute_batch(ddl)?;
    }
    for row in &plan.initial {
        write_row(conn, plan, row, 1)?;
    }
    execute_rules(conn, plan)?;
    let mut before = snapshot(conn, plan)?;
    for (tick, arrivals) in plan.schedule.iter().enumerate() {
        for arrival in arrivals {
            write_row(conn, plan, &arrival.row, arrival.sign)?;
        }
        // The emission preserves this order. Current map-owned bundles run in
        // level_before_edges; empty phases deliberately perform no SQL.
        for phase in &plan.tick_order {
            if phase == "level_before_edges" {
                execute_rules(conn, plan)?;
            }
        }
        let after = snapshot(conn, plan)?;
        println!("{}", tick_json(tick + 1, &before, &after));
        before = after;
    }
    Ok(())
}

fn execute_rules(conn: &Connection, plan: &Plan) -> rusqlite::Result<()> {
    for rule in &plan.rules {
        let _ = (&rule.id, &rule.head);
        conn.execute_batch(&rule.delete)?;
        for insert in &rule.inserts {
            conn.execute_batch(insert)?;
        }
    }
    Ok(())
}

fn write_row(conn: &Connection, plan: &Plan, row: &Row, sign: i8) -> rusqlite::Result<()> {
    let rel = plan.rels.iter().find(|rel| rel.name == row.rel).expect("row relation in plan");
    let table = row.rel.split('/').next().expect("relation name");
    let values: Vec<SqlValue> = row.values.iter().map(|value| storage_value(conn, value)).collect::<rusqlite::Result<_>>()?;
    let placeholders = std::iter::repeat("?").take(values.len()).collect::<Vec<_>>().join(", ");
    let columns = rel.columns.iter().map(|column| format!("\"{column}\"")).collect::<Vec<_>>().join(", ");
    let sql = if sign > 0 {
        format!("INSERT OR IGNORE INTO \"{table}\" ({columns}) VALUES ({placeholders})")
    } else {
        let where_clause = rel.columns.iter().map(|column| format!("\"{column}\" = ?")).collect::<Vec<_>>().join(" AND ");
        format!("DELETE FROM \"{table}\" WHERE {where_clause}")
    };
    let bindings: Vec<&dyn ToSql> = values.iter().map(|value| value as &dyn ToSql).collect();
    conn.execute(&sql, params_from_iter(bindings))?;
    Ok(())
}

fn storage_value(conn: &Connection, value: &Value) -> rusqlite::Result<SqlValue> {
    match value {
        Value::String(text) => {
            conn.execute("INSERT OR IGNORE INTO \"__str\" (\"content\") VALUES (?1)", params![text])?;
            let id = conn.query_row("SELECT \"__id\" FROM \"__str\" WHERE \"content\" = ?1", params![text], |row| row.get::<_, i64>(0))?;
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
        let rows = statement.query_map([], |row| {
            let mut values = Vec::with_capacity(count);
            for column in 0..count {
                values.push(sql_value(row.get_ref(column)?));
            }
            Ok(Value::Array(values))
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        all.insert(rel.name.clone(), sorted(rows));
    }
    Ok(all)
}

fn sql_value(value: rusqlite::types::ValueRef<'_>) -> Value {
    use rusqlite::types::ValueRef;
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(value) => json!(value),
        ValueRef::Real(value) => json!(value),
        ValueRef::Text(value) => Value::String(String::from_utf8_lossy(value).into_owned()),
        ValueRef::Blob(value) => Value::String(String::from_utf8_lossy(value).into_owned()),
    }
}

fn sorted(mut rows: Vec<Value>) -> Vec<Value> {
    rows.sort_by_key(|row| serde_json::to_string(row).expect("JSON row"));
    rows
}

fn tick_json(tick: usize, before: &Rows, after: &Rows) -> String {
    let mut deltas = Vec::new();
    for name in before.keys().chain(after.keys()).collect::<std::collections::BTreeSet<_>>() {
        let old = before.get(name).cloned().unwrap_or_default();
        let new = after.get(name).cloned().unwrap_or_default();
        let add: Vec<Value> = new.iter().filter(|row| !old.contains(row)).cloned().collect();
        let del: Vec<Value> = old.iter().filter(|row| !new.contains(row)).cloned().collect();
        if !add.is_empty() || !del.is_empty() {
            let relation = serde_json::to_string(name.split('/').next().unwrap()).expect("relation JSON");
            let add = serde_json::to_string(&add).expect("add JSON");
            let del = serde_json::to_string(&del).expect("del JSON");
            deltas.push(format!("{relation}:{{\"add\":{add},\"del\":{del}}}"));
        }
    }
    format!("{{\"tick\":{tick},\"deltas\":{{{}}}}}", deltas.join(","))
}
