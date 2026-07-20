//! Differential oracle: validate our Rust `module_edge` against `rust-analyzer
//! scip` (real symbol-resolved ground truth). RA's SCIP index records, per file,
//! occurrences of symbols with a Definition/Reference role. Aggregating to file
//! level — a reference to symbol S in file A whose definition is in file B — gives
//! the ground-truth file dependency graph. We assert every edge we emit is a real
//! RA edge (precision == 1.0) and report recall (we are diet, so recall < 1).
//!
//! `#[ignore]`d — no rust-analyzer binary is a genuine environmental gap, not
//! something the default `cargo test` run should silently count as coverage.
//! Set SPREFA_RUST_ANALYZER to point at a binary explicitly.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use protobuf::Message;
use scip::types::{Index, SymbolRole};

const DL: &str = env!("CARGO_BIN_EXE_dl");

/// Locate a real rust-analyzer binary (the rustup proxy on this machine is a
/// stub, so prefer the VSCode-bundled server). None -> skip the test.
fn find_ra() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("SPREFA_RUST_ANALYZER") {
        let p = PathBuf::from(p);
        if p.is_file() { return Some(p); }
    }
    let home = std::env::var("HOME").ok()?;
    let ext = Path::new(&home).join(".vscode/extensions");
    let mut found: Option<PathBuf> = None;
    if let Ok(entries) = std::fs::read_dir(&ext) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("rust-lang.rust-analyzer-") {
                let bin = e.path().join("server/rust-analyzer");
                if bin.is_file() { found = Some(bin); }
            }
        }
    }
    found
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap().flatten() {
        let p = e.path();
        let d = dst.join(e.file_name());
        if p.is_dir() { copy_dir(&p, &d); } else { std::fs::copy(&p, &d).unwrap(); }
    }
}

/// file -> file edges from a SCIP index: for each symbol, the file that defines it;
/// then every reference occurrence in a different file yields ref_file -> def_file.
/// Local symbols only (def is in some indexed document).
fn ra_edges(index: &Index) -> HashSet<(String, String)> {
    let mut def_file: HashMap<String, String> = HashMap::new();
    for doc in &index.documents {
        for occ in &doc.occurrences {
            if occ.symbol_roles & (SymbolRole::Definition as i32) != 0 && !occ.symbol.starts_with("local ") {
                def_file.entry(occ.symbol.clone()).or_insert_with(|| doc.relative_path.clone());
            }
        }
    }
    let mut edges = HashSet::new();
    for doc in &index.documents {
        for occ in &doc.occurrences {
            if occ.symbol_roles & (SymbolRole::Definition as i32) != 0 { continue; }
            if let Some(def) = def_file.get(&occ.symbol) {
                if *def != doc.relative_path {
                    edges.insert((doc.relative_path.clone(), def.clone()));
                }
            }
        }
    }
    edges
}

fn our_edges(dir: &Path) -> HashSet<(String, String)> {
    let prog = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "**/*.rs", path, rev), match(path, rev, /./, line).
? module_edge(s, d).
"#;
    std::fs::write(dir.join("mg.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("mg.dl"))
        .args(["--db", dir.join("mg.db").to_str().unwrap()])
        .current_dir(dir)
        .output().expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut edges = HashSet::new();
    let mut in_edge = false;
    for line in stdout.lines() {
        if line.starts_with("? module_edge") { in_edge = true; continue; }
        if line.starts_with('?') { in_edge = false; continue; }
        if in_edge {
            if let Some((s, d)) = line.trim().split_once('\t') {
                edges.insert((s.to_string(), d.to_string()));
            }
        }
    }
    edges
}

struct OracleStats {
    ours: HashSet<(String, String)>,
    ra: HashSet<(String, String)>,
    matched: HashSet<(String, String)>,
    precision: f64,
    recall: f64,
    extra: Vec<(String, String)>,
    missed: Vec<(String, String)>,
}

fn run_ra_edges(ra: &Path, root: &Path, name: &str) -> HashSet<(String, String)> {
    let scip_out = std::env::temp_dir().join(format!("{name}.scip"));
    let status = Command::new(ra)
        .args(["scip", root.to_str().unwrap(), "--output", scip_out.to_str().unwrap()])
        .output().expect("run rust-analyzer scip");
    assert!(scip_out.is_file(), "rust-analyzer scip produced no index: {}",
        String::from_utf8_lossy(&status.stderr));

    let bytes = std::fs::read(&scip_out).unwrap();
    let index = Index::parse_from_bytes(&bytes).expect("parse scip");
    ra_edges(&index)
}

fn filter_prefix(edges: HashSet<(String, String)>, prefix: Option<&str>) -> HashSet<(String, String)> {
    match prefix {
        Some(prefix) => edges.into_iter()
            .filter(|(src, dst)| src.starts_with(prefix) && dst.starts_with(prefix))
            .collect(),
        None => edges,
    }
}

fn compare_with_ra(ra: &Path, root: &Path, name: &str, prefix: Option<&str>) -> OracleStats {
    let ra_edges = filter_prefix(run_ra_edges(ra, root, name), prefix);
    let ours = filter_prefix(our_edges(root), prefix);
    let matched: HashSet<_> = ours.intersection(&ra_edges).cloned().collect();
    let precision = if ours.is_empty() { 1.0 } else { matched.len() as f64 / ours.len() as f64 };
    let recall = if ra_edges.is_empty() { 1.0 } else { matched.len() as f64 / ra_edges.len() as f64 };
    let extra: Vec<_> = ours.difference(&ra_edges).cloned().collect();
    let missed: Vec<_> = ra_edges.difference(&ours).cloned().collect();
    OracleStats { ours, ra: ra_edges, matched, precision, recall, extra, missed }
}

fn print_stats(label: &str, s: &OracleStats) {
    eprintln!("[oracle:{label}] our edges={} ra edges={} matched={} precision={:.2} recall={:.2}",
        s.ours.len(), s.ra.len(), s.matched.len(), s.precision, s.recall);
    eprintln!("[oracle:{label}] ours not in RA: {:?}", s.extra);
    eprintln!("[oracle:{label}] RA not in ours: {:?}", s.missed);
}

#[test]
#[ignore = "needs rust-analyzer on PATH (set SPREFA_RUST_ANALYZER)"]
fn module_edge_is_subset_of_rust_analyzer() {
    let ra = find_ra().expect("needs rust-analyzer on PATH (set SPREFA_RUST_ANALYZER)");
    let tmp = std::env::temp_dir().join("sprefa_oracle_ra_ws");
    let _ = std::fs::remove_dir_all(&tmp);
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ra_ws");
    copy_dir(&fixture, &tmp);

    let stats = compare_with_ra(&ra, &tmp, "sprefa_oracle_ra_ws", None);
    print_stats("fixture", &stats);

    assert!(!stats.ours.is_empty(), "we produced no edges");
    assert!(stats.extra.is_empty(), "every emitted edge must be a real RA edge; extras: {:?}", stats.extra);
}

#[test]
#[ignore = "needs rust-analyzer on PATH (SPREFA_RUST_ANALYZER); manual recall snapshot over the real v5 crate, RA output includes known duplicate-symbol warnings"]
fn real_v5_crate_recall_snapshot() {
    let ra = find_ra().expect("needs rust-analyzer on PATH (set SPREFA_RUST_ANALYZER)");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let stats = compare_with_ra(&ra, root, "sprefa_oracle_v5_real", Some("src/"));
    print_stats("v5-real", &stats);
    assert!(!stats.ours.is_empty(), "we produced no edges");
}

/// THE "how close to SCIP are we" number, at the resolution tier (call graph),
/// scored so the percent can only UNDER-report. See `oracle_parity`'s module
/// doc for the full confirmed-positives-only contract this test enforces;
/// `oracle_parity::score_parity` is the shared implementation, also used by
/// the TypeScript (`oracle_ts.rs`) and Kotlin (`oracle_kotlin_parity.rs`)
/// twins.
///
/// Run: cargo test --test it call_resolution_parity_vs_rust_analyzer -- --ignored --nocapture
#[test]
#[ignore = "needs rust-analyzer on PATH (SPREFA_RUST_ANALYZER); manual scip-parity snapshot, runs rust-analyzer scip over this crate (~minutes)"]
fn call_resolution_parity_vs_rust_analyzer() {
    let ra = find_ra().expect("needs rust-analyzer on PATH (set SPREFA_RUST_ANALYZER)");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    // --- ground truth: rust-analyzer's SCIP index over this crate.
    let scip_out = std::env::temp_dir().join("sprefa_oracle_parity.scip");
    let status = Command::new(&ra)
        .args(["scip", root.to_str().unwrap(), "--output", scip_out.to_str().unwrap()])
        .output().expect("run rust-analyzer scip");
    assert!(scip_out.is_file(), "rust-analyzer scip produced no index: {}",
        String::from_utf8_lossy(&status.stderr));
    let index = Index::parse_from_bytes(&std::fs::read(&scip_out).unwrap()).expect("parse scip");

    // --- ours, index-free: call_site (the attempts) + site_pick (the
    // per-call-site resolved picks; see oracle_parity::SITE_PICK_TAIL) from a
    // scratch db with NO index.scip and no ambient config repos.
    let tmp = std::env::temp_dir().join("sprefa_oracle_parity_db");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let prog = format!(
        "rel seen(path: file).\nseen(path) <- scan(\"WORK\", \"src/**/*.rs\", path, rev).\n{}",
        crate::oracle_parity::SITE_PICK_TAIL);
    std::fs::write(tmp.join("parity.dl"), &prog).unwrap();
    let out = Command::new(DL)
        .arg(tmp.join("parity.dl"))
        .args(["--db", tmp.join("db").to_str().unwrap(), "--no-daemon"])
        .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .env_remove("SPREFA_SCIP_INDEX")
        .current_dir(root)
        .output().expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);

    let stats = crate::oracle_parity::score_parity(&index, root, "src/", &stdout);
    assert!(stats.total_sites > 0, "no call sites extracted:\n{stdout}");
    assert!(stats.denom() > 0, "no RA-confirmable call sites; oracle can't score");

    eprintln!("[oracle:parity] confirmed={} wrong={} bare={} multi(excluded)={}",
        stats.confirmed, stats.wrong, stats.bare, stats.multi);
    eprintln!("[oracle:parity] scip-parity={:.1}% (confirmed positives only) precision={:.3}",
        stats.parity() * 100.0, stats.precision());
    for ex in &stats.wrong_examples { eprintln!("[oracle:parity] wrong: {ex}"); }
    assert!(stats.confirmed > 0, "zero confirmed resolutions");
    assert!(stats.precision() >= 0.95,
        "resolver is buying coverage with wrong joins: precision {:.3} < 0.95; {:?}",
        stats.precision(), stats.wrong_examples);
}
