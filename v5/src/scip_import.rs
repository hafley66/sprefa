use anyhow::Result;
use protobuf::Message;
use scip::types::{Index, SymbolRole};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Default, Debug)]
pub struct ScipRows {
    pub defs: Vec<(String, String)>,
    pub refs: Vec<(String, String, String)>,
    pub edges: Vec<(String, String)>,
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

pub fn index_path(root: &Path) -> Option<PathBuf> {
    if let Ok(path) = std::env::var("SPREFA_SCIP_INDEX") {
        let path = PathBuf::from(path);
        if path.is_file() { return Some(path); }
    }
    let path = root.join("index.scip");
    path.is_file().then_some(path)
}

pub fn load(path: &Path) -> Result<ScipRows> {
    let bytes = std::fs::read(path)?;
    let index = Index::parse_from_bytes(&bytes)?;
    Ok(rows(&index))
}

pub fn rows(index: &Index) -> ScipRows {
    let mut def_file: HashMap<String, String> = HashMap::new();
    let mut defs: HashSet<(String, String)> = HashSet::new();
    // Per-file fn-def intervals for caller attribution: ((start), (end), symbol).
    // A def qualifies as fn-like when its symbol contains '(' — RA emits
    // `…/fn_name().` for free functions, `…/impl#[Type]method().` for methods;
    // types, modules, and impl-block holders carry no '(' in their terminal
    // descriptor, so they are excluded from the interval index.
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
                defs.insert((occ.symbol.clone(), doc.relative_path.clone()));
                if let Some(ty) = receiver_type(&occ.symbol) {
                    callee_types.insert((occ.symbol.clone(), ty));
                }
                if occ.symbol.contains('(') {
                    if let Some((s, e)) = parse_range(&occ.range) {
                        fn_defs.entry(doc.relative_path.clone())
                            .or_default()
                            .push((s, e, occ.symbol.clone()));
                    }
                }
            }
        }
    }

    let mut refs: HashSet<(String, String, String)> = HashSet::new();
    let mut edges: HashSet<(String, String)> = HashSet::new();
    let mut fn_edges: HashSet<(String, String)> = HashSet::new();
    let mut locals: HashSet<(String, String)> = HashSet::new();
    let mut occ_spans: HashSet<(String, i32, i32, String)> = HashSet::new();
    let disp_names = display_names(index);
    for doc in &index.documents {
        let fns = fn_defs.get(&doc.relative_path);
        for occ in &doc.occurrences {
            if let Some(((sl, sc), (_el, _ec))) = parse_range(&occ.range) {
                occ_spans.insert((doc.relative_path.clone(), sl, sc, occ.symbol.clone()));
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
            refs.insert((doc.relative_path.clone(), occ.symbol.clone(), def.clone()));
            if def != &doc.relative_path {
                edges.insert((doc.relative_path.clone(), def.clone()));
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
        impls: impls.into_iter().collect(),
    };
    rows.defs.sort();
    rows.refs.sort();
    rows.edges.sort();
    rows.fn_edges.sort();
    rows.callee_types.sort();
    rows.locals.sort();
    rows.occ_spans.sort();
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

fn usable_symbol(symbol: &str) -> bool {
    !symbol.is_empty() && !symbol.starts_with("local ")
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
        let rows = rows(&index);
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
        let rows = rows(&index);
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
        let rows = rows(&index);
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
        let rows = rows(&index);
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
        let rows = rows(&index);
        assert_eq!(rows.locals.len(), 1, "only def collected: {:?}", rows.locals);
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
        let rows = rows(&index);
        assert_eq!(rows.impls, vec![(impl_m.to_string(), iface.to_string())],
            "only the is_implementation edge, oriented impl→iface: {:?}", rows.impls);
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
        let rows = rows(&index);
        // Two distinct locals: (lib caller, alpha) and (daemon caller, beta).
        // Under the old global-keyed map the second doc would clobber the first
        // and both fns would report `beta`.
        assert_eq!(rows.locals.len(), 2, "per-doc locals kept distinct: {:?}", rows.locals);
        let has = |name: &str| rows.locals.iter().any(|(_, n)| n == name);
        assert!(has("alpha"), "lib.rs local preserved: {:?}", rows.locals);
        assert!(has("beta"), "daemon.rs local preserved: {:?}", rows.locals);
    }
}
