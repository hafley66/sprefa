//! Volume rail for the source stage. A per-line source rule over a real
//! checkout must run to completion, not die on the staged row/byte caps.
//!
//! The incident: `MAX_STAGE_BYTES` was 64MiB while `MAX_STAGE_ROWS` was
//! 1_000_000, so the byte term bound first at roughly 650k staged rows, and a
//! per-line rule whose regex matched per character amplified one line into ~34
//! identical `(path, line)` tuples. Together they capped a whole-repo scan at
//! well under 20,000 distinct lines, and the engine could not scan its own
//! `src/` tree. Every `.dl` rail in the repo uses a selective regex and stayed
//! green for five days.
//!
//! Why a fixture and not a synthetic file: the trip point sat between 7,643
//! and 20,100 staged rows, so a few hundred lines of test data proves nothing.
//! These tests cross ~20,000 staged rows by construction, and the large arm
//! crosses the old 64MiB byte cap outright. Fixture gating follows
//! `perf_stress` / `perf_stress_c`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

/// `/./` matches per character, so this rule stages one row per character and
/// leans on the per-owner duplicate filter to collapse them to one row per
/// non-blank line. That amplification is the exact shape `oracle_corpus`
/// runs, and the shape that tripped the bound.
const PER_LINE_PROGRAM: &str = "\
rel src_line(source_file: file, line: int).
src_line(source_file, line) <- scan(\"WORK\", \"GLOB\", source_file, rev),
    match_line(source_file, rev, /./, line).
? src_line(source_file, line).
";

fn fixture(env_key: &str, name: &str) -> Option<PathBuf> {
    if let Ok(dir) = std::env::var(env_key) {
        let dir = PathBuf::from(dir);
        if dir.is_dir() {
            return Some(dir);
        }
    }
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/.fixtures")
        .join(name);
    dir.is_dir().then_some(dir)
}

/// Run the per-line program against `root` and return the staged `src_line`
/// row count. Hermetic: `--no-daemon`, a scratch `--db`, no ambient config.
/// Panics with BOTH streams on failure, since the engine reports bound trips
/// on stderr and a stdout-only assert reads as "extracted nothing".
fn count_lines_scanned(root: &Path, glob: &str, tag: &str) -> usize {
    let scratch = std::env::temp_dir().join(format!("sprefa-source-stage-volume-{tag}"));
    let _ = fs::remove_dir_all(&scratch);
    fs::create_dir_all(&scratch).unwrap();
    let program_path = scratch.join("volume.dl");
    fs::write(&program_path, PER_LINE_PROGRAM.replace("GLOB", glob)).unwrap();

    let out = Command::new(DL)
        .arg(&program_path)
        .args(["--db", scratch.join("db").to_str().unwrap(), "--no-daemon"])
        .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .env_remove("SPREFA_SCIP_INDEX")
        .current_dir(root)
        .output()
        .expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "dl exited {:?} scanning {root:?}\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
        out.status.code()
    );
    let _ = fs::remove_dir_all(&scratch);
    // Header line is `? src_line => ...`; every other line is a row.
    stdout.lines().filter(|line| !line.starts_with('?')).count()
}

/// No fixture needed: sprefa's own `src/` is ~90k lines over ~200 files, which
/// is 4x the old effective ceiling. This is the case the release blocker was
/// filed against, and it runs everywhere the test suite runs.
#[test]
fn per_line_rule_scans_this_crates_own_source() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let rows = count_lines_scanned(&root, "src/**/*.rs", "self");
    assert!(
        rows > 20_000,
        "per-line scan of our own src/ staged only {rows} rows; the old effective \
         ceiling sat between 7,643 and 20,100"
    );
}

/// The byte-cap arm. rust-analyzer is ~1,500 `.rs` files, far past 650k staged
/// rows, so it crosses the old 64MiB `MAX_STAGE_BYTES` outright rather than
/// merely clearing the amplification.
#[test]
#[ignore = "needs tests/.fixtures/rust-analyzer corpus (provision with `just v5-fixture-rust`, or set SPREFA_BENCH_ROOT)"]
fn per_line_rule_scans_a_large_rust_checkout() {
    let root = fixture("SPREFA_BENCH_ROOT", "rust-analyzer").expect(
        "needs tests/.fixtures/rust-analyzer corpus (provision with `just v5-fixture-rust`, or set SPREFA_BENCH_ROOT)");
    let rows = count_lines_scanned(&root, "**/*.rs", "rust-analyzer");
    eprintln!("[stage-volume] rust-analyzer fixture: {rows} staged rows");
    assert!(
        rows > 200_000,
        "per-line scan of the rust-analyzer fixture staged only {rows} rows"
    );
}
