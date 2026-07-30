//! Win D import-scoped SCIP ambiguity narrowing, relocated from
//! `extract/mod.rs` (decomposition plan step 8,
//! plans/2026-07-18-decomposition-normalization.md): `narrow_ambiguous`
//! plus the `ScipOccIndex` occurrence index it and the extract paths key by.

use std::collections::HashMap;

use super::*;

/// Directory portion of a `/`-separated relative path (empty string for a
/// bare filename, never a trailing slash). Used only by `narrow_ambiguous`'s
/// same-directory criterion.
pub(crate) fn path_dir(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some((dir, _)) => dir,
        None => "",
    }
}

/// Win D, import-scoped ambiguity narrowing. `resolve`/`resolve_callee` in
/// `refresh_type_rels`/`refresh_call_rels` call this only when a name's
/// def bucket already has more than one candidate symbol (the plain
/// unique-in-repo path stays untouched). A candidate survives the filter when
/// its declaring file is:
///   (a) the referencing file itself,
///   (b) directly imported by the referencing file (a `module_edge_rev` row
///       at this rev), or
///   (c) in the same directory as the referencing file.
/// Exactly one survivor resolves the ambiguity only when it survived via (a)
/// or (b) ("strong" reasons); a survivor that only matches via (c) stays
/// bare. Directory co-location is a weak signal (sibling files that never
/// import each other are common), so a same-directory-ONLY tie is honest
/// ambiguity, not a resolution: a wrong join is worse than a missing one.
/// More than one survivor (still genuinely ambiguous after narrowing) or zero
/// survivors also stay bare.
pub(crate) fn narrow_ambiguous<'a>(
    candidates: &[&'a str],
    repo: &str,
    rev: &str,
    referencing_file: &str,
    sym_file: &HashMap<(&str, &str, &str), &str>,
    imports: &HashMap<(String, String), HashSet<String>>,
) -> Option<&'a str> {
    let referencing_dir = path_dir(referencing_file);
    let imported = imports.get(&(rev.to_string(), referencing_file.to_string()));
    let mut survivor: Option<(&'a str, bool)> = None;
    let mut survivor_count = 0u32;
    for sym in candidates {
        let Some(def_file) = sym_file.get(&(repo, rev, *sym)) else {
            continue;
        };
        let is_self = *def_file == referencing_file;
        let is_imported = imported.map(|set| set.contains(*def_file)).unwrap_or(false);
        let is_same_dir = path_dir(def_file) == referencing_dir;
        if is_self || is_imported || is_same_dir {
            survivor_count += 1;
            survivor = Some((sym, is_self || is_imported));
        }
    }
    match (survivor_count, survivor) {
        (1, Some((sym, true))) => Some(sym),
        _ => None,
    }
}

/// Occurrence-level SCIP resolution index (position-before-name). The name-level
/// override (`scip_name_defs`) keys a def only by (repo, file, bare descriptor
/// name), so a name carried by two DIFFERENT def symbols in one file is dropped
/// (the conflict refusal, commit 9fd029b) and every shared name (`build`/`new`/
/// `shutdown` — most of real trait-heavy code) resolves bare even though the
/// index holds the exact symbol at every span. `scip_occurrence` carries a
/// 0-based per-occurrence line for each symbol; joined to the def location, a
/// call site's (file, line) picks the ONE symbol occurring there, disambiguating
/// what the bare name cannot — the conflict refusal becomes moot wherever a
/// position exists.
///
/// Built once per call-family refresh (collect-then-index, no per-site SQL —
/// same posture as `scip_name_defs`). Empty when no index is loaded, so the
/// name path carries unchanged. Repo-scoped throughout (cross-repo SCIP
/// resolution was deliberately removed, the D3 fix); occurrences are consulted
/// only at rev == "WORK" by the caller, since a SCIP index is a working-tree
/// artifact.
#[derive(Default)]
pub(crate) struct ScipOccIndex {
    /// (repo, file, 0-based line) -> the symbols occurring on that line.
    pub(crate) occ_at: HashMap<(String, String, i64), Vec<String>>,
    /// (repo, symbol) -> its definition file (from `scip_def`, the authoritative
    /// def location the resolver joins into `sym_at`, exactly like the name
    /// map's `def_file`).
    pub(crate) def_file_of: HashMap<(String, String), String>,
    /// symbol -> trailing descriptor name (the as-written call text a plain or
    /// method call carries). Cached so a lookup never recomputes the moniker
    /// parse.
    pub(crate) desc_name: HashMap<String, String>,
    /// (repo, file, symbol) -> the LOCAL binding names an aliased import gives
    /// the symbol in that file (`import { a as b }` -> {"b"}), from
    /// `scip_binding`. A call written with the alias matches here even though the
    /// descriptor name is the canonical `a`.
    pub(crate) binding_names: HashMap<(String, String, String), HashSet<String>>,
}

/// The outcome of an occurrence-level lookup at one call site.
pub(crate) enum OccPick {
    /// Exactly one symbol occurs at this (file, line) under the call's
    /// as-written name: resolved to its def file.
    Resolved(String),
    /// More than one DISTINCT symbol shares the call's name on this line: the
    /// position can't tell them apart, so refuse (honest bare). Never falls
    /// through to the name map — that would let a coincidental single-def name
    /// resolve a site the position just refuted.
    Refuse,
    /// No occurrence on this line carries the call's name (an unindexed file, or
    /// a site the compiler never recorded): defer to the name-level map.
    Fallthrough,
}

impl ScipOccIndex {
    /// True when `callee` (the as-written call text) addresses `symbol` in
    /// `file`: it equals the symbol's descriptor name, or a local alias the file
    /// binds it to.
    pub(crate) fn names_match(&self, repo: &str, file: &str, symbol: &str, callee: &str) -> bool {
        if self.desc_name.get(symbol).map(String::as_str) == Some(callee) {
            return true;
        }
        self.binding_names
            .get(&(repo.to_string(), file.to_string(), symbol.to_string()))
            .is_some_and(|set| set.contains(callee))
    }

    /// Resolve a call site by position. `line1` is the call site's 1-based line
    /// (`call_site` lines are 1-based across all fronts); the SINGLE conversion
    /// to SCIP's 0-based occurrence line happens right here.
    pub(crate) fn resolve(&self, repo: &str, file: &str, callee: &str, line1: u32) -> OccPick {
        let line0 = line1 as i64 - 1; // 1-based call site -> 0-based scip occurrence.
        let Some(syms) = self
            .occ_at
            .get(&(repo.to_string(), file.to_string(), line0))
        else {
            return OccPick::Fallthrough;
        };
        let mut matched: HashSet<&str> = HashSet::new();
        for sym in syms {
            if self.names_match(repo, file, sym, callee) {
                matched.insert(sym.as_str());
            }
        }
        match matched.len() {
            0 => OccPick::Fallthrough,
            1 => {
                let sym = *matched.iter().next().unwrap();
                match self.def_file_of.get(&(repo.to_string(), sym.to_string())) {
                    Some(def) => OccPick::Resolved(def.clone()),
                    // The one matching symbol has no in-index def (its definition
                    // is outside the indexed set): the name map can't do better,
                    // so defer rather than refuse.
                    None => OccPick::Fallthrough,
                }
            }
            _ => OccPick::Refuse,
        }
    }
}
