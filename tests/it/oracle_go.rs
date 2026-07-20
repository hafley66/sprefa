//! Go twin of `oracle_rust`'s `call_resolution_parity_vs_rust_analyzer`: scores
//! the index-free call resolver against `scip-go` ground truth over
//! `tests/fixtures/go_ws` (a cross-package function call, a struct method call
//! from another file, an aliased `import alias "module/pkg"`, and one bare name
//! defined in two packages). Shares `oracle_parity`'s scorer verbatim — same
//! confirmed-positives-only contract as `oracle_rust`/`oracle_ts`/`oracle_kotlin`.
//!
//! `#[ignore]`d — no `scip-go` on PATH is a genuine environmental gap, not
//! something the default `cargo test` run should silently count as coverage.
//! Override the binary with SPREFA_SCIP_GO. Run explicitly with `--ignored`.
//!
//! Go `call_site` lines are 1-based (typegraph extractor); SCIP ranges are
//! 0-based. `oracle_parity`'s scorer already does the `line1.saturating_sub(1)`
//! offset for every language, so this test does not re-offset. Go source files
//! live at the module root (`main.go`, `api/api.go`, ...), so the source prefix
//! is empty: the hermetic fixture has no vendored or std files to scope out.

use std::path::Path;
use std::process::Command;

use protobuf::Message;
use scip::types::Index;

use crate::oracle_parity;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn find_scip_go() -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("SPREFA_SCIP_GO") {
        let p = std::path::PathBuf::from(p);
        if p.is_file() { return Some(p); }
    }
    let out = Command::new("which").arg("scip-go").output().ok()?;
    if !out.status.success() { return None; }
    let p = std::path::PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    p.is_file().then_some(p)
}

/// See `oracle_parity`'s module doc for the exact scoring contract.
///
/// Run: cargo test --test it go_call_resolution_parity_vs_scip -- --nocapture
#[test]
#[ignore = "needs scip-go on PATH (set SPREFA_SCIP_GO)"]
fn go_call_resolution_parity_vs_scip() {
    let scip_go = find_scip_go().expect("needs scip-go on PATH (set SPREFA_SCIP_GO)");

    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/go_ws");

    // Two SEPARATE scratch copies: scip-go loads the module (a `go`-toolchain
    // build tree) and writes its index into one copy; dl scans the other. The
    // dl copy never receives the produced index -- the engine's scip importer
    // auto-loads an `index.scip` (or SPREFA_SCIP_INDEX) at a scanned root, which
    // would make this measure the SCIP-import tier instead of the index-free
    // resolver.
    let index_dir = std::env::temp_dir().join("sprefa_oracle_go_parity_index");
    let _ = std::fs::remove_dir_all(&index_dir);
    oracle_parity::copy_dir(&fixture, &index_dir);

    let scip_out = index_dir.join("index.scip");
    let run = Command::new(&scip_go)
        .args(["index", "--output", scip_out.to_str().unwrap()])
        .current_dir(&index_dir)
        .output().expect("run scip-go index");
    assert!(scip_out.is_file(), "scip-go produced no index: {}",
        String::from_utf8_lossy(&run.stderr));
    let index = Index::parse_from_bytes(&std::fs::read(&scip_out).unwrap()).expect("parse scip");

    let dl_dir = std::env::temp_dir().join("sprefa_oracle_go_parity_dl");
    let _ = std::fs::remove_dir_all(&dl_dir);
    oracle_parity::copy_dir(&fixture, &dl_dir);

    let prog = format!(
        "rel seen(path: file).\nseen(path) <- scan(\"WORK\", \"**/*.go\", path, rev), match(path, rev, /./, line).\n{}",
        oracle_parity::SITE_PICK_TAIL);
    std::fs::write(dl_dir.join("parity.dl"), &prog).unwrap();
    let out = Command::new(DL)
        .arg(dl_dir.join("parity.dl"))
        .args(["--db", dl_dir.join("db").to_str().unwrap(), "--no-daemon"])
        .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .env_remove("SPREFA_SCIP_INDEX")
        .current_dir(&dl_dir)
        .output().expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);

    // Go module files sit at the root; the empty prefix scopes nothing out (the
    // hermetic fixture has no vendored/std sources).
    let stats = oracle_parity::score_parity(&index, &dl_dir, "", &stdout);
    assert!(stats.total_sites > 0, "no call sites extracted:\n{stdout}");
    assert!(stats.denom() > 0, "no scip-confirmable call sites; oracle can't score");

    eprintln!("[oracle:go-parity] confirmed={} wrong={} bare={} multi(excluded)={}",
        stats.confirmed, stats.wrong, stats.bare, stats.multi);
    eprintln!("[oracle:go-parity] scip-parity={:.1}% (confirmed positives only) precision={:.3}",
        stats.parity() * 100.0, stats.precision());
    for ex in &stats.wrong_examples { eprintln!("[oracle:go-parity] wrong: {ex}"); }
    assert!(stats.confirmed > 0, "zero confirmed resolutions");
    assert!(stats.precision() >= 0.95,
        "resolver is buying coverage with wrong joins: precision {:.3} < 0.95; {:?}",
        stats.precision(), stats.wrong_examples);
}
