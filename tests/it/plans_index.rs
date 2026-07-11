//! examples/gen-plans-index.dl e2e: a `<!-- todo(category): text -->` markdown
//! comment in a plans/*.md doc round-trips into PLANS.md via `comment_node`
//! (tree-sitter-md html_block filter), and the drift rail exits 2 when the
//! index is stale. Fixture plans dir, hermetic (--no-daemon + scratch --db).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("plans_index_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("plans")).unwrap();
    // The generator splices zones of an existing PLANS.md (markers committed).
    fs::write(dir.join("PLANS.md"),
        "# PLANS.md\n\n## By category\n\n\
         <!-- BEGIN: plans-by-category -->\n<!-- END: plans-by-category -->\n\n\
         ## By plan\n\n\
         <!-- BEGIN: plans-by-plan -->\n<!-- END: plans-by-plan -->\n\n\
         hand-owned prose survives regen\n").unwrap();
    fs::write(dir.join("plans/2026-01-01-alpha.md"),
        "# Alpha plan\n\n## Context\nbody text.\n\n\
         <!-- todo(perf): speed up the alpha pass -->\n\
         <!-- todo(decision): multi-line body\nspanning two lines -->\n\n\
         ```markdown\n<!-- todo(docs): fenced sample, never a row -->\n```\n").unwrap();
    dir
}

fn dl(dir: &Path, args: &[&str]) -> (String, i32) {
    let prog = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/gen-plans-index.dl")).unwrap();
    fs::write(dir.join("gen-plans-index.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("gen-plans-index.dl"))
        .args(["--db", dir.join("db").to_str().unwrap(), "--no-daemon"])
        .args(args)
        .env("SPREFA_CONFIG", "/nonexistent/sprefa-test.toml")
        .current_dir(dir)
        .output()
        .expect("run dl");
    (format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)),
     out.status.code().unwrap_or(-1))
}

#[test]
fn todo_comments_round_trip_into_plans_md() {
    let d = sandbox("roundtrip");
    let (out, code) = dl(&d, &[]);
    assert_eq!(code, 0, "gen run failed:\n{out}");
    let plans = fs::read_to_string(d.join("PLANS.md")).unwrap();
    // Category-grouped row with plans/<file>:<line> ref.
    assert!(plans.contains("- `perf` plans/2026-01-01-alpha.md:6 — speed up the alpha pass"),
        "perf row:\n{plans}");
    // Multi-line body flattened to one row.
    assert!(plans.contains("- `decision` plans/2026-01-01-alpha.md:7 — multi-line body spanning two lines"),
        "flattened multi-line row:\n{plans}");
    // Per-plan section carries the same items keyed by plan.
    assert!(plans.contains("- plans/2026-01-01-alpha.md:6 `perf` — speed up the alpha pass"),
        "by-plan row:\n{plans}");
    // A fenced code sample is inline content, not an html_block — never a row.
    assert!(!plans.contains("fenced sample"), "fenced sample leaked:\n{plans}");
    // Hand-owned prose outside the markers survives.
    assert!(plans.contains("hand-owned prose survives regen"), "prose lost:\n{plans}");
    // Clean check right after a regen: exit 0.
    let (out2, code2) = dl(&d, &["--check"]);
    assert_eq!(code2, 0, "clean check:\n{out2}");
}

#[test]
fn drift_rail_fires_on_stale_index() {
    let d = sandbox("drift");
    let (out, code) = dl(&d, &[]);
    assert_eq!(code, 0, "gen run failed:\n{out}");
    // Add a todo without regenerating: forward drift, exit 2.
    let plan = d.join("plans/2026-01-01-alpha.md");
    let mut body = fs::read_to_string(&plan).unwrap();
    body.push_str("\n<!-- todo(bug): fresh unindexed item -->\n");
    fs::write(&plan, body).unwrap();
    let (out2, code2) = dl(&d, &["--check"]);
    assert_eq!(code2, 2, "stale check should exit 2:\n{out2}");
    assert!(out2.contains("plans-index-drift"), "drift code:\n{out2}");
    // Remove ALL todos without regenerating: reverse drift (orphan rows).
    fs::write(&plan, "# Alpha plan\n\n## Context\nbody text.\n").unwrap();
    let (out3, code3) = dl(&d, &["--check"]);
    assert_eq!(code3, 2, "orphan check should exit 2:\n{out3}");
    assert!(out3.contains("plans-index-orphan"), "orphan code:\n{out3}");
}
