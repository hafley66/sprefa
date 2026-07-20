# De-intern the dataflow coordinate id from `_strings`

2026-07-20. `_strings` is 27% of the sprefa-root db; 126,345 of 137,746 rows
(91.7%) are dataflow coordinate id strings `file:line:col:kind`. The id integer
is a JOIN HANDLE (blake3 hash, `StringId::of`); its TEXT only exists so the id
can decode back for display. Kill the text; keep the hash; reconstruct display
from real columns.

User authorized: break backward compat, the `df_node` arity, and test
expectations.

## Ground truth confirmed
- id minted 4-part `file:line:col:kind` in `push_node`
  (src/graph/typegraph/mod.rs:717) for rust/kotlin/python/go.
- id minted 3-part `file:byte_off:kind` in `ts_push`
  (src/graph/typegraph/ts/flow.rs:11) for ts/tsx/js. DIVERGENT.
- `df_node` decl `(id,kind,var,fn,file,line)` decls.rs:623; no `col` column.
- id columns interned via explicit `sym()` in extract/dataflow.rs; the OTHER
  df columns (kind/var/fn/file) interned generically by `encode_rel_rows`.
- `nest.call_id` and `template_parts.node` carry a coordinate id but are pushed
  as `Value::Text` and interned by `encode_rel_rows` (mod.rs:872) — they
  re-intern the coordinate unless also converted.
- Decode boundary: `sym_decode` (lower.rs:31) `SELECT content FROM _strings
  WHERE id=cell`, feeding `rel_*_txt` views (create_rel_view, declare.rs:124)
  and `?` query output (lower.rs:801-950). anchor.rs reads `rel_df_node_txt`.
- Closure/SCC of df_edge (`df_reaches <- closure(df_edge)`, a USER convention,
  only in tests): `rebuild_closures` -> `load_edges` reads `rel_df_edge_txt`
  (decoded text) -> SCC node table `name TEXT`; the closure VIEW re-encodes the
  output via `SELECT id FROM _strings WHERE content=na.name` (declare.rs:1215).
  BOTH steps need the coordinate text in _strings.
- `sprf_sym(text)` (db.rs:511) is a PURE hash (`StringId::of`, no queue),
  registered on both write+read connections. `sprf_sym_intern` (db.rs:274)
  QUEUES on the write connection — must NOT be used in a view.
- Flow panel (flow-panel.html:776) reads base `rel_df_node`/`rel_df_edge`, uses
  id/from/to as opaque graph keys, labels by var/kind — never displays the id.
  No consumer parses the id on `:` by arity (verified). anchor.rs reads the
  `_txt` view, so it keeps working once the view reconstructs.

## Design: coordinate stays an integer hash; a `coord` column redirects decode

### Layer 1 — type signatures
- `struct Col { ..., coord: bool }` (ast.rs). `Col::node(name) -> Col` =
  `{ ty: Type::Text, brand: None, raw: false, coord: true }`. `interned()`
  UNCHANGED (`ty.textish() && !raw` = true), so storage stays INTEGER, int
  joins/closure-sym treatment/`sprf_sym` literal filters all keep working. Only
  DECODE and WRITE-intern branch on `coord`.
- Surface keyword `node` -> a coord Text column, so a user closure head
  (`rel df_reaches(from: node, to: node)`) reconstructs on display. `node`
  unifies with `text` in typecheck (both ty=Text) — no new base type, no
  unification churn.
- `struct DfNode { ..., col: u32 }` (typegraph/mod.rs). Both `push_node` and
  `ts_push` set it; `ts_push` also switches the id to 4-part.
- lower.rs helper `fn coord_decode(cell: &str) -> String` — the ONE
  reconstruction expression, used by every coord decode site:
  `(SELECT (SELECT content FROM _strings WHERE id=dn."file") || ':' || dn."line"
   || ':' || dn."col" || ':' || (SELECT content FROM _strings WHERE id=dn."kind")
   FROM rel_df_node dn WHERE dn."id" = {cell} LIMIT 1)`.
  Works for df_node.id (self-lookup) AND every edge-like coord column (df_edge,
  df_arg, df_lit, df_param, nest.call_id, template_parts.node), because every
  such value is a df_node id.

### Layer 2 — pseudo-code
- ts_push: `let (line,col) = line_col(starts, byte_off); id =
  format!("{file}:{line}:{col}:{kind}")` (was `{file}:{byte_off}:{kind}`).
  push_node: `id = format!("{file}:{line}:{col}:{kind}")` (unchanged), set
  `col`.
- extract/dataflow.rs `collect_dataflow_rows`: drop the SymSink; every id column
  uses `let nid = |s:&str| Value::Int(StringId::of(s).sqlite())` (pure hash, no
  queue). Push `i(n.col)` into df_node/df_node_rev rows. `nest.call_id` uses
  `nid`. kind/var/fn/file stay `t()` (encode_rel_rows still interns them).
- extract/text.rs `collect_template_rows`: `template_parts.node` uses `nid`.
- decls.rs: append `c("col", Type::Int)` to df_node (arity 6->7) and df_node_rev
  (before rev). Mark every coordinate id column `Col::node(...)`: df_node.id,
  df_node_rev.id, df_node_repo(.id)/_rev.id, df_edge.from/to, df_param.id,
  df_arg(.call/.arg)/_rev, df_field(.id/.value)/_rev, df_lit.id/_rev.id,
  nest.call_id, template_parts.node. (nest.loop_id stays plain interned text =
  `file:start`.)
- create_rel_view (declare.rs:122): `if col.coord { coord_decode(rel.col) }
  else if col.interned() { _strings } else { raw }`.
- declare_closure output (declare.rs:1215/1226): interned branch ->
  `sprf_sym(na.name)` (was the `_strings` content lookup). Equivalent for
  interned-text closures, correct for de-interned coord closures. The `_txt`
  closure view (1262) reconstructs a coord head column via coord_decode.
- lower.rs term_sql_text / interp_sql / lower_query / lower_query_agg: a coord
  column/var decodes via coord_decode, not sym_decode. VarTy gains `coord`,
  threaded from column meta.
- head_term_sql for a coord column: `Term::Str(s) -> sprf_sym('...')` (pure
  hash, was sprf_sym_intern) so a rule head literal into a coord column does not
  re-queue the coordinate.

### Layer 3 — storage layout / read-write sequence
- df_node table: `id INTEGER, kind INTEGER, var INTEGER, fn INTEGER, file
  INTEGER, line INTEGER, col INTEGER`. `_strings` no longer holds any
  `file:line:col:kind` row. `col` is functionally determined by `id`, so it does
  NOT change dedup cardinality; the full-row PK stays a valid set key.
- Write: extract pushes pure-hash Int ids -> encode_rel_rows skips them (only
  interns Value::Text) -> flush_syms drains only kind/var/fn/file/lit-text.
- Read/display: coord columns reconstruct via coord_decode -> rel_df_node lookup.
- df_reaches VIEW: load_edges reads rel_df_edge_txt (reconstructed coordinates)
  -> SCC on coordinate names -> output re-encoded by `sprf_sym(name)` = the same
  df id hash. `? df_reaches(from,to)` decodes via coord_decode.

### Layer 4 — uniqueness / where layers disagree
- Layer 1 says col is identity; Layer 3 says col is FD-by-id payload. They
  disagree deliberately: col is DECLARED in the row (identity for the writer's
  full-row PK) but is redundant with id (id = hash of the string that contains
  col). Keeping it in the PK is harmless (never splits a row) and lets
  reconstruction read it from the same table.
- TS `col` holds a 0-based BYTE column (line_col), Rust `col` holds syn's
  0-based CHAR column. Different bases, but each frontend stores exactly the
  value it minted into the id, so reconstruction is byte-exact per frontend and
  `sprf_sym("file:line:col:kind")` literal filters match.

## Step 5 (after key work green): collapse df_node/df_lit to VIEWS
Once the writer dedup == full PK and the `_rev` twins carry col, collapse
df_node and df_lit to `SELECT DISTINCT ... FROM rel_<name>_rev` views (the
`view_body` primitive), reclaiming table + autoindex (~14.6MB). The two-distinct
-rev parity property (view_backed_rel.rs / dataflow.rs) must still hold.

## Proof required (measured)
- BEFORE (v11 tip) / AFTER, in a scratch DL_STATE_DIR: `SELECT COUNT(*) FROM
  _strings`, `SELECT SUM(pgsize) FROM dbstat`. Coordinate rows gone.
- Join integrity test: df_node/df_edge/df_reaches return the same logical rows.
- Display reconstruction test: a known node reconstructs `file:line:col:kind`.
- Two-distinct-rev parity for df_node/df_lit as views.

## FINAL OUTCOME (2026-07-20) — deviations from the plan above

Three deviations, all with receipts:

1. **COALESCE fallback beats mass retyping.** Rather than retype every ecosystem
   `text` column carrying a df id to `node` (dozens of `.dl` files, judgment-
   heavy), `sym_decode` now `COALESCE((_strings lookup), (rel_df_node
   reconstruct))`. SQLite short-circuits, so a normal interned string never pays
   the fallback. Any `text` column holding a df id reconstructs automatically —
   zero `.dl` churn beyond the arity change. The `node` type + `Col::coord` still
   exist for the builtin df columns (precise, no `_strings` attempt) and for
   authors who want to spell it; std/flow.dl + std/strings.dl were retyped to
   `node` (harmless, slightly cheaper decode), examples/tests stay `text`.

2. **Tree-sitter line-bump fix (kotlin/python/go).** Those front-ends minted ids
   from the 0-based row, then bumped `n.line += 1` to 1-based — leaving the id's
   line one behind the stored column, which the de-intern reconstruction exposed
   (id != file:line:col:kind, so `sprf_sym(reconstruct)` missed the stored hash
   and the closure re-encode produced empty rows). Fixed with
   `bump_node_lines_1based` (typegraph/mod.rs): bump line AND rebuild the id +
   remap every id-referencing fact. ts_template_parts node id switched to the
   same 4-part scheme.

3. **Step 5 (df_node/df_lit -> views) DEFERRED, with receipt.** df_node_rev is
   keyed `(id, rev)`, which drops divergent-(var,fn)-within-one-rev rows the old
   df_node table keeps (measured: 66,852 -> 66,635, 217 rows, on the sandbox
   corpus). Collapsing df_node to a view over that twin changes join results,
   violating "same logical results". df_node/df_lit stay base tables until the
   twin's key is widened to the full row (a clean follow-up). The df_node table +
   autoindex (~7.9MB on the sandbox / ~14.6MB projected on the root) is the
   deferred reclaim; the `_strings` de-intern win stands independently.

### Measured (sandbox: 84 rust + 3 ts files, df_node = 66,852 identical B/A)
- `_strings` rows 73,329 -> 6,719 (-90.8%). Coordinate df-id rows 64,881 -> 0
  (the 39 stragglers are 9 fn_sym `::`-chains + 30 re-interned template nodes,
  none a df coordinate id). `_strings` table bytes 3.61MB -> 0.34MB.
- dbstat total 23.87MB -> 21.12MB (-11.5%) with df_node kept a table; -7.9MB more
  is available via the deferred step 5.
- template_parts.node kept interned (module-level templates have no df_node to
  reconstruct from; ~120 rows, negligible).

## Ecosystem / test / doc update (authorized, ~243 df test refs)
Positional `df_node(id,kind,var,fn,file,line)` -> 7-arg (append col or `_`):
std/flow.dl, std/entry.dl, .dl/*, examples/*, bench/*, book/*, docs/*, and the
tests/it df suite. `? df_reaches` test snippets declare `node` columns. TS df id
fixtures change (3-part -> 4-part) across type_graph_js/flow_jsx/flow_std/
template_parts/string_flow tests.
