//! Per-tick write ledger: every writer is named with its rel, row count, and seam.

use sprefa_v5::{db, engine::Engine, lex, parse};
use std::fs;
use std::path::PathBuf;

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("write_ledger_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

const PROG: &str = r#"
rel m(p: file, line: int).
m(p, line) <- scan("WORK", "src/*.rs", p, rev), match(p, rev, /fn \w+/, line).
rel n(p: file, line: int).
n(p, line) <- node(p, line, kind, mid, lo, hi).
? n(p, line).
"#;

fn rows_for_tick(eng: &Engine, tick: i64) -> Vec<(String, i64, String)> {
    let rows = eng
        .query_sql(
            "SELECT rel, rows, seam FROM _write_ledger WHERE tick = ?1 ORDER BY rows DESC",
            &[serde_json::json!(tick)],
        )
        .unwrap();
    rows.into_iter()
        .map(|r| {
            (
                r[0].as_str().unwrap().to_string(),
                r[1].as_i64().unwrap(),
                r[2].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn max_tick(eng: &Engine) -> i64 {
    let rows = eng.query_sql("SELECT MAX(tick) FROM _write_ledger", &[]).unwrap();
    rows.first()
        .and_then(|r| r.first())
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
}

#[test]
fn write_ledger_captures_extract_and_derived_writes() {
    let d = sandbox("ledger");
    fs::write(d.join("src/a.rs"), "fn alpha() {}\n").unwrap();

    let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    eng.tick(&prog, true).unwrap();

    let tick1 = max_tick(&eng);
    assert!(tick1 > 0, "ledger should have a tick row after first tick");
    let rows = rows_for_tick(&eng, tick1);
    assert!(
        !rows.is_empty(),
        "first tick should produce ledger rows, got none"
    );

    // At least one extract-family rel (spine's `node`) was written on the source seam.
    let has_extract = rows.iter().any(|(rel, rows, seam)| {
        rel == "node" && *rows > 0 && seam == "source"
    });
    assert!(
        has_extract,
        "ledger should record a source write to an extract-family rel (node); got {rows:?}"
    );

    // At least one derived rel (`n`) was written on the derived seam.
    let has_derived = rows.iter().any(|(rel, rows, seam)| {
        rel == "n" && *rows > 0 && seam == "derived"
    });
    assert!(
        has_derived,
        "ledger should record a derived write to `n`; got {rows:?}"
    );

    // Warm no-change tick: the ledger should gain no new rows. A settled tick
    // writes nothing — this is the core contract that keeps the daemon from looping.
    eng.tick(&prog, true).unwrap();
    let new_rows = eng
        .query_sql(
            "SELECT COUNT(*) FROM _write_ledger WHERE tick > ?1",
            &[serde_json::json!(tick1)],
        )
        .unwrap();
    let count = new_rows
        .first()
        .and_then(|r| r.first())
        .and_then(|v| v.as_i64())
        .unwrap_or(-1);
    assert_eq!(
        count, 0,
        "a settled warm tick must not produce new ledger rows"
    );
}
