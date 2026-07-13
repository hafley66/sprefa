# Bespoke-SQL de-coupling: a typed rel-access layer

**Problem, measured** (`.dl/rusqlite-coupling.dl`, 2026-07-12):
rusqlite is *bound* in **5 files**, but SQL is *hand-rolled* at **341 sites across
219 functions**, vs **132** that route through the typed write helpers. There is
no single rel-access layer, so:

1. A storage-representation change (0.10.0 interning) is **341 edits**, not one —
   and each missed site is a silent off-ramp (the rev-sweep wipe, the module
   retraction, json-agg decode, the lsp/scip/effect readers).
2. SQLite **coerces instead of erroring**: `intcol = 'text'` returns 0 rows, so an
   off-ramp fails silently far downstream instead of at the seam.

**Goal:** rel read / write / filter go through one typed layer that owns the
sym↔text handshake, so representation is *one decision*, and the storage boundary
is *typed* (SQLite is made to yell). Burn the 341 down; ratchet it with the
dogfood rail so it can't climb back.

**Out of scope (legitimately bespoke SQL, do not touch):** `src/lower.rs` (the
datalog→SQL fixpoint compiler *is* SQL generation) and the graph algorithms in
`src/engine/derive.rs` (BFS/SCC adjacency). The target is rel **CRUD**: the
readers/writers/filters in `meta.rs` (54), `extract/mod.rs` (54), `lens.rs` (37),
`anchor.rs` (30), `rpc.rs`, and the consumer readers (`lsp`/`scip`/`effect`).

---

## 1. Type signatures

```rust
// ── Read: always decodes interned columns; no caller ever sees a raw StringId ─
/// Select `cols` from `rel` under `filter`, decoded. Interned columns come back
/// as Value::Text (via _strings), int columns pass through. Interning a new
/// column is then invisible to every reader.
fn rel_select(&self, rel: &str, cols: &[&str], filter: &Filter) -> Result<Vec<Row>>;
fn rel_for_each(&self, rel: &str, cols: &[&str], filter: &Filter,
                f: impl FnMut(&Row) -> Result<()>) -> Result<()>;   // streaming, no Vec

// ── Filter: a predicate that KNOWS interned-ness, so it hashes text literals ──
enum Pred {
    Eq(&'static str, Value),         // textval -> sprf_sym(...) when col interned
    In(&'static str, Vec<Value>),    // same, set form
    IsNull(&'static str),
    Rev(&'static str, RevSet),       // the rev-twin set-diff, in id-space
}
struct Filter(Vec<Pred>);            // AND of preds; empty = all rows
type Row = Vec<Value>;               // cells typed by the rel's Col::ty

// ── Write: consolidate on the existing interning path ────────────────────────
fn rel_insert(&self, rel: &str, cols: &[&str], rows: &[Row]) -> Result<usize>;   // = encode_rel_rows + insert
fn rel_delete(&self, rel: &str, filter: &Filter) -> Result<usize>;              // filter hashed to id-space

// ── Guard: make the storage boundary typed (SQLite yells) ────────────────────
fn col_ddl(col: &Col) -> String;     // interned -> INTEGER CHECK(typeof("c") IN ('integer','null'))
// + debug_assert in lower::eq_cond: never emit bare `=` between a sym cell and a
//   text operand that did not route through sprf_sym.
```

## 2. Pseudo-code bodies

```rust
fn rel_select(&self, rel, cols, filter) {
    // meta = self.rels[rel]
    // SELECT <cols> FROM rel_<rel>_txt   -- the decoded view already exists (declare.rs)
    //   WHERE <filter.to_sql(meta)>      -- Eq/In on interned col => sprf_sym(literal)
    // query_map: cell i -> Value by meta.cols[i].ty (Int stays Int, rest Text)
}
fn rel_delete(&self, rel, filter) {
    // DELETE FROM rel_<rel> WHERE <filter.to_sql(meta)>
    // Pred::Eq on interned col lowers `= sprf_sym('lit')`; Pred::Rev lowers
    //   `"rev" (NOT) IN (SELECT sprf_sym(rev) FROM <live text set>)`  -- the fix
    //   already shipped ad-hoc in sweep_gone_revs / refresh_rel_for_revs
}
fn col_ddl(col) {
    // if col.interned() { format!(r#""{n}" INTEGER CHECK(typeof("{n}") IN ('integer','null'))"#) }
    // else { format!(r#""{n}" {}"#, col.sql()) }
}
// Filter::to_sql(meta): per Pred, look up the col's interned() and render:
//   interned + text literal  -> `"c" = sprf_sym('v')`   (id-space, no decode)
//   interned + read/display  -> handled by the _txt view on the read side
//   non-interned             -> plain `"c" = 'v'`
```

## 3. Instance lifetimes

- `rel_select`/`rel_delete`/`rel_insert` are `&self` methods on `Engine`; **no new
  persistent state**. `Filter` is a short-lived builder owned by the caller for
  one call.
- `SymSink` (write-path interner) already exists per-call, flushed at the end of
  `encode_rel_rows`; `rel_insert` reuses it unchanged.
- `rel_<name>_txt` views are created **once per rel** at declare time
  (`create_rel_view`, already shipped) and live for the connection's lifetime.
- The `CHECK` constraint is part of the table DDL: created once, enforced on
  every insert for the table's life.

## 4. Storage layout → read/write sequence → uniqueness

**Storage (unchanged shape, one addition):**
- `rel_<name>` raw table: interned cols `INTEGER` **+ CHECK(typeof=integer|null)**;
  int cols `INTEGER`; raw text cols `TEXT`.
- `_strings(id, content, norm)`: content-addressed reverse map (`id = hash64(content)`).
- `rel_<name>_txt` view: decodes interned cols. **The read surface.**

**Sequence:**
- *write:* `Value::Text` → `encode_rel_rows` interns → `INTEGER` cell → CHECK passes.
  A raw-text write into an interned col (an off-ramp) now **fails at the DB** with
  the value, not silently.
- *read:* always via `rel_<name>_txt` → text out. Raw `rel_<name>` is engine-internal.
- *filter/join:* text literal → `sprf_sym(hash)` → `INTEGER` compare (never decode
  the column side).

**Uniqueness:** `_strings` content-hash is the identity — equal text ⇒ equal id
(already invariant). The CHECK guarantees an interned column *only* ever holds an
id or NULL, so the id-space compare is total.

---

## Migration & ratchet (phased; rail-gated)

- **Phase 0 — guard first.** Land `col_ddl` CHECK + the `eq_cond` debug_assert.
  Run the suite in debug: every remaining off-ramp now self-identifies with a
  stack trace / CHECK failure instead of a silent 0-row. (This is the tool that
  makes the rest fast — see the 0.10.0 triage, where silent-0 cost hours.)
- **Phase 1 — read accessor.** Add `rel_select`/`rel_for_each`; migrate the
  point-query readers (`lsp`, `scip`, `effect`, `rpc`, `anchor`).
- **Phase 2 — delete/filter helper.** Add `rel_delete` + `Filter`; fold the
  hand-rolled rev/path set-diff DELETEs (`sweep_gone_revs`, `refresh_rel_for_revs`,
  module retraction) into it — those already carry the ad-hoc `sprf_sym` fix.
- **Phase 3 — burn down per hotspot.** `meta.rs` (54), `extract/mod.rs` (54),
  `lens.rs` (37) in order; each file drops toward the typed helpers.
- **Ratchet.** Promote `.dl/rusqlite-coupling.dl` to a `--check` gate: `raw_sql_sites`
  and `bespoke_hotspots` may only ever decrease. Layer 5 (call-graph blast radius)
  lights up once the SCIP diet lands `call_edge`, giving the "who transitively
  touches bespoke SQL" number for free.

**Definition of done:** rel CRUD (the 341 minus the lower.rs/derive.rs exempt set)
goes through `rel_select`/`rel_insert`/`rel_delete`; interned columns carry the
CHECK; the rail gate is armed. A future coordinate-type change is one decl edit,
verified by the rail, not a 341-site archaeology dig.
