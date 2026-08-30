//! The ratchet's shared library: corpus enumeration, the
//! `src_path src_name dst_path dst_name` normal form (the Rust port of
//! `plans/extract-bench-2026-08-29/normalize.py`, which stays the reference),
//! scoring against the committed oracle tsvs, and RATCHET.tsv IO. Shared by
//! `tests/ratchet_recall.rs` and `tests/bench_normal_form.rs` via
//! `mod bench;`; a directory under `tests/` compiles no test binary of its
//! own.
//!
//! Direction convention (ORACLES.REPORT.md:583): recall = overlap / |oracle|,
//! precision = overlap / |ours|, both percent.

// Each test binary links the whole module but uses only its half (the parity
// test never scores; the ratchet never serializes raw JSONL), so unused-code
// warnings here are the sharing, not rot.
#![allow(dead_code)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use sprefa_extract::{resolve_project, FlatFact, ResolveArms, ResolveRequest, ScipMode, ScipRecords};

/// Where the committed oracle tsvs and RATCHET.tsv live.
pub const BENCH_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../plans/extract-bench-2026-08-29"
);

/// The per-call wall budget (the timeout-gun law; every extract call under
/// timeout 30). The go corpus sits near 12 s median at #579, known red and
/// owned by the speed lane, so the budget has room but is not open-ended.
pub const WALL_BUDGET_MS: u128 = 30_000;

pub struct Corpus {
    pub lang: &'static str,
    pub root: PathBuf,
    /// (family, oracle tsv file name) pairs; every file must sit in BENCH_DIR.
    pub oracles: &'static [(&'static str, &'static str)],
}

/// The three corpora in COMMON.md order. Roots are machine-local checkouts;
/// the `RATCHET_*_ROOT` overrides exist so another machine can point the
/// ratchet at its own copies.
pub fn corpus(lang: &str) -> Corpus {
    let root = |var: &str, default: &str| {
        std::env::var(var)
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(default))
    };
    match lang {
        "ts5" => Corpus {
            lang: "ts5",
            root: root("RATCHET_TS_ROOT", "/Users/chrishafley/projects/TypeScript-5.9"),
            oracles: &[
                ("call", "ts5.oracle.call.tsv"),
                ("call", "ts.codeql2.call.tsv"),
                ("module", "ts.madge.module.tsv"),
            ],
        },
        "go" => Corpus {
            lang: "go",
            root: root("RATCHET_GO_ROOT", "/Users/chrishafley/projects/typescript-go"),
            oracles: &[
                ("call", "go.oracle.call.vta.bare.tsv"),
                ("call", "go.codeql2.call.tsv"),
                ("module", "go.oracle.module.tsv"),
                ("type", "go.oracle.type.typedecl.tsv"),
            ],
        },
        "rust" => Corpus {
            lang: "rust",
            root: root("RATCHET_RUST_ROOT", "/Users/chrishafley/projects/rust-analyzer"),
            oracles: &[
                ("call", "rust.oracle.call.tsv"),
                ("type", "rust.oracle.type.typedecl.tsv"),
            ],
        },
        other => panic!("unknown ratchet corpus '{other}'"),
    }
}

/// The file rule the bench lab measured against (ORACLES.REPORT.md:30-31):
/// ts5 is `src/**` minus `src/lib` (the bundled lib .d.ts files), go is every
/// `.go` under the root, rust is every `.rs` under `crates/` whose path
/// carries a `src` component. Generated-but-tracked files (rust-analyzer's
/// `proc-macro-test` pair) come and go with local builds; the ratchet pins
/// whatever is on disk.
fn wants(lang: &str, rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    match lang {
        "ts5" => {
            parts.first() == Some(&"src")
                && !(parts.len() >= 2 && parts[1] == "lib")
                && rel.ends_with(".ts")
        }
        "go" => rel.ends_with(".go"),
        "rust" => {
            parts.first() == Some(&"crates")
                && parts.len() > 2
                && parts[1..].contains(&"src")
                && rel.ends_with(".rs")
        }
        _ => false,
    }
}

pub fn enumerate(corpus: &Corpus) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![corpus.root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if path.is_dir() {
                if name.starts_with('.') || name == "target" || name == "node_modules" {
                    continue;
                }
                stack.push(path);
                continue;
            }
            let Ok(rel) = path.strip_prefix(&corpus.root) else {
                continue;
            };
            let Some(rel) = rel.to_str() else { continue };
            if wants(corpus.lang, rel) {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

// ── the normal form (the normalize.py port) ─────────────────────────────────

/// normalize.py `relp`: a path under the corpus root becomes root-relative,
/// anything else passes through untouched.
fn rel(root: &str, path: &str) -> String {
    match path.strip_prefix(root) {
        Some(stripped) => stripped.trim_start_matches('/').to_string(),
        None => path.to_string(),
    }
}

fn edge_row(root: &str, src_path: &str, src_name: Option<&str>, dst_path: &str, dst_name: Option<&str>) -> String {
    format!(
        "{}\t{}\t{}\t{}",
        rel(root, src_path),
        src_name.unwrap_or(""),
        rel(root, dst_path),
        dst_name.unwrap_or(""),
    )
}

pub struct NormalForms {
    pub call: BTreeSet<String>,
    pub type_edges: BTreeSet<String>,
    pub module: BTreeSet<String>,
}

/// `FlatFact` rows to the three tsv families, exactly as normalize.py's
/// `resolved_to_tsv` + `resolved_import_to_module_tsv` fold them: call rows
/// from `resolved_edge`, type rows from `resolved_type_edge`, module rows
/// from `resolved_import` with the names dropped.
pub fn normal_form(root: &Path, facts: &[FlatFact]) -> NormalForms {
    let root = root.to_str().unwrap_or_default();
    let mut forms = NormalForms {
        call: BTreeSet::new(),
        type_edges: BTreeSet::new(),
        module: BTreeSet::new(),
    };
    for fact in facts {
        match fact {
            FlatFact::ResolvedEdge {
                caller_path,
                caller_name,
                callee_path,
                callee_name,
                ..
            } => {
                forms.call.insert(edge_row(
                    root,
                    caller_path,
                    caller_name.as_deref(),
                    callee_path,
                    callee_name.as_deref(),
                ));
            }
            FlatFact::ResolvedTypeEdge {
                owner_path,
                owner_name,
                target_path,
                target_name,
                ..
            } => {
                forms.type_edges.insert(edge_row(
                    root,
                    owner_path,
                    owner_name.as_deref(),
                    target_path,
                    target_name.as_deref(),
                ));
            }
            FlatFact::ResolvedImportRow {
                src_path, target_path, ..
            } => {
                forms
                    .module
                    .insert(format!("{}\t\t{}\t", rel(root, src_path), rel(root, target_path)));
            }
            _ => {}
        }
    }
    forms
}

pub fn family_rows<'a>(forms: &'a NormalForms, family: &str) -> &'a BTreeSet<String> {
    match family {
        "call" => &forms.call,
        "type" => &forms.type_edges,
        "module" => &forms.module,
        other => panic!("unknown oracle family '{other}'"),
    }
}

pub fn load_tsv(path: &Path) -> BTreeSet<String> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    text.lines()
        .filter(|line| !line.is_empty())
        .map(String::from)
        .collect()
}

// ── scoring ─────────────────────────────────────────────────────────────────

pub struct Score {
    pub ours: usize,
    pub oracle: usize,
    pub overlap: usize,
    /// overlap / |oracle|, percent.
    pub recall: f64,
    /// overlap / |ours|, percent.
    pub precision: f64,
}

fn pct(overlap: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        overlap as f64 * 100.0 / total as f64
    }
}

pub fn score(ours: &BTreeSet<String>, oracle: &BTreeSet<String>) -> Score {
    let overlap = ours.intersection(oracle).count();
    Score {
        ours: ours.len(),
        oracle: oracle.len(),
        overlap,
        recall: pct(overlap, oracle.len()),
        precision: pct(overlap, ours.len()),
    }
}

// ── measurement ─────────────────────────────────────────────────────────────

pub struct Measurement {
    pub files: usize,
    /// Median of the 3 in-process runs.
    pub wall_ms: u128,
    /// Process-peak RSS after the runs (getrusage high-water mark).
    pub rss_mb: u64,
    pub forms: NormalForms,
}

fn request<'a>(files: &'a [PathBuf]) -> ResolveRequest<'a> {
    // The diet_scip arms the CLI builds for `--family diet_scip` / `--resolve
    // --family call,type` (parse_arms in src/bin/extract.rs; diet_scip in
    // src/project.rs). ScipMode::Off is what makes the family diet.
    ResolveRequest {
        paths: files,
        arms: ResolveArms {
            call: true,
            types: true,
            flow: false,
        },
        scip: ScipMode::Off,
        project_root: None,
        scip_records: ScipRecords::all(),
        occurrence_text: false,
    }
}

/// 3 in-process runs, median wall, the last run's rows normalized. The rows
/// are a pure function of the file set, so any run's rows are the set; the
/// last one is kept so the earlier copies free before RSS is read.
pub fn measure(corpus: &Corpus) -> Measurement {
    let files = enumerate(corpus);
    assert!(
        files.len() >= 500,
        "ratchet {}: enumerated only {} files under {}; corpus rule broken?",
        corpus.lang,
        files.len(),
        corpus.root.display(),
    );
    println!(
        "ratchet {}: {} files under {}",
        corpus.lang,
        files.len(),
        corpus.root.display()
    );
    let mut walls = Vec::with_capacity(3);
    let mut facts = Vec::new();
    for run in 0..3 {
        let start = Instant::now();
        let out = resolve_project(&request(&files))
            .unwrap_or_else(|err| panic!("ratchet {}: resolve failed: {err}", corpus.lang));
        let wall_ms = start.elapsed().as_millis();
        assert!(
            wall_ms <= WALL_BUDGET_MS,
            "ratchet {} run {}: wall {wall_ms} ms over the {} ms per-call budget",
            corpus.lang,
            run + 1,
            WALL_BUDGET_MS,
        );
        println!("ratchet {}: run {} wall {wall_ms} ms", corpus.lang, run + 1);
        walls.push(wall_ms);
        facts = out;
    }
    walls.sort();
    Measurement {
        files: files.len(),
        wall_ms: walls[1],
        rss_mb: peak_rss_mb(),
        forms: normal_form(&corpus.root, &facts),
    }
}

/// `getrusage` high-water RSS in MB. ru_maxrss is bytes on darwin and
/// kilobytes on linux.
pub fn peak_rss_mb() -> u64 {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    let code = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    assert_eq!(code, 0, "getrusage failed");
    let usage = unsafe { usage.assume_init() };
    #[cfg(target_os = "macos")]
    let bytes = usage.ru_maxrss as f64;
    #[cfg(not(target_os = "macos"))]
    let bytes = usage.ru_maxrss as f64 * 1024.0;
    (bytes / (1024.0 * 1024.0)).round() as u64
}

// ── RATCHET.tsv ─────────────────────────────────────────────────────────────

pub const RATCHET_HEADER: &str = "# extract ratchet: diet_scip (resolve call+types, 3 runs per corpus, median wall / process-peak rss) vs the committed oracle tsvs;\n\
     # recall = overlap/|oracle|, precision = overlap/|ours|, percent; check: 0.10 pt / wall +15% / rss +10% (ceilings at the worst of repeated runs); local-only (COMMON.md), never CI; bump: RATCHET_BUMP=1 improves floors/ceilings (walls/rss only by 10%+ margins), RATCHET_FORCE=1 rewrites.\n\
     lang\tfamily\toracle\trecall\tprecision\twall_ms\trss_mb\tmeasured_at_sha";

pub fn ratchet_path() -> PathBuf {
    Path::new(BENCH_DIR).join("RATCHET.tsv")
}

#[derive(Clone)]
pub struct RatchetRow {
    pub lang: String,
    pub family: String,
    pub oracle: String,
    pub recall: f64,
    pub precision: f64,
    pub wall_ms: u128,
    pub rss_mb: u64,
    pub sha: String,
}

/// The only subprocess in the ratchet: the receipt wants the measured-at sha,
/// and reading it via git is one call outside the measurement path.
pub fn measured_at_sha() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

pub fn read_ratchet() -> Option<Vec<RatchetRow>> {
    let text = std::fs::read_to_string(ratchet_path()).ok()?;
    let mut rows = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') || line.starts_with("lang\t") || line.is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        assert!(
            cols.len() == 8,
            "RATCHET.tsv row has {} columns, expected 8: {line}",
            cols.len()
        );
        rows.push(RatchetRow {
            lang: cols[0].to_string(),
            family: cols[1].to_string(),
            oracle: cols[2].to_string(),
            recall: cols[3].parse().unwrap_or_else(|_| panic!("RATCHET.tsv recall: {line}")),
            precision: cols[4].parse().unwrap_or_else(|_| panic!("RATCHET.tsv precision: {line}")),
            wall_ms: cols[5].parse().unwrap_or_else(|_| panic!("RATCHET.tsv wall_ms: {line}")),
            rss_mb: cols[6].parse().unwrap_or_else(|_| panic!("RATCHET.tsv rss_mb: {line}")),
            sha: cols[7].to_string(),
        });
    }
    Some(rows)
}

pub fn write_ratchet(rows: &[RatchetRow]) {
    let mut text = String::from(RATCHET_HEADER);
    text.push('\n');
    for row in rows {
        text.push_str(&format!(
            "{}\t{}\t{}\t{:.2}\t{:.2}\t{}\t{}\t{}\n",
            row.lang, row.family, row.oracle, row.recall, row.precision, row.wall_ms, row.rss_mb, row.sha
        ));
    }
    std::fs::write(ratchet_path(), text).expect("write RATCHET.tsv");
}

// ── the ratchet itself ──────────────────────────────────────────────────────

pub const RECALL_TOLERANCE_PT: f64 = 0.10;
pub const WALL_TOLERANCE_PCT: f64 = 15.0;
pub const RSS_TOLERANCE_PCT: f64 = 10.0;
/// A bump tightens a wall/rss ceiling only when the measurement is at least
/// this far below it, outside the run-to-run band.
pub const CEILING_TIGHTEN_MARGIN: f64 = 0.90;

/// One corpus: measure, print the table, then check (default) or bump
/// (`RATCHET_BUMP=1`). `RATCHET_FORCE=1` alongside bump rewrites every
/// measured row regardless of direction.
pub fn ratchet(lang: &str) {
    let corpus = corpus(lang);
    if !corpus.root.is_dir() {
        println!(
            "ratchet {}: absent (corpus root {} missing), skipped",
            corpus.lang,
            corpus.root.display()
        );
        return;
    }
    let measurement = measure(&corpus);
    let sha = measured_at_sha();
    let mut floors = read_ratchet().unwrap_or_default();
    let bump = std::env::var("RATCHET_BUMP").is_ok();
    let force = std::env::var("RATCHET_FORCE").is_ok();
    if floors.is_empty() && !bump {
        panic!(
            "ratchet: {} has no RATCHET.tsv rows; run once with RATCHET_BUMP=1 to plant the floors",
            corpus.lang
        );
    }

    println!(
        "\n{:<6} {:<8} {:<32} {:>7} {:>7} {:>7} {:>8} {:>9} {:>8} {:>7} verdict",
        "lang", "family", "oracle", "ours", "oracle", "overlap", "recall", "precision", "wall_ms", "rss_mb"
    );
    let mut failures = Vec::new();
    let mut improved = 0usize;
    let mut unchanged = 0usize;
    for (family, oracle_file) in corpus.oracles {
        let oracle_path = Path::new(BENCH_DIR).join(oracle_file);
        let row_key = |rows: &[RatchetRow]| {
            rows.iter()
                .position(|row| row.lang == corpus.lang && row.family == *family && row.oracle == *oracle_file)
        };
        if !oracle_path.is_file() {
            println!(
                "{:<6} {:<8} {:<32} absent ({} missing), skipped",
                corpus.lang, family, oracle_file, oracle_path.display()
            );
            continue;
        }
        let oracle_rows = load_tsv(&oracle_path);
        let ours = family_rows(&measurement.forms, family);
        let verdict = score(ours, &oracle_rows);
        let floor = row_key(&floors).map(|index| floors[index].clone());
        let mut line_verdict = String::from("no-floor");
        if let Some(floor) = &floor {
            if verdict.recall < floor.recall - RECALL_TOLERANCE_PT {
                failures.push(format!(
                    "ratchet {} {} {}: recall {:.2} below floor {:.2} by {:.2} pt (tolerance {RECALL_TOLERANCE_PT})",
                    corpus.lang,
                    family,
                    oracle_file,
                    verdict.recall,
                    floor.recall,
                    floor.recall - verdict.recall
                ));
            }
            if verdict.precision < floor.precision - RECALL_TOLERANCE_PT {
                failures.push(format!(
                    "ratchet {} {} {}: precision {:.2} below floor {:.2} by {:.2} pt (tolerance {RECALL_TOLERANCE_PT})",
                    corpus.lang,
                    family,
                    oracle_file,
                    verdict.precision,
                    floor.precision,
                    floor.precision - verdict.precision
                ));
            }
            if measurement.wall_ms as f64 > floor.wall_ms as f64 * (1.0 + WALL_TOLERANCE_PCT / 100.0) {
                failures.push(format!(
                    "ratchet {} {} {}: wall {} ms above ceiling {} ms by {:.1}% (tolerance {WALL_TOLERANCE_PCT}%)",
                    corpus.lang,
                    family,
                    oracle_file,
                    measurement.wall_ms,
                    floor.wall_ms,
                    (measurement.wall_ms as f64 / floor.wall_ms as f64 - 1.0) * 100.0
                ));
            }
            if measurement.rss_mb as f64 > floor.rss_mb as f64 * (1.0 + RSS_TOLERANCE_PCT / 100.0) {
                failures.push(format!(
                    "ratchet {} {} {}: rss {} MB above ceiling {} MB by {:.1}% (tolerance {RSS_TOLERANCE_PCT}%)",
                    corpus.lang,
                    family,
                    oracle_file,
                    measurement.rss_mb,
                    floor.rss_mb,
                    (measurement.rss_mb as f64 / floor.rss_mb as f64 - 1.0) * 100.0
                ));
            }
            line_verdict = if failures.is_empty() { "ok" } else { "FAIL" }.to_string();
        }
        println!(
            "{:<6} {:<8} {:<32} {:>7} {:>7} {:>7} {:>8.2} {:>9.2} {:>8} {:>7} {}",
            corpus.lang,
            family,
            oracle_file,
            verdict.ours,
            verdict.oracle,
            verdict.overlap,
            verdict.recall,
            verdict.precision,
            measurement.wall_ms,
            measurement.rss_mb,
            line_verdict
        );

        if bump {
            let measured = RatchetRow {
                lang: corpus.lang.to_string(),
                family: family.to_string(),
                oracle: oracle_file.to_string(),
                recall: verdict.recall,
                precision: verdict.precision,
                wall_ms: measurement.wall_ms,
                rss_mb: measurement.rss_mb,
                sha: sha.clone(),
            };
            match row_key(&floors) {
                Some(index) => {
                    let floor = &floors[index];
                    // Wall and rss swing run to run (go rss 750-833 MB at one
                    // sha), so a bump tightens their ceilings only outside
                    // that noise band; otherwise every lucky run would drag
                    // the ceiling onto the optimistic end and the next normal
                    // run would go red. Recall/precision are stable to 0.01
                    // pt and move on any improvement.
                    let better = measured.recall > floor.recall
                        || measured.precision > floor.precision
                        || (measured.wall_ms as f64) < (floor.wall_ms as f64) * CEILING_TIGHTEN_MARGIN
                        || (measured.rss_mb as f64) < (floor.rss_mb as f64) * CEILING_TIGHTEN_MARGIN;
                    let worse = measured.recall < floor.recall
                        || measured.precision < floor.precision
                        || measured.wall_ms > floor.wall_ms
                        || measured.rss_mb > floor.rss_mb;
                    if better || (force && worse) {
                        println!(
                            "bump {} {} {}: recall {:.2}->{:.2} precision {:.2}->{:.2} wall {}->{} rss {}->{} ({})",
                            corpus.lang,
                            family,
                            oracle_file,
                            floor.recall, measured.recall,
                            floor.precision, measured.precision,
                            floor.wall_ms, measured.wall_ms,
                            floor.rss_mb, measured.rss_mb,
                            if force && worse { "forced" } else { "improved" }
                        );
                        floors[index] = measured;
                        improved += 1;
                    } else {
                        unchanged += 1;
                    }
                }
                None => {
                    println!(
                        "bump {} {} {}: new row (recall {:.2}, precision {:.2}, wall {}, rss {})",
                        corpus.lang, family, oracle_file, measured.recall, measured.precision, measured.wall_ms, measured.rss_mb
                    );
                    floors.push(measured);
                    improved += 1;
                }
            }
        }
    }
    if bump {
        floors.sort_by(|a, b| (&a.lang, &a.family, &a.oracle).cmp(&(&b.lang, &b.family, &b.oracle)));
        write_ratchet(&floors);
        println!(
            "ratchet {}: wrote {} ({improved} rows moved, {unchanged} held)",
            corpus.lang,
            ratchet_path().display()
        );
        return;
    }
    if !failures.is_empty() {
        for failure in &failures {
            println!("{failure}");
        }
        panic!(
            "ratchet {}: {} row(s) regressed against RATCHET.tsv",
            corpus.lang,
            failures.len()
        );
    }
    println!("ratchet {}: all rows hold", corpus.lang);
}
