//! Proof that `sg(...)` binds a precise match span (line, col, end_line,
//! end_col), so diagnostics underline exactly the match instead of the whole
//! line. Columns are 0-based byte offsets, lines 1-based. Asserted via the
//! machine-readable `--diag-json` output.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

// Bind the full span off `dbg!($X)` and emit it as a diag.
const PROG: &str = r#"
diag(path, line, col, end_line, end_col,
     severity: "error", code: "no-dbg", msg: "dbg") <-
  scan("WORK", "src/**/*.rs", path, rev),
  sg(path, rev, :rust, "dbg!($X)", line, col, end_line, end_col).
"#;

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sg_span_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("p.dl"), PROG).unwrap();
    dir
}

fn diag_json(dir: &std::path::Path) -> serde_json::Value {
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .current_dir(dir)
        .args(["--db", dir.join("db").to_str().unwrap(), "--diag-json"])
        .output()
        .expect("run dl");
    serde_json::from_slice(&out.stdout).expect("valid JSON")
}

#[test]
fn sg_binds_precise_byte_span() {
    let d = sandbox("precise");
    // `dbg!(x)` sits at 4 spaces of indent on line 2; it spans bytes 4..11.
    fs::write(d.join("src/a.rs"), "fn f() {\n    dbg!(x);\n}\n").unwrap();
    let v = diag_json(&d);
    let f = &v.as_array().expect("array")[0];
    assert_eq!(f["line"], 2, "1-based line");
    assert_eq!(f["col"], 4, "0-based start column at the indent");
    assert_eq!(f["endLine"], 2);
    assert_eq!(f["endCol"], 11, "end column just past the closing paren");
}

#[test]
fn column_shifts_with_indent() {
    // Same match deeper in: the column must track the actual indent, proving it
    // is a real span and not a hardcoded 0 / whole-line fallback.
    let d = sandbox("indent");
    fs::write(
        d.join("src/a.rs"),
        "fn f() {\n    if true {\n        dbg!(y);\n    }\n}\n",
    )
    .unwrap();
    let v = diag_json(&d);
    let f = &v.as_array().expect("array")[0];
    assert_eq!(f["line"], 3);
    assert_eq!(f["col"], 8, "two levels of indent");
    assert_eq!(f["endCol"], 15);
}
