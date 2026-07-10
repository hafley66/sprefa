//! Differential oracle: validate our Rust `module_edge` against `rust-analyzer
//! scip` (real symbol-resolved ground truth). RA's SCIP index records, per file,
//! occurrences of symbols with a Definition/Reference role. Aggregating to file
//! level — a reference to symbol S in file A whose definition is in file B — gives
//! the ground-truth file dependency graph. We assert every edge we emit is a real
//! RA edge (precision == 1.0) and report recall (we are diet, so recall < 1).
//!
//! Skips (does not fail) when no rust-analyzer binary is found, since CI machines
//! may lack one. Set SPREFA_RUST_ANALYZER to point at a binary explicitly.

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
fn module_edge_is_subset_of_rust_analyzer() {
    let Some(ra) = find_ra() else {
        eprintln!("SKIP oracle_rust: no rust-analyzer binary (set SPREFA_RUST_ANALYZER)");
        return;
    };
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
#[ignore = "manual recall snapshot over the real v5 crate; RA output includes known duplicate-symbol warnings"]
fn real_v5_crate_recall_snapshot() {
    let Some(ra) = find_ra() else {
        eprintln!("SKIP oracle_rust real crate: no rust-analyzer binary (set SPREFA_RUST_ANALYZER)");
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let stats = compare_with_ra(&ra, root, "sprefa_oracle_v5_real", Some("src/"));
    print_stats("v5-real", &stats);
    assert!(!stats.ours.is_empty(), "we produced no edges");
}

/// THE "how close to SCIP are we" number, at the resolution tier (call graph),
/// scored so the percent can only UNDER-report:
///
/// - the denominator is every call site where rust-analyzer's index gives an
///   unambiguous in-repo ground truth (the occurrence under the callee name
///   slices exactly, one def file);
/// - a site counts toward the percent ONLY when our index-free resolver picked
///   the SAME def file (a confirmed positive);
/// - a resolution SCIP can't confirm is EXCLUDED, never counted as a win;
/// - a resolution SCIP contradicts is a `wrong` — tracked separately and
///   bounded by the precision assert, so the resolver can't buy coverage with
///   bad joins.
///
/// Every fuzzy step (range slicing, name matching) fails toward EXCLUSION:
/// an encoding or attribution mismatch shrinks the denominator, it never
/// inflates the numerator.
///
/// Run: cargo test --test it call_resolution_parity_vs_rust_analyzer -- --ignored --nocapture
#[test]
#[ignore = "manual scip-parity snapshot; runs rust-analyzer scip over this crate (~minutes)"]
fn call_resolution_parity_vs_rust_analyzer() {
    let Some(ra) = find_ra() else {
        eprintln!("SKIP oracle parity: no rust-analyzer binary (set SPREFA_RUST_ANALYZER)");
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    // --- ground truth: per (file, 0-based line), the occurrences with their
    // source slice and the defining file of their symbol (in-repo defs only).
    let scip_out = std::env::temp_dir().join("sprefa_oracle_parity.scip");
    let status = Command::new(&ra)
        .args(["scip", root.to_str().unwrap(), "--output", scip_out.to_str().unwrap()])
        .output().expect("run rust-analyzer scip");
    assert!(scip_out.is_file(), "rust-analyzer scip produced no index: {}",
        String::from_utf8_lossy(&status.stderr));
    let index = Index::parse_from_bytes(&std::fs::read(&scip_out).unwrap()).expect("parse scip");

    let mut def_file: HashMap<String, String> = HashMap::new();
    for doc in &index.documents {
        for occ in &doc.occurrences {
            if occ.symbol_roles & (SymbolRole::Definition as i32) != 0 && !occ.symbol.starts_with("local ") {
                def_file.entry(occ.symbol.clone()).or_insert_with(|| doc.relative_path.clone());
            }
        }
    }
    // (file, line) -> [(source slice, def_file)] for reference occurrences of
    // symbols defined somewhere in the index.
    let mut refs_at: HashMap<(String, u32), Vec<(String, String)>> = HashMap::new();
    for doc in &index.documents {
        if !doc.relative_path.starts_with("src/") { continue; }
        let Ok(content) = std::fs::read_to_string(root.join(&doc.relative_path)) else { continue; };
        let lines: Vec<&str> = content.lines().collect();
        for occ in &doc.occurrences {
            if occ.symbol_roles & (SymbolRole::Definition as i32) != 0 { continue; }
            let Some(def) = def_file.get(&occ.symbol) else { continue; };
            // SCIP packed range: [line, col, end_col] or [line, col, end_line, end_col].
            let r = &occ.range;
            if r.len() < 3 { continue; }
            let (line, col, end_line, end_col) = if r.len() == 3 {
                (r[0], r[1], r[0], r[2])
            } else {
                (r[0], r[1], r[2], r[3])
            };
            if line != end_line { continue; } // multi-line ref: exclude
            let Some(text) = lines.get(line as usize) else { continue; };
            let (lo, hi) = (col as usize, end_col as usize);
            if hi > text.len() || lo >= hi { continue; } // non-ascii col drift: exclude
            let Some(slice) = text.get(lo..hi) else { continue; };
            refs_at.entry((doc.relative_path.clone(), line as u32))
                .or_default().push((slice.to_string(), def.clone()));
        }
    }

    // --- ours, index-free: call_site (the attempts) + call_edge (the picks)
    // from a scratch db with NO index.scip and no ambient config repos.
    let tmp = std::env::temp_dir().join("sprefa_oracle_parity_db");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let prog = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.rs", path, rev).
? call_site(repo, caller, callee, file, line).
? call_edge(caller, callee, kind).
"#;
    std::fs::write(tmp.join("parity.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(tmp.join("parity.dl"))
        .args(["--db", tmp.join("db").to_str().unwrap(), "--no-daemon"])
        .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .env_remove("SPREFA_SCIP_INDEX")
        .current_dir(root)
        .output().expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);

    // caller/callee syms are "repo::path::kind::name"; reduce to (path, name).
    fn sym_parts(sym: &str) -> Option<(String, String)> {
        let mut it = sym.rsplitn(3, "::");
        let name = it.next()?;
        let _kind = it.next()?;
        let repo_path = it.next()?;
        let path = repo_path.split_once("::").map(|(_, p)| p).unwrap_or(repo_path);
        Some((path.to_string(), name.to_string()))
    }
    let mut sites: Vec<(String, String, u32)> = Vec::new(); // (file, callee, 1-based line)
    let mut picks: HashMap<(String, String), HashSet<String>> = HashMap::new(); // (file, callee name) -> def files
    let mut section = "";
    for line in stdout.lines() {
        if line.starts_with("? call_site") { section = "site"; continue; }
        if line.starts_with("? call_edge") { section = "edge"; continue; }
        if line.starts_with('?') { section = ""; continue; }
        let cells: Vec<&str> = line.trim_end().split('\t').collect();
        match section {
            "site" if cells.len() >= 5 => {
                if let Ok(n) = cells[4].parse::<u32>() {
                    if cells[3].starts_with("src/") {
                        sites.push((cells[3].to_string(), cells[2].to_string(), n));
                    }
                }
            }
            "edge" if cells.len() >= 2 => {
                if let (Some((cf, _)), Some((df, dn))) = (sym_parts(cells[0]), sym_parts(cells[1])) {
                    picks.entry((cf, dn)).or_default().insert(df);
                }
            }
            _ => {}
        }
    }
    assert!(!sites.is_empty(), "no call sites extracted:\n{stdout}");

    // --- score. A site enters the denominator only when RA slices exactly one
    // def file for the callee's last name segment at that (file, line).
    let (mut confirmed, mut wrong, mut bare, mut multi) = (0usize, 0usize, 0usize, 0usize);
    let mut wrong_examples: Vec<String> = Vec::new();
    for (file, callee, line1) in &sites {
        let needle = callee.rsplit('.').next().unwrap_or(callee);
        let Some(cands) = refs_at.get(&(file.clone(), line1.saturating_sub(1))) else { continue; };
        let truths: HashSet<&String> = cands.iter()
            .filter(|(slice, _)| slice == needle)
            .map(|(_, def)| def).collect();
        if truths.len() != 1 { continue; } // no truth or ambiguous truth: exclude
        let truth = *truths.iter().next().unwrap();
        // method-name picks are keyed both bare and Type.name; try both.
        let ours = picks.get(&(file.clone(), callee.clone()))
            .or_else(|| picks.get(&(file.clone(), needle.to_string())));
        match ours {
            None => bare += 1,
            Some(set) if set.len() > 1 => multi += 1, // ambiguous pick: excluded, reported
            Some(set) => {
                let pick = set.iter().next().unwrap();
                if pick == truth { confirmed += 1; }
                else {
                    wrong += 1;
                    if wrong_examples.len() < 10 {
                        wrong_examples.push(format!("{file}:{line1} {callee}: ours={pick} ra={truth}"));
                    }
                }
            }
        }
    }
    let denom = confirmed + wrong + bare;
    assert!(denom > 0, "no RA-confirmable call sites; oracle can't score");
    let parity = confirmed as f64 / denom as f64;
    let precision = if confirmed + wrong == 0 { 1.0 }
        else { confirmed as f64 / (confirmed + wrong) as f64 };
    eprintln!("[oracle:parity] confirmed={confirmed} wrong={wrong} bare={bare} multi(excluded)={multi}");
    eprintln!("[oracle:parity] scip-parity={:.1}% (confirmed positives only) precision={:.3}",
        parity * 100.0, precision);
    for ex in &wrong_examples { eprintln!("[oracle:parity] wrong: {ex}"); }
    assert!(confirmed > 0, "zero confirmed resolutions");
    assert!(precision >= 0.95,
        "resolver is buying coverage with wrong joins: precision {precision:.3} < 0.95; {wrong_examples:?}");
}
