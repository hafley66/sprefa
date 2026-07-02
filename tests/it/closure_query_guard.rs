//! The closure-query guard: a `?` query that would evaluate a closure head's
//! SQL VIEW materializes the FULL reachability relation (a LIMIT does not
//! short-circuit the UNION + recursive CTE), which is unbounded on a dense
//! edge rel — measured minutes of CPU on a 471k-edge flow graph. Over the
//! `DL_CLOSURE_QUERY_MAX_EDGES` cap the query must fail LOUDLY with the pin
//! hint instead of hanging the tick; pinned forms answer through the seeded
//! condensation walk regardless of the cap.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

const PROG: &str = r#"
rel e(a: text, b: text).
e("a", "b").
e("b", "c").
e("x", "y").

rel reach(from: text, to: text).
reach(a, b) <- closure(e).
"#;

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("closure_query_guard_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, query: &str, cap: Option<&str>) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), format!("{PROG}\n{query}\n")).unwrap();
    let mut cmd = Command::new(DL);
    cmd.arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap()]);
    if let Some(cap) = cap { cmd.env("DL_CLOSURE_QUERY_MAX_EDGES", cap); }
    let out = cmd.output().expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Unpinned closure query over an edge rel bigger than the cap: refused with
/// the pin hint (loud, names the knob), other queries unaffected, exit 0.
#[test]
fn unpinned_closure_query_over_cap_fails_loudly() {
    let dir = sandbox("cap");
    let (code, stdout, stderr) = run(&dir, "? reach(from, to).", Some("2"));
    assert_eq!(code, 0, "query failure reports and continues:\n{stderr}");
    assert!(
        stderr.contains("DL_CLOSURE_QUERY_MAX_EDGES") && stderr.contains("pin an endpoint"),
        "guard message names the knob and the fix:\n{stderr}"
    );
    assert!(!stdout.contains("a\tb"), "no reach rows printed:\n{stdout}");
}

/// The same graph under the default cap answers fine (3 edges << 20k).
#[test]
fn unpinned_closure_query_under_cap_answers() {
    let dir = sandbox("under");
    let (code, stdout, stderr) = run(&dir, "? reach(from, to).", None);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("a\tc"), "transitive pair present:\n{stdout}");
}

/// A pinned query rides the seeded condensation walk, so the cap is irrelevant.
#[test]
fn pinned_closure_query_ignores_cap() {
    let dir = sandbox("pinned");
    let (code, stdout, stderr) = run(&dir, "? reach(\"a\", to).", Some("2"));
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("a\tc"), "seeded walk reaches c:\n{stdout}");
    assert!(!stderr.contains("DL_CLOSURE_QUERY_MAX_EDGES"), "guard silent:\n{stderr}");
}

/// Both endpoints pinned: an existence probe answered by the walk (one row when
/// reachable, zero when not), never the view — so it also ignores the cap.
#[test]
fn both_pinned_closure_query_is_an_existence_probe() {
    let dir = sandbox("pair");
    let (code, stdout, stderr) = run(&dir, "? reach(\"a\", \"c\").\n? reach(\"a\", \"y\").", Some("2"));
    assert_eq!(code, 0, "{stderr}");
    let secs: Vec<&str> = stdout.split("? reach").collect();
    assert!(secs.len() >= 3, "two query sections:\n{stdout}");
    assert!(secs[1].contains("(1 rows)") && secs[1].contains("a\tc"), "a reaches c:\n{stdout}");
    assert!(secs[2].contains("(0 rows)"), "a does not reach y:\n{stdout}");
    assert!(!stderr.contains("DL_CLOSURE_QUERY_MAX_EDGES"), "guard silent:\n{stderr}");
}
