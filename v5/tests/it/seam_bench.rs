//! Cross-seam scoring rig. The apparatus for the open question: "are these
//! discrete joins actually useful for AI-session / cross-repo change-awareness,
//! and would embeddings add anything?" It plants a tiny polyglot corpus
//! (`bench/seams/corpus/`) with KNOWN cross-seam answers, reconstructs a git
//! history with per-subtree authors (`bench/seams/authors.tsv`), runs each
//! checked-in `.dl` seam query via `--query-json`, and scores the returned set
//! against the checked-in gold (`bench/seams/gold.json`) as precision/recall/F1.
//!
//! Why a rig and not just asserts: the gold + scoring make it an A/B bench. A
//! discrete seam query gets a number now; drop an embedding-augmented `.dl`
//! (e.g. a future `graph_similar` seed step) scored against the SAME gold, and
//! the delta tells you whether embeddings help — empirically, not by vibe. New
//! seam = one `.dl` + one `gold.json` entry; "AI authors the stitch" = a new
//! `.dl` the rig scores.
//!
//! Seams shipped:
//!   xlang-siblings  code<->code  one name across Rust/TS/Kotlin (type graph)
//!   doc-drift       doc->code    headings resolving to no symbol (anti-join)
//!   authorship      git          who created each file (`created` built-in)
//!   impact          integrator   one symbol's blast across xlang + doc seams

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");
const SEAMS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/bench/seams");

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().expect("git");
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

/// Recursively copy `src` tree into `dst` (dirs created as needed).
fn copy_tree(src: &Path, dst: &Path) {
    for e in fs::read_dir(src).unwrap().flatten() {
        let from = e.path();
        let to = dst.join(e.file_name());
        if from.is_dir() {
            fs::create_dir_all(&to).unwrap();
            copy_tree(&from, &to);
        } else {
            fs::create_dir_all(to.parent().unwrap()).unwrap();
            fs::copy(&from, &to).unwrap();
        }
    }
}

/// Build the scored repo: copy the corpus, init git, and commit each authored
/// subtree as its own author so `created(path,_,email,_)` recovers the manifest.
fn build_repo() -> PathBuf {
    let dir = std::env::temp_dir().join("seam_bench_repo");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    copy_tree(&PathBuf::from(SEAMS).join("corpus"), &dir);
    git(&dir, &["init", "-q"]);

    let manifest = fs::read_to_string(PathBuf::from(SEAMS).join("authors.tsv")).unwrap();
    for line in manifest.lines().filter(|l| !l.trim().is_empty()) {
        let mut f = line.split('\t');
        let (prefix, name, email) =
            (f.next().unwrap(), f.next().unwrap(), f.next().unwrap());
        git(&dir, &["add", "--", prefix]);
        git(&dir, &["-c", &format!("user.name={name}"), "-c", &format!("user.email={email}"),
                    "commit", "-q", "-m", &format!("add {prefix}")]);
    }
    fs::canonicalize(&dir).unwrap()
}

/// Run one seam `.dl` and return the JSON-line record for `query`.
fn run_seam(repo: &Path, seam: &str, query: &str) -> serde_json::Value {
    let prog = PathBuf::from(SEAMS).join(format!("{seam}.dl"));
    let db = std::env::temp_dir().join(format!("seam_bench_{seam}.db"));
    let _ = fs::remove_file(&db);
    let out = Command::new(DL).arg(&prog)
        .args(["--root", repo.to_str().unwrap(), "--db", db.to_str().unwrap(), "--query-json"])
        .output().expect("run dl");
    assert!(out.status.success(),
        "{seam}: dl failed\n{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<serde_json::Value>(l).expect("json line"))
        .find(|r| r["query"] == query)
        .unwrap_or_else(|| panic!("{seam}: no `? {query}` record in output"))
}

/// Project a record's rows down to its `key` columns, as a set of joined cells.
fn project(rec: &serde_json::Value, key: &[String]) -> BTreeSet<String> {
    let cols: Vec<String> = rec["columns"].as_array().unwrap()
        .iter().map(|c| c.as_str().unwrap().to_string()).collect();
    let idx: Vec<usize> = key.iter()
        .map(|k| cols.iter().position(|c| c == k)
            .unwrap_or_else(|| panic!("key col `{k}` not in {cols:?}")))
        .collect();
    rec["rows"].as_array().unwrap().iter().map(|row| {
        let r = row.as_array().unwrap();
        idx.iter().map(|&i| match &r[i] {
            serde_json::Value::String(s) => s.clone(),
            v => v.to_string(),
        }).collect::<Vec<_>>().join("\u{1f}")
    }).collect()
}

fn gold_set(entry: &serde_json::Value) -> BTreeSet<String> {
    entry["gold"].as_array().unwrap().iter().map(|row| {
        row.as_array().unwrap().iter().map(|c| match c {
            serde_json::Value::String(s) => s.clone(),
            v => v.to_string(),
        }).collect::<Vec<_>>().join("\u{1f}")
    }).collect()
}

/// Run every seam in gold.json, score it, print a report, and require that the
/// discrete queries are EXACT on the planted data (P == R == 1.0). A miss is a
/// real measurement: it means an extractor dropped a planted fact, not a flaky
/// test — fix the corpus/query, don't loosen the bar.
#[test]
fn seams_score_exact_on_planted_corpus() {
    let repo = build_repo();
    let gold: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(PathBuf::from(SEAMS).join("gold.json")).unwrap()).unwrap();

    eprintln!("\n  seam              P     R    F1   pred/gold");
    eprintln!("  ----------------------------------------------");
    let mut failures = Vec::new();
    for (seam, entry) in gold.as_object().unwrap() {
        let query = entry["query"].as_str().unwrap();
        let key: Vec<String> = entry["key"].as_array().unwrap()
            .iter().map(|c| c.as_str().unwrap().to_string()).collect();
        let rec = run_seam(&repo, seam, query);
        let pred = project(&rec, &key);
        let gset = gold_set(entry);
        let hit = pred.intersection(&gset).count() as f64;
        let p = if pred.is_empty() { 0.0 } else { hit / pred.len() as f64 };
        let r = if gset.is_empty() { 0.0 } else { hit / gset.len() as f64 };
        let f1 = if p + r == 0.0 { 0.0 } else { 2.0 * p * r / (p + r) };
        eprintln!("  {seam:<16} {p:.2}  {r:.2}  {f1:.2}   {}/{}", pred.len(), gset.len());
        if (p - 1.0).abs() > 1e-9 || (r - 1.0).abs() > 1e-9 {
            failures.push(format!(
                "{seam}: P={p:.2} R={r:.2}\n    pred {pred:?}\n    gold {gset:?}"));
        }
    }
    eprintln!();
    assert!(failures.is_empty(), "seam(s) not exact:\n  {}", failures.join("\n  "));
}
