//! `match`'s optional trailing args after `line`, disambiguated by count:
//! 1 ⇒ `id` (the whole-match spine id), 2 ⇒ `col, end_col` (0-based byte
//! columns of the whole match within the line), 3 ⇒ `id, col, end_col`. The
//! columns drive sub-line diagnostic spans (the rails unwrap-budget squiggle
//! lands on `.unwrap()`, not the whole line).

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("match_col_span_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn run_json(dir: &PathBuf, prog: &str) -> Vec<serde_json::Value> {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .current_dir(dir)
        .args([
            "--db",
            dir.join("db").to_str().unwrap(),
            "--no-daemon",
            "--query-json",
        ])
        .output()
        .expect("run dl");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("bad json line `{l}`: {e}")))
        .collect()
}

#[test]
fn match_binds_whole_match_columns() {
    let d = sandbox("cols");
    // `    x.unwrap();` — `.unwrap()` is 9 bytes starting at byte column 5
    // (after the 4-space indent + `x`).
    fs::write(d.join("src/a.rs"), "    x.unwrap();\n").unwrap();

    let prog = r#"
rel hit(l: int, c: int, ec: int).
hit(l, c, ec) <- scan("WORK","src/**/*.rs",path,rev),
  match(path, rev, /\.unwrap\(\)/, l, c, ec).
? hit(l, c, ec).
"#;
    let recs = run_json(&d, prog);
    let q = recs
        .iter()
        .find(|r| r["query"] == "hit")
        .expect("hit query");
    assert_eq!(q["count"], 1, "one match: {q}");
    let row = &q["rows"][0];
    assert_eq!(row[0].as_i64(), Some(1), "line 1: {row}");
    assert_eq!(
        row[1].as_i64(),
        Some(5),
        "`.unwrap()` starts at col 5: {row}"
    );
    assert_eq!(
        row[2].as_i64(),
        Some(14),
        "`.unwrap()` ends at col 14 (5+9): {row}"
    );
}

#[test]
fn match_id_and_columns_together() {
    let d = sandbox("id_cols");
    fs::write(d.join("src/a.rs"), "    x.unwrap();\n").unwrap();

    // 3 trailing args: id, col, end_col. The id still resolves through `ref`.
    let prog = r#"
rel hit(id: text, c: int, ec: int).
rel located(lo: int, hi: int, c: int, ec: int).
hit(id, c, ec) <- scan("WORK","src/**/*.rs",path,rev),
  match(path, rev, /\.unwrap\(\)/, _l, id, c, ec).
located(lo, hi, c, ec) <- hit(id, c, ec), ref(id, _s, _f, lo, hi).
? located(lo, hi, c, ec).
"#;
    let recs = run_json(&d, prog);
    let q = recs
        .iter()
        .find(|r| r["query"] == "located")
        .expect("located query");
    assert_eq!(q["count"], 1, "one located: {q}");
    let row = &q["rows"][0];
    // Byte span in the file: cols 5..14 on the only line, so file bytes 5..14.
    assert_eq!(row[0].as_i64(), Some(5), "lo: {row}");
    assert_eq!(row[1].as_i64(), Some(14), "hi: {row}");
    assert_eq!(row[2].as_i64(), Some(5), "col: {row}");
    assert_eq!(row[3].as_i64(), Some(14), "end_col: {row}");
}
