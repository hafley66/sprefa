# View-backed declared-rel primitive (2026-07-20)

Storage diet: a declared rel that is a pure rev-deduped projection of its `_rev`
twin holds a full duplicate table today (rows + full-row PK autoindex). Replace
that duplicate with a SQL `VIEW` over the `_rev` twin. Zero rows of its own,
zero autoindex, zero digest churn; queries and `_txt` decode still resolve it.

## Part 1: the primitive

### Type signatures first

```rust
// src/ast.rs — new field on RelDecl (Default = None; only Rust-authored
// built-in decls set it).
pub struct RelDecl {
    // ...existing fields...
    /// A view-backed rel: instead of a base table, `Engine::declare` issues
    /// `CREATE VIEW rel_<name> AS <body>`. The body is authored SQL, always a
    /// `SELECT DISTINCT <non-rev cols> FROM rel_<name>_rev` for the twin-dedup
    /// rels this ships for. No `__src`, no autoindex, no rows, skipped by the
    /// write path and by digest/derived-complete tracking. None = base table.
    pub view_body: Option<String>,
}

// src/ast.rs — new field on RelMeta so the query/index paths can tell a
// view-backed rel from a base table (Default = false).
pub struct RelMeta {
    // ...existing fields...
    pub view_backed: bool,
}

// src/engine/declare.rs — new method.
impl Engine {
    fn declare_view_backed(&mut self, d: &RelDecl, body: &str) -> Result<()>;
    // // pseudo-code:
    // // drop any prior _txt view, then any prior base table OR view of this
    // //   name (a run before conversion left a real table behind):
    // DROP VIEW IF EXISTS rel_<name>_txt;
    // DROP TABLE IF EXISTS rel_<name>;
    // DROP VIEW IF EXISTS rel_<name>;
    // // the view itself:
    // CREATE VIEW rel_<name> AS <body>;
    // // this rel never has rows/digest/completion of its own — clear any stale
    // //   tracking a prior base-table incarnation wrote:
    // DELETE FROM _reldigest WHERE rel IN ('<name>', 'rows:<name>', 'drv:<name>');
    // DELETE FROM _derived_complete WHERE rel = '<name>';
    // // register meta (view_backed = true) so create_auto_indexes skips it and
    // //   lower_rule still joins it by name;
    // self.rels.insert(name, RelMeta { view_backed: true, ..cols/key/... });
    // // build the _txt decode view over the new view (create_rel_view reads
    // //   FROM rel_<name>, which resolves against a view fine):
    // self.create_rel_view(name, meta);
}
```

`Engine::declare` gains a single early branch:

```rust
pub(crate) fn declare(&mut self, d: &RelDecl) -> Result<()> {
    // ...port envelope check (unchanged)...
    if let Some(body) = &d.view_body {
        return self.declare_view_backed(d, body);
    }
    // ...existing base-table migration + CREATE TABLE path...
    // plus one guard in the table path: DROP VIEW IF EXISTS rel_<name> before
    //   CREATE TABLE, so a rel that reverts from view-backed to table-backed
    //   drops the stale view (PRAGMA table_info reports a view's columns too,
    //   so the existing drift check would not catch it).
}
```

`create_auto_indexes` candidate filter — one added predicate:

```rust
// existing: raw.retain(|(rel, _)| !closures.contains_key(rel));
raw.retain(|(rel, _)| {
    !closures.contains_key(rel)
        && !self.rels.get(rel).is_some_and(|m| m.view_backed)
});
```

Without this, a served derived rule joining a converted rel (or `closure(type_edge)`
via `traversal_edge_cols`) proposes `idx_<rel>_<col>`, and `CREATE INDEX ... ON
rel_<rel>(col)` fails on a view.

### Instance lifetimes

- `RelDecl.view_body`: static, authored in `src/engine/decls.rs`; lives as long
  as the decl vec (rebuilt each `declare_builtins`).
- `RelMeta.view_backed`: lives in `Engine.rels` for the engine's lifetime,
  refreshed every `declare_all`.
- The `rel_<name>` VIEW and `rel_<name>_txt` VIEW: persisted schema objects,
  rewritten only when their `CREATE` text changes (create_rel_view already
  string-compares sqlite_master to skip an unchanged DDL rewrite).

### Storage layout, reads and writes, uniqueness

- Base-table rel today: `rel_<name>` (rows + `__src` + full-row PK autoindex on a
  rowid table) + `rel_<name>_txt` decode view + `_reldigest` rows
  (`rows:<name>` etc).
- View-backed rel after: `rel_<name>` = VIEW over `rel_<name>_rev`, no rows, no
  autoindex, no `__src`, no `_reldigest`/`_derived_complete` row. `rel_<name>_txt`
  view unchanged in shape (decodes interned columns from the view).
- Writes: none. The per-rel `refresh_rel`/`append_rel`/`INSERT OR IGNORE` calls
  for the converted rels are deleted (Part 2). Reactivity is preserved because
  the `_rev` twin's own `refresh_rel*` still flips the family's `rows_changed` /
  the family returns `Ok(true)`, so dependents re-derive; the view reads live
  from the twin.
- Uniqueness: the `SELECT DISTINCT` in the body is the uniqueness contract. It is
  row-identical to the old base table ONLY when the old table's PK was the full
  row (INSERT OR IGNORE == DISTINCT) and the Rust direct-write dedup key was the
  full column set. Verified true for all 8 rels below; proven per-rel by the
  two-direction EXCEPT test.

## Part 2: convert 8 proven-safe twin rels

Convert (VIEW over `_rev` twin, delete the old write):

| rel | view body (over `rel_<name>_rev`) |
|---|---|
| df_node_repo | `SELECT DISTINCT "id","repo"` |
| df_arg | `SELECT DISTINCT "call","pos","arg"` |
| df_field | `SELECT DISTINCT "id","field","value"` |
| type_edge | `SELECT DISTINCT "from","to","kind","repo"` |
| module_unresolved | `SELECT DISTINCT "file","specifier","reason","line"` |
| module_binding_resolved | `SELECT DISTINCT "file","local","source","dst"` |
| module_binding | `SELECT DISTINCT "file","local_name","source_module","imported_name","kind"` |
| const_value | `SELECT DISTINCT "repo","sym","field","text","kind","file","line"` |

Delete write path:
- dataflow.rs: the `refresh_rel`/`append_rel` calls for df_node_repo/df_arg/df_field,
  plus their now-unused row collection (`seen_*`, `*_rows`, `DataflowRowSet`
  fields node_repo/arg/field). The `_rev` twin collection stays.
- extract/mod.rs `rebuild_legacy_module_rels`: delete the module_unresolved,
  module_binding_resolved, module_binding INSERTs (KEEP module_edge — reader-blocked).
- extract/mod.rs `rebuild_legacy_type_rels`: delete type_edge and const_value
  INSERTs (KEEP type_entity, type_link — reader-blocked).

### DO NOT touch (documented at the call sites)
- df_node, df_lit: Rust dedup key is the interpolated `id` alone (`seen_node`,
  `seen_lit`), narrower than the declared full-row PK, so a DISTINCT view returns
  extra rows. Not safe.
- type_entity, type_link, module_edge, call_def, call_edge: reader-blocked
  (editors/vscode-dl/media/flow-panel.html hardcodes `type='table'` and reads
  rel_type_entity/rel_type_link/rel_module_edge/rel_call_def/rel_call_edge).

## Test (tests/it) and measurement
- Two-distinct-rev git corpus (HEAD commit + edited WORK), driven to fixpoint via
  the library Engine. For each converted rel with a non-empty `_rev` twin: build
  `old_<name>` with the legacy full-row-PK DDL, `INSERT OR IGNORE ... SELECT cols
  FROM rel_<name>_rev` (the exact pre-change rebuild), assert
  `view EXCEPT old` and `old EXCEPT view` both empty. Assert df_node_repo carries
  a logical row present at BOTH revs (so DISTINCT is actually exercised).
- Measurement: build a sprefa-corpus db under a worktree-local DL_STATE_DIR
  before and after; report `SELECT SUM(pgsize) FROM dbstat`.
