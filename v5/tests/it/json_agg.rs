//! json aggregation heads + json scalar fns (2026-07-10): dl stays flat in
//! storage; nesting is OUTPUT, built by SQLite's own json functions. Two head
//! aggregates (json_group_array, json_group_object) collect a group's rows; the
//! json_object / json_array / json scalar fns build a value; composition is by
//! stratification. Determinism rides `ORDER BY` inside the aggregate so the rel's
//! content digest is stable tick to tick. Same sandbox harness as tests/it/agg.rs.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("json_agg_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap(), "--no-daemon"])
        .current_dir(dir)
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn check(dir: &Path, prog: &str) -> (i32, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .args(["--check", dir.join("p.dl").to_str().unwrap()])
        .args(["--db", dir.join("db").to_str().unwrap(), "--no-daemon"])
        .current_dir(dir)
        .output()
        .expect("run dl --check");
    (
        out.status.code().unwrap_or(-1),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

/// Extract the single-column value rows printed under `? rel =>`.
fn payload_rows(out: &str, rel: &str) -> Vec<String> {
    let block = out.split(&format!("? {rel} =>")).nth(1).unwrap_or("");
    let mut rows = Vec::new();
    for line in block.lines().skip(1) {
        let l = line.trim();
        if l.is_empty() || l.starts_with('(') {
            continue;
        }
        if l.starts_with('?') {
            break;
        }
        rows.push(l.to_string());
    }
    rows
}

/// Two-strata composition: an order's items collect into a JSON array, then the
/// order embeds that array under a JSON object. The output must be VALID json.
#[test]
fn nested_two_strata_produces_valid_json() {
    let d = sandbox("nested");
    let prog = r#"
rel order_item(order_id: text, item_name: text).
order_item("A", "widget").
order_item("A", "gadget").
order_item("B", "sprocket").

rel orders(order_id: text).
orders("A").
orders("B").

rel item_json(order_id: text, items: text).
item_json(order_id, json_group_array(item_name)) <- order_item(order_id, item_name).

rel order_json(order_id: text, payload: text).
order_json(order_id, json_object("id", order_id, "items", json(items))) <-
    orders(order_id), item_json(order_id, items).

? order_json(order_id, payload).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    // Every payload row parses as valid json AND nests the array as json (not a
    // string). Rows print as `order_id<TAB>payload`; take the payload column.
    let rows: Vec<String> = payload_rows(&out, "order_json")
        .iter()
        .filter_map(|l| l.split('\t').nth(1).map(|s| s.to_string()))
        .collect();
    assert!(!rows.is_empty(), "no json rows in:\n{out}");
    let mut saw_a = false;
    for row in &rows {
        let v: serde_json::Value =
            serde_json::from_str(row).unwrap_or_else(|e| panic!("invalid json `{row}`: {e}"));
        assert!(v.get("id").is_some(), "object has id: {row}");
        let items = v.get("items").expect("items key");
        assert!(
            items.is_array(),
            "items nested as a JSON array, not a string: {row}"
        );
        if v["id"] == serde_json::json!("A") {
            saw_a = true;
            let mut names: Vec<String> = items
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_str().unwrap().to_string())
                .collect();
            let mut expect = vec!["gadget".to_string(), "widget".to_string()];
            names.sort();
            expect.sort();
            assert_eq!(names, expect, "order A collects both items");
        }
    }
    assert!(saw_a, "order A present in:\n{out}");
}

/// Determinism: the SAME program over the SAME db on two runs produces
/// byte-identical aggregate output (ORDER BY inside the aggregate).
#[test]
fn json_agg_output_is_deterministic() {
    let d = sandbox("determ");
    let prog = r#"
rel item(g: text, name: text).
item("g", "delta").
item("g", "alpha").
item("g", "charlie").
item("g", "bravo").
rel arr(g: text, names: text).
arr(g, json_group_array(name)) <- item(g, name).
? arr(g, names).
"#;
    let (c1, out1, _) = run(&d, prog);
    let (c2, out2, _) = run(&d, prog);
    assert_eq!(c1, 0);
    assert_eq!(c2, 0);
    let r1 = payload_rows(&out1, "arr");
    let r2 = payload_rows(&out2, "arr");
    assert_eq!(
        r1, r2,
        "aggregate output must be byte-identical across runs"
    );
    // And the array is sorted (deterministic order), not insertion order.
    assert!(
        r1.iter()
            .any(|l| l.contains(r#"["alpha","bravo","charlie","delta"]"#)),
        "array is ORDER BY-sorted: {r1:?}"
    );
}

/// A json aggregate lands a text json string; heading an int column is a
/// brand-mismatch typecheck diag (mirrors count/sum-into-text).
#[test]
fn json_agg_into_int_column_is_a_diag() {
    let d = sandbox("intcol");
    let prog = r#"
rel item(g: text, name: text).
item("g", "x").
rel bad(g: text, arr: int).
bad(g, json_group_array(name)) <- item(g, name).
? bad(g, arr).
"#;
    let (code, out) = check(&d, prog);
    assert_ne!(code, 0, "must fail --check:\n{out}");
    assert!(out.contains("brand-mismatch"), "brand-mismatch diag: {out}");
    assert!(out.contains("json_group_array"), "names the agg: {out}");
}

/// json_group_array inside a recursive cycle is the existing not-stratified diag
/// (an aggregate edge inside an SCC has no stratified meaning).
#[test]
fn json_agg_in_recursive_cycle_is_not_stratified() {
    let d = sandbox("cycle");
    let prog = r#"
rel edge(a: text, b: text).
edge("x", "y").
rel loop_rel(a: text, names: text).
loop_rel(a, json_group_array(b)) <- edge(a, b), loop_rel(b, _).
? loop_rel(a, names).
"#;
    let (code, out) = check(&d, prog);
    assert_ne!(code, 0, "must fail --check:\n{out}");
    assert!(out.contains("not-stratified"), "not-stratified diag: {out}");
}

/// A source rule (scan/match/...) cannot build json in its head — derived-only.
#[test]
fn source_rule_json_head_is_refused() {
    let d = sandbox("source");
    fs::write(d.join("data.txt"), "alpha\nbravo\n").unwrap();
    // A match source rule whose head builds a json array = json-in-source error.
    let prog = r#"
rel bad(payload: text).
bad(json_array(word)) <- scan("WORK", "data.txt", path, rev), match(path, rev, /(?<word>\w+)/, line).
? bad(payload).
"#;
    let (code, out) = check(&d, prog);
    assert_ne!(code, 0, "must fail --check:\n{out}");
    assert!(out.contains("json-in-source"), "json-in-source diag: {out}");
}

/// The shipped example runs and emits valid nested json (corpus-free rel_catalog).
#[test]
fn example_json_out_runs() {
    let d = sandbox("example");
    let ex = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/json-out.dl");
    let out = Command::new(DL)
        .arg(&ex)
        .args(["--db", d.join("db").to_str().unwrap(), "--no-daemon"])
        .current_dir(&d)
        .output()
        .expect("run example");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code().unwrap_or(-1),
        0,
        "example failed:\n{stdout}\n{stderr}"
    );
    // The catalog_json row is one big JSON array of per-group objects.
    let cat = payload_rows(&stdout, "catalog_json");
    assert_eq!(cat.len(), 1, "one catalog_json row:\n{stdout}");
    let v: serde_json::Value = serde_json::from_str(&cat[0])
        .unwrap_or_else(|e| panic!("catalog_json invalid: {e}\n{}", cat[0]));
    assert!(v.is_array(), "catalog_json is a JSON array");
    let core = v
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["group"] == serde_json::json!("core"))
        .expect("a core group object");
    assert!(core["rels"].is_array(), "rels nested as an array");
}
