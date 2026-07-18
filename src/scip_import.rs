use anyhow::Result;
use protobuf::Message;
use scip::types::{Index, SymbolRole};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Default, Debug)]
pub struct ScipRows {
    /// `(symbol, file, repo)` — the `repo` is the origin index's repo id, so two
    /// roots of the same crate that emit identical (symbol, file) pairs stay
    /// distinct rows instead of collapsing on merge.
    pub defs: Vec<(String, String, String)>,
    /// `(file, symbol, def_file, repo)` — `repo` scopes resolution to the ref
    /// site's origin index (a head ref never resolves to a base def).
    pub refs: Vec<(String, String, String, String)>,
    /// `(src, dst, repo)`.
    pub edges: Vec<(String, String, String)>,
    /// Function-level call edges (caller_fn moniker, callee moniker). The caller
    /// is the innermost enclosing fn def of the reference's range in the same
    /// file; the callee is the referenced symbol (already resolved by RA to its
    /// def moniker). The 100%-recall version of sprefa's heuristic `call_edge`.
    pub fn_edges: Vec<(String, String)>,
    /// Receiver type for each method symbol (method moniker → type name).
    /// Extracted from the `impl#[Type]` or `for#[Type]` segment of RA monikers.
    /// Doubles as a caller's OWN type (callers are method monikers too), so dl
    /// can ask "fn calls methods on T but is not itself on T" = feature envy.
    pub callee_types: Vec<(String, String)>,
    /// Local-variable declarations attributed to their enclosing fn.
    /// (caller_fn moniker, local_name). Locals include both `let` bindings and
    /// function parameters; RA emits both with the `local ` prefix. Shadowing
    /// disambiguators (`foo#1`) are stripped. Drives the missing-type /
    /// context-object / param-fan-out recipes.
    pub locals: Vec<(String, String)>,
    /// Per-occurrence spans `(file, line0, col0, symbol)`, 0-based, every
    /// occurrence (def + ref, local + global). The CST⨝symbol join: lets a
    /// detector normalize each identifier to its resolved symbol (or `ID` for
    /// opaque locals) instead of erasing all names uniformly — the basis of the
    /// symbol/type-shape clone kernel.
    pub occ_spans: Vec<(String, i32, i32, String)>,
    /// Full per-occurrence rows for the `scip_occurrence` relation:
    /// `(file, symbol, start_line, start_col, end_line, end_col, role, repo)`.
    /// 0-based line/col (raw SCIP); `role` ∈ {definition, reference} from the
    /// `symbol_roles` bitmask; `repo` is the document's origin repo id (same
    /// keying as `defs`/`refs`). Records EVERY occurrence, incl. locals, so a
    /// detector sees the same breadth `occ_spans` does — this is the S1 fix
    /// (`scip_ref` had no line/column). Kept separate from `occ_spans` so the
    /// in-process clone kernel (`propose.rs`) keeps its narrow tuple shape.
    pub occurrences: Vec<(String, String, i32, i32, i32, i32, String, String)>,
    /// Interface/supertype dispatch edges from SCIP
    /// `SymbolInformation.relationships` (the `is_implementation` flag). Each row
    /// is (impl_sym, iface_sym): the implementing/overriding symbol declares a
    /// relationship to the symbol it implements — a Kotlin/TS interface method →
    /// its concrete override, a class → its supertype. Occurrences alone don't
    /// carry the virtual-dispatch hop, so this is the only place the
    /// interface→impl path lives. (Per the SCIP doc example, `Dog#sound()` has an
    /// `is_implementation` relationship to `Animal#sound()`, so the row is
    /// (`Dog#sound()`, `Animal#sound()`) = (impl, iface).)
    pub impls: Vec<(String, String)>,
}

/// Where the importer looks for a SCIP index:
///   1. `$SPREFA_SCIP_INDEX` (explicit override)
///   2. the NEWEST (mtime) existing candidate among
///      `<root>/index.scip` (the `just oracle-index` / rust-analyzer default),
///      `<root>/.dl/.state/index.scip` (current `dl index` output location),
///      `<root>/.dl/index.scip` (pre-`.state/` location).
/// Newest-wins, not a fixed location order: a fixed order silently shadows a
/// fresh re-index at any lower-priority path (a Jul-15 `.dl/.state/index.scip`
/// served every load for two days while a fresh root `index.scip` sat ignored
/// — the arch-flow staleness rail caught it). Whichever tool wrote an index
/// most recently is the index the user means.
pub fn index_path(root: &Path) -> Option<PathBuf> {
    if let Ok(path) = std::env::var("SPREFA_SCIP_INDEX") {
        let path = PathBuf::from(path);
        if path.is_file() { return Some(path); }
    }
    [
        root.join("index.scip"),
        crate::state_dir(root).join("index.scip"),
        root.join(".dl").join("index.scip"),
    ]
    .into_iter()
    .filter(|cand| cand.is_file())
    .max_by_key(|cand| {
        std::fs::metadata(cand)
            .and_then(|md| md.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    })
}

/// Load one index and tag every def/ref/edge row with the repo id of its
/// origin. `root` is the on-disk root the index was produced under, `slug` the
/// fallback repo name when a document's file has no ancestor `.git`. The repo id
/// is derived identically to the extractor's `repo_id_of` (nearest-`.git`
/// basename) so the tagged rows join the resolver's per-repo keys.
pub fn load(path: &Path, root: &Path, slug: &str) -> Result<ScipRows> {
    let bytes = std::fs::read(path)?;
    let index = Index::parse_from_bytes(&bytes)?;
    Ok(rows(&index, root, slug))
}

/// The repo id a document answers to: the basename of the nearest ancestor
/// `.git` of `root/relpath`, falling back to `slug`. Mirrors engine
/// `repo_id_of` so scip_* rows share the resolver's repo keying.
fn repo_of(root: &Path, relpath: &str, slug: &str) -> String {
    crate::repo::nearest_git(&root.join(relpath))
        .and_then(|g| g.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| slug.to_string())
}

/// Merge several per-language SCIP indexes into one on disk. SCIP is document-
/// keyed, so a union is just concatenating `documents` + `external_symbols`
/// (each indexer already namespaces its symbols by tool/package, so there is no
/// key collision across languages). Used by `dl index` on a polyglot workspace
/// so a single `index.scip` co-loads every language's facts. `metadata` is
/// carried from the first input.
pub fn merge_files(inputs: &[PathBuf], out: &Path) -> Result<usize> {
    let mut merged: Option<Index> = None;
    let mut n_docs = 0usize;
    for path in inputs {
        let bytes = std::fs::read(path)?;
        let idx = Index::parse_from_bytes(&bytes)?;
        n_docs += idx.documents.len();
        match &mut merged {
            None => merged = Some(idx),
            Some(m) => {
                m.documents.extend(idx.documents);
                m.external_symbols.extend(idx.external_symbols);
            }
        }
    }
    let merged = merged.unwrap_or_default();
    let bytes = merged.write_to_bytes()?;
    std::fs::write(out, bytes)?;
    Ok(n_docs)
}

pub fn rows(index: &Index, root: &Path, slug: &str) -> ScipRows {
    // Per-document repo id (nearest-`.git` basename), computed once per path so
    // every def/ref/edge from that document carries its origin repo. Because
    // `rows` runs per-input index (never over a blind pre-merge), `def_file` is
    // scoped to this index's documents — the cross-root symbol collapse is gone.
    let mut repo_of_doc: HashMap<&str, String> = HashMap::new();
    for doc in &index.documents {
        repo_of_doc.entry(doc.relative_path.as_str())
            .or_insert_with(|| repo_of(root, &doc.relative_path, slug));
    }
    let doc_repo = |path: &str| -> String {
        repo_of_doc.get(path).cloned().unwrap_or_else(|| slug.to_string())
    };
    let mut def_file: HashMap<String, String> = HashMap::new();
    let mut defs: HashSet<(String, String, String)> = HashSet::new();
    // Per-file fn-def intervals for caller attribution: ((start), (end), symbol).
    // A def qualifies as fn-like when its terminal descriptor is a method/
    // function call descriptor, which every indexer (RA, scip-typescript,
    // scip-go, scip-python) ends with `().`: `…/fn_name().`,
    // `…/impl#[Type]method().`. Types, modules, and impl-block holders carry no
    // such descriptor. Crucially, a PARAMETER descriptor (`…/getPet().(id)`)
    // also contains '(' but ends with `)` not `).`; the old `contains('(')`
    // test mis-registered params as enclosing fns, so a body call would be
    // attributed to the nearest parameter instead of the method (scip-python
    // emits params, RA/scip-go inline them, so this only bit the Python tier).
    let mut fn_defs: HashMap<String, Vec<((i32, i32), (i32, i32), String)>> = HashMap::new();
    // (method moniker, receiver type) for every method def. Covers BOTH callees
    // and callers (a caller that is itself a method has a receiver type here),
    // so dl can read a fn's own type and its callees' types from one relation.
    let mut callee_types: HashSet<(String, String)> = HashSet::new();

    for doc in &index.documents {
        for occ in &doc.occurrences {
            if !usable_symbol(&occ.symbol) { continue; }
            if is_def(occ.symbol_roles) {
                def_file.entry(occ.symbol.clone()).or_insert_with(|| doc.relative_path.clone());
                defs.insert((occ.symbol.clone(), doc.relative_path.clone(), doc_repo(&doc.relative_path)));
                if let Some(ty) = receiver_type(&occ.symbol) {
                    callee_types.insert((occ.symbol.clone(), ty));
                }
                if is_callable_def(&occ.symbol) {
                    if let Some((s, e)) = parse_range(&occ.range) {
                        fn_defs.entry(doc.relative_path.clone())
                            .or_default()
                            .push((s, e, occ.symbol.clone()));
                    }
                }
            }
        }
    }

    let mut refs: HashSet<(String, String, String, String)> = HashSet::new();
    let mut edges: HashSet<(String, String, String)> = HashSet::new();
    let mut fn_edges: HashSet<(String, String)> = HashSet::new();
    let mut locals: HashSet<(String, String)> = HashSet::new();
    let mut occ_spans: HashSet<(String, i32, i32, String)> = HashSet::new();
    let mut occurrences: HashSet<(String, String, i32, i32, i32, i32, String, String)> =
        HashSet::new();
    let disp_names = display_names(index);
    for doc in &index.documents {
        let fns = fn_defs.get(&doc.relative_path);
        for occ in &doc.occurrences {
            if let Some(((sl, sc), (el, ec))) = parse_range(&occ.range) {
                occ_spans.insert((doc.relative_path.clone(), sl, sc, occ.symbol.clone()));
                if !occ.symbol.is_empty() {
                    occurrences.insert((
                        doc.relative_path.clone(),
                        occ.symbol.clone(),
                        sl, sc, el, ec,
                        role_label(occ.symbol_roles).to_string(),
                        doc_repo(&doc.relative_path),
                    ));
                }
            }
            // Local symbols (params + lets) are filtered from the main path by
            // `usable_symbol`. They get their own collection: a local DEF is the
            // binding site; attribute it to the enclosing fn via the same
            // predecessor search used for fn_edges. RA emits local symbols as
            // opaque IDs (`local 0`); the source name is the display_name on
            // the matching SymbolInformation, so we join through `disp_names`.
            // Shadowing disambiguators (`foo#1`) appear in display_name too;
            // strip them so cross-fn counting isn't fragmented.
            if occ.symbol.starts_with("local ") && is_def(occ.symbol_roles) {
                if let Some(fns) = fns {
                    if let Some((start, _)) = parse_range(&occ.range) {
                        if let Some(caller) = enclosing_fn(fns, start) {
                            let key = (doc.relative_path.clone(), occ.symbol.clone());
                            let raw = disp_names.get(&key).map(|s| s.as_str()).unwrap_or("");
                            if !raw.is_empty() {
                                let name = raw.split('#').next().unwrap_or(raw);
                                locals.insert((caller.clone(), name.to_string()));
                            }
                        }
                    }
                }
                continue;
            }
            if !usable_symbol(&occ.symbol) || is_def(occ.symbol_roles) { continue; }
            let Some(def) = def_file.get(&occ.symbol) else { continue };
            let repo = doc_repo(&doc.relative_path);
            refs.insert((doc.relative_path.clone(), occ.symbol.clone(), def.clone(), repo.clone()));
            if def != &doc.relative_path {
                edges.insert((doc.relative_path.clone(), def.clone(), repo));
            }
            // Attribute this reference to its innermost enclosing fn def. Both
            // ranges come from the same SCIP index, so the 0-based line/col base
            // is internally consistent regardless of sprefa's own line convention.
            if let Some(fns) = fns {
                if let Some((start, _)) = parse_range(&occ.range) {
                    if let Some(caller) = enclosing_fn(fns, start) {
                        fn_edges.insert((caller.clone(), occ.symbol.clone()));
                    }
                }
            }
        }
    }

    // Interface/supertype dispatch edges. SCIP attaches these to the
    // SymbolInformation of the IMPLEMENTING symbol (per-document in doc.symbols,
    // plus index.external_symbols for out-of-workspace targets), as a
    // relationship to the symbol it implements with `is_implementation` set.
    let mut impls: HashSet<(String, String)> = HashSet::new();
    let sym_infos = index
        .documents
        .iter()
        .flat_map(|d| d.symbols.iter())
        .chain(index.external_symbols.iter());
    for si in sym_infos {
        if si.symbol.is_empty() { continue; }
        for rel in &si.relationships {
            if rel.is_implementation && !rel.symbol.is_empty() {
                impls.insert((si.symbol.clone(), rel.symbol.clone()));
            }
        }
    }

    let mut rows = ScipRows {
        defs: defs.into_iter().collect(),
        refs: refs.into_iter().collect(),
        edges: edges.into_iter().collect(),
        fn_edges: fn_edges.into_iter().collect(),
        callee_types: callee_types.into_iter().collect(),
        locals: locals.into_iter().collect(),
        occ_spans: occ_spans.into_iter().collect(),
        occurrences: occurrences.into_iter().collect(),
        impls: impls.into_iter().collect(),
    };
    rows.defs.sort();
    rows.refs.sort();
    rows.edges.sort();
    rows.fn_edges.sort();
    rows.callee_types.sort();
    rows.locals.sort();
    rows.occ_spans.sort();
    rows.occurrences.sort();
    rows.impls.sort();
    rows
}

/// Parse a SCIP packed occurrence range into ((start_line, start_col),
/// (end_line, end_col)). SCIP encodes ranges as a repeated int32: either
/// `[sl, sc, el, ec]` (4-el) or `[sl, sc, ec]` (3-el, end_line == start_line).
/// All values are 0-based. Returns None for malformed ranges.
fn parse_range(r: &[i32]) -> Option<((i32, i32), (i32, i32))> {
    match r {
        [sl, sc, el, ec] => Some(((*sl, *sc), (*el, *ec))),
        [sl, sc, ec] => Some(((*sl, *sc), (*sl, *ec))),
        _ => None,
    }
}

/// Attribute a reference at `pos` to its enclosing fn. SCIP def occurrences
/// mark only the fn's identifier (a name-sized range), not the body, so an
/// end-bound containment test can't work. Instead: the enclosing fn is the one
/// whose def STARTS most recently at or before the ref (predecessor search on
/// start position). This is correct for any ref inside a fn body — the
/// most-recently-opened fn before the ref is the enclosing one. It
/// mis-attributes the rare module/impl-level ref that sits after a fn's body
/// but before the next fn; acceptable noise for a call-graph extractor.
fn enclosing_fn(
    fns: &[((i32, i32), (i32, i32), String)],
    pos: (i32, i32),
) -> Option<&String> {
    fns.iter()
        .filter(|f| f.0 <= pos)
        .max_by_key(|f| f.0)
        .map(|f| &f.2)
}

fn is_def(roles: i32) -> bool {
    roles & (SymbolRole::Definition as i32) != 0
}

/// Closed-vocabulary role for a `scip_occurrence` row, off the `symbol_roles`
/// bitmask: definition | import | write | read | reference, first match wins.
/// Write outranks read so a compound read+write access (`x += 1`) reports the
/// rarer, more queryable side; a bare occurrence (no bits) is `reference`.
fn role_label(roles: i32) -> &'static str {
    if is_def(roles) {
        "definition"
    } else if roles & (SymbolRole::Import as i32) != 0 {
        "import"
    } else if roles & (SymbolRole::WriteAccess as i32) != 0 {
        "write"
    } else if roles & (SymbolRole::ReadAccess as i32) != 0 {
        "read"
    } else {
        "reference"
    }
}

/// The local binding text at a single-line occurrence: the source slice
/// `[start_col, end_col)` of `line_text`, measured in UTF-16 code units (the
/// SCIP/LSP default column encoding). ASCII identifiers — the overwhelming
/// majority — slice identically under bytes/chars/UTF-16, so this only matters
/// for a line with a wide character before the occurrence. Returns "" for an
/// out-of-range or inverted span.
fn slice_local_name(line_text: &str, start_col: i32, end_col: i32) -> String {
    if start_col < 0 || end_col <= start_col { return String::new(); }
    let units: Vec<u16> = line_text.encode_utf16().collect();
    let (lo, hi) = (start_col as usize, end_col as usize);
    if hi > units.len() { return String::new(); }
    String::from_utf16_lossy(&units[lo..hi])
}

/// Local binding names for a batch of occurrences produced under one on-disk
/// `root` (one input index). Reads each distinct file's WORK content ONCE
/// (cached — never a per-occurrence read), then slices each single-line
/// occurrence range to its source text. Returns
/// `(file, symbol, local_name, start_line, start_col, repo)`, deduped. This is
/// the S2 fix: the LOCAL binding (e.g. `bar` from `import { foo as bar }`) is
/// the source text at the range, joinable to the canonical `symbol` — a
/// name-based join that `scip_name` (canonical only) silently dropped now lands.
/// SCIP is WORK-only, so the slice reflects the current on-disk tree; a stale
/// index vs a later edit can mis-name (the importer's standing staleness).
/// Multi-line and unreadable ranges are skipped.
pub fn local_bindings(
    occurrences: &[(String, String, i32, i32, i32, i32, String, String)],
    root: &Path,
) -> Vec<(String, String, String, i32, i32, String)> {
    let mut lines_of: HashMap<&str, Option<Vec<String>>> = HashMap::new();
    let mut out: HashSet<(String, String, String, i32, i32, String)> = HashSet::new();
    for (file, symbol, sl, sc, el, ec, _role, repo) in occurrences {
        if sl != el { continue; } // single-line occurrences only
        let lines = lines_of.entry(file.as_str()).or_insert_with(|| {
            std::fs::read_to_string(root.join(file))
                .ok()
                .map(|content| content.lines().map(str::to_string).collect())
        });
        let Some(lines) = lines else { continue };
        let Some(line_text) = lines.get(*sl as usize) else { continue };
        let name = slice_local_name(line_text, *sc, *ec);
        if !name.is_empty() {
            out.insert((file.clone(), symbol.clone(), name, *sl, *sc, repo.clone()));
        }
    }
    let mut out: Vec<_> = out.into_iter().collect();
    out.sort();
    out
}

fn usable_symbol(symbol: &str) -> bool {
    !symbol.is_empty() && !symbol.starts_with("local ")
}

/// A callable definition (free fn or method) for the enclosing-fn interval
/// index. Every supported indexer terminates a callable's descriptor with `().`,
/// so the symbol ends with `).`. A parameter descriptor (`…getPet().(id)`) also
/// contains parens but ends with a bare `)`, so it is excluded — otherwise a
/// body reference would be attributed to the nearest parameter, not the method.
fn is_callable_def(symbol: &str) -> bool {
    symbol.ends_with(").")
}

/// Extract the receiver type from a RA method moniker. RA encodes the impl
/// holder inline: `…/impl#[Engine]tick().` (inherent impl) or
/// `…/impl#[Trait]for#[Engine]tick().` (trait impl, receiver = the for-type).
/// Returns the bare type name (e.g. `Engine`), or None for free fns / types /
/// any symbol without an impl holder. The type name alone is not globally
/// unique (two crates could each define `Engine`), but within one RA index of
/// one crate it is — and name coincidence is the cheap strong signal we want.
fn receiver_type(symbol: &str) -> Option<String> {
    let after_for = |key: &str| -> Option<String> {
        let i = symbol.find(key)?;
        let rest = &symbol[i + key.len()..];
        rest.find(']').map(|j| rest[..j].to_string())
    };
    // Trait impl: the receiving type is the `for#[Type]`, not the trait.
    after_for("for#[").or_else(|| after_for("impl#["))
}

/// Build a map from (relative_path, symbol) to its `SymbolInformation.display_name`.
/// RA emits local symbols as opaque IDs (`local 0`, `local 1`, ...) that are
/// scoped PER-DOCUMENT: every file reuses `local 0` for its own first local.
/// Keying by the bare symbol collapses all docs onto one entry (last write
/// wins), so every `local N` def across the whole index would resolve to one
/// file's name. Keying by (relative_path, symbol) keeps each doc's locals
/// distinct. The source-level variable name lives in `display_name` on the
/// matching SymbolInformation (per-document in `doc.symbols`), not in the
/// symbol string. Without this join, every local name resolves to a numeric ID.
fn display_names(index: &Index) -> HashMap<(String, String), String> {
    let mut m = HashMap::new();
    for doc in &index.documents {
        for si in &doc.symbols {
            if !si.display_name.is_empty() {
                m.insert((doc.relative_path.clone(), si.symbol.clone()), si.display_name.clone());
            }
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use scip::types::{Document, Index, Occurrence, SymbolRole};

    fn occ_r(symbol: &str, roles: i32, range: [i32; 4]) -> Occurrence {
        let mut o = Occurrence::new();
        o.symbol = symbol.to_string();
        o.symbol_roles = roles;
        o.range = range.to_vec();
        o
    }

    // Test indexes use synthetic relative paths with no ancestor `.git`, so the
    // repo id falls back to this slug. The repo column is exercised directly in
    // `two_roots_keep_symbols_distinct` below via `rows`.
    fn test_rows(index: &Index) -> ScipRows {
        rows(index, Path::new("/no/such/scip/root"), "testrepo")
    }

    #[test]
    fn two_roots_keep_symbols_distinct() {
        // Two roots of the same crate emit byte-identical (symbol, relative_path)
        // pairs. The OLD importer merged their documents blind, so `def_file`'s
        // first-wins map dropped the second root entirely (zero net rows). Now
        // each index is loaded per-input with its repo threaded through, so both
        // roots' defs AND refs survive, tagged by distinct repo.
        let def = "rust-analyzer cargo sprefa-dl 0.4.1 `crate`/answer().";
        let mk = || {
            let mut d = Document::new();
            d.relative_path = "src/lib.rs".to_string();
            d.occurrences = vec![
                occ_r(def, SymbolRole::Definition as i32, [0, 0, 0, 6]),
            ];
            let mut r = Document::new();
            r.relative_path = "src/main.rs".to_string();
            r.occurrences = vec![occ_r(def, 0, [1, 0, 1, 6])];
            let mut idx = Index::new();
            idx.documents = vec![d, r];
            idx
        };
        let a = rows(&mk(), Path::new("/roots/base"), "base");
        let b = rows(&mk(), Path::new("/roots/head"), "head");
        // Each index alone produces one def + one ref (identical strings).
        assert_eq!(a.defs.len(), 1, "base defs: {:?}", a.defs);
        assert_eq!(b.defs.len(), 1, "head defs: {:?}", b.defs);
        // The concatenation the caller performs: distinct repo values, both
        // roots' rows present — the second index adds NET rows (the bug: it added
        // zero because the merged first-wins map already held every string).
        let mut defs = a.defs.clone();
        defs.extend(b.defs.clone());
        assert_eq!(defs.len(), 2, "both roots' defs survive: {defs:?}");
        assert_eq!(a.defs[0].2, "base", "base def repo tag: {:?}", a.defs);
        assert_eq!(b.defs[0].2, "head", "head def repo tag: {:?}", b.defs);
        // Refs likewise carry the origin repo; def_file resolution is within-index.
        assert_eq!(a.refs.len(), 1, "base refs: {:?}", a.refs);
        assert_eq!(a.refs[0].2, "src/lib.rs", "def_file resolved within base");
        assert_eq!(a.refs[0].3, "base", "base ref repo tag: {:?}", a.refs);
        assert_eq!(b.refs[0].3, "head", "head ref repo tag: {:?}", b.refs);
        // Edges too (main.rs -> lib.rs, per repo).
        assert_eq!(a.edges[0], ("src/main.rs".into(), "src/lib.rs".into(), "base".into()));
        assert_eq!(b.edges[0], ("src/main.rs".into(), "src/lib.rs".into(), "head".into()));
    }

    #[test]
    fn fn_edge_attributes_ref_to_enclosing_fn() {
        let callee = "pkg/callee().";
        let caller = "pkg/caller().";
        let mut doc = Document::new();
        doc.relative_path = "src/lib.rs".to_string();
        // callee def at line 0; caller fn def at line 2; ref to callee at line 5
        // (inside caller's body per predecessor search: most recent start <= 5).
        doc.occurrences = vec![
            occ_r(callee, SymbolRole::Definition as i32, [0, 0, 0, 10]),
            occ_r(caller, SymbolRole::Definition as i32, [2, 0, 2, 8]),
            occ_r(callee, 0, [5, 0, 5, 8]),
        ];
        let mut index = Index::new();
        index.documents = vec![doc];
        let rows = test_rows(&index);
        assert_eq!(rows.fn_edges.len(), 1,
            "expected 1 fn_edge, got {}: {:?}", rows.fn_edges.len(), rows.fn_edges);
        assert_eq!(rows.fn_edges[0].0, caller, "caller attribution");
        assert_eq!(rows.fn_edges[0].1, callee, "callee");
    }

    #[test]
    fn fn_edge_ref_before_first_fn_is_unattributed() {
        let callee = "pkg/callee().";
        let caller = "pkg/caller().";
        let mut doc = Document::new();
        doc.relative_path = "src/lib.rs".to_string();
        // callee def at line 0; caller def at line 10; ref at line 2 — BEFORE
        // caller starts. Predecessor search finds callee (line 0) as the most
        // recent start, producing a self-edge (callee→callee). This is the
        // known limitation of name-only ranges: a module-level ref after a fn
        // def can't be distinguished from a body ref. The self-edge is the
        // tell-tale; callers that care filter caller==callee.
        doc.occurrences = vec![
            occ_r(callee, SymbolRole::Definition as i32, [0, 0, 0, 10]),
            occ_r(caller, SymbolRole::Definition as i32, [10, 0, 10, 8]),
            occ_r(callee, 0, [2, 0, 2, 8]),
        ];
        let mut index = Index::new();
        index.documents = vec![doc];
        let rows = test_rows(&index);
        // Self-edge callee→callee (the name-only-range limitation, documented).
        assert_eq!(rows.fn_edges.len(), 1, "self-edge from predecessor search: {:?}", rows.fn_edges);
        assert_eq!(rows.fn_edges[0].0, callee);
        assert_eq!(rows.fn_edges[0].1, callee);
    }

    #[test]
    fn local_def_attributed_to_enclosing_fn() {
        // caller fn def at line 0 (name range 0..6); local `parser_state` def
        // at line 5 inside caller's body. The local symbol is `local 0`; its
        // source-level name lives in the SymbolInformation.display_name for
        // that symbol. Predecessor search attributes the local to caller.
        use scip::types::SymbolInformation;
        let caller = "pkg/caller().";
        let mut si = SymbolInformation::new();
        si.symbol = "local 0".to_string();
        si.display_name = "parser_state".to_string();
        let mut doc = Document::new();
        doc.relative_path = "src/lib.rs".to_string();
        doc.occurrences = vec![
            occ_r(caller, SymbolRole::Definition as i32, [0, 0, 0, 6]),
            occ_r("local 0", SymbolRole::Definition as i32, [5, 8, 5, 21]),
        ];
        doc.symbols = vec![si];
        let mut index = Index::new();
        index.documents = vec![doc];
        let rows = test_rows(&index);
        assert_eq!(rows.locals.len(), 1, "expected 1 local: {:?}", rows.locals);
        assert_eq!(rows.locals[0].0, caller, "attributed to caller");
        assert_eq!(rows.locals[0].1, "parser_state", "display_name resolved");
    }

    #[test]
    fn local_shadowing_suffix_is_stripped() {
        // RA disambiguates shadowed locals as `foo`, `foo#1`. The display_name
        // carries the suffix; both shadows collapse to bare `foo` so cross-fn
        // counting isn't fragmented by shadowing depth.
        use scip::types::SymbolInformation;
        fn si(sym: &str, name: &str) -> SymbolInformation {
            let mut s = SymbolInformation::new();
            s.symbol = sym.to_string();
            s.display_name = name.to_string();
            s
        }
        let caller = "pkg/caller().";
        let mut doc = Document::new();
        doc.relative_path = "src/lib.rs".to_string();
        doc.occurrences = vec![
            occ_r(caller, SymbolRole::Definition as i32, [0, 0, 0, 6]),
            occ_r("local 0", SymbolRole::Definition as i32, [3, 8, 3, 11]),
            occ_r("local 1", SymbolRole::Definition as i32, [5, 8, 5, 11]),
        ];
        doc.symbols = vec![si("local 0", "foo"), si("local 1", "foo#1")];
        let mut index = Index::new();
        index.documents = vec![doc];
        let rows = test_rows(&index);
        // Deduped: both shadows collapse to (caller, "foo").
        assert_eq!(rows.locals.len(), 1, "shadows deduped: {:?}", rows.locals);
        assert_eq!(rows.locals[0], (caller.to_string(), "foo".to_string()));
    }

    #[test]
    fn local_ref_not_collected() {
        // Only local DEFS are collected (binding sites). A reference to a
        // local (an occurrence without the Definition role) is ignored — we
        // want one row per declared name per fn, not per use.
        use scip::types::SymbolInformation;
        let caller = "pkg/caller().";
        let mut si = SymbolInformation::new();
        si.symbol = "local 0".to_string();
        si.display_name = "x".to_string();
        let mut doc = Document::new();
        doc.relative_path = "src/lib.rs".to_string();
        doc.occurrences = vec![
            occ_r(caller, SymbolRole::Definition as i32, [0, 0, 0, 6]),
            occ_r("local 0", SymbolRole::Definition as i32, [3, 8, 3, 9]),
            occ_r("local 0", 0, [4, 8, 4, 9]),  // ref, no role
        ];
        doc.symbols = vec![si];
        let mut index = Index::new();
        index.documents = vec![doc];
        let rows = test_rows(&index);
        assert_eq!(rows.locals.len(), 1, "only def collected: {:?}", rows.locals);
    }

    #[test]
    fn body_ref_attributes_to_method_not_parameter() {
        // scip-python emits parameter symbols (`…getPet().(id)`) whose def range
        // sits between the method's name range and the body. A parameter
        // descriptor ends with `)`, not `).`, so it must NOT register as an
        // enclosing fn — otherwise the predecessor search attributes the body's
        // call to the nearest PARAMETER instead of the method, and the dispatch
        // hop dead-ends. Layout mirrors scip-python: method def, then its param
        // def, then a body ref to a callee.
        let method = "scip-python . . pkg/PetClient#getPet().";
        let param = "scip-python . . pkg/PetClient#getPet().(id)";
        let callee = "scip-python . . pkg/httpExec().";
        let mut doc = Document::new();
        doc.relative_path = "petapi.py".to_string();
        doc.occurrences = vec![
            occ_r(method, SymbolRole::Definition as i32, [0, 4, 0, 10]),
            occ_r(param, SymbolRole::Definition as i32, [0, 11, 0, 13]),
            occ_r(callee, SymbolRole::Definition as i32, [3, 0, 3, 8]),
            occ_r(callee, 0, [1, 8, 1, 16]),  // body call to httpExec
        ];
        let mut index = Index::new();
        index.documents = vec![doc];
        let rows = test_rows(&index);
        // exactly one fn_edge: the METHOD calls httpExec (not the parameter).
        let call: Vec<_> = rows.fn_edges.iter().filter(|(_, c)| c == callee).collect();
        assert_eq!(call.len(), 1, "one body call attributed: {:?}", rows.fn_edges);
        assert_eq!(call[0].0, method, "attributed to the method, not the param: {:?}", rows.fn_edges);
    }

    #[test]
    fn impl_relationship_yields_dispatch_edge() {
        // SCIP attaches the dispatch hop to the IMPLEMENTING symbol: the impl
        // method `Dog#sound()` carries an `is_implementation` relationship to the
        // interface method `Animal#sound()`. The row is (impl, iface). A plain
        // `is_reference` relationship (no implementation) is NOT a dispatch edge.
        use scip::types::{Relationship, SymbolInformation};
        let iface = "scip-kotlin . . Animal#sound().";
        let impl_m = "scip-kotlin . . Dog#sound().";
        let other = "scip-kotlin . . Cat#purr().";
        let mut rel_impl = Relationship::new();
        rel_impl.symbol = iface.to_string();
        rel_impl.is_implementation = true;
        let mut si_impl = SymbolInformation::new();
        si_impl.symbol = impl_m.to_string();
        si_impl.relationships = vec![rel_impl];
        // a reference-only relationship that must NOT become a dispatch edge.
        let mut rel_ref = Relationship::new();
        rel_ref.symbol = iface.to_string();
        rel_ref.is_reference = true;
        let mut si_ref = SymbolInformation::new();
        si_ref.symbol = other.to_string();
        si_ref.relationships = vec![rel_ref];
        let mut doc = Document::new();
        doc.relative_path = "Dog.kt".to_string();
        doc.symbols = vec![si_impl, si_ref];
        let mut index = Index::new();
        index.documents = vec![doc];
        let rows = test_rows(&index);
        assert_eq!(rows.impls, vec![(impl_m.to_string(), iface.to_string())],
            "only the is_implementation edge, oriented impl→iface: {:?}", rows.impls);
    }

    #[test]
    fn parse_range_four_and_three_element_forms() {
        // 4-el: explicit end line/col.
        assert_eq!(parse_range(&[2, 4, 3, 9]), Some(((2, 4), (3, 9))));
        // 3-el short form: end line == start line, third value is end col.
        assert_eq!(parse_range(&[2, 4, 9]), Some(((2, 4), (2, 9))));
        // malformed lengths → None.
        assert_eq!(parse_range(&[1, 2]), None);
        assert_eq!(parse_range(&[1, 2, 3, 4, 5]), None);
    }

    #[test]
    fn role_label_maps_definition_and_reference() {
        assert_eq!(role_label(SymbolRole::Definition as i32), "definition");
        // Definition co-set with another bit is still a definition.
        assert_eq!(
            role_label(SymbolRole::Definition as i32 | SymbolRole::Import as i32),
            "definition"
        );
        // No Definition bit: the widened vocabulary keeps the finer roles.
        assert_eq!(role_label(0), "reference");
        assert_eq!(role_label(SymbolRole::Import as i32), "import");
        assert_eq!(role_label(SymbolRole::WriteAccess as i32), "write");
        assert_eq!(role_label(SymbolRole::ReadAccess as i32), "read");
        // Compound read+write reports write (the rarer, more queryable side).
        assert_eq!(
            role_label(SymbolRole::ReadAccess as i32 | SymbolRole::WriteAccess as i32),
            "write"
        );
    }

    #[test]
    fn slice_local_name_ascii_and_utf16() {
        // `import { foo as bar }` — `bar` is UTF-16 units [16, 19).
        let line = "import { foo as bar }";
        assert_eq!(slice_local_name(line, 16, 19), "bar");
        // A wide char before the occurrence shifts UTF-16 columns; the slice
        // still lands on the identifier when the index is UTF-16-based.
        let wide = "let 😀 = bar";           // 😀 = 2 UTF-16 units
        // "let " = 4, "😀" = 2 (→6), " = " = 3 (→9), "bar" = [9, 12).
        assert_eq!(slice_local_name(wide, 9, 12), "bar");
        // Out-of-range / inverted spans yield "".
        assert_eq!(slice_local_name(line, 100, 103), "");
        assert_eq!(slice_local_name(line, 5, 5), "");
    }

    #[test]
    fn occurrences_carry_end_position_role_and_repo() {
        let def = "pkg/answer().";
        let mut d = Document::new();
        d.relative_path = "src/lib.rs".to_string();
        d.occurrences = vec![
            occ_r(def, SymbolRole::Definition as i32, [0, 0, 0, 6]),
            occ_r(def, 0, [4, 8, 4, 14]), // a reference to the same symbol
        ];
        let mut idx = Index::new();
        idx.documents = vec![d];
        let rows = rows(&idx, Path::new("/roots/head"), "head");
        // Two occurrence rows, one per span, tagged def vs ref, repo = slug.
        assert_eq!(rows.occurrences.len(), 2, "{:?}", rows.occurrences);
        let def_row = rows.occurrences.iter().find(|o| o.6 == "definition").unwrap();
        assert_eq!(*def_row, ("src/lib.rs".into(), def.into(), 0, 0, 0, 6, "definition".into(), "head".into()));
        let ref_row = rows.occurrences.iter().find(|o| o.6 == "reference").unwrap();
        assert_eq!(*ref_row, ("src/lib.rs".into(), def.into(), 4, 8, 4, 14, "reference".into(), "head".into()));
    }

    #[test]
    fn local_bindings_slice_source_text_for_alias() {
        // Write a file whose import aliases `foo` to `bar`; the occurrence of the
        // CANONICAL `foo` symbol sits over `bar`'s range. local_bindings slices
        // "bar" (the local name scip_name can't give).
        let dir = std::env::temp_dir().join("scip_local_bindings_unit");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("app.ts"), "import { foo as bar } from \"./mod\"\n").unwrap();
        let canonical = "scip-typescript npm mod 1.0.0 `mod`/foo#";
        // `bar` starts at UTF-16 col 16, ends at 19 (0-based).
        let occ = vec![(
            "app.ts".to_string(), canonical.to_string(),
            0, 16, 0, 19, "reference".to_string(), "app".to_string(),
        )];
        let binds = local_bindings(&occ, &dir);
        assert_eq!(binds.len(), 1, "{binds:?}");
        assert_eq!(binds[0],
            ("app.ts".into(), canonical.into(), "bar".into(), 0, 16, "app".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn local_bindings_skip_multiline_and_missing_files() {
        // Multi-line occurrence (sl != el) is skipped; a file that isn't on disk
        // yields nothing (no panic).
        let occ = vec![
            ("gone.ts".to_string(), "sym".to_string(), 0, 0, 0, 3, "reference".to_string(), "r".to_string()),
            ("gone.ts".to_string(), "sym".to_string(), 0, 0, 5, 3, "reference".to_string(), "r".to_string()),
        ];
        let binds = local_bindings(&occ, Path::new("/no/such/root"));
        assert!(binds.is_empty(), "{binds:?}");
    }

    #[test]
    fn local_ids_are_scoped_per_document() {
        // RA reuses `local 0` in every document: each file's first local is
        // `local 0`. A global symbol->display_name map would collapse them to
        // one entry (last doc wins), making every local-5 def across the whole
        // index resolve to a single file's name — manufacturing false
        // cross-fn name repetition. Per-document keying prevents the collision.
        use scip::types::SymbolInformation;
        fn si(sym: &str, name: &str) -> SymbolInformation {
            let mut s = SymbolInformation::new();
            s.symbol = sym.to_string();
            s.display_name = name.to_string();
            s
        }
        fn doc(path: &str, local0_name: &str) -> Document {
            let caller = format!("pkg/{path}().");
            let mut d = Document::new();
            d.relative_path = path.to_string();
            d.occurrences = vec![
                occ_r(&caller, SymbolRole::Definition as i32, [0, 0, 0, 6]),
                occ_r("local 0", SymbolRole::Definition as i32, [3, 8, 3, 14]),
            ];
            d.symbols = vec![si("local 0", local0_name)];
            d
        }
        let mut index = Index::new();
        index.documents = vec![
            doc("src/lib.rs", "alpha"),
            doc("src/daemon.rs", "beta"),
        ];
        let rows = test_rows(&index);
        // Two distinct locals: (lib caller, alpha) and (daemon caller, beta).
        // Under the old global-keyed map the second doc would clobber the first
        // and both fns would report `beta`.
        assert_eq!(rows.locals.len(), 2, "per-doc locals kept distinct: {:?}", rows.locals);
        let has = |name: &str| rows.locals.iter().any(|(_, n)| n == name);
        assert!(has("alpha"), "lib.rs local preserved: {:?}", rows.locals);
        assert!(has("beta"), "daemon.rs local preserved: {:?}", rows.locals);
    }
}
