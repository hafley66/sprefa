//! Shared SCIP call-resolution parity scorer for the differential oracles
//! (`oracle_rust.rs`, `oracle_ts.rs`, `oracle_kotlin_parity.rs`). Factored out
//! of `oracle_rust.rs`'s `call_resolution_parity_vs_rust_analyzer` (the
//! contract that test's doc comment states, preserved here verbatim):
//!
//! - ground truth = per (file, line) reference occurrences whose source slice
//!   matches the callee name, with an unambiguous in-repo def file;
//! - a call site counts toward the parity percent ONLY when our index-free
//!   pick equals SCIP's def file (a confirmed positive);
//! - a resolution SCIP can't confirm is EXCLUDED, never counted as a win;
//! - a resolution SCIP contradicts is a `wrong`, tracked separately and
//!   bounded by a `precision >= 0.95` assert in each caller, so the resolver
//!   can't buy coverage with bad joins;
//! - every fuzzy step (range slicing, name matching, multi-line ranges, col
//!   drift, an unreadable file) fails toward EXCLUSION — the percent may
//!   under-report, it must never inflate.
//!
//! Language-agnostic: `call_site`/`call_edge` are generic builtin rels (every
//! `TypeLang` front-end shares `mint_sym`'s `file::kind::name` shape), so one
//! scorer serves Rust/rust-analyzer, TypeScript/scip-typescript, and
//! Kotlin/scip-java. Each caller supplies its own SCIP index, corpus root,
//! and a `source_prefix` that scopes both the ground truth and the dl run's
//! call sites to the fixture/crate under test.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use scip::types::{Index, SymbolRole};

/// Copy a directory tree (test fixtures run out of a scratch copy, never the
/// checked-in fixture itself, so a `--fix`/index run can't dirty the repo).
pub(crate) fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap().flatten() {
        let p = e.path();
        let d = dst.join(e.file_name());
        if p.is_dir() { copy_dir(&p, &d); } else { std::fs::copy(&p, &d).unwrap(); }
    }
}

/// Scoring result. `denom()`/`parity()`/`precision()` compute the same
/// fractions `call_resolution_parity_vs_rust_analyzer` printed inline.
#[derive(Debug, Default)]
pub(crate) struct ParityStats {
    /// Call sites extracted under `source_prefix`, before any ground-truth
    /// filtering (an extraction failure shows up as zero here, distinct from
    /// "extracted fine but nothing was RA-confirmable").
    pub total_sites: usize,
    pub confirmed: usize,
    pub wrong: usize,
    pub bare: usize,
    pub multi: usize,
    pub wrong_examples: Vec<String>,
}

impl ParityStats {
    pub fn denom(&self) -> usize { self.confirmed + self.wrong + self.bare }
    /// Confirmed-positives-only percent: can only be driven down by fuzzy
    /// exclusions, never up.
    pub fn parity(&self) -> f64 {
        let d = self.denom();
        if d == 0 { 0.0 } else { self.confirmed as f64 / d as f64 }
    }
    pub fn precision(&self) -> f64 {
        let scored = self.confirmed + self.wrong;
        if scored == 0 { 1.0 } else { self.confirmed as f64 / scored as f64 }
    }
}

/// def_file per non-local symbol: the file of its first-seen Definition
/// occurrence in the index (in-repo defs only; `local `-prefixed symbols are
/// per-occurrence scratch, never callable across files).
fn def_files(index: &Index) -> HashMap<String, String> {
    let mut def_file: HashMap<String, String> = HashMap::new();
    for doc in &index.documents {
        for occ in &doc.occurrences {
            if occ.symbol_roles & (SymbolRole::Definition as i32) != 0 && !occ.symbol.starts_with("local ") {
                def_file.entry(occ.symbol.clone()).or_insert_with(|| doc.relative_path.clone());
            }
        }
    }
    def_file
}

/// Ground truth: (file, 0-based line) -> [(source slice, def_file)] for every
/// reference occurrence under `source_prefix` whose symbol has an in-repo
/// definition somewhere in the index. Every fuzzy step here — a multi-line
/// range, a non-ASCII column drift, a doc whose file isn't readable at
/// `root` — EXCLUDES the occurrence rather than guessing at it.
pub(crate) fn refs_at(index: &Index, root: &Path, source_prefix: &str)
    -> HashMap<(String, u32), Vec<(String, String)>>
{
    let def_file = def_files(index);
    let mut refs_at: HashMap<(String, u32), Vec<(String, String)>> = HashMap::new();
    for doc in &index.documents {
        if !doc.relative_path.starts_with(source_prefix) { continue; }
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
    refs_at
}

/// `call_site`/`call_edge` syms are "repo::path::kind::name" (`mint_sym` is
/// shared across every `TypeLang`); reduce to (path, name).
fn sym_parts(sym: &str) -> Option<(String, String)> {
    let mut it = sym.rsplitn(3, "::");
    let name = it.next()?;
    let _kind = it.next()?;
    let repo_path = it.next()?;
    let path = repo_path.split_once("::").map(|(_, p)| p).unwrap_or(repo_path);
    Some((path.to_string(), name.to_string()))
}

/// Parse the `? call_site(repo, caller, callee, file, line)` and
/// `? call_edge(caller, callee, kind)` query blocks out of `dl`'s stdout: the
/// index-free resolver's attempts (sites) and picks. `source_prefix` scopes
/// call sites to the corpus under test the same way it scopes the ground
/// truth (a `--db`-backed run may also see vendored/std files no oracle
/// covers).
pub(crate) fn parse_call_sections(stdout: &str, source_prefix: &str)
    -> (Vec<(String, String, u32)>, HashMap<(String, String), HashSet<String>>)
{
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
                    if cells[3].starts_with(source_prefix) {
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
    (sites, picks)
}

/// Score `sites` against `picks` and the `truth` ground truth. A site enters
/// the denominator only when the ground truth slices exactly one def file
/// for the callee's last name segment at that (file, line); an ambiguous
/// truth, an unresolved pick, or a multi-candidate pick is EXCLUDED (never a
/// win, never a loss). Method-name picks are keyed both bare and
/// `Type.name`; both are tried, matching the `call_site`/`call_edge` sym
/// convention.
pub(crate) fn score(
    sites: &[(String, String, u32)],
    picks: &HashMap<(String, String), HashSet<String>>,
    truth: &HashMap<(String, u32), Vec<(String, String)>>,
) -> ParityStats {
    let (mut confirmed, mut wrong, mut bare, mut multi) = (0usize, 0usize, 0usize, 0usize);
    let mut wrong_examples: Vec<String> = Vec::new();
    for (file, callee, line1) in sites {
        let needle = callee.rsplit('.').next().unwrap_or(callee);
        let Some(cands) = truth.get(&(file.clone(), line1.saturating_sub(1))) else { continue; };
        let truths: HashSet<&String> = cands.iter()
            .filter(|(slice, _)| slice == needle)
            .map(|(_, def)| def).collect();
        if truths.len() != 1 { continue; } // no truth or ambiguous truth: exclude
        let truth_file = *truths.iter().next().unwrap();
        let ours = picks.get(&(file.clone(), callee.clone()))
            .or_else(|| picks.get(&(file.clone(), needle.to_string())));
        match ours {
            None => bare += 1,
            Some(set) if set.len() > 1 => multi += 1, // ambiguous pick: excluded, reported
            Some(set) => {
                let pick = set.iter().next().unwrap();
                if pick == truth_file { confirmed += 1; }
                else {
                    wrong += 1;
                    if wrong_examples.len() < 10 {
                        wrong_examples.push(format!("{file}:{line1} {callee}: ours={pick} truth={truth_file}"));
                    }
                }
            }
        }
    }
    ParityStats { total_sites: sites.len(), confirmed, wrong, bare, multi, wrong_examples }
}

/// Convenience: ground truth + section parse + score in one call, for a `dl`
/// run whose stdout carries `? call_site`/`? call_edge` query blocks.
pub(crate) fn score_parity(index: &Index, root: &Path, source_prefix: &str, dl_stdout: &str) -> ParityStats {
    let truth = refs_at(index, root, source_prefix);
    let (sites, picks) = parse_call_sections(dl_stdout, source_prefix);
    score(&sites, &picks, &truth)
}
