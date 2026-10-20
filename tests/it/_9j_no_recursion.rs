//! No function in eval, IR and goal lowering, the runtime, or the sqlite_ivm
//! crates calls itself, alone or through a cycle
//! (plans/v8/2026-10-02-kernel-recursion.brief.md). `ryi --resolve` writes the
//! syntax-resolved call graph; a function is its definition span, and a call
//! belongs to the innermost function whose span holds the call site.
//!
//! FAIL-FIRST RECEIPT: at 1e8118b01 the query names 31 functions under `src/`
//! and 11 under `sqlite_ivm/crates/`.

use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The call graph of `root` and the functions in scope on a call cycle.
const CYCLES: &str = "
CREATE TEMP TABLE fdef AS
  SELECT _input_path AS path, span__start AS start, span__end AS stop, name FROM node
  WHERE family = 'call' AND kind IN ('function', 'method');
CREATE INDEX temp.fdef_at ON fdef(path, start);
CREATE TEMP TABLE call AS
  SELECT DISTINCT
    (SELECT f.path || ':' || f.start || ':' || f.name FROM fdef f
      WHERE f.path = r.caller_path AND f.start <= r.caller_site_start AND r.caller_site_end <= f.stop
      ORDER BY f.start DESC LIMIT 1) AS caller,
    d.path || ':' || d.start || ':' || d.name AS callee
  FROM resolved_edge r JOIN fdef d ON d.path = r.callee_path AND d.start = r.callee_start;
CREATE INDEX temp.call_from ON call(caller);
";

const IN_SCOPE: &str = "
WITH RECURSIVE reach(origin, here) AS (
  SELECT caller, callee FROM call
  WHERE caller LIKE 'src/_6_eval/%' OR caller LIKE 'src/_2_lower/%' OR caller LIKE 'src/_9_runtime/%'
     OR caller GLOB 'sqlite_ivm/crates/ivm-*/src/*.rs:*'
  UNION
  SELECT reach.origin, call.callee FROM reach JOIN call ON call.caller = reach.here
)
SELECT origin FROM reach WHERE origin = here ORDER BY 1
";

fn v8_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn cycles(extract: &Path, root: &str) -> Vec<String> {
    let db = std::env::temp_dir().join(format!(
        "dl8-no-recursion-{}-{}.db",
        std::process::id(),
        root.replace('/', "_")
    ));
    let _ = std::fs::remove_file(&db);
    let output = Command::new(extract)
        .current_dir(v8_root())
        .args([root, "--resolve", "--kinds", "cst,call", "--sqlite"])
        .arg(&db)
        .env("RUST_LOG", "warn")
        .output()
        .expect("ryi runs");
    assert!(
        output.status.success(),
        "ryi {root}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let connection = Connection::open(&db).unwrap();
    let resolved: i64 = connection
        .query_row("SELECT count(*) FROM resolved_edge", [], |row| row.get(0))
        .unwrap();
    assert!(resolved > 0, "ryi resolved no call under {root}");
    connection.execute_batch(CYCLES).unwrap();
    let mut statement = connection.prepare(IN_SCOPE).unwrap();
    let found = statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    drop(statement);
    drop(connection);
    let _ = std::fs::remove_file(&db);
    found
}

#[test]
fn kernel_eval_lowering_and_runtime_have_no_call_cycle() {
    let Ok(extract) = dl8::_9_runtime::executors::ryi_resolve::extract_bin() else {
        println!("SKIP: no ryi on PATH and SPREFA_EXTRACT_BIN unset");
        return;
    };
    let found: Vec<String> = ["src", "sqlite_ivm/crates"]
        .iter()
        .flat_map(|root| cycles(&extract, root))
        .collect();
    assert!(
        found.is_empty(),
        "functions on a call cycle (path:definition byte:name):\n{}",
        found.join("\n")
    );
}
