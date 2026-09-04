// sprefa:auto-begin dl7-native-reachability
// generated from checked DL7 through the DBSP lowering
// program structure is native Rust; cells use the kernel Value plane

#[allow(unused_imports)]
use dd_runner::kernel::{Aggregate, LiteralEquals, Operator, Predicate, Projection};
use dd_runner::{Rel, Row, Rule};
#[allow(unused_imports)]
use std::collections::BTreeMap;

#[rustfmt::skip]
pub fn relations() -> Vec<Rel> {
    vec![
        Rel { name: String::from("edge"), columns: vec![String::from("from"), String::from("to")], select_all: String::from("SELECT t.\"from\" AS \"from\", t.\"to\" AS \"to\" FROM \"edge\" t") },
        Rel { name: String::from("path"), columns: vec![String::from("from"), String::from("to")], select_all: String::from("SELECT t.\"from\" AS \"from\", t.\"to\" AS \"to\" FROM \"path\" t") },
    ]
}

#[rustfmt::skip]
pub fn ddl() -> Vec<String> {
    vec![
        String::from("CREATE TABLE IF NOT EXISTS \"__str\" (\"__id\" INTEGER PRIMARY KEY, \"content\" TEXT NOT NULL UNIQUE)"),
        String::from("CREATE TABLE IF NOT EXISTS \"edge\" (\"from\" INTEGER NOT NULL, \"to\" INTEGER NOT NULL, UNIQUE (\"from\", \"to\"))"),
        String::from("CREATE TABLE IF NOT EXISTS \"path\" (\"from\" INTEGER NOT NULL, \"to\" INTEGER NOT NULL, UNIQUE (\"from\", \"to\"))"),
    ]
}

#[rustfmt::skip]
pub fn rules() -> Vec<Rule> {
    vec![
        Rule { id: String::from("map_121"), head: String::from("path"), delete: String::from("DELETE FROM \"path\""), inserts: vec![String::from("INSERT OR IGNORE INTO \"path\" (\"from\", \"to\") SELECT \"b0\".\"from\", \"b0\".\"to\" FROM \"edge\" \"b0\""), String::from("INSERT OR IGNORE INTO \"path\" (\"from\", \"to\") SELECT \"b0\".\"from\", \"b1\".\"to\" FROM \"path\" \"b0\", \"edge\" \"b1\" WHERE \"b0\".\"to\" = \"b1\".\"from\"")] },
    ]
}

#[rustfmt::skip]
pub fn tick_order() -> Vec<String> {
    vec![
        String::from("absorb_arrivals"),
        String::from("index_delta"),
        String::from("level_before_edges"),
        String::from("edge_arrivals"),
        String::from("edge_departures"),
        String::from("level_after_edges"),
        String::from("iterate"),
        String::from("consolidate"),
        String::from("retain"),
        String::from("boundary"),
        String::from("carry"),
        String::from("drain"),
    ]
}

#[rustfmt::skip]
pub fn initial() -> Vec<Row> {
    vec![
    ]
}

#[rustfmt::skip]
pub fn operators() -> Vec<Operator> {
    vec![
        Operator { id: String::from("map_121"), kind: String::from("map"), head: String::from("path"), refs: vec![String::from("edge")], bindings: BTreeMap::from([(String::from("b0"), String::from("edge"))]), predicates: vec![], projection: vec![Projection { head: String::from("from"), source: Some(String::from("b0.from")), value: None }, Projection { head: String::from("to"), source: Some(String::from("b0.to")), value: None }], aggregate: None },
        Operator { id: String::from("map_122"), kind: String::from("map"), head: String::from("path"), refs: vec![String::from("path"), String::from("edge")], bindings: BTreeMap::from([(String::from("b0"), String::from("path")), (String::from("b1"), String::from("edge"))]), predicates: vec![Predicate { column_equals: Some([String::from("b0.to"), String::from("b1.from")]), literal_equals: None }], projection: vec![Projection { head: String::from("from"), source: Some(String::from("b0.from")), value: None }, Projection { head: String::from("to"), source: Some(String::from("b1.to")), value: None }], aggregate: None },
    ]
}
// sprefa:auto-end dl7-native-reachability

pub fn shootout(graph_case: &str, n: usize) -> dd_runner::kernel::Result<serde_json::Value> {
    if n == 0 {
        return Err("shootout N must be greater than zero".into());
    }
    let setup_started = std::time::Instant::now();
    let mut runtime = dd_runner::kernel::Runtime::open(&relations(), &initial(), operators())?;
    let setup_ms = setup_started.elapsed().as_secs_f64() * 1000.0;
    let edge_count = match graph_case {
        "chain" => n - 1,
        "ring" => n,
        other => return Err(format!("unknown shootout case {other}")),
    };
    let mut arrivals = (0..n.saturating_sub(1))
        .map(|from| dd_runner::SignedRow {
            sign: 1,
            row: dd_runner::Row {
                rel: "edge".into(),
                values: vec![serde_json::json!(from), serde_json::json!(from + 1)],
            },
        })
        .collect::<Vec<_>>();
    if graph_case == "ring" {
        arrivals.push(dd_runner::SignedRow {
            sign: 1,
            row: dd_runner::Row {
                rel: "edge".into(),
                values: vec![serde_json::json!(n - 1), serde_json::json!(0)],
            },
        });
    }
    let closure_started = std::time::Instant::now();
    let _ = runtime.tick(1, &arrivals)?;
    let closure_ms = closure_started.elapsed().as_secs_f64() * 1000.0;
    Ok(serde_json::json!({
        "runtime":"dbsp-generated",
        "version":env!("CARGO_PKG_VERSION"),
        "case":graph_case,
        "n":n,
        "edge_count":edge_count,
        "closure_count":runtime.row_count("path"),
        "setup_ms":setup_ms,
        "closure_ms":closure_ms,
    }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn generated_chain_and_ring_have_exact_closures() {
        assert_eq!(super::shootout("chain", 4).unwrap()["closure_count"], 6);
        assert_eq!(super::shootout("ring", 4).unwrap()["closure_count"], 16);
    }
}
