# module_binding: alias-aware imports at the syntactic tier

Opus-planned 2026-07-10. Closes the ledgered gap "Aliased/default imports at the
SYNTACTIC tier": `import { foo as bar }`, `use x::y as z`, `import a.b.C as D` are
invisible to the extraction-time name resolver without an index.scip — name-keyed def
buckets have no bucket for the local alias. scip_binding solves it WITH an index;
this arc is the index-free equivalent, riding the module resolvers' existing parse.

## 1. Findings (file:line)

- Resolver gap: `by_name: HashMap<(&str,&str,&str), Vec<&str>>` at
  src/engine/extract.rs:791-793, `sym_at` at :792, lookup at :808; call twin
  `resolve_callee` at extract.rs:1372-1389. A ref to alias `bar` never hits a bucket.
- SCIP pattern to mirror: `scip_binding` decl src/rels/scip.rs:68-73, populated
  scip.rs:116; anchor union src/anchor.rs:225-242; extraction SCIP override read once
  per family at extract.rs:806/938.
- Module family already parses and DROPS the alias:
  - Rust: `expand_use_leaves` splits `" as "` at src/modgraph.rs:556 and keeps only
    the source leaf; `UseLeaf` (modgraph.rs:494-501) has no alias field.
  - TS/JS: `TsResolver::edges` (modgraph.rs:986-1038) matches only the specifier via
    `ts_spec_re` (modgraph.rs:912-920); the import clause is never parsed. Module
    resolver is regex-only; stay string-level (one-parse rule).
  - Kotlin: `kotlin_import_re` (modgraph.rs:1071-1074) stops before ` as alias`.
- Carrier: `ModuleRef` (modgraph.rs:38-45); all rows built in `module_rows_for_rev`
  (extract.rs:287-353); `ModuleRows` at src/engine/mod.rs:1730-1751; insert paths
  `refresh_module_rels` (extract.rs:404-435), `insert_module_rows` (:355-364),
  `rebuild_legacy_module_rels` (:380-395).
- Rev/digest precedent: `MODULE_RELS` mod.rs:189-196; `module_edge_rev` = source of
  truth, legacy = rev-deduped union (extract.rs:385-388); resolver reads per-rev via
  `module_import_map()` (extract.rs:966-978); digest folds module_edge_rev at every
  rev under with_scip at extract.rs:582-594 (eed54cc); `REV_TWINS` extract.rs:656-661;
  per-rev/per-path DELETEs :463-466 and :486-497.
- Gating: `module_rels_needed` (mod.rs:1160+, extract_family.rs:125) already forces
  the module family for type/call/doc programs — no gating change.
- Guard/catalog mechanical: reserved-name guard checks MODULE_RELS at mod.rs:3959-3961;
  rel_catalog auto-derives from all_builtin_decls() (src/rels/catalog.rs:47-75);
  tests/it/rel_catalog.rs:15-19 enforces docs.
- Default-export honesty: src/typegraph.rs has NO `export default` handling — no
  `default`-named entity exists, so default-import resolution is out of reach this arc.

## 2. Rel schema

Name: **module_binding** (fits module_* family + scip_binding precedent).

```
module_binding_rev(file: path, local: text, source: text, dst: path, rev: text)  -- source of truth
module_binding(file: path, local: text, source: text, dst: path)                 -- rev-deduped union
```

file = importing file; local = local binding name; source = exported name ("default"
for a default import); dst = resolved target file (Resolution::File only).

- MODULE_RELS [&str; 6] -> [&str; 8] (mod.rs:189).
- Two RelDecls in module_rel_decls() (mod.rs:765), group "module", docs describing
  the index-free scip_binding equivalent.
- Add "module_binding_rev" to REV_TWINS.

## 3. Per-language extraction

- Carrier: `ModuleRef` gains one field `pub bindings: Vec<(String, String)>`
  ((local, source) pairs, default empty). One specifier ref carries all its locals —
  no synthetic refs, no duplicate module_import rows. All existing literals gain
  `bindings: vec![]`.
- Rust: `UseLeaf` gains `alias: Option<String>` set in `expand_use_leaves`'s leaf
  branch (split `" as "`, nth(1), trim, strip r#; skip when collapsed). Brace-group
  leaves flow through the same branch, so `use a::{b as c}` is covered.
  `RustResolver::edges` switches to `expand_use_leaves` and pushes
  (alias, full.rsplit("::").next()) when alias present && !collapsed.
- TS/JS: new regex over the strip_noise'd text for `import <clause> from "spec"`
  (NOT export-from re-exports, NOT bare import, NOT require):
  `(?ms)\bimport\s+([^;'"`]*?)\s+from\s+['"`]([^'"`]+)['"`]`.
  New `fn parse_ts_import_clause(clause: &str) -> Vec<(String, String)>`:
  default ident -> (ident, "default"); `{ a as b }` -> (b, a); plain named `c` ->
  skip (local==source already covered by by_name); `* as ns` -> skip; strip leading
  `type ` tokens. Pairs attach to the matching specifier's ModuleRef.
- Kotlin: extend kotlin_import_re with `(?:[ \t]+as[ \t]+([\w`]+))?`; in
  `KotlinResolver::edges` (modgraph.rs:1205-1243) push (alias, spec.rsplit('.').next())
  per Resolution::File. Wildcards/same-package never carry an alias.

## 4. Emit + wiring (all five points)

In `module_rows_for_rev` per-file loop: for each (local, source) in mref.bindings
with Resolution::File(dst), dst != path -> push [path, local, source, dst, rev] into
new `ModuleRows.bindings` (+ extend). Then:

- insert_module_rows (:355): insert_rows module_binding_rev.
- refresh_module_rels (:425): refresh_rel module_binding_rev.
- rebuild_legacy_module_rels (:380): DELETE + INSERT OR IGNORE union into module_binding.
- refresh_module_rels_for_revs (:463): per-rev DELETE.
- refresh_module_rels_for_paths (:486): per-path DELETE (rev=? AND file IN ...).

N+1 ban: rows collected, one insert_rows call per flush.

## 5. Resolver alias hop (both closures)

New `module_binding_map(&self) -> HashMap<(rev, file), HashMap<local, (source, dst)>>`
beside module_import_map, read once per family in refresh_type_rels (after :807) and
refresh_call_rels (after :1370).

Hop position: after the SCIP override, BEFORE the by_name bucket:

```
if rev==WORK && scip hit -> canonical (unchanged)
if !sym_at.contains((repo, file, rev, name)) {        // local def shadows import
    if let Some((source, dst)) = aliases[(rev, file)].get(name) {
        return sym_at.get((repo, dst, rev, source)).map(qualify);
        // miss (barrel re-export, default) -> None; deliberately do NOT fall
        // through to by_name — a coincidental global match on the alias name
        // elsewhere would be a WRONG join. Honest bare wins.
    }
}
by_name bucket -> unique | narrow_ambiguous | bare   // unchanged
```

## 6. anchor.rs

Union block after the scip_binding block (anchor.rs:242): module_binding joined to
type_entity ON te.name = mb.source AND te.file = mb.dst, WHERE mb.local LIKE ?
(family "module", rel "module_binding"). All reads via tbl(). Add
`? module_binding(_, _, _, _).` to probe_program() (anchor.rs:598-628).

## 7. Digest / incremental

- extract_input_digest (extract.rs:582-594): fold module_binding_rev rows at this rev
  alongside the module_edge_rev fold (an edited alias must flip the type/call
  warm-tick skip).
- module_input_digest needs NO change (folds file content, fully determines bindings).
- REV_TWINS sweep + the two DELETE points above cover incremental/retraction.

## 8. Tests

Unit (modgraph.rs #[cfg(test)]): rust_use_alias_captured (bare + brace),
ts_import_clause_parse (named alias, default, namespace-skip), kotlin_import_alias_captured.

e2e tests/it/resolver_import_alias.rs (mirror resolver_import_narrowing.rs; register
in tests/it/main.rs):
(a) Rust aliased use resolves in call_edge + type_link WITHOUT index.scip.
(b) `dl what mk` surfaces the canonical def via module_binding (what.rs invocation shape).
(c) Shadowing negative: local `fn helper()` beats `use ...::make as helper`.
(d) Rev flip on re-tick (digest fold proof); use the revision_marker length-varying
    trick (resolver_import_narrowing.rs:55-66 mtime/size fast-path note).
(e) TS aliased import call resolves index-free.

Regen README/reference via generators after the RelDecls land (never hand-edit
autogen zones).

## 9. Build order

1. (S) Schema: MODULE_RELS + RelDecls + ModuleRows.bindings + REV_TWINS.
2. (S) ModuleRef.bindings field, all literals updated.
3. (M) Per-language extraction + modgraph unit tests.
4. (S) module_rows_for_rev emit + five wiring points.
5. (S) module_binding_map + alias hop in both closures.
6. (S) Digest fold.
7. (S) anchor.rs union + probe line.
8. (M) e2e resolver_import_alias.rs + docs regen.

## 10. Non-goals

- Chained/barrel re-exports (dst doesn't declare source -> honest bare).
- Wildcard re-exports / namespace imports / Kotlin `import a.b.*`.
- Default-import RESOLUTION: rows emitted with source="default" but nothing resolves
  them (typegraph has no default-export entity); future bridge item.
- Cross-repo path collision: no repo column, same pre-existing module-graph residual
  (extract.rs:962), not widened here.
- oxc-grade TS clause parsing (string-level honors one-parse; exotic clauses stay
  honest bare).
