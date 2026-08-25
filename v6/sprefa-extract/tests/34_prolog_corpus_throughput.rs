// TIMING: guards the prolog `call`-family projector against a return to
// double-walking every clause body. Pre-fix, `project_calls` ran
// `walk_goals` (site-only) and `walk_goals_refs` (ref-only) as two separate
// spine walks over the same body; `src/lang/prolog/_0_source.rs` now merges
// them into one `walk_goals` that pushes both `aux.sites` and `aux.refs`.
//
// FAIL-PRE-FIX release numbers, one process over the whole 187-file /
// 3,488,900-byte `../prolog` corpus (`--family call`, no --resolve, one
// `extract` invocation per file since the CLI takes exactly one PATH outside
// --resolve): issue's measured baseline 0.51s wall. In-process library call
// (`dispatch` + `flatten`, no subprocess spawn per file) measured 80.34
// ns/byte (280.3ms) before this fix, 71.05 ns/byte (247.9ms) after.
//
// Debug (unoptimized) measured 348.19 ns/byte (1.21s) on this fix. The
// budget below is 3x that, so a debug `cargo test` run passes with headroom;
// a return to the double walk would push release past the 3x debug budget
// only under a much bigger regression, so this is a coarse net, not a tight
// one. The `--bench` flag's per-family stderr breakdown is the tight probe
// for a `call`-specific regression; this test is the CI-portable backstop.
const NS_PER_BYTE_BUDGET: f64 = 1100.0;

use sprefa_extract::{dispatch, flatten, FamilyMask};
use std::time::Instant;

fn corpus_files() -> Vec<std::path::PathBuf> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../prolog");
    let mut files = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read prolog corpus dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                if path.file_name().and_then(|name| name.to_str()) == Some("labs") {
                    continue;
                }
                stack.push(path);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("pl") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

#[test]
fn call_family_projection_stays_under_the_debug_scaled_byte_budget() {
    let files = corpus_files();
    assert!(
        files.len() >= 150,
        "the v6/prolog corpus should carry well over 150 .pl files, found {}; \
         the corpus walk itself may be broken",
        files.len()
    );
    let mask = FamilyMask {
        call: true,
        ..FamilyMask::NONE
    };
    let mut total_bytes = 0u64;
    let mut total_facts = 0u64;
    let started = Instant::now();
    for path in &files {
        let content = std::fs::read(path).expect("read corpus file");
        total_bytes += content.len() as u64;
        let path_str = path.to_string_lossy().to_string();
        if let Some(out) = dispatch(&path_str, &content, mask) {
            total_facts += flatten(&out).len() as u64;
        }
    }
    let elapsed = started.elapsed();
    let ns_per_byte = elapsed.as_nanos() as f64 / total_bytes as f64;
    assert!(
        total_facts > 0,
        "the corpus produced no call facts at all; the mask or the walk broke"
    );
    assert!(
        ns_per_byte <= NS_PER_BYTE_BUDGET,
        "call-family projection over {} files / {total_bytes} bytes took {elapsed:?} \
         ({ns_per_byte:.1} ns/byte), over the {NS_PER_BYTE_BUDGET} ns/byte budget: \
         check for a returned double walk in src/lang/prolog/_0_source.rs",
        files.len()
    );
}
