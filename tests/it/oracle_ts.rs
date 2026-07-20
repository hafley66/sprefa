//! TypeScript twin of `oracle_rust`'s `call_resolution_parity_vs_rust_analyzer`:
//! scores the index-free call resolver against `scip-typescript` ground truth
//! over `tests/fixtures/ts_ws` (cross-file function calls, a method call, an
//! ALIASED import, and one ambiguous bare name defined in two files). Shares
//! `oracle_parity`'s scorer verbatim — same confirmed-positives-only contract.
//!
//! `#[ignore]`d — no `scip-typescript` on PATH is a genuine environmental gap,
//! not something the default `cargo test` run should silently count as
//! coverage. Override with SPREFA_SCIP_TYPESCRIPT; falls back to
//! `npx --yes @sourcegraph/scip-typescript` when no bare binary is on PATH but
//! npx is. Run explicitly with `--ignored`.

use std::path::Path;
use std::process::Command;

use protobuf::Message;
use scip::types::Index;

use crate::oracle_parity;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn bin_on_path(name: &str) -> bool {
    Command::new("which").arg(name).output().map(|o| o.status.success()).unwrap_or(false)
}

/// Locate a way to run `scip-typescript index`: an explicit override, a bare
/// binary on PATH, or `npx --yes @sourcegraph/scip-typescript` as a last
/// resort. Returns the argv prefix (program + leading args); the caller
/// appends `index --output <path>`.
fn find_scip_ts() -> Option<Vec<String>> {
    if let Ok(p) = std::env::var("SPREFA_SCIP_TYPESCRIPT") {
        return Some(vec![p]);
    }
    if bin_on_path("scip-typescript") {
        return Some(vec!["scip-typescript".into()]);
    }
    if bin_on_path("npx") {
        return Some(vec!["npx".into(), "--yes".into(), "@sourcegraph/scip-typescript".into()]);
    }
    None
}

/// See `oracle_parity`'s module doc for the exact scoring contract.
///
/// Run: cargo test --test it ts_call_resolution_parity_vs_scip -- --nocapture
#[test]
#[ignore = "needs scip-typescript on PATH or npx (set SPREFA_SCIP_TYPESCRIPT)"]
fn ts_call_resolution_parity_vs_scip() {
    let scip_ts = find_scip_ts().expect("needs scip-typescript on PATH or npx (set SPREFA_SCIP_TYPESCRIPT)");

    // Copy the fixture into a scratch dir dl will scan; the SCIP index is
    // written OUTSIDE that dir so the engine's scip importer never sees an
    // `index.scip` (or SPREFA_SCIP_INDEX) at the scanned root -- this test
    // measures the index-free resolver, not the SCIP-import tier.
    let tmp = std::env::temp_dir().join("sprefa_oracle_ts_ws");
    let _ = std::fs::remove_dir_all(&tmp);
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts_ws");
    oracle_parity::copy_dir(&fixture, &tmp);

    let scip_out = std::env::temp_dir().join("sprefa_oracle_ts_parity.scip");
    let _ = std::fs::remove_file(&scip_out);
    let mut cmd = Command::new(&scip_ts[0]);
    cmd.args(&scip_ts[1..]);
    cmd.args(["index", "--output"]).arg(&scip_out);
    cmd.current_dir(&tmp);
    let run = cmd.output().expect("run scip-typescript index");
    assert!(scip_out.is_file(), "scip-typescript produced no index: {}",
        String::from_utf8_lossy(&run.stderr));
    let index = Index::parse_from_bytes(&std::fs::read(&scip_out).unwrap()).expect("parse scip");

    let prog = format!(
        "rel seen(path: file).\nseen(path) <- scan(\"WORK\", \"src/**/*.ts\", path, rev).\n{}",
        oracle_parity::SITE_PICK_TAIL);
    std::fs::write(tmp.join("parity.dl"), &prog).unwrap();
    let out = Command::new(DL)
        .arg(tmp.join("parity.dl"))
        .args(["--db", tmp.join("db").to_str().unwrap(), "--no-daemon"])
        .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .env_remove("SPREFA_SCIP_INDEX")
        .current_dir(&tmp)
        .output().expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);

    let stats = oracle_parity::score_parity(&index, &tmp, "src/", &stdout);
    assert!(stats.total_sites > 0, "no call sites extracted:\n{stdout}");
    assert!(stats.denom() > 0, "no scip-confirmable call sites; oracle can't score");

    eprintln!("[oracle:ts-parity] confirmed={} wrong={} bare={} multi(excluded)={}",
        stats.confirmed, stats.wrong, stats.bare, stats.multi);
    eprintln!("[oracle:ts-parity] scip-parity={:.1}% (confirmed positives only) precision={:.3}",
        stats.parity() * 100.0, stats.precision());
    for ex in &stats.wrong_examples { eprintln!("[oracle:ts-parity] wrong: {ex}"); }
    assert!(stats.confirmed > 0, "zero confirmed resolutions");
    assert!(stats.precision() >= 0.95,
        "resolver is buying coverage with wrong joins: precision {:.3} < 0.95; {:?}",
        stats.precision(), stats.wrong_examples);
}
