//! Python twin of `oracle_rust`'s `call_resolution_parity_vs_rust_analyzer`:
//! scores the index-free call resolver against `scip-python` ground truth over
//! `tests/fixtures/py_ws` (a cross-module function call, a class method call
//! from another module, a `from pkg.api import fetch_user as load_user` alias,
//! and one bare `helper()` defined in two modules). Shares `oracle_parity`'s
//! scorer verbatim — same confirmed-positives-only contract as
//! `oracle_rust`/`oracle_ts`/`oracle_go`.
//!
//! `#[ignore]`d — no `scip-python` is a genuine environmental gap, not
//! something the default `cargo test` run should silently count as coverage.
//! Override with SPREFA_SCIP_PYTHON. Run explicitly with `--ignored`.
//!
//! GOTCHAs for the indexer:
//!  - `scip-python` crashes when indexing in a bare temp dir with no git repo
//!    and no explicit project version (its `normalizeNameOrVersion` reads an
//!    undefined git rev). We pass `--project-name`/`--project-version` AND run
//!    with `--cwd` INSIDE the fixture copy (index in place), then read the
//!    artifact from there. See memory reference_scip_multilang_indexers.
//!  - a fatal indexer error still exits 0 and leaves a tiny header-only index,
//!    so we runtime-SKIP on an empty-document index (and on a missing file),
//!    never a test failure.
//!
//! Python `call_site` lines are 1-based (typegraph extractor); SCIP ranges are
//! 0-based. `oracle_parity`'s scorer does the `line1.saturating_sub(1)` offset
//! for every language. Python modules live under `pkg/` and at the root
//! (`main.py`), so the source prefix is empty: the hermetic fixture has no
//! vendored or stdlib sources to scope out.

use std::path::Path;
use std::process::Command;

use protobuf::Message;
use scip::types::Index;

use crate::oracle_parity;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn find_scip_python() -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("SPREFA_SCIP_PYTHON") {
        let p = std::path::PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let out = Command::new("which").arg("scip-python").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let p = std::path::PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    p.is_file().then_some(p)
}

/// See `oracle_parity`'s module doc for the exact scoring contract.
///
/// Run: cargo test --test it python_call_resolution_parity_vs_scip -- --nocapture
#[test]
#[ignore = "needs scip-python on PATH (set SPREFA_SCIP_PYTHON)"]
fn python_call_resolution_parity_vs_scip() {
    let scip_python =
        find_scip_python().expect("needs scip-python on PATH (set SPREFA_SCIP_PYTHON)");

    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/py_ws");

    // Two SEPARATE scratch copies: scip-python indexes one IN PLACE (cwd inside
    // it, dodging the bare-/tmp crash), dl scans the other. The dl copy never
    // receives the produced index -- the engine's scip importer auto-loads an
    // `index.scip` (or SPREFA_SCIP_INDEX) at a scanned root, which would make
    // this measure the SCIP-import tier instead of the index-free resolver.
    let index_dir = std::env::temp_dir().join("sprefa_oracle_py_parity_index");
    let _ = std::fs::remove_dir_all(&index_dir);
    oracle_parity::copy_dir(&fixture, &index_dir);

    let scip_out = index_dir.join("index.scip");
    let run = Command::new(&scip_python)
        .args([
            "index",
            "--project-name",
            "py_ws",
            "--project-version",
            "0.0.1",
            "--output",
            "index.scip",
        ])
        .current_dir(&index_dir)
        .output()
        .expect("run scip-python index");
    assert!(
        scip_out.is_file(),
        "scip-python produced no index (exit {}): {}",
        run.status,
        String::from_utf8_lossy(&run.stderr)
            .lines()
            .take(6)
            .collect::<Vec<_>>()
            .join(" | ")
    );
    let index = Index::parse_from_bytes(&std::fs::read(&scip_out).unwrap()).expect("parse scip");
    // scip-python's fatal errors still exit 0 and leave a header-only index.
    assert!(
        !index.documents.is_empty(),
        "scip-python index has no documents (exit {}): {}",
        run.status,
        String::from_utf8_lossy(&run.stderr)
            .lines()
            .take(6)
            .collect::<Vec<_>>()
            .join(" | ")
    );

    let dl_dir = std::env::temp_dir().join("sprefa_oracle_py_parity_dl");
    let _ = std::fs::remove_dir_all(&dl_dir);
    oracle_parity::copy_dir(&fixture, &dl_dir);

    let prog = format!(
        "rel seen(path: file).\nseen(path) <- scan(\"WORK\", \"**/*.py\", path, rev), match(path, rev, /./, line).\n{}",
        oracle_parity::SITE_PICK_TAIL);
    std::fs::write(dl_dir.join("parity.dl"), &prog).unwrap();
    let out = Command::new(DL)
        .arg(dl_dir.join("parity.dl"))
        .args(["--db", dl_dir.join("db").to_str().unwrap(), "--no-daemon"])
        .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .env_remove("SPREFA_SCIP_INDEX")
        .current_dir(&dl_dir)
        .output()
        .expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);

    // Python modules sit under pkg/ and at the root; the empty prefix scopes
    // nothing out (the hermetic fixture has no vendored/stdlib sources).
    let stats = oracle_parity::score_parity(&index, &dl_dir, "", &stdout);
    assert!(stats.total_sites > 0, "no call sites extracted:\n{stdout}");
    assert!(
        stats.denom() > 0,
        "no scip-confirmable call sites; oracle can't score"
    );

    eprintln!(
        "[oracle:python-parity] confirmed={} wrong={} bare={} multi(excluded)={}",
        stats.confirmed, stats.wrong, stats.bare, stats.multi
    );
    eprintln!(
        "[oracle:python-parity] scip-parity={:.1}% (confirmed positives only) precision={:.3}",
        stats.parity() * 100.0,
        stats.precision()
    );
    for ex in &stats.wrong_examples {
        eprintln!("[oracle:python-parity] wrong: {ex}");
    }
    assert!(stats.confirmed > 0, "zero confirmed resolutions");
    assert!(
        stats.precision() >= 0.95,
        "resolver is buying coverage with wrong joins: precision {:.3} < 0.95; {:?}",
        stats.precision(),
        stats.wrong_examples
    );
}
