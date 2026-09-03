//! THE `scip` FAMILY'S PROJECTION: a loaded SCIP index -> v5's `scip_*`
//! relation shapes, as flat rows.
//!
//! Ported from v5 `src/scip_import.rs::rows` (the body, the conventions and the
//! comments that explain WHY each convention is what it is). Column names and
//! row semantics match v5 `src/rels/scip.rs`'s decls, so a program written
//! against v5's `scip_def` / `scip_ref` / `scip_fn_edge` reads these rows
//! unchanged.
//!
//! HOW THIS RELATES TO `crate::scip_rows`. That module is PASSTHROUGH: every
//! field scip.proto serializes, deliberately unjoined, on the grounds that
//! v5's ten relations are each a filter or a join over those rows and joins
//! belong in the dl layer. That grounds holds for `--scip-facts` and this
//! module does not disturb it. What it does not answer is the demand this
//! family exists for: a caller who wants the v5 vocabulary must otherwise
//! reimplement seven non-obvious conventions (enclosing-fn attribution by
//! predecessor search, the `).` callable test, the `for#[T]` receiver parse,
//! the per-document `local N` scoping, the display-name join, the descriptor
//! name scan, first-wins def resolution), each of which v5 got wrong once
//! before getting it right. Those are ported code, not joins.
//!
//! Note the asymmetry that follows: the SAME index yields both wires, and they
//! are consistent by construction because both read one decoded `ScipIndex`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::types::{FlatFact, OccurrenceRole, ScipIndex, ScipOccurrence};

/// Project a loaded index to the v5 relation rows, sorted and deduped.
///
/// `root` is the on-disk directory the index's document paths are relative to;
/// it is read only to compute the `repo` column (v5 keys every def/ref/edge by
/// origin repo so two roots of the same crate that emit byte-identical
/// (symbol, path) pairs stay distinct rows instead of collapsing). `slug` is
/// the fallback repo id for a document with no ancestor `.git`.
pub fn v5_rel_rows(index: &ScipIndex, root: &Path, slug: &str) -> Vec<FlatFact> {
    let repos = RepoMap::new(index, root, slug);

    // Pass one: definitions. `def_file` is FIRST-WINS per symbol, which is v5's
    // resolution and is scoped to THIS index's documents.
    let mut def_file: BTreeMap<&str, &str> = BTreeMap::new();
    let mut defs: BTreeSet<(&str, &str, String)> = BTreeSet::new();
    let mut callee_types: BTreeSet<(&str, String)> = BTreeSet::new();
    // Per-file callable-def start positions for caller attribution.
    let mut fn_defs: BTreeMap<&str, Vec<((i32, i32), &str)>> = BTreeMap::new();

    for document in &index.documents {
        let path = document.relative_path.as_str();
        for occurrence in &document.occurrences {
            let symbol = index.symbol(occurrence.symbol);
            if !usable_symbol(symbol) || !is_definition(occurrence) {
                continue;
            }
            def_file.entry(symbol).or_insert(path);
            defs.insert((symbol, path, repos.of(path)));
            if let Some(receiver) = receiver_type(symbol) {
                callee_types.insert((symbol, receiver));
            }
            if is_callable_def(symbol) {
                fn_defs
                    .entry(path)
                    .or_default()
                    .push((start_of(occurrence), symbol));
            }
        }
    }

    // Pass two: references, file edges, call edges, locals.
    let display = display_names(index);
    let mut refs: BTreeSet<(&str, &str, &str, String)> = BTreeSet::new();
    let mut edges: BTreeSet<(&str, &str, String)> = BTreeSet::new();
    let mut fn_edges: BTreeSet<(&str, &str)> = BTreeSet::new();
    let mut locals: BTreeSet<(&str, &str)> = BTreeSet::new();

    for document in &index.documents {
        let path = document.relative_path.as_str();
        let callables = fn_defs.get(path);
        for occurrence in &document.occurrences {
            let symbol = index.symbol(occurrence.symbol);
            // Locals are filtered out of the main path by `usable_symbol` and
            // collected here instead: a local DEFINITION is the binding site,
            // attributed to its enclosing callable by the same predecessor
            // search the call edges use. Indexers emit locals as opaque ids
            // (`local 0`), so the source name comes from the matching
            // SymbolInformation's display_name; shadowing disambiguators
            // (`foo#1`) are stripped so counting is not fragmented per shadow.
            if symbol.starts_with("local ") && is_definition(occurrence) {
                if let Some(caller) =
                    callables.and_then(|fns| enclosing_fn(fns, start_of(occurrence)))
                {
                    if let Some(raw) = display.get(&(path, symbol)) {
                        let name = raw.split('#').next().unwrap_or(raw);
                        if !name.is_empty() {
                            locals.insert((caller, name));
                        }
                    }
                }
                continue;
            }
            if !usable_symbol(symbol) || is_definition(occurrence) {
                continue;
            }
            let Some(defined_in) = def_file.get(symbol).copied() else {
                continue;
            };
            let repo = repos.of(path);
            refs.insert((path, symbol, defined_in, repo.clone()));
            if defined_in != path {
                edges.insert((path, defined_in, repo));
            }
            // Both ranges come from the same index, so the 0-based line/col
            // base is internally consistent whatever the consumer's own
            // convention is.
            if let Some(caller) = callables.and_then(|fns| enclosing_fn(fns, start_of(occurrence)))
            {
                fn_edges.insert((caller, symbol));
            }
        }
    }

    // The implements graph. SCIP attaches it to the IMPLEMENTING symbol's
    // SymbolInformation (per-document, plus the index's external symbols for
    // out-of-workspace targets), as a relationship carrying `is_implementation`.
    // Occurrences alone never carry the virtual-dispatch hop, so this is the
    // only place the interface-to-impl path exists.
    let mut impls: BTreeSet<(&str, &str)> = BTreeSet::new();
    let mut relationships: BTreeSet<(&str, &str, bool, bool, bool, bool)> = BTreeSet::new();
    let infos = index
        .documents
        .iter()
        .flat_map(|document| document.symbols.iter())
        .chain(index.external_symbols.iter());
    for info in infos {
        if index.symbol(info.symbol).is_empty() {
            continue;
        }
        for related in &info.relationships {
            if index.symbol(related.symbol).is_empty() {
                continue;
            }
            if related.is_implementation {
                impls.insert((index.symbol(info.symbol), index.symbol(related.symbol)));
            }
            // The raw relationship row is the v5 family's unprojection: every
            // flag scip carried rides the wire, so a consumer can answer
            // subclass-of, trait-member and override questions the impl
            // projection throws away.
            relationships.insert((
                index.symbol(info.symbol),
                index.symbol(related.symbol),
                related.is_reference,
                related.is_implementation,
                related.is_type_definition,
                related.is_definition,
            ));
        }
    }

    // One row per distinct (symbol, descriptor name) over the defined symbols.
    let names: BTreeSet<(&str, String)> = defs
        .iter()
        .filter_map(|(symbol, _, _)| descriptor_name(symbol).map(|name| (*symbol, name)))
        .collect();

    let mut out = Vec::with_capacity(
        defs.len()
            + names.len()
            + refs.len()
            + edges.len()
            + fn_edges.len()
            + callee_types.len()
            + locals.len()
            + impls.len()
            + relationships.len(),
    );
    out.extend(
        defs.into_iter()
            .map(|(symbol, file, repo)| FlatFact::ScipDefRow {
                symbol: symbol.to_string(),
                file: file.to_string(),
                repo,
            }),
    );
    out.extend(
        names
            .into_iter()
            .map(|(symbol, name)| FlatFact::ScipNameRow {
                symbol: symbol.to_string(),
                name,
            }),
    );
    out.extend(
        refs.into_iter()
            .map(|(file, symbol, def_file, repo)| FlatFact::ScipRefRow {
                file: file.to_string(),
                symbol: symbol.to_string(),
                def_file: def_file.to_string(),
                repo,
            }),
    );
    out.extend(
        edges
            .into_iter()
            .map(|(src, dst, repo)| FlatFact::ScipEdgeRow {
                src: src.to_string(),
                dst: dst.to_string(),
                repo,
            }),
    );
    out.extend(
        fn_edges
            .into_iter()
            .map(|(caller, callee)| FlatFact::ScipFnEdgeRow {
                caller: caller.to_string(),
                callee: callee.to_string(),
            }),
    );
    out.extend(
        callee_types
            .into_iter()
            .map(|(sym, receiver_type)| FlatFact::ScipCalleeTypeRow {
                sym: sym.to_string(),
                receiver_type,
            }),
    );
    out.extend(
        locals
            .into_iter()
            .map(|(enclosing_fn, name)| FlatFact::ScipLocalRow {
                enclosing_fn: enclosing_fn.to_string(),
                name: name.to_string(),
            }),
    );
    out.extend(
        impls
            .into_iter()
            .map(|(implementor, iface)| FlatFact::ScipImplRow {
                implementor: implementor.to_string(),
                iface: iface.to_string(),
            }),
    );
    out.extend(relationships.into_iter().map(
        |(
            symbol,
            related_symbol,
            is_reference,
            is_implementation,
            is_type_definition,
            is_definition,
        )| {
            FlatFact::ScipRelationshipRow {
                symbol: symbol.to_string(),
                related_symbol: related_symbol.to_string(),
                is_reference,
                is_implementation,
                is_type_definition,
                is_definition,
            }
        },
    ));
    out
}

/// Per-document repo id, computed once per path. v5 keys every def/ref/edge by
/// the basename of the document's nearest ancestor `.git`, so rows from two
/// checkouts of the same crate never collapse onto each other.
struct RepoMap<'a> {
    by_path: BTreeMap<&'a str, String>,
    slug: String,
}

impl<'a> RepoMap<'a> {
    fn new(index: &'a ScipIndex, root: &Path, slug: &str) -> Self {
        let mut by_path = BTreeMap::new();
        for document in &index.documents {
            let path = document.relative_path.as_str();
            by_path
                .entry(path)
                .or_insert_with(|| repo_of(root, path, slug));
        }
        Self {
            by_path,
            slug: slug.to_string(),
        }
    }

    fn of(&self, path: &str) -> String {
        self.by_path
            .get(path)
            .cloned()
            .unwrap_or_else(|| self.slug.clone())
    }
}

/// The repo id one document answers to: the basename of the nearest ancestor
/// directory holding a `.git`, else `slug`. v5 spells this through its own
/// `repo::nearest_git`; the walk is the same and lives here because this crate
/// carries no repo module.
///
/// STATED DEVIATION FROM v5: the walk STOPS AT `root` and never climbs past it.
/// v5 walks to the filesystem root because its engine is handed an absolute
/// repo root and wants the enclosing checkout's name; this binary is handed one
/// project and must answer the same way wherever that project is checked out.
/// An unbounded walk would put the checkout directory's name in every row, so a
/// golden would pin a worktree path and the same corpus would yield different
/// facts in two clones. A nested repo INSIDE the root still gets its own id,
/// which is the cross-repo case v5's repo column exists for.
fn repo_of(root: &Path, relative: &str, slug: &str) -> String {
    let document: PathBuf = root.join(relative);
    let mut cursor: Option<&Path> = document.parent();
    while let Some(dir) = cursor {
        if dir == root {
            break;
        }
        if dir.join(".git").exists() {
            if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
                return name.to_string();
            }
        }
        cursor = dir.parent();
    }
    slug.to_string()
}

/// Whether an occurrence carries the definition role.
fn is_definition(occurrence: &ScipOccurrence) -> bool {
    occurrence.roles.contains(OccurrenceRole::DEFINITION)
}

/// An occurrence's 0-based (line, col) start. The decode already normalized
/// both scip.proto range encodings to the quad, so this is a destructure.
fn start_of(occurrence: &ScipOccurrence) -> (i32, i32) {
    (occurrence.range[0], occurrence.range[1])
}

/// A symbol usable in the def/ref graph: nonempty and not document-scoped.
/// `local N` is reused per document, so joining on it across files would mint
/// relationships between unrelated files.
pub(crate) fn usable_symbol(symbol: &str) -> bool {
    !symbol.is_empty() && !symbol.starts_with("local ")
}

/// A callable definition, for the enclosing-fn index. Every supported indexer
/// terminates a callable's descriptor with `().`, so the symbol ends `).`. A
/// PARAMETER descriptor (`…getPet().(id)`) also holds parens but ends with a
/// bare `)`; excluding it is the fix for a body reference being attributed to
/// the nearest parameter instead of its method.
pub(crate) fn is_callable_def(symbol: &str) -> bool {
    symbol.ends_with(").")
}

/// Attribute a position to its enclosing callable.
///
/// SCIP definition occurrences mark only the callable's IDENTIFIER, not its
/// body, so a containment test cannot work: the enclosing callable is the one
/// whose definition starts most recently at or before the position. Correct for
/// any reference inside a body; it mis-attributes the rare module-level
/// reference that sits after one body and before the next definition, which is
/// acceptable noise for a call-graph extractor and is v5's own stated tradeoff.
fn enclosing_fn<'a>(callables: &[((i32, i32), &'a str)], pos: (i32, i32)) -> Option<&'a str> {
    callables
        .iter()
        .filter(|(start, _)| *start <= pos)
        .max_by_key(|(start, _)| *start)
        .map(|(_, symbol)| *symbol)
}

/// The receiver type of a method moniker. rust-analyzer encodes the impl holder
/// inline: `…/impl#[Engine]tick().` (inherent) or
/// `…/impl#[Trait]for#[Engine]tick().` (trait impl, receiver = the for-type).
/// None for free functions, types, and any symbol with no impl holder. The bare
/// type name is not globally unique, but within one index of one crate it is,
/// and name coincidence is the cheap strong signal wanted here.
fn receiver_type(symbol: &str) -> Option<String> {
    let after = |key: &str| -> Option<String> {
        let at = symbol.find(key)?;
        let rest = &symbol[at + key.len()..];
        rest.find(']').map(|end| rest[..end].to_string())
    };
    after("for#[").or_else(|| after("impl#["))
}

/// The trailing identifier run of a symbol: `… Foo#` -> `Foo`, `… bar().` ->
/// `bar`. Ported from v5 `engine::scip_descriptor_name`.
pub(crate) fn descriptor_name(symbol: &str) -> Option<String> {
    let is_ident = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    let mut last: Option<(usize, usize)> = None;
    let mut run_start: Option<usize> = None;
    for (index, &byte) in symbol.as_bytes().iter().enumerate() {
        if is_ident(byte) {
            run_start.get_or_insert(index);
        } else if let Some(start) = run_start.take() {
            last = Some((start, index));
        }
    }
    if let Some(start) = run_start.take() {
        last = Some((start, symbol.len()));
    }
    last.map(|(start, end)| symbol[start..end].to_string())
}

/// (document path, symbol) -> display_name, for the local-name join.
///
/// KEYED BY THE PAIR, not by the symbol alone, and that is the whole point:
/// indexers scope `local N` PER DOCUMENT, so every file reuses `local 0` for
/// its own first binding. A bare-symbol key collapses every document onto one
/// entry and resolves every `local 0` in the index to one file's name.
fn display_names(index: &ScipIndex) -> BTreeMap<(&str, &str), &str> {
    let mut names = BTreeMap::new();
    for document in &index.documents {
        for info in &document.symbols {
            if !info.display_name.is_empty() {
                names.insert(
                    (document.relative_path.as_str(), index.symbol(info.symbol)),
                    info.display_name.as_str(),
                );
            }
        }
    }
    names
}
