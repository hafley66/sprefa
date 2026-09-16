//! Kotlin twin of `oracle_rust`'s `call_resolution_parity_vs_rust_analyzer`:
//! scores the index-free call resolver against `scip-java` ground truth over
//! the existing `tests/fixtures/kt_ws` Gradle fixture (a cross-file method
//! call, a cross-file function call, and an aliased `import a.b.C as D`
//! use). Shares `oracle_parity`'s scorer verbatim — same confirmed-
//! positives-only contract as `oracle_rust`/`oracle_ts`.
//!
//! `#[ignore]`d — no `scip-java`/JDK is a genuine environmental gap, not
//! something the default `cargo test` run should silently count as coverage.
//! Override with SPREFA_SCIP_JAVA.
//!
//! Kotlin `call_site` lines are 1-based (normalized in the typegraph
//! extractor, same as Rust); SCIP ranges are 0-based. `oracle_parity`'s
//! scorer already does the `line1.saturating_sub(1)` offset for every
//! language, so this test does not re-offset.

use std::path::Path;
use std::process::Command;

use protobuf::Message;
use scip::types::Index;

use crate::oracle_parity;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn find_scip_java() -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("SPREFA_SCIP_JAVA") {
        let p = std::path::PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let out = Command::new("which").arg("scip-java").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let p = std::path::PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    p.is_file().then_some(p)
}

/// See `oracle_parity`'s module doc for the exact scoring contract.
///
/// Run: cargo test --test it kotlin_call_resolution_parity_vs_scip -- --nocapture
#[test]
#[ignore = "needs scip-java / JDK on PATH (set SPREFA_SCIP_JAVA)"]
fn kotlin_call_resolution_parity_vs_scip() {
    let scip_java = find_scip_java().expect("needs scip-java / JDK on PATH (set SPREFA_SCIP_JAVA)");

    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/kt_ws");

    // Two SEPARATE scratch copies: scip-java indexes one (a Gradle build
    // artifact tree it may write into), dl scans the other. Never the same
    // directory, and dl's copy never receives the produced index -- the
    // engine's scip importer auto-loads an `index.scip` (or
    // SPREFA_SCIP_INDEX) at a scanned root, which would make this measure
    // the SCIP-import tier instead of the index-free resolver.
    let index_dir = std::env::temp_dir().join("sprefa_oracle_kt_parity_index");
    let _ = std::fs::remove_dir_all(&index_dir);
    oracle_parity::copy_dir(&fixture, &index_dir);

    let scip_out = index_dir.join("index.scip");
    let run = Command::new(&scip_java)
        .args(["index", "--output", scip_out.to_str().unwrap()])
        .current_dir(&index_dir)
        .output()
        .expect("run scip-java index");
    assert!(
        scip_out.is_file(),
        "scip-java produced no index: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let index = Index::parse_from_bytes(&std::fs::read(&scip_out).unwrap()).expect("parse scip");

    let dl_dir = std::env::temp_dir().join("sprefa_oracle_kt_parity_dl");
    let _ = std::fs::remove_dir_all(&dl_dir);
    oracle_parity::copy_dir(&fixture, &dl_dir);

    let prog = format!(
        "rel seen(path: file).\nseen(path) <- scan(\"WORK\", \"**/*.kt\", path, rev).\n{}",
        oracle_parity::SITE_PICK_TAIL
    );
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

    // only .kt sources have a ground truth; the index also covers stdlib jars.
    let stats = oracle_parity::score_parity(&index, &dl_dir, "src/", &stdout);
    assert!(stats.total_sites > 0, "no call sites extracted:\n{stdout}");
    assert!(
        stats.denom() > 0,
        "no scip-confirmable call sites; oracle can't score"
    );

    eprintln!(
        "[oracle:kotlin-parity] confirmed={} wrong={} bare={} multi(excluded)={}",
        stats.confirmed, stats.wrong, stats.bare, stats.multi
    );
    eprintln!(
        "[oracle:kotlin-parity] scip-parity={:.1}% (confirmed positives only) precision={:.3}",
        stats.parity() * 100.0,
        stats.precision()
    );
    for ex in &stats.wrong_examples {
        eprintln!("[oracle:kotlin-parity] wrong: {ex}");
    }
    assert!(stats.confirmed > 0, "zero confirmed resolutions");
    assert!(
        stats.precision() >= 0.95,
        "resolver is buying coverage with wrong joins: precision {:.3} < 0.95; {:?}",
        stats.precision(),
        stats.wrong_examples
    );
}
