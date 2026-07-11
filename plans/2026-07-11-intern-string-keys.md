# Intern hot join keys: TEXT columns -> StringId ints via `_strings`

## Why (measured)

2026-07-11 cold-tick profiles (macOS `sample`, hermetic `dl --check`):
- Before the 512MB cache fix (33f549b): ~half the derived wall in
  `BtreeIndexMoveto -> pread` (page-cache misses).
- After: 37.2s, CPU-bound — top leaves are `_platform_memcmp`,
  `vdbeRecordCompareString`, `sqlite3VdbeRecordCompareWithSkip`. Every join
  probe compares long TEXT keys (`repo::path::kind::name` syms, file paths,
  qualified names) millions of times across fixpoint passes. Int keys turn
  those memcmps into single-word compares and shrink every index (smaller
  keys = more keys per page = fewer page touches).

## Decision: no `_sym` table — reuse `_strings`

`StringId::of(text)` = `hash64(bytes)` (src/spine.rs:52), content-addressed.
The id is computable from the string alone:
- emit side needs NO lookup/coordination (the v4 per-row-intern trap stays
  dead; batched `insert_spine_strings` covers decode idempotently),
- lower can rewrite a `.dl` text literal to its id at compile time by hashing,
- decode is the one existing join surface (`JOIN _strings ON id`).
A dedicated `_sym` table would be the same (id, text) shape with no new
property. Dense ids (the only thing it could add) would break content
addressing, and the dense-id consumers (scc, node2vec) already build their
own in-memory maps. Caveat accepted: interns linger (existing C2 GC posture);
ADD a collision check (same id, different text -> loud bail) at intern time —
a silent 64-bit collision would corrupt joins.

## Design layers (planning protocol)

### 1. Type signatures

```rust
// ast.rs — column-level opt-in, spelled in decls not inferred:
pub enum Type { Text, Int, /* existing */ , Sym }  // Sym = interned text
// Col.ty = Sym: storage INTEGER (StringId), surface semantics = text

// lower.rs
fn lower_term_for_col(term: &Term, col_ty: Type) -> Sql
// pseudo: Sym col + text literal  -> StringId::of(lit).0 as i64 literal
//         Sym col + variable      -> plain int column ref (joins stay int=int)
//         Sym col + fn call/LIKE/template/glob -> wrap col in decode join:
//             (SELECT content FROM _strings WHERE id = <col>)
//         head insert into Sym col from a text-typed source -> intern at emit

// engine emit paths (extract.rs, refresh_rel seam)
fn intern_batch(rows: &mut [Row], sym_cols: &[usize], strings_out: &mut Vec<(i64, String)>)
// pseudo: per row/col: id = StringId::of(text); push (id, text) to strings_out;
//         replace cell with id. One insert_spine_strings flush per refresh.

// query print seam (run_query / print_query_result / rel_rows / dl --rows)
fn decode_sym_cols(sql: Sql, decl: &RelDecl) -> Sql
// pseudo: SELECT wraps Sym cols in the _strings decode join so every
//         user-visible surface still shows text. `?` output byte-identical.
```

### 2. Instance lifetimes

- `StringId` values: stateless, derived from content; live as INTEGER cells in
  rel tables, valid forever (content-addressed).
- `_strings` rows: inserted at extraction/derivation emit; never retracted
  (existing posture); survive db reopen.
- No in-memory intern map at all — the hash IS the map.

### 3. Storage layout, reads/writes, uniqueness

- Sym columns: `INTEGER NOT NULL` in the rel table; indexes over them are int
  btrees. `_strings(id INTEGER PRIMARY KEY, content TEXT, norm TEXT)` (exists).
- Writes: emit paths batch-intern (one `insert_spine_strings` per refresh, one
  `insert_rows` per rel — no new per-row writes). Collision guard: the
  `_strings` upsert becomes INSERT .. ON CONFLICT(id) DO NOTHING plus a debug
  assertion path comparing content on conflict (cheap: conflict rows only).
- Reads: rule bodies join int=int; only fn-call/LIKE/glob/template sites and
  the print seam pay the decode subquery.
- Uniqueness: unchanged (full-row PK / key(...) still applies; int cells
  compare exactly like their texts did, since id is a function of text).

## What stays TEXT

- Columns fed to substring ops in hot paths where decode-per-row would erase
  the win (measure in the spike; likely `df_lit.text`, `doc_comment.text`,
  msg/detail columns). Interning targets KEYS (sym, file, name, kind, repo,
  rev, caller/callee), not payload prose.
- User-declared rels default to TEXT unless the user writes `sym` — zero
  breaking change to existing programs. Builtin decls opt in column by column.

## Phases

- **P0 spike (S, DO FIRST — go/no-go number)**: hand-convert ONE hot join
  offline: dump `call_edge`+`call_def`+the flow_edge feeder rels from a cold
  db, re-key sym/file columns to StringId ints in a scratch db, run the
  worst `_stmt_ms` statements against both. Report ms before/after. No engine
  changes. If the win is <2x on those statements, stop and rethink.
- **P1 plumbing (M)**: `Type::Sym` through parse/typecheck/lower/declare
  (storage INTEGER, brand-style unification so sym/text mismatches are loud);
  literal rewrite + decode-wrap in lower; print-seam decode; collision guard.
- **P2 builtin key columns (M)**: flip the hot builtin decls (call_edge,
  call_def, call_name, call_site.callee, type_link, type_entity.sym/parent,
  df_node.var/fn, df_arg/df_field ids stay as-is, module_edge cols, file/path
  cols where joined). Extraction emit interns batched. Rebuild-per-tick tables
  mean no migration — bump the extract digests (exe identity already does).
- **P3 std/.dl sweep (S)**: std/flow.dl, std/entry.dl and friends need zero
  edits if P1's lowering is complete — verify with the suite + byte-compare
  of `?` query outputs on a fixture db. perf-rails re-measure; re-promote
  tick-over-budget to error when derived lands under 10s.

## Turnkey Rust-side API (Chris, 2026-07-11): auto-interning types

Emit code must not be able to forget interning or do it per-row. New types make
the correct path the only path:

```rust
// src/spine.rs (or src/sym.rs if spine.rs nears the size law)
pub struct Sym(StringId);            // an interned string VALUE; Copy
pub struct SymSink { pending: Vec<(i64, String)> }
impl SymSink {
    pub fn sym(&mut self, text: &str) -> Sym
    // pseudo: id = StringId::of(text); pending.push((id.sqlite(), text)); Sym(id)
    // dedup inside flush (sort+dedup by id), NOT per-call hashmap
}
impl Db {
    pub fn flush_syms(&self, sink: &mut SymSink)
    // pseudo: one insert_spine_strings batch; clears pending
}
impl Drop for SymSink { /* debug_assert!(pending.is_empty(), "unflushed syms") */ }
```

Invariants that make it turnkey:
- The ONLY constructor of `Sym` is `SymSink::sym` — you cannot mint an id
  without its text being queued for `_strings`. (`StringId::of` stays pub for
  lower's compile-time literal rewrite; it returns `StringId`, not `Sym`.)
- `Sym` implements the row-cell conversion (`Into` whatever `insert_rows`
  params take) as its INTEGER form; there is no `Display`/`to_string`, so a
  sym can't silently land in a TEXT column.
- One `SymSink` per refresh pass, flushed once alongside the rel's
  `insert_rows` — batching is structural, N+1 impossible by construction.
- Drop guard screams in debug if a sink dies unflushed.

Lifetimes: `SymSink` lives for one refresh fn body; `Sym` cells live in the
row Vecs until `insert_rows`; `_strings` rows persist (existing posture).

## Verification

- P0: before/after ms table for the top-5 `_stmt_ms` statements.
- P1-P3: full suite; `?` outputs byte-identical on fixture dbs; oracle parity
  scores unchanged (scorers read query text output); cold-tick wall
  before/after with the same profile methodology; magic-rel + recompute rails.

## Coordination

- Sequenced AFTER the instrumentation branch (sum+passes `_stmt_ms`) and the
  slow-rule factoring branch land — both change the numbers this plan is
  measured against, and factoring may drop some columns off the hot list.
- The engine/mod.rs split plan (separate) touches the same emit files; do not
  run both at once.

<!-- todo(perf): P0 spike — re-key call_edge/flow feeders to StringId in a scratch db, before/after ms -->
<!-- todo(decision): which payload TEXT columns stay text (df_lit.text, doc_comment.text) — decided by the spike's decode-cost numbers -->
