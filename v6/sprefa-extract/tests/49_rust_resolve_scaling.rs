// TEST: the rust `--resolve` name-match rebuilt the file's own blob per CALL
// SITE, and each rebuild scanned the whole corpus `DefIndex`: probes grew as
// sites x index entries x candidate blobs.
//
// FAIL-FIRST, pre-fix, this corpus cut to 50 files because 400 does not
// terminate in a usable time: 25,349,600 own-blob probes against the 200
// bound. Post-fix the same 50 files read 100, and 400 files read 800.
// The wall leg pre-fix measured 0.89s at 200 files and 4.08s at 400 (4.6x
// against the 2.5 budget), release binary.
//
// The count is the gate and the wall is the second one, the `n_plus_one.rs`
// discipline: a quadratic that slips under the count bound cannot also hold
// the ratio.
//
// The third case is the shape the corpus crawl hit:
// `rust-analyzer/crates/syntax` resolved in 19.16 s and rc=124 under
// `timeout 10`, because one generated file (`ast/generated/nodes.rs`) carries
// 2,508 defs and 3,320 sites. Post-fix that directory resolves in 290 ms.
// The case below reproduces the shape without the external checkout: pre-fix
// it did not finish under `timeout 60`, post-fix it runs in 0.15 s (release).

use std::process::Command;
use std::time::Instant;

const RATIO_BUDGET: f64 = 2.5;

/// The counter is process-wide, so two cases reading it at once would each see
/// the other's arithmetic.
static PROBE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Defs and call sites per file. The quadratic term is sites x index entries,
/// so a file needs both in quantity before the growth shows: at 3 each the
/// pre-fix binary measures flat.
const DEFS_PER_FILE: usize = 20;

/// Every file declares the shared `helper` name plus a chain of unique defs
/// that each call the previous one, so the same-file leg runs at every site
/// and `corpus_defs("helper")` grows with the corpus. Distinct bodies keep the
/// ContentIds distinct.
fn corpus_files(dir: &std::path::Path, n: usize) -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();
    for i in 0..n {
        let path = dir.join(format!("f{i}.rs"));
        let mut body =
            format!("pub fn helper() -> u32 {{ {i} }}\npub fn a0_{i}() -> u32 {{ helper() }}\n");
        for j in 1..DEFS_PER_FILE {
            body.push_str(&format!(
                "pub fn a{j}_{i}() -> u32 {{ a{prev}_{i}() }}\n",
                prev = j - 1
            ));
        }
        std::fs::write(&path, body).unwrap();
        paths.push(path);
    }
    paths
}

fn corpus_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("sprefa-extract-49-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn resolve_wall(bin: &str, paths: &[std::path::PathBuf]) -> f64 {
    let t = Instant::now();
    let out = Command::new(bin)
        .arg("--resolve")
        .args(paths)
        .output()
        .unwrap();
    assert!(out.status.success(), "resolve failed: {:?}", out.stderr);
    t.elapsed().as_secs_f64()
}

fn resolve_probes(paths: &[std::path::PathBuf]) -> u64 {
    let before = sprefa_extract::lang::rust::own_blob_probes();
    let facts = sprefa_extract::resolve_project(&sprefa_extract::ResolveRequest {
        paths,
        arms: sprefa_extract::ResolveArms {
            call: true,
            types: false,
            flow: false,
        },
        scip: Default::default(),
        project_root: None,
        scip_records: Default::default(),
        occurrence_text: false,
        rust_checker: None,
        ts_checker: None,
    })
    .expect("the corpus resolves");
    assert!(
        facts.len() >= paths.len(),
        "{} facts over {} files is an empty resolve",
        facts.len(),
        paths.len()
    );
    sprefa_extract::lang::rust::own_blob_probes() - before
}

/// The blob is a per-FILE fact, so the corpus index is joined once per file and
/// the seed is the file's rarest def name: a handful of probes per file, never
/// one whole-index scan per call site.
#[test]
fn own_blob_probes_stay_linear_in_the_file_count() {
    let dir = corpus_dir("probes");
    let paths = corpus_files(&dir, 400);
    let _serial = PROBE_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let probes = resolve_probes(&paths);
    assert!(
        probes <= 4 * paths.len() as u64,
        "{probes} own-blob probes over {} files is a per-site rescan of the corpus index",
        paths.len()
    );
}

#[test]
fn rust_resolve_wall_grows_linearly_with_file_count() {
    let dir = corpus_dir("wall");
    let bin = env!("CARGO_BIN_EXE_extract");
    let paths200 = corpus_files(&dir, 200);
    let paths400 = corpus_files(&dir, 400);

    let wall200 = resolve_wall(bin, &paths200);
    let wall400 = resolve_wall(bin, &paths400);

    assert!(
        wall400 / wall200 < RATIO_BUDGET,
        "wall(400)={wall400:.3}s vs wall(200)={wall200:.3}s exceeds {RATIO_BUDGET}x"
    );
}

/// `rust-analyzer/crates/syntax` sizes: one generated file of 2,508 defs and
/// 3,320 sites beside 57 ordinary ones.
const GENERATED_DEFS: usize = 2_508;
const MODULE_FILES: usize = 57;

/// One machine-generated node file in a crate-sized corpus is the shape that
/// took `--resolve` over the 10-second law: its site count multiplies the
/// whole corpus index once per site.
#[test]
fn a_generated_node_file_resolves_under_the_ten_second_law() {
    let dir = corpus_dir("generated");
    let mut big = String::from("pub fn helper() -> u32 { 0 }\n");
    for i in 0..GENERATED_DEFS {
        let prev = if i == 0 {
            "helper".to_string()
        } else {
            format!("n{}", i - 1)
        };
        big.push_str(&format!("pub fn n{i}() -> u32 {{ {prev}() + helper() }}\n"));
    }
    std::fs::write(dir.join("nodes.rs"), big).unwrap();
    let mut paths = corpus_files(&dir, MODULE_FILES);
    paths.push(dir.join("nodes.rs"));

    let wall = resolve_wall(env!("CARGO_BIN_EXE_extract"), &paths);
    assert!(
        wall < 10.0,
        "{wall:.3}s over {} files is a per-site rescan, not a resolve",
        paths.len()
    );
}

/// The same edge `60_rust_corpus_scope.rs` pins, re-asserted beside the perf
/// budget: the speedup must not reach the same-file answer by dropping the
/// file-identity join and taking the first corpus `helper`.
#[test]
fn same_file_helper_still_wins_after_the_hoist() {
    let out = Command::new(env!("CARGO_BIN_EXE_extract"))
        .args([
            "--resolve",
            "tests/fixtures/rust_scopes/corpus_scope_a.rs",
            "tests/fixtures/rust_scopes/corpus_scope_b.rs",
        ])
        .output()
        .expect("extract binary runs");
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    let text = String::from_utf8(out.stdout).expect("stdout is UTF-8");
    let edge = text
        .lines()
        .find(|line| line.contains("resolved_edge"))
        .expect("one resolved edge");
    assert!(
        edge.contains(r#""callee_path":"tests/fixtures/rust_scopes/corpus_scope_b.rs""#),
        "callee should be the same file's helper, got: {edge}"
    );
}
