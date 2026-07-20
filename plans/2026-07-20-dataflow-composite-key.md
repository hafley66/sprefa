# Dataflow node/lit key shape: dedup == PK == view-DISTINCT

Branch v11. Owner: df_node / df_lit KEY SHAPE only. Sibling agent owns
declare.rs + the twin-VIEW conversion of df_node_repo / df_arg / df_field /
type_edge / module_* / const_value. Do NOT touch those rels' decls.

## 0. The two harms, re-measured against the code (not assumed)

1. **_strings bloat.** `df_node.id = format!("{file}:{line}:{col}:{kind}")`
   (`src/graph/typegraph/mod.rs:713`) is interned via `sym()` in
   `collect_dataflow_rows` (`src/engine/extract/dataflow.rs`). One distinct
   coordinate string per node -> ~505k one-use `_strings` rows (storage-endgame
   plan section 2: coordinate composites 505,627 rows / 24.95MB content).

2. **Uncollapsible to a view.** The Rust dedup `seen_node: HashSet<&str>` keys
   on `n.id` ALONE. The declared PRIMARY KEY is the full row
   (id, kind, var, fn, file, line). id-only is NARROWER than the PK, so two
   facts sharing an id but differing in var/fn collapse in the table
   (first-seen wins) yet a `SELECT DISTINCT id,kind,var,fn,file,line` over the
   same facts keeps both. Table rowcount != DISTINCT rowcount -> a view
   collapse would change the row set. df_lit has the same shape: `seen_lit`
   keys on id alone, but the row is (id, text, kind).

## 1. Where the four layers disagree (stated up front)

- **Type signatures / storage layout** say the identity is (file, line, col,
  kind) — exactly what the id string bakes in. **The data** says otherwise:
  across the 2 live revs, 503 df_node ids carry divergent var/fn and 10 df_lit
  ids carry divergent text/kind. So (file,line,col,kind) is NOT a key over the
  multi-rev union; var/fn (text/kind for lit) are IDENTITY, not payload. The
  proof requirement ("keep the two divergent rows distinct") forces the wider
  key. This is the brief's own hedge realized: var/fn ARE identity.
- **The brief's "composite integer PRIMARY KEY, drop the string"** disagrees
  with **the .dl ecosystem + the display layer**: `df_node` is read as a fixed
  6-arg positional relation across std/flow.dl, .dl/flow-panel.dl,
  examples/*.dl and 243 test references; there is no `col` column and adding
  one re-arities every reader. `col` lives ONLY inside the id string, so the
  string cannot be replaced by the existing columns. And `?`/`_txt` decode the
  id via `(SELECT content FROM _strings WHERE id = cell)` — the gate test
  (`tests/it/dataflow.rs::rust_lift_closes_transitively`) reads the decoded
  coordinate string as the node's join token across df_node/df_edge/df_reaches.
  De-interning now returns NULL for every df id in `?` output and breaks that
  test. Physically removing the string is the deferred dense-dictionary arc
  (storage-endgame "diet Direction 1a"): it needs a display-boundary
  reconstruction that does not exist yet, and it is cross-cutting into files
  and tests this agent does not own.
- **The plan's `key(id)` recommendation** (storage-endgame section 3)
  disagrees with **this arc's goal**: `key(id)` would forbid the 503 divergent
  rows the proof requires be kept. Keep the default full-row PK; do NOT declare
  `key(id)`.

Resolution: this arc lands the part that is correct, in-scope, and green — the
dedup-key fix that makes df_node/df_lit SAFE to collapse to a DISTINCT view
(the stated job: "make it safe to, by fixing the key"). The _strings physical
removal is quantified here and left to the dense-dictionary arc with the
blockers named.

## 2. Type signatures + pseudo-code

```rust
// src/engine/extract/dataflow.rs, collect_dataflow_rows

// WAS: HashSet<&str> keyed on id alone.
// NOW: key on the full declared PRIMARY KEY tuple, so table dedup == PK ==
//      SELECT DISTINCT id,kind,var,fn,file,line.
let mut seen_node: HashSet<(&str, &str, &str, &str, &str, u32)> = HashSet::new();
// insert (id, kind, var, fn_sym, file, line)

// WAS: HashSet<&str> keyed on id alone.
// NOW: (id, text, kind) — a literal's identity is its node id plus the string
//      value and its kind (lit|template|concat), matching df_lit's full row.
let mut seen_lit: HashSet<(&str, &str, &str)> = HashSet::new();
// insert (id, text, kind)
```

No signature change to `push_node` (the id string stays the interned display +
join handle). No new column. No decl column change (df_node stays the 6-col
Text-id shape; the .dl arity is preserved).

## 3. Storage layout, reads/writes, uniqueness

- df_node table: unchanged columns (id, kind, var, fn, file, line), default
  full-row PRIMARY KEY. Writer now emits one row per DISTINCT full tuple.
  Uniqueness condition: (id, kind, var, fn, file, line) — since id functionally
  encodes (file,line,col,kind), the effective discriminants beyond the id are
  var and fn. Reads: every `df_node(id, kind, var, fn, file, line)` join is
  unchanged; id values are byte-identical to today.
- df_lit table: unchanged columns (id, text, kind). Writer emits one row per
  (id, text, kind). Uniqueness: (id, text, kind).
- Both the wholesale path (`refresh_rel` -> `reload_rel`, DELETE+INSERT of the
  Rust-deduped set) and the cold-slice path (`append_rel`, INSERT OR IGNORE)
  share `collect_dataflow_rows`, so the one dedup fix covers both. The DB
  full-row PK already permits both divergent rows; the bug was purely the Rust
  pre-dedup dropping the second before it reached the DB.
- _rev twins (df_node_rev key (id,rev), df_lit_rev) already separate by rev and
  a single parse has one node per (file,line,col,kind); unchanged.

## 4. Proof

`tests/it/dataflow.rs`: one git checkout, committed rev + WORK-divergent tree,
same file where a param sits at the SAME (file,line,col,kind) in both revs but
carries a different var name (and a string literal at the same position with
different text). Assert the non-rev df_node keeps BOTH var values for that one
id (fails on id-only dedup) and df_lit keeps both texts, and that no two output
rows are fully identical (DISTINCT == rowcount).

Plus a measured before-number for `_strings` on a built corpus to quantify the
harm-1 bloat that remains for the dense-dictionary arc.
