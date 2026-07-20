//! Family-derive reactive engine — host extraction families as pure `derive`
//! functions over owned input tables, with the engine owning dep capture,
//! affected-set computation, and reconcile. See
//! `plans/2026-07-15-family-derive-reactive-engine.md`.
//!
//! Step 0 surface: the `Family` trait, a SQLite-backed `Ctx` that records a
//! dep per input row read (by stable integer PK), a `RowSink`, and a
//! `derive_family` runner. The first hosted family is the projection
//! `call_site` ([`call_site::CallSite`]); the aggregation tier
//! (`call_edge` / support) is step 2.

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::ast::Value;
use crate::db::{Db, SqlVal};

pub(crate) mod call_def;
pub(crate) mod call_def_rev;
pub(crate) mod call_site;
pub(crate) mod call_edge;
pub(crate) mod call_edge_rev;
pub(crate) mod call_kind;
pub(crate) mod call_name;
pub(crate) mod router;

pub(crate) use call_edge::CallEdge;
pub(crate) use call_site::CallSite;
pub(crate) use router::FamilyRouter;

/// The families the reactive call-rel flip routes through, collected from the
/// self-registration inventory (`register_family!` in each family file), sorted
/// by name so the derive/return order is deterministic across builds. Adding a
/// family touches no code here — it submits itself.
pub(crate) fn call_families() -> Vec<&'static dyn Family> {
    let mut families: Vec<&'static dyn Family> =
        inventory::iter::<FamilyReg>().map(|reg| reg.0).collect();
    families.sort_by_key(|family| family.name());
    families
}

/// Every internal input relation any call family reads — the changed-set passed
/// to `react` on a full refresh, when the whole owned baseline was rewritten
/// (nothing to skip). Framework-computed as the union of each family's
/// self-declared `input_rels`; a delta path passes only the subset it touched.
pub(crate) fn call_input_rels() -> std::collections::HashSet<&'static str> {
    call_families()
        .iter()
        .flat_map(|family| family.input_rels().iter().copied())
        .collect()
}

/// The exact write footprint of an owner/site delta (`apply_call_owner_delta`):
/// it rewrites the owner row + its sites + their resolutions, but leaves
/// `_call_def` alone because the def set is unchanged (the delta bails
/// otherwise). Feeding this to `react` reruns `CallSite`/`CallEdge` and SKIPS
/// `CallName`, whose footprint is `{_call_def}` — the live, non-latent skip.
pub(crate) fn call_owner_delta_rels() -> std::collections::HashSet<&'static str> {
    ["_call_owner", "_call_raw_site", "_call_resolution"]
        .into_iter()
        .collect()
}

/// A captured input dependency: the relation read plus the stable integer
/// primary key of the row read. The engine asks "did any row a family read
/// move?" by intersecting these with a delta's changed keys. Keyed by PK,
/// not vector index, so identity survives reordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DepKey {
    pub rel: &'static str,
    pub pk: i64,
}

/// One emitted output row (a public-relation tuple as `Value` cells).
pub(crate) type OutRow = Vec<Value>;

/// Sink a family emits into. Plain Vec; the router wraps [`reconcile`] around
/// its output to turn a fresh derivation into a [`RowDelta`].
pub(crate) struct RowSink {
    pub rows: Vec<OutRow>,
}

/// A row-level output delta: the rows to retract and the rows to insert to move
/// a relation from its memoized prior state to a fresh derivation. This is the
/// reconcile/render unit — the engine applies it incrementally (retract old +
/// insert new) rather than overwriting the whole relation, so a RETRACTED input
/// row propagates to a retracted output row instead of being silently rebuilt.
#[derive(Debug, Default)]
pub(crate) struct RowDelta {
    pub retracted: Vec<OutRow>,
    pub inserted: Vec<OutRow>,
}

impl RowDelta {
    pub(crate) fn is_empty(&self) -> bool {
        self.retracted.is_empty() && self.inserted.is_empty()
    }
}

/// Type-tagged string identity for one derived row: distinguishes `Int(0)` from
/// `Text("0")` from `Null` so the diff never conflates cells across types. These
/// are set-valued relations, so the whole tuple is the identity.
fn row_key(row: &OutRow) -> String {
    let mut key = String::new();
    for cell in row {
        match cell {
            Value::Int(n) => {
                key.push('i');
                key.push_str(&n.to_string());
            }
            Value::Text(s) => {
                key.push('t');
                key.push_str(s);
            }
            Value::Null => key.push('n'),
        }
        key.push('\u{1}');
    }
    key
}

/// Diff a fresh derivation `new` against the memoized `old`, returning the
/// minimal retract+insert delta by full-tuple identity. Deduplicates `new`
/// (set semantics). Empty delta = the derivation is unchanged.
pub(crate) fn reconcile(old: &[OutRow], new: Vec<OutRow>) -> RowDelta {
    let old_keys: HashSet<String> = old.iter().map(row_key).collect();
    let mut new_keys: HashSet<String> = HashSet::with_capacity(new.len());
    let mut inserted = Vec::new();
    for row in new {
        let key = row_key(&row);
        let fresh = new_keys.insert(key.clone());
        if fresh && !old_keys.contains(&key) {
            inserted.push(row);
        }
    }
    let retracted = old
        .iter()
        .filter(|row| !new_keys.contains(&row_key(row)))
        .cloned()
        .collect();
    RowDelta { retracted, inserted }
}

/// Cross-family scan cache for one `react_deltas` flip. Multiple families in
/// the registry can issue the byte-identical `(rel, pk_col, cols)` read in
/// the same tick — `CallEdge` and `CallEdgeRev` (`call_edge.rs`,
/// `call_edge_rev.rs`) both scan `_call_owner`, `_call_raw_site`, and
/// `_call_resolution` with IDENTICAL column lists, because they reconstruct
/// the same support keys and differ only in whether `rev` survives into the
/// output. Without this cache, every tick that touches those three owned
/// tables (i.e. essentially any edit that adds, moves, or removes a call
/// site) re-runs that full-table SQL scan twice — once per family — even
/// though the second run can only ever reproduce the first's rows. Keyed by
/// the exact request (`rel` + `pk_col` + `cols`, joined); scoped to one
/// `react_deltas` call via `FamilyRouter::react_deltas` and dropped at its
/// end, since a later tick's rows may differ and nothing here may outlive one
/// flip.
///
/// Keyed two levels deep, `rel` then column list, rather than by one
/// concatenated string: a composite key belongs in two positions, not folded
/// into one text field with a separator (`.dl/composite-key-string.dl`).
#[derive(Default)]
pub(crate) struct ScanCache {
    entries: HashMap<&'static str, HashMap<String, Vec<(i64, OutRow)>>>,
}

impl ScanCache {
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

/// Tracked read context. Every `scan` records a `DepKey` per row read, so the
/// engine learns a family's inputs by intercepting reads (the MobX/SolidJS
/// `computed` model), not from a declared dep array (the React `useMemo`
/// model, which undercaptures — the alias-bug class).
///
/// `scan_ns` accumulates the wall time spent inside `scan`'s SQL round trip
/// across every call a `derive` makes (cache hits excluded — see below);
/// `cache_hits` counts how many of those calls were instead served from a
/// shared [`ScanCache`]. The corpus-scaling profiling harness
/// (`tests/it/family_scaling_probe.rs`) reads both back through
/// [`derive_family_timed`]'s `DeriveTiming` to attribute a family's cost
/// between "reading the owned table", "in-process joins/grouping over the
/// rows", and "reused another family's read this tick".
pub(crate) struct Ctx<'a> {
    db: &'a Db,
    deps: HashSet<DepKey>,
    scan_ns: u128,
    cache_hits: usize,
    cache: Option<&'a mut ScanCache>,
}

impl<'a> Ctx<'a> {
    pub(crate) fn new(db: &'a Db) -> Self {
        Self { db, deps: HashSet::new(), scan_ns: 0, cache_hits: 0, cache: None }
    }

    /// Same as `new`, but shares `cache` with every other family derived in
    /// the same `react_deltas` flip — see [`ScanCache`].
    pub(crate) fn new_cached(db: &'a Db, cache: &'a mut ScanCache) -> Self {
        Self { db, deps: HashSet::new(), scan_ns: 0, cache_hits: 0, cache: Some(cache) }
    }

    /// Scan an internal table: return each row's integer PK plus the requested
    /// columns as `Value` cells. Records a `DepKey { rel, pk }` for every row
    /// returned. Columns are read as `Option<i64>`; NULL becomes `Value::Null`
    /// (the `_call_*` schema is `NOT NULL` on the PK and most sid columns, but
    /// `classification_sid` and `unique_sym_sid` are nullable).
    ///
    /// Unconditional `SELECT {cols} FROM {rel}` on a cache miss — every call
    /// reads the WHOLE owned table, not just the rows a triggering delta
    /// touched. This is the corpus-proportional cost the scaling probe
    /// measures: a family whose footprint intersects one changed relation
    /// re-scans that relation's full row count on every affected tick,
    /// regardless of how many rows actually moved. A shared [`ScanCache`]
    /// (when present) collapses a byte-identical repeat request within the
    /// same flip to a clone of the first family's rows instead of a second
    /// full-table read; it does not change the per-flip cost's dependence on
    /// total corpus size, only how many times that cost is paid.
    pub(crate) fn scan(
        &mut self,
        rel: &'static str,
        pk_col: &str,
        cols: &[&str],
    ) -> Result<Vec<(i64, OutRow)>> {
        let mut col_list = String::with_capacity(pk_col.len() + cols.len() * 8);
        col_list.push_str(pk_col);
        for c in cols {
            col_list.push(',');
            col_list.push_str(c);
        }
        // The cache key is (`rel`, `col_list`) — `col_list` alone collides
        // across tables that happen to share a column name set (e.g. two owned
        // tables both scanned by their `site_id` PK).
        if let Some(cache) = &self.cache {
            if let Some(cached_rows) =
                cache.entries.get(rel).and_then(|by_cols| by_cols.get(col_list.as_str()))
            {
                let cached_rows = cached_rows.clone();
                for (pk, _) in &cached_rows {
                    self.deps.insert(DepKey { rel, pk: *pk });
                }
                self.cache_hits += 1;
                return Ok(cached_rows);
            }
        }
        let sql = format!("SELECT {col_list} FROM {rel}");
        let started = Instant::now();
        let rows: Vec<(i64, OutRow)> = self.db.query_rows(rel, &sql, &[], |row| {
            let pk: i64 = row.get(0)?;
            let mut out = Vec::with_capacity(cols.len());
            for i in 0..cols.len() {
                let cell: Option<i64> = row.get(i + 1)?;
                out.push(cell.map(Value::Int).unwrap_or(Value::Null));
            }
            Ok((pk, out))
        })?;
        self.scan_ns += started.elapsed().as_nanos();
        for (pk, _) in &rows {
            self.deps.insert(DepKey { rel, pk: *pk });
        }
        if let Some(cache) = &mut self.cache {
            cache.entries.entry(rel).or_default().insert(col_list, rows.clone());
        }
        Ok(rows)
    }
}

/// A derived relation. The family declares its name, its output columns, the
/// input relations it reads, and writes one pure `derive` body that reads
/// inputs through `Ctx` and emits output rows. No delta method, no reproject,
/// no preflight, no central registration: the engine owns those, and the
/// family self-registers with `register_family!`.
///
/// The four declared slots are the whole authoring surface. `out_cols` lets the
/// render write the public rel generically (no per-family `match name` arm);
/// `input_rels` lets the changed-set union be framework-computed (no
/// hand-maintained `call_input_rels`). This is the v3 min-author-ops
/// `(1,0,0)` shape: a new family is one file, zero central edits.
pub(crate) trait Family: Send + Sync {
    fn name(&self) -> &'static str;
    /// The public relation's column names, in emit order. The render writes
    /// `tbl(name)` from these, so adding a family needs no routing-arm edit.
    fn out_cols(&self) -> &'static [&'static str];
    /// Every internal relation this family's `derive` scans. The engine unions
    /// these across families to build the full changed-set for a cold refresh.
    fn input_rels(&self) -> &'static [&'static str];
    fn derive(&self, ctx: &mut Ctx, out: &mut RowSink) -> Result<()>;
}

/// Self-registration cell. Each family file emits one `register_family!(TYPE)`
/// (a wrapped `inventory::submit!`), so the family set is collected at binary
/// init with zero central edits. `call_families` iterates this registry.
pub(crate) struct FamilyReg(pub &'static dyn Family);
inventory::collect!(FamilyReg);

/// Register a zero-sized family unit struct into the [`FamilyReg`] inventory.
/// `&$ty` const-promotes to `'static` (the struct is a unit literal), then
/// unsizes to `&'static dyn Family`.
macro_rules! register_family {
    ($ty:ident) => {
        inventory::submit! { $crate::engine::family::FamilyReg(&$ty) }
    };
}
pub(crate) use register_family;

/// Wall-time breakdown of one `derive_family` call: `total_ns` is the whole
/// `Family::derive` invocation, `scan_ns` is the portion of it spent inside
/// `Ctx::scan`'s SQL round trips that actually hit the database (cache hits
/// excluded). `total_ns - scan_ns` is the family's in-process compute
/// (building HashMaps, joining, grouping) over the rows `scan` already
/// returned, PLUS any cache-hit clone cost. `cache_hits` counts how many of
/// this derive's `scan` calls were served from the flip's shared
/// [`ScanCache`] instead of running SQL. Test-only surface, read through
/// `FamilyRouter::react_deltas` and `Engine::call_router_last_timings`
/// (`router.rs`) by the corpus-scaling profiling harness.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DeriveTiming {
    pub total_ns: u128,
    pub scan_ns: u128,
    pub cache_hits: usize,
}

/// Run one family's derive against `db`, returning its output rows and the
/// input dependencies it captured. Cold-load and rederive use the same path;
/// the difference is only which inputs are present. Existing call sites
/// (`storage/call.rs`'s unit tests, `router.rs`'s `cold`/`react`) keep this
/// exact 2-tuple contract; [`derive_family_timed_cached`] is the
/// timing-and-cache-carrying twin used only where the extra breakdown is
/// read.
pub(crate) fn derive_family(db: &Db, family: &dyn Family) -> Result<(Vec<OutRow>, HashSet<DepKey>)> {
    let mut ctx = Ctx::new(db);
    let mut sink = RowSink { rows: Vec::new() };
    family.derive(&mut ctx, &mut sink)?;
    Ok((sink.rows, ctx.deps))
}

/// [`derive_family`] plus a [`DeriveTiming`] breakdown, and every `Ctx::scan`
/// call checks `cache` first: a byte-identical `(rel, pk_col, cols)` request
/// already served earlier in the same `react_deltas` flip is cloned from
/// `cache` instead of re-running SQL. Used by `FamilyRouter::react_deltas`,
/// which owns one `ScanCache` per flip and lends it to every family it
/// reruns, in registry (sorted-name) order — see [`ScanCache`] for why
/// `CallEdge`/`CallEdgeRev` are the pair this actually collapses.
pub(crate) fn derive_family_timed_cached(
    db: &Db,
    family: &dyn Family,
    cache: &mut ScanCache,
) -> Result<(Vec<OutRow>, HashSet<DepKey>, DeriveTiming)> {
    let mut ctx = Ctx::new_cached(db, cache);
    let mut sink = RowSink { rows: Vec::new() };
    let started = Instant::now();
    family.derive(&mut ctx, &mut sink)?;
    let timing =
        DeriveTiming { total_ns: started.elapsed().as_nanos(), scan_ns: ctx.scan_ns, cache_hits: ctx.cache_hits };
    Ok((sink.rows, ctx.deps, timing))
}

// --- Extraction-family rel-name inventories -----------------------------
// The per-family reserved relation lists, relocated from `engine/mod.rs`
// (decomposition plan step 7) to live beside the family registry. Consumed
// via `engine::*` re-exports; declare/decls/tick loop over them each tick.

/// The module-graph relations (modgraph.rs). Reserved like BUILTIN_RELS, declared
/// every tick, but populated by `refresh_module_rels` only when the program
/// references one (resolution parses every file, so it is lazy). `module_edge` is
/// the 2-col convenience closure edge; `module_edge_rev` is the rev-aware form.
pub(crate) const MODULE_RELS: [&str; 10] = [
    "module_import",
    "module_edge",
    "module_edge_rev",
    "module_unresolved",
    "module_unresolved_rev",
    "crate_edge",
    "module_binding_resolved_rev",
    "module_binding_resolved",
    "module_binding_rev",
    "module_binding",
];

/// Syntax-only type graph. `kind` is edge metadata; closure(type_edge) walks
/// the first two columns. `type_edge`/`type_edge_rev` are name-keyed (the
/// historic contract) with a trailing `repo` column so two trees scanned in
/// the same engine instance that happen to share a type name (e.g. two
/// frozen prior versions of the same crate) don't collapse into one node —
/// the column is appended last specifically so it never shifts `from`/`to`
/// out of cols[0]/cols[1]. The sem-style additions are def-keyed: `type_entity`
/// is the declared-symbol table (kind, parent, location), `type_sig` is each
/// callable's arrow `[...A] => B` exploded by slot, and `type_link` is the
/// SCIP-resolved graph where endpoints are definition symbols, not bare names
/// (already repo-prefixed via type_entity's sym, so it doesn't need its own
/// repo column).
pub(crate) const TYPE_RELS: [&str; 7] = [
    "type_edge",
    "type_edge_rev",
    "type_entity",
    "type_entity_rev",
    "type_sig",
    "type_link",
    "type_link_rev",
];

/// Phase D diet-SCIP call graph. `call_def` is each callable (sym, kind, file,
/// span); `call_site` is each call occurrence (caller sym, callee text, file,
/// line); `call_edge` is the resolved closure edge; `call_edge_rev` is the
/// rev-aware source of truth (same split as type_edge / type_edge_rev).
/// `call_kind` is the per-fn read/write classification of those call sites,
/// keyed by the bare callee name (execute/query_row/etc.) so a rail can join
/// on `write` only. Symbols are `file::kind::name`, the same shape
/// `type_entity` uses, so the call and type graphs share nodes and a join
/// reaches both.
pub(crate) const CALL_RELS: [&str; 7] = [
    "call_def",
    "call_def_rev",
    "call_site",
    "call_edge",
    "call_edge_rev",
    "call_name",
    "call_kind",
];

/// Intra-procedural dataflow lift: `df_node(id, kind, var, fn, file, line)` is a
/// value-bearing program point, `df_edge(from, to)` is local value flow. A rule
/// `df_reaches(a,b) <- closure(df_edge)` walks the lifted graph on the shared SCC
/// engine. `loop_over` records each loop's span + variable for the
/// loop-invariant-call flag; `allocates` marks fns whose body builds a
/// collection; `nest(call_id, loop_id, depth, collection)` records each call's
/// enclosing loop nest, composing over `call_edge` into symbolic Big-O
/// ("depth-N over C") without resolving trip counts. `df_arg` records which
/// positional slot an argument value feeds (receiver = -1); `df_field` is named
/// value flow into a composite (struct-literal field, object-literal property,
/// Kotlin named argument). See `typegraph::DataflowFacts`.
/// `df_node`/`df_node_repo`/`df_arg`/`df_field` gain `_rev` twins (D5.4): the
/// diff-consumed df rels carry `rev` as a real trailing column, with the node
/// id reusing the same interned id as the legacy rel; `df_node_rev` keys on
/// `(id, rev)` so two revs stay disjoint without folding both into one string.
/// The legacy rels
/// keep raw ids (single-rev daemon sees today's behavior). `df_edge`/`loop_over`/
/// `allocates`/`nest`/`df_param` stay WORK-only (flow/perf inputs, deferred).
/// `df_lit`/`df_lit_rev` (string-values arc, item 1): one row per STRING-
/// carrying `df_node` (kind lit/template/concat) with its cooked/raw text;
/// same `rev`-as-a-column shape as `df_field`/`df_field_rev`. See
/// `typegraph::DataflowFacts::lits`.
pub(crate) const DATAFLOW_RELS: [&str; 15] = [
    "df_node",
    "df_node_rev",
    "df_node_repo",
    "df_node_repo_rev",
    "df_edge",
    "loop_over",
    "allocates",
    "nest",
    "df_param",
    "df_arg",
    "df_arg_rev",
    "df_field",
    "df_field_rev",
    "df_lit",
    "df_lit_rev",
];

/// Document structure from non-source text (markdown today; comments and other
/// tree-sitter grammars to follow via `ingest::IngestLang`). `doc_node` is one row
/// per heading / code block / section: (file, line, kind, name, parent). The
/// `parent` column is the enclosing heading text, so a rule can walk the section
/// tree. `doc_ref` is the doc→code bridge: (file, line, sym) where a heading's
/// name matches a `type_entity` name. Populated by the `ingest` registry over
/// `_file`'s document-typed files (a source rule scanning `**/*.md` feeds `_file`,
/// same as the source langs).
pub(crate) const DOC_RELS: [&str; 2] = ["doc_node", "doc_ref"];

/// Doc comments attached to declared entities (Tier 1/2 doc gen). `doc_comment`
/// is one row per documented `type_entity`: (repo, sym, line, text), the cleaned
/// block bound to the same sym. `doc_tag` is the structured split: (repo, sym,
/// tag, arg, text) where tag is `param`/`returns`/`deprecated`/`section`/... .
/// Both are populated in `refresh_type_rels` from the one parse that already
/// builds `type_entity`, by the per-language AST locators in `typegraph`.
pub(crate) const DOC_TEXT_RELS: [&str; 2] = ["doc_comment", "doc_tag"];

/// String values folded from `const`/`as const` bindings (string-values arc,
/// item 3): `const_value(repo, sym, field, text, kind, file, line)` — one row
/// per string-valued leaf, `sym` the owning `type_entity` (the const itself,
/// or the enum for a string member), `field` a dotted key path ("" for a bare
/// const). `const_value_rev` is the rev-carrying twin (rev is a plain trailing
/// column, like `type_entity_rev` — sym never collides across revs the way a
/// line-keyed df id does, so no id-salting here). Both ride `refresh_type_rels`
/// (the same TypeFacts parse `doc_comment` rides), so a program that asks for
/// either gates the type family the same way `doc_text_rels_used` does. `line`
/// is 1-based (rustc/tsc convention), same as `type_entity.line`.
pub(crate) const CONST_VALUE_RELS: [&str; 2] = ["const_value", "const_value_rev"];

/// Every comment in every parsed file as a grammar-backed fact:
/// `comment_node(path, line, col, end_line, end_col, text, kind)`. Unlike
/// `doc_comment` (which rides the TypeLang parse and covers only the three
/// TypeLang languages' DOC comments bound to an entity), `comment_node` is its
/// OWN family: it records EVERY comment — line, block, and doc — across the
/// oxc TS/TSX front-end AND every tree-sitter grammar the `ast` op loads
/// (Rust, Kotlin, Python, Go, C, bash, ...). `line`/`col` are 1-based line,
/// 0-based byte column (the `sg`/`diag` convention); `text` is the comment body
/// with tokens stripped; `kind` ∈ line | block | doc. String-literal safe: a
/// `//` inside a string is lexed as string content, never a comment row. The
/// eslint/biome suppression grammar (`std/suppress.dl`) is pure dl over this.
pub(crate) const COMMENT_RELS: [&str; 1] = ["comment_node"];

/// Every template literal in every TS/TSX/JS/JSX/MJS/CJS file, split into its
/// ordered static/interpolated pieces:
/// `template_parts(file, line, node, idx, kind, text)`. Own family (rides the
/// oxc parse `TsTypes` already does, but is not gated behind `type`/`call`/
/// `dataflow` — a program reading only `template_parts` shouldn't pay for
/// those passes). `node` groups a template literal occurrence's pieces (the
/// byte offset of its own span start, stable across ticks for unchanged
/// content); `idx` orders them 0-based; `kind` is `static` | `expr`; `text` is
/// the static chunk verbatim (raw, unescaped) or the interpolated expression's
/// exact source text. `line` is 1-based (the `comment_node`/`sg`/`diag`
/// convention). Template-built import paths / URLs / route keys become
/// joinable: `template_parts(file, _, node, 0, "static", "GET /users/"), ...`.
/// Kotlin string templates and Rust `format!`-style macros are OUT of scope
/// (Rust has no native template-literal syntax); this family emits nothing
/// for either language rather than guessing at a shape.
pub(crate) const TEMPLATE_RELS: [&str; 1] = ["template_parts"];

/// Every runtime-computed edge marker in every TS/TSX/JS/JSX/MJS/CJS file:
/// `unresolved(file, line, reason, detail)`. Own family (rides the oxc parse,
/// not gated behind `type`/`call`/`dataflow`/`module`, matching
/// `template_parts`). Distinguishes "an edge exists but its target is
/// computed at runtime" from `module_unresolved`'s "no edge exists" (a
/// specifier that resolved to no project file at all) — this rel does NOT
/// replace `module_unresolved`, it is a separate, generic surface for the
/// runtime-computed flavor. `line` is 1-based (the `comment_node`/`sg`/`diag`
/// convention); `detail` is the computed thing's exact source text, verbatim.
/// `reason` is a closed v1 vocabulary, each bucket re-derived from an AST
/// shape another pass in this codebase already visits for a different
/// purpose: `dynamic-import` (`import(expr)` / `require(expr)` whose argument
/// isn't a plain string literal), `computed-member-call` (`obj[key]()` — the
/// call-site walk already sees this callee shape and silently drops it),
/// `spread-call-args` (`f(...args)` — the dataflow arg walk already sees a
/// spread argument and silently drops it). TS/TSX/JS/JSX/MJS/CJS only in v1;
/// Python star-imports and `sys.path` mutation stay out (already surfaced via
/// `module_unresolved` / a loud eprintln respectively) to avoid a
/// cross-family digest dependency — see `typegraph::UnresolvedRef`.
pub(crate) const UNRESOLVED_RELS: [&str; 1] = ["unresolved"];

// The git-derived families `changed` / `changed_line` / `created`, the analysis
// families `agent` / `dl_diag` / `type_shape` / `type_lgg` / catalog, the SCIP
// importer `scip_*`, the clone proposers `propose_extract` / `propose_clone`,
// and the embedding `similar` now live behind `trait RelKind` in the `rels`
// module dir (decls + gate + refresh per family, one registry the
// tick/declare/guard sites loop over).

/// Ref-spine query relations: thin views over the `_strings` / `_where_bytes`
/// meta tables. `string(id, text, norm)` resolves an interned StringId to its
/// content; `ref(id, string, file, lo, hi)` locates each interned string's byte
/// span, `id` being the `_where_bytes` id (the rewrite coordinate an `edit` keys
/// off). Join them to ask "where does <text> occur": `string(s, "Foo", _),
/// ref(_, s, f, lo, hi)`. Populated for regex/ast/sg captures and import refs.
pub(crate) const SPINE_RELS: [&str; 2] = ["string", "ref"];

/// CST-as-relation (christmas #3): every NAMED tree-sitter node of every scanned
/// file as a row. `node(id, kind, file, lo, hi, parent)` — `id`/`parent` are
/// kind-salted `_where_bytes` ids (so `ref(id, sid, _, lo, hi)` ->
/// `string(sid, text, _)` recovers each node's source bytes); `file` is the
/// content FileId, `kind` the tree-sitter node kind, `[lo, hi)` the byte span.
/// `child(parent, child)` is the 2-col edge so `anc(a,b) <- closure(child).`
/// gives ancestor/descendant with the engine's existing recursion. Populated by
/// `refresh_node_rels` over the whole tree (no query) when the rels are used.
pub(crate) const NODE_RELS: [&str; 2] = ["node", "child"];

/// Daemon-state query relations: thin views over the persisted `_program` /
/// `_ref` / `_rev_log` meta tables, so a dashboard can ask the warm engine what
/// it loaded and which watched refs have moved. `program(path, hash, mtime)` is
/// the loaded `.dl` file set; `head(repo, name, oid)` is the last-seen oid of
/// every watched ref (HEAD plus each program-scanned rev); `rev_advanced(repo,
/// name, old, new)` is the advance log the daemon appends when a watched ref
/// moves. Populated by `refresh_daemon_rels`; the daemon writes the underlying
/// tables via `save_program_meta` / `save_repos_meta` / `observe_ref`.
pub(crate) const DAEMON_RELS: [&str; 3] = ["program", "head", "rev_advanced"];

/// The clock relation. `every(secs)` is an engine-populated source rel that holds
/// the interval `N` only on the tick that crosses an `N`-second boundary (and on
/// the first tick), so a body atom `every(30)` self-throttles the rule that joins
/// it. Edge-triggered off wall-clock seconds, bucket-per-N stored in `_carry_meta`
/// (`every:N`), so the cadence is exact regardless of how often the daemon ticks.
pub(crate) const EVERY_RELS: [&str; 1] = ["every"];

/// The persistent clock relation. `clock(secs, bucket)` holds, on EVERY tick, the
/// current bucket `now / secs` for each `secs` period the program names — a
/// monotone integer that advances once per `secs` wall-clock seconds. Unlike the
/// edge-triggered `every` (present only on the boundary tick), `clock` is always
/// present, so a body atom `clock(300, b)` binds `b` to the live bucket and varies
/// any join — or an `@async` request digest — exactly once per period. That is the
/// dl-native cadence primitive: time as a fact you join against, no `@next`
/// counter. Reuses `now_secs`; lazy per `clock_rels_used`.
pub(crate) const CLOCK_RELS: [&str; 1] = ["clock"];

/// The effect-drain audit view: a thin query rel over `pending_effect`, the job
/// table @async/@stream requests land in. One row per distinct request (digest
/// `id`), carrying its template `kind`, the `head` rel it rebuilds, the job
/// `state` (queued|running|done|failed|orphaned), the request `args` JSON (the hole map —
/// the call's parameters, the endpoint analog), and `req_tx` (the tx it was
/// queued at). This is the dl-native call log: `? effect_log(...)` shows the
/// drain queue live, and it doubles as the parity surface against ghcacher's
/// `call_log`. Lazy like every other built-in group; a program that never reads
/// it pays nothing (`pending_effect` is still written, just not projected).
pub(crate) const EFFECT_RELS: [&str; 1] = ["effect_log"];

/// The diagnostic sink. Unlike every other built-in, `diag` is engine-declared
/// but USER-WRITTEN: a rule heads it to emit an editor squiggle (`--lsp`), a
/// check finding (`--check` exit code), or a daemon-hook message. Fixed 9-col
/// schema (was a magic user-declared name whose columns the engine mapped by
/// NAME — the merged `.dl/` namespace collided when two files declared it with
/// different columns). Write only the columns you need via named args
/// (`diag(path: p, line: l, msg: m) <- ...`); the rest lower to NULL and take
/// defaults in `Engine::diags` (severity "warn", end_line = line, ints 0). Read
/// only, never populated by a refresh — `rebuild_derived` fills it from the
/// program's rules like any other derived rel.
pub(crate) const DIAG_RELS: [&str; 1] = ["diag"];

/// The diag-stage routing sink. Same shape as `diag` (engine-declared,
/// USER-WRITTEN): a rail heads `diag_stage(code, stage)` to route a diagnostic
/// code to a surface (live / commit / agent-turn / agent-session). Fixed 2-col
/// schema. Read only, never populated by a refresh — `rebuild_derived` fills it
/// from the program's rules like any other derived rel. Presentation-time
/// filtering only; the db keeps every `diag` row. See R7 (src/stage.rs).
pub(crate) const DIAG_STAGE_RELS: [&str; 1] = ["diag_stage"];

/// The hover-note sink. Same shape as `diag` (engine-declared, USER-WRITTEN): a
/// rule heads `hover_note(path, line, col, end_line, end_col, md)` to attach
/// markdown to a source span; the LSP hover path appends each matching row's
/// `md` to the hover it synthesizes at that position. Positions are 0-based,
/// the same convention as `diag`. Fixed 6-col schema. Read only, never
/// populated by a refresh — `rebuild_derived` fills it from the program's
/// rules like any other derived rel; a program that never heads it leaves the
/// table empty (or undeclared, tolerated by `Engine::hover_notes_at`).
pub(crate) const HOVER_RELS: [&str; 1] = ["hover_note"];

/// The drawable-graph SINK relations. A user HEADS these from a rule (like
/// `diag`) to emit a graph the flow panel draws with ZERO bespoke SQL:
/// `graph_node(id, label, kind, file, line, parent)` is one vertex,
/// `graph_edge(src, dst, kind)` one edge. Fixed schema so any program's graph
/// composes into the same two tables the panel's always-available "Graph"
/// preset reads (`rel_graph_node` / `rel_graph_edge`). Pre-declared (catalogued,
/// so the binding shows in `rel_catalog`) and reserved against a `rel`
/// re-declaration — head them directly, name only the columns you use (the rest
/// lower to NULL: no file/line/parent = an unplaced, unnested node). Read only,
/// never populated by a refresh — `rebuild_derived` fills them from the
/// program's rules like any other derived rel, so an unheaded program leaves
/// them empty (and the preset shows the "nothing to draw" hint).
pub(crate) const GRAPH_RELS: [&str; 2] = ["graph_node", "graph_edge"];

/// The harness-hook event log. `hook_event(kind, session, seq, json)` accumulates
/// one row per coding-agent hook invocation (`dl --hook`): kind = the harness
/// event name (UserPromptSubmit / PostToolUse / ...), session = the event's
/// session id, seq = an ingest-time monotone millis stamp (orders events within a
/// session), json = the raw event JSON. Rows are written out-of-tick by the
/// `hook_event` RPC / the in-process feed, never by a refresh; a program extracts
/// fields with the term-form `json`/`jsonp` predicates, mirroring how
/// `mcp_request` carries raw JSON. Lazy per `hook_rels_used`.
pub(crate) const HOOK_RELS: [&str; 1] = ["hook_event"];

/// The diagnostic-mute set. `diag_mute(code)` holds one row per diagnostic code
/// the editor session has silenced. Engine-owned and WRITABLE, but only through
/// `toggle_diag_mute` (the LSP `dl.toggleDiagCode` command), never a rule head —
/// so it mirrors `hook_event`'s out-of-tick write shape, not `diag`'s
/// rule-headed one. Rows persist in the db, so a mute survives a daemon restart.
/// Read at the LSP publish seam to drop muted `diag` rows before they reach the
/// editor; `--check` / `--parse-only` read `diag` directly and are UNAFFECTED
/// (mute is an editor affordance, not a CI gate — see the lsp.rs module doc).
pub(crate) const MUTE_RELS: [&str; 1] = ["diag_mute"];

/// The demand / overlay SINK relations. A user HEADS these from a rule (like
/// `diag` / `repo`), and the rows drive engine behavior the name is bound to:
/// `scip_want` → SCIP index demand, `rev_cmp_want` → git ancestry demand,
/// `def_target` → LSP go-to-definition, `effect_cmd` → per-kind effect-template
/// overlay, `checkout` → git checkout sweep (clone-missing + fetch +
/// fast-forward the default branch to origin, the ghcacher keep-current half).
/// Pre-declared builtins (so the binding shows in `rel_catalog` /
/// `dl docs relations`) and reserved against a `rel` re-declaration — head them
/// directly, do not `rel`-declare them, exactly like `diag`. This is what makes
/// them first-class instead of magic: the engine reading them by name is reading
/// a catalogued builtin, not an undocumented convention. See docs/reference/
/// magic-rels.md and the `.dl/magic-rel-audit.dl` rail.
pub(crate) const DEMAND_RELS: [&str; 5] = [
    "scip_want",
    "rev_cmp_want",
    "def_target",
    "effect_cmd",
    "checkout",
];

/// The derived-shape SINK relation. A user HEADS `type_decl_row(shape, pos, col,
/// ty)` from a rule (like `diag` / `graph_node`) to DERIVE a relation schema from
/// data — column names + base types computed by rules rather than written by
/// hand. The engine consumes it across a one-tick phase delay: at the end of a
/// tick its rows persist to the `_shapes` meta table; on the NEXT tick's declare,
/// a `rel name: shape.` decl whose shape has no syntax `type name(...)` decl
/// resolves its columns from the persisted rows (a `shape-pending` info diag until
/// then). Syntax shapes win on a name clash (`shape-shadowed` warn). Pre-declared
/// (catalogued, group "types") and reserved against a `rel` re-declaration — head
/// it directly, like diag. Derived-only: it must be filled by a derived rule (a
/// term-extract rule feeding it must route through its own rel first, the repo
/// mixed-kind law).
pub(crate) const TYPE_DECL_RELS: [&str; 1] = ["type_decl_row"];


#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::engine::Engine;
    use std::fs;
    use std::path::PathBuf;

    // ---- reconcile: diff-engine unit tests (T1, capstone retraction plan) ----
    //
    // `Value` derives only `Clone, Debug` (no `PartialEq`), so these tests
    // compare rows via `row_key` — the same full-tuple identity function
    // `reconcile` itself uses — rather than `assert_eq!` on raw `OutRow`s.

    /// Build an all-`Int` `OutRow` from plain integers (the common case below).
    fn int_row(cell_values: &[i64]) -> OutRow {
        cell_values.iter().map(|value| Value::Int(*value)).collect()
    }

    /// `row_key` per row, in argument order (both `RowDelta.retracted` and
    /// `.inserted` are built in deterministic input order, never hashset
    /// iteration order, so an order-preserving comparison is exact — no sort
    /// needed and no accidental laundering of a real ordering bug).
    fn row_keys(rows: &[OutRow]) -> Vec<String> {
        rows.iter().map(row_key).collect()
    }

    #[test]
    fn reconcile_prev_equals_next_is_empty_delta() {
        let prev = vec![int_row(&[1, 2]), int_row(&[3, 4])];
        let next = prev.clone();
        let delta = reconcile(&prev, next);
        assert!(delta.is_empty(), "identical prev/next must reconcile to an empty delta");
    }

    #[test]
    fn reconcile_prev_empty_is_all_inserts() {
        let prev: Vec<OutRow> = Vec::new();
        let next = vec![int_row(&[1, 2]), int_row(&[3, 4])];
        let delta = reconcile(&prev, next.clone());
        assert!(delta.retracted.is_empty(), "never-derived prev has nothing to retract");
        assert_eq!(
            row_keys(&delta.inserted),
            row_keys(&next),
            "never-derived prev must insert every fresh row"
        );
    }

    #[test]
    fn reconcile_next_empty_is_all_retracts() {
        let prev = vec![int_row(&[1, 2]), int_row(&[3, 4])];
        let next: Vec<OutRow> = Vec::new();
        let delta = reconcile(&prev, next);
        assert!(delta.inserted.is_empty(), "everything-gone next has nothing to insert");
        assert_eq!(
            row_keys(&delta.retracted),
            row_keys(&prev),
            "everything-gone next must retract every prior row"
        );
    }

    #[test]
    fn reconcile_disjoint_sets_full_retract_and_full_insert() {
        let prev = vec![int_row(&[1, 1]), int_row(&[2, 2])];
        let next = vec![int_row(&[3, 3]), int_row(&[4, 4])];
        let delta = reconcile(&prev, next.clone());
        assert_eq!(row_keys(&delta.retracted), row_keys(&prev), "disjoint sets retract all of prev");
        assert_eq!(row_keys(&delta.inserted), row_keys(&next), "disjoint sets insert all of next");
    }

    #[test]
    fn reconcile_overlap_is_exact_set_difference_both_ways() {
        let shared_row = int_row(&[1, 1]);
        let prev_only_row = int_row(&[2, 2]);
        let next_only_row = int_row(&[3, 3]);
        let prev = vec![shared_row.clone(), prev_only_row.clone()];
        let next = vec![shared_row, next_only_row.clone()];
        let delta = reconcile(&prev, next);
        assert_eq!(
            row_keys(&delta.retracted),
            row_keys(&[prev_only_row]),
            "only the prev-only row retracts; the shared row is untouched"
        );
        assert_eq!(
            row_keys(&delta.inserted),
            row_keys(&[next_only_row]),
            "only the next-only row inserts; the shared row is untouched"
        );
    }

    /// Pinned semantic: `reconcile` treats `new` as a SET. A row repeated in
    /// `new` collapses to one insertion — the 2nd/3rd copy is neither
    /// re-inserted nor retracted. `HashSet<String>::insert` on `row_key` is
    /// the dedup mechanism (see `reconcile`'s `fresh` check).
    #[test]
    fn reconcile_duplicate_rows_in_next_dedupe_to_one_insert() {
        let prev: Vec<OutRow> = Vec::new();
        let repeated_row = int_row(&[1, 1]);
        let next = vec![repeated_row.clone(), repeated_row.clone(), repeated_row.clone()];
        let delta = reconcile(&prev, next);
        assert_eq!(
            row_keys(&delta.inserted),
            row_keys(&[repeated_row]),
            "pinned: 3 identical `new` rows dedupe to a single insert, not one per copy"
        );
    }

    /// Pinned semantic: `row_key` tags each cell with its `Value` variant
    /// (`i`/`t`/`n` prefix) before appending the payload, so `Int(1)` and
    /// `Text("1")` in the same column hash to different keys. A column that
    /// only differs by type affinity is a full-tuple identity difference —
    /// never coerced or compared numerically/textually across variants.
    #[test]
    fn reconcile_type_affinity_difference_is_a_distinct_tuple() {
        let prev = vec![vec![Value::Int(1)]];
        let next = vec![vec![Value::Text("1".to_string())]];
        let delta = reconcile(&prev, next.clone());
        assert_eq!(row_keys(&delta.retracted), row_keys(&prev), "Int(1) row must retract, not match Text(\"1\")");
        assert_eq!(row_keys(&delta.inserted), row_keys(&next), "Text(\"1\") row must insert as a distinct tuple");
    }

    /// Pinned semantic: `row_key` maps every `Value::Null` cell to the same
    /// bare `n` marker with no payload attached, so two otherwise-identical
    /// NULL-bearing tuples produce the SAME key and compare EQUAL for
    /// reconcile identity — NULL == NULL here, the opposite of SQL's
    /// NULL <> NULL. A prev row surviving unchanged into next (both carrying
    /// NULL in the same column) must not spuriously retract+insert.
    #[test]
    fn reconcile_null_bearing_tuples_treat_null_as_equal_to_null() {
        let prev = vec![vec![Value::Int(1), Value::Null]];
        let next = vec![vec![Value::Int(1), Value::Null]];
        let delta = reconcile(&prev, next);
        assert!(delta.is_empty(), "pinned: NULL cells compare equal to NULL for row identity");
    }

    /// Minimal Rust: `beta` calls `alpha` so both `call_site` and the resolved
    /// `call_edge` are non-empty after a real extraction.
    const RUST_SRC: &str = "\
fn alpha() {}
fn beta() {
    alpha();
}
fn gamma() {
    beta();
    alpha();
}
";

    fn fresh_engine(root: &PathBuf) -> Engine {
        let mut engine = Engine::new(db::open(None).unwrap(), root.clone());
        engine.ensure_meta().unwrap();
        engine.declare_builtins().unwrap();
        // `WORK` is an ALIAS resolved at the scan seam; this fixture skips the
        // scan, so resolve it directly and stamp the row with the result.
        engine.resolve_self_rev().unwrap();
        engine
            .db
            .exec_params(
                "_file",
                "INSERT INTO _file (repo, path, rev, hash, mtime, size) \
                 VALUES ('', 'lib.rs', ?1, '', 0, 0)",
                &[SqlVal::from(engine.self_rev_text())],
            )
            .unwrap();
        engine
    }

    /// A fresh engine rooted at `dir` (which must already hold `lib.rs`) with a
    /// `_file` row carrying `hash` as its fact digest. The delta path reads the
    /// `hash` column as the owner's fact digest, so a caller drives a real
    /// working-tree edit by rewriting `lib.rs` on disk and bumping this column.
    fn engine_at(dir: &PathBuf, hash: &str) -> Engine {
        let mut engine = Engine::new(db::open(None).unwrap(), dir.clone());
        engine.ensure_meta().unwrap();
        engine.declare_builtins().unwrap();
        engine.resolve_self_rev().unwrap();
        engine
            .db
            .exec_params(
                "_file",
                "INSERT INTO _file (repo, path, rev, hash, mtime, size) \
                 VALUES ('', 'lib.rs', ?1, ?2, 0, 0)",
                &[SqlVal::from(engine.self_rev_text()), SqlVal::from(hash)],
            )
            .unwrap();
        engine
    }

    fn names(engine: &Engine) -> Vec<[i64; 2]> {
        let mut v: Vec<[i64; 2]> = engine
            .db
            .query_rows(
                "rel_call_name",
                "SELECT sym, name FROM rel_call_name",
                &[],
                |row| Ok([row.get(0)?, row.get(1)?]),
            )
            .unwrap();
        v.sort();
        v
    }

    fn make_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sprf-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn snapshot(engine: &Engine) -> (Vec<[i64; 5]>, Vec<[i64; 3]>) {
        let site: Vec<[i64; 5]> = {
            let mut v: Vec<[i64; 5]> = engine
                .db
                .query_rows(
                    "rel_call_site",
                    "SELECT repo, caller, callee, file, line FROM rel_call_site",
                    &[],
                    |row| Ok([row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?]),
                )
                .unwrap();
            v.sort();
            v
        };
        let edge: Vec<[i64; 3]> = {
            let mut v: Vec<[i64; 3]> = engine
                .db
                .query_rows(
                    "rel_call_edge",
                    "SELECT caller, callee, kind FROM rel_call_edge",
                    &[],
                    |row| Ok([row.get(0)?, row.get(1)?, row.get(2)?]),
                )
                .unwrap();
            v.sort();
            v
        };
        (site, edge)
    }

    /// The real-extraction proof: a genuine `refresh_call_rels` over Rust
    /// source on disk populates `rel_call_site`/`rel_call_edge` non-vacuously
    /// through the family router — the SOLE writer of every public call rel
    /// (P4, capstone cutover; there is no more legacy projection to diff
    /// against).
    #[test]
    fn family_flag_matches_legacy_on_real_extraction() {
        let dir = std::env::temp_dir().join(format!(
            "sprf-family-flip-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("lib.rs"), RUST_SRC).unwrap();

        let mut engine = fresh_engine(&dir);
        engine.refresh_call_rels().unwrap();
        let (site, edge) = snapshot(&engine);
        let _ = fs::remove_dir_all(&dir);

        assert!(
            !site.is_empty(),
            "extraction produced no call_site rows; the rail is vacuous"
        );
        assert!(
            !edge.is_empty(),
            "extraction produced no resolved call_edge rows; the rail is vacuous"
        );

        // The persistent router: refresh_call_rels above cold-derived every
        // family into the engine's cross-tick memo. Drive the LIVE flip method
        // with a _call_resolution-only changed-set: call_edge/call_edge_rev
        // read that table, call_site does not, so a genuine skip must fall
        // out — and it only can if the memo survived the refresh tick (an
        // empty memo would rerun everything via react's None-branch). This
        // exercises the real Engine method + persistent RefCell memo + the
        // router's skip, end to end.
        let mut resolution_only = HashSet::new();
        resolution_only.insert("_call_resolution");
        let rerun = engine.flip_call_rels_via_router(&resolution_only).unwrap();
        assert_eq!(
            rerun,
            vec!["call_edge", "call_edge_rev"],
            "live persistent router reruns the two _call_resolution readers and skips \
             call_site/call_kind/call_name on a _call_resolution-only change"
        );
        let (after_site, after_edge) = snapshot(&engine);
        assert_eq!(after_site, site, "skipped call_site must be untouched");
        assert_eq!(after_edge, edge, "reran call_edge must stay correct");
    }

    /// The live-skip proof on a REAL delta (not a hand-passed changed-set): a
    /// working-tree edit that removes ONE call site while leaving every
    /// definition intact. That is exactly the shape the owner-delta fast path
    /// accepts (`apply_call_owner_delta` requires the def digest unchanged), and
    /// it rewrites `_call_owner`/`_call_raw_site`/`_call_resolution` but never
    /// `_call_def`. So the router, fed the delta's real footprint, reruns
    /// `CallSite`/`CallEdge` and SKIPS `CallName` — the skip pays off in
    /// production, not just in a synthetic changed-set.
    #[test]
    fn family_delta_skips_call_name_on_site_only_change() {
        use crate::rels::PathRefreshContext;

        // v1 -> v2 retargets gamma's second call (`alpha()` -> `beta()`). The
        // edit is line-count preserving, so every def's (line, end) span — hence
        // the owner def digest — is byte-identical and the owner-delta fast path
        // accepts it. call_site/call_edge lose the gamma->alpha tuple; call_name
        // (the def-name projection) is unchanged.
        const V1: &str = "\
fn alpha() {}
fn beta() {
    alpha();
}
fn gamma() {
    beta();
    alpha();
}
";
        const V2: &str = "\
fn alpha() {}
fn beta() {
    alpha();
}
fn gamma() {
    beta();
    beta();
}
";

        // Reference expectation for v2: a fresh engine that extracts v2 directly
        // (no delta path involved) — the oracle the delta-path result below must
        // match.
        let reference_dir = make_dir("family-delta-reference");
        fs::write(reference_dir.join("lib.rs"), V2).unwrap();
        let mut reference = engine_at(&reference_dir, "v2");
        reference.refresh_call_rels().unwrap();
        let (reference_site_v2, reference_edge_v2) = snapshot(&reference);
        let reference_name_v2 = names(&reference);
        let _ = fs::remove_dir_all(&reference_dir);
        assert!(!reference_name_v2.is_empty(), "fixture: v2 has def names");

        // Engine under test: full refresh over v1 builds the baseline AND the
        // persistent router memo (every family cold-derived).
        let dir = make_dir("family-delta");
        fs::write(dir.join("lib.rs"), V1).unwrap();
        let mut engine = engine_at(&dir, "v1");
        engine.refresh_call_rels().unwrap();
        let (site_v1, _edge_v1) = snapshot(&engine);
        let name_v1 = names(&engine);
        assert_eq!(name_v1, reference_name_v2, "defs unchanged across v1/v2");
        assert!(!name_v1.is_empty(), "fixture: v1 has def names");

        // Real working-tree edit: rewrite the file and bump its fact digest.
        fs::write(dir.join("lib.rs"), V2).unwrap();
        engine.db
            .exec_on("_file", "UPDATE _file SET hash = 'v2' WHERE path = 'lib.rs'")
            .unwrap();

        // Drive the genuine incremental path.
        let mut changed_paths = std::collections::HashSet::new();
        changed_paths.insert("lib.rs".to_string());
        let context = PathRefreshContext {
            changed_paths: &changed_paths,
            module_dependency_changed: false,
            module_full_refresh: false,
        };
        let outcome = engine.refresh_call_rels_delta(&context).unwrap();
        assert_eq!(
            outcome,
            crate::rels::CallPathRefreshOutcome::Applied,
            "site-only delta must take the owner-delta fast path",
        );

        // (iii) the reran families are byte-identical to a reference v2 extraction.
        let (site_after, edge_after) = snapshot(&engine);
        assert_eq!(site_after, reference_site_v2, "reran call_site must match reference v2");
        assert_eq!(edge_after, reference_edge_v2, "reran call_edge must match reference v2");
        // The change was real: the gamma->alpha site is gone.
        assert_ne!(site_after, site_v1, "delta must have altered call_site");

        // (ii) the SKIPPED family's public rel is untouched by the delta: the
        // router leaves call_name alone, so it still holds the v1 rows (equal
        // to v2, since defs are identical).
        assert_eq!(names(&engine), name_v1, "skipped call_name rows must be unchanged");

        // (i) the skip itself: replay the router with the delta's real footprint
        // (`react` is a pure function of memo footprints + changed-set, so the
        // replay's rerun set equals what the live delta just used). call_name is
        // absent -> CallName was skipped, not rerun-to-the-same-rows.
        let rerun = engine
            .flip_call_rels_via_router(&crate::engine::family::call_owner_delta_rels())
            .unwrap();
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(
            rerun,
            vec!["call_edge", "call_edge_rev", "call_kind", "call_site"],
            "owner-delta footprint must rerun call_site/call_edge/call_edge_rev/call_kind \
             (every _call_raw_site reader) and SKIP call_name (footprint {{_call_def}})",
        );
    }
}
