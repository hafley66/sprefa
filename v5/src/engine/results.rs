//! Public query-result row shapes returned by the engine's read paths
//! (LSP, RPC, `dl q`): relocated from `engine/mod.rs` (decomposition plan
//! step 7, plans/2026-07-18-decomposition-normalization.md). Pure data —
//! no behavior lives here.

use crate::spine;

/// One row of the `diag` relation, normalized for the LSP. Columns are mapped
/// by NAME from the `.dl` author's `rel diag(...)` decl (order-free); only
/// path/line/msg are required, the span/severity fields default. See docs/lsp.md.
#[derive(Clone, Debug)]
pub struct DiagRow {
    pub path: String,
    pub line: i64,
    pub col: i64,
    pub end_line: i64,
    pub end_col: i64,
    pub severity: String,
    pub code: String,
    pub msg: String,
    pub hint: Option<String>,
}

/// One `?` query result, captured for the daemon RPC `query` path. Same shape
/// as `--query-json` per-row objects; the foreground path prints via `run_query`
/// instead.
#[derive(Clone, Debug)]
pub struct QueryResult {
    pub rel: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
}

/// One located reference for the `refs_lens` navigation surface (Track B). A hit
/// carries its OWN repo so the LSP can map the slug back to that repo's on-disk
/// root (the multi-repo `root.join` fix), plus a 0-based line/col range matching
/// what `resolve_span` produces. `role` labels the edge (declaration kind, `call`,
/// an import, a `type_link` kind, `caller`/`callee`, or `text`); `container` names
/// the enclosing symbol when known.
#[derive(Clone, Debug, serde::Serialize)]
pub struct RefHit {
    pub repo: String,
    pub path: String,
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub role: String,
    pub container: String,
}

/// The grouped references result for one cursor position, produced by
/// `Engine::refs_lens`. `tier` is the resolution grade (`resolved` = joined
/// through the type/call graph by name, `textual` = the ref-spine same-string
/// fallback). `symbol` is the preferred definition symbol (same-repo-then-same-
/// file wins when a name maps to several); `display_name` is the bare identifier
/// under the cursor.
#[derive(Clone, Debug, serde::Serialize)]
pub struct RefLens {
    pub tier: String,
    pub symbol: String,
    pub display_name: String,
    pub declarations: Vec<RefHit>,
    pub uses: Vec<RefHit>,
    pub containing_types: Vec<RefHit>,
    pub callers: Vec<RefHit>,
    pub callees: Vec<RefHit>,
}

/// One point-lookup hit for the "follow the user" navigation surface
/// (`Engine::locate`, Track B B4). Cheap by construction: this is a single
/// cursor -> symbol -> declaration-site resolution, never a uses/callers
/// collection or a closure walk — the panel calls it on every cursor move, so
/// it stays a point query same as `resolve_sym_hit`. `tier` mirrors
/// `RefLens.tier` minus "textual" (a grep-grade hit would center the graph on
/// nothing, so follow mode never falls that far). `role` is the edge/occurrence
/// role at the declaration site (a SCIP role for tier "compiler", the
/// type_entity/call_def kind for tier "resolved").
#[derive(Clone, Debug, serde::Serialize)]
pub struct LocateHit {
    pub tier: String,
    pub symbol: String,
    pub display_name: String,
    pub role: String,
    pub repo: String,
    pub file: String,
    pub line: u32,
}

/// One declared symbol for the nearly-free LSP surfaces (`workspace/symbol` and
/// `textDocument/documentSymbol`). Carries its OWN repo so the LSP maps the slug
/// back to that repo's on-disk root, `line` is 1-based as stored in the rels, and
/// `sym`/`parent` are the `file::kind::name` cross-graph keys the document-symbol
/// handler nests by. `container` names the enclosing symbol for the flat
/// workspace list.
#[derive(Clone, Debug)]
pub struct SymbolRow {
    pub repo: String,
    pub sym: String,
    pub name: String,
    pub kind: String,
    pub parent: String,
    pub file: String,
    pub line: i64,
    pub container: String,
}

/// One resolvable node for the call-hierarchy / type-hierarchy LSP surfaces
/// (Track B B5, `textDocument/prepareCallHierarchy` +
/// `textDocument/prepareTypeHierarchy` and their incoming/outgoing/super/sub
/// twins). Reuses the same two-tier resolution ladder as `locate`/`refs_lens`:
/// `sym` is the resolved-tier join key (`call_def.sym` / `type_entity.sym`),
/// `scip_symbol` is the compiler-tier SCIP moniker — exactly one of the two is
/// non-empty, mirroring `tier`. Each item carries its OWN repo (the multi-repo
/// URI fix `refhit_location`/`workspace_symbols` already use). `line`/
/// `end_line` are 0-based (unlike `SymbolRow`, which keeps the 1-based rel
/// convention) so the LSP handler can build a `Range` with no further
/// arithmetic — this struct exists purely to cross the engine/LSP boundary.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct HierarchyItem {
    pub tier: String,
    pub sym: String,
    pub scip_symbol: String,
    pub name: String,
    pub kind: String,
    pub repo: String,
    pub file: String,
    pub line: u32,
    pub end_line: u32,
}

/// One 1-hop call-hierarchy neighbor: the neighboring `HierarchyItem` plus the
/// call-site line(s) inside the CALLER (`from_ranges` in the LSP spec is
/// always relative to the caller, for both `incomingCalls` and
/// `outgoingCalls`). 0-based, matching `HierarchyItem.line`.
#[derive(Clone, Debug, serde::Serialize)]
pub struct HierarchyCallEdge {
    pub item: HierarchyItem,
    pub from_lines: Vec<u32>,
}

/// Carry set for `refresh_spine_rels_delta`. Accumulates the new rows produced
/// during a single tick so the incremental Some() path can replay only those rows
/// rather than projecting the full `_strings` / `_where_bytes` tables.
///
/// Incremental-load lever: the wholesale `_strings` / `_where_bytes` read in
/// `refresh_spine_rels_delta(None)` is correct but scales with total interned
/// strings, not per-tick delta. The staged per-tick vecs in
/// `insert_spine_where_bytes` are the future `Some()` source: collect the new
/// StringIds and WhereBytes there, pass them here, then flush one
/// `insert_rows` call per table (collect-then-flush, never per-row). The
/// `retracted_paths` list drives the corresponding delete from `string` / `ref`
/// before the new rows land.
pub struct SpineDelta {
    pub strings_added: Vec<spine::StringId>,
    pub spans_added: Vec<spine::WhereBytes>,
    pub retracted_paths: Vec<(spine::RepoId, String)>,
}
