# eval on sqlite_ivm: design

Step 3 of `plans/v8/2026-09-18-eval-on-sqlite-ivm.brief.md`. Base
`origin/main` `e88470c6e`. Read by the coordinator and Chris before step 4
dispatches. Plain-words twin: `2026-09-18-eval-on-sqlite-ivm.visual.human.unga.md`.

## Forks for Chris

Each fork has a recommendation. Step 4 dispatches after every row has a
decision.

| # | fork | options | recommend | why |
|---|---|---|---|---|
| F1 | Rust emitter surface for `std/store.dl7` (Addendum 2) | (a) structs only; (b) structs plus `insert_*` / `select_*` functions | b | with (a), `src/_9_runtime/` hand-writes every INSERT and SELECT, and each one repeats column names. Addendum 2 bans that duplicate |
| F2 | what a row cell holds | (a) the arena `TermId` (`src/_6_eval/_0_term.rs:13`), with constructors as registered scalar functions over the shared arena; (b) a 64-bit structural hash id; (c) an encoded term BLOB | a | the arena is already mirrored as `sym`/`term`/`term_arg` (`src/_9_runtime/_1_sqlite.rs:613-627`) and reloaded by `load_arena` (`:659`). Ids equal today's, so oracles decode unchanged. (b) can collide. (c) copies a whole list into every cons prefix, so storage grows with the square of list length |
| F3 | view granularity | (a) one sqlite_ivm view per derived product, with upstream products inlined as CTEs (`src/_5_reify/_7_sqlite.rs:1-2`, today's emitter); (b) one view per program, whose output is the tagged union of every derived product | b | a view cannot read a view: sources must be ordinary `main` tables (sqlite_ivm README "Supported query shapes", last paragraph). Under (a), a product with five consumers is arranged five times. Under (b), each CTE is arranged once; README lists "shared CTE consumers" |
| F4 | nonlinear recursion (two goals on the same recursive SCC in one rule) | (a) diagnostic `nonlinear_recursion` (already emitted, `_7_sqlite.rs:509-510`); (b) a budgeted, Rust-sequenced `INSERT INTO t SELECT ... FROM t a JOIN t b` round loop on plain tables | a first; b only if step 4 counts any in prelude or macrotime | SQLite rejects a second reference to the recursive table in one step (README "accepted grammar"). The count over today's prelude and macrotime is unmeasured; step 4 prints it in a span |
| F5 | products whose rules need bound head arguments (the `demanded` set from step 1) | (a) magic-set rewrite: a demand CTE of call patterns joined into the rule body, with a `depth` column capped by a named constant; (b) keep top-down `demand` in Rust | a | (b) is the relational Rust the user banned. (a) is plain SQL and satisfies the bounded-loop law through its depth cap |

## 1. `std/store.dl7`: declared once, emitted twice

The store's own products are dl7 products. dl8 lowers them the way it lowers
any product. `build.rs` runs the lowering into `OUT_DIR`, and both outputs are
frozen under `oracle/store/` and diffed by a test (Addendum 2). Product syntax
follows `std/fs.dl7:4-8`. Key positions ride `dl6.relation` rows
(`std/dl6.dl7:5-8`). Import and seed spelling follow
`fixtures/openapi/todo.dl7:3,7`.

Every cell of a non-dictionary product is a `TermId` (F2a), so its SQL type is
`INTEGER REFERENCES term(id)`. The dl7 column types matter to the checker only.

| `std/store.dl7` | emitted DDL | emitted Rust (F1b) |
|---|---|---|
| `(: sym (* (: id int) (: text str)))` | `CREATE TABLE sym(id INTEGER PRIMARY KEY, text TEXT NOT NULL UNIQUE)` | `struct Sym { id: i64, text: String }`, `insert_sym(&Connection, &[Sym])` |
| `(: term (* (: id int) (: kind int) (: ival int) (: rval float) (: sym int)))` | `CREATE TABLE term(id INTEGER PRIMARY KEY, kind INTEGER NOT NULL, ival INTEGER NOT NULL, rval REAL NOT NULL, sym INTEGER NOT NULL REFERENCES sym(id))` | `struct Term { .. }`, `insert_term`, `select_term_after(watermark)` |
| `(: term_arg (* (: term int) (: position int) (: child int)))` with `(dl6.relation term_arg 3 [0 1])` | `CREATE TABLE term_arg(term INTEGER NOT NULL, position INTEGER NOT NULL, child INTEGER NOT NULL, PRIMARY KEY(term, position), CHECK(child < term)) WITHOUT ROWID` | `struct TermArg { .. }`, `insert_term_arg` |
| `(: node (* (: id type)))` | `CREATE TABLE node(__id INTEGER PRIMARY KEY, id INTEGER NOT NULL UNIQUE)` | `struct Node { id: TermId }` |
| `(: colon (* (: owner type) (: label any) (: target any) (: ordinal int)))`, the `:` product (`src/_1_macrotime/_2_protocol.rs:368`) | `CREATE TABLE colon(__id INTEGER PRIMARY KEY, owner INTEGER NOT NULL, label INTEGER NOT NULL, target INTEGER NOT NULL, ordinal INTEGER NOT NULL, UNIQUE(owner, label, target, ordinal))` | `struct Colon { .. }`, `insert_colon(&Connection, &[Colon]) -> usize`, `delete_colon` |
| `syntax_frontier/2`, `syntax_form/1`, `syntax_atom/2`, `syntax_literal/2`, `syntax_variable/3`, `source/8` (`_2_protocol.rs:367-374`) | one table each, same shape as `colon`, `UNIQUE` over every column | one struct each |
| `(: effect (* (: relation type) (: application type)))` | view output, product tag `effect` (section 5) | `struct Effect { .. }`, `select_pending_effects` |
| `(: intern_snapshot (* (: constructor type) (: arguments any) (: application type)))` | view output | `struct InternSnapshot { .. }` |
| `(: answer (* (: relation type) (: row any)))` | one source table per served product, named for the product | `insert_answers` |

`insert_*` is one multi-row `INSERT OR IGNORE` per chunk of
`SQLITE_LIMIT_VARIABLE_NUMBER` bindings (today's `append`,
`_1_sqlite.rs:181`), inside `sql()`. `select_*` is one statement inside
`sql()`.

## 2. Kernel goals in SQL

The lowering fixes each goal's mode from the binding order of the rule body.
b means bound and f means free. A mode not in this table is a lowering
diagnostic that names the goal, as `Unsupported::Kernel` does today
(`_7_sqlite.rs:726`). Every `dl_*` function is registered on every connection
with `SQLITE_DETERMINISTIC` (README "Load deterministic registered functions on
every connection") and returns NULL when its input has the wrong shape. The
lowering adds `IS NOT NULL` guards for every function a goal uses. A constant
term inlines as its integer `TermId`, because bind parameters are rejected.

| kernel (`src/_6_eval/_4_kernel.rs:10-23`) | mode | SQL |
|---|---|---|
| `nil(L)` | f | `L := <TermId of const([])>` literal |
| `cons(H, T, L)` | b b f | `L := dl_cons(H, T)` |
| | f f b | `H := dl_head(L)`, `T := dl_tail(L)` |
| | b b b | `L = dl_cons(H, T)` |
| `str.nil(S)` | f | literal `TermId` of `const("")` |
| `str.cons(H, T, S)` | b b f | `S := dl_str_cat(H, T)` |
| | f f b | `H := dl_str_first(S)`, `T := dl_str_rest(S)`; `""` returns NULL, so it has no row (`_4_kernel.rs:196-198`) |
| `edge_ref(O, L, R)` | b b f | `R := dl_edge_ref(O, L)` |
| `intern(C, A, R)` | b b f | `R := dl_application(C, A)`; the `(C, A, R)` triple is also a row of the view's `intern` output |
| `int.lt/le/eq/ne/ge/gt(L, R)` | b b | `dl_int(L) < dl_int(R)`, and so on for each operator |
| `int.add(L, R, S)` | b b f | `S := dl_int_add(L, R)`, NULL on overflow (`_4_kernel.rs:262-264`) |
| | b b b | `S = dl_int_add(L, R)` |
| `any.lt(L, R)` | b b | `dl_term_lt(L, R) = 1`, standard order over the arena (`_0_term.rs:1-3`) |
| `count_step` | fold | `COUNT(*)` per group |
| `min_step` / `max_step` | fold | `FIRST_VALUE(x) OVER (PARTITION BY g ORDER BY dl_order_key(x) ASC/DESC)` inside a FROM subquery. The README requires the subquery when a window and an aggregate compose |
| program-step fold (`fold_generic`, `_5_evaluate.rs:610`) | fold | `ROW_NUMBER()` in a FROM subquery, then a linear `WITH RECURSIVE` over `rn = previous.rn + 1` whose step is the rule's step goal. The recursion is bounded by the group's row count |
| negative kernel goal | b… | `NOT (<positive condition>)` (`_4_kernel.rs:320-339`) |

`dl_order_key(t) -> BLOB` compares bytewise in SWI-Prolog standard order. It
exists because arena id order is not term order.

## 3. Negation, strata, what sqlite_ivm rejects

A negative goal on a product becomes a correlated `NOT EXISTS (SELECT 1 FROM
<earlier CTE> WHERE ...)` under AND. Stratification (`_2_stratify.rs:61`) puts
the negated product in an earlier CTE. Inside one F3b view each stratum is a
CTE group, and the README accepts NOT EXISTS consumers in later strata.

| sqlite_ivm or SQLite rejects (README "accepted grammar") | arises from | rewrite |
|---|---|---|
| second reference to the recursive table in one step | nonlinear recursion | F4 |
| mutual recursion between two CTEs (`circular reference`) | an SCC of several products | one CTE per SCC with a `product` discriminator column; columns padded to the SCC's widest arity with the `TermId` of `none` |
| aggregate, EXISTS, NOT EXISTS or IN over the step's own relation | a non-stratifiable program | already a stratify diagnostic |
| `UNION ALL` recursion | nothing | the lowering emits UNION only |
| query without FROM | a constant fact | FROM a one-row `unit` source table |
| bind parameters | constants | integer `TermId` literals |
| scalar and IN subqueries | kernel deconstruction | the `dl_*` functions in section 2 |
| volatile functions | nothing | every `dl_*` function is deterministic |
| custom aggregates | min/max/fold steps | windows and linear recursion (section 2) |
| outer join, USING, ORDER BY, LIMIT inside a recursive CTE | nothing | never emitted |

F5a demand spelling: for a product `p` in `demanded`, each call site with bound
positions `B` contributes the rows of `demand_p_B(bound columns, depth)`,
projected from the caller's body prefix. `p`'s rule body joins
`demand_p_B`. A recursive demand adds `depth + 1` and
`WHERE depth < DEMAND_DEPTH_LIMIT`. A view row tagged
`demand_depth_exceeded` is the named diagnostic at the cap.

Served goals: `write_effect` (`_5_evaluate.rs:278`) becomes an `effect` output
row projected from the body prefix that ends at the served goal. Its argument
list is `dl_effect_arguments(v0, v1, ...)`, which maps NULL to `const(none)`.

## 4. `IEvaluate`

```rust
pub trait IEvaluate {
    /// Source tables from `std/store.dl7` plus one sqlite_ivm view for the
    /// program, in one transaction. A changed rule set drops and recreates the view.
    fn declare(&mut self, program: &Program) -> Result<Declared, Stop>;
    /// Seeds in, one INSERT or DELETE per product per chunk, one transaction.
    fn apply(&mut self, delta: SeedDelta) -> Result<Applied, Stop>;
    /// Rows of the named products, decoded through the arena, sorted by `Universe::cmp`.
    fn read(&self, products: &[TermId]) -> Result<Closure, Stop>;
}

pub struct SeedDelta { pub insert: Vec<Row>, pub delete: Vec<Row> }
```

Instance lifetimes:

| instance | lives | owns |
|---|---|---|
| `SqliteEvaluate` (impl `IEvaluate`) | one compile | one `Connection` from `open()`, the arena handle, the view name |
| arena `Arc<Mutex<Universe>>` | one compile | shared by `&mut Universe` callers and every `dl_*` closure. `rusqlite` scalar closures require `Send + 'static`. The mutex is uncontended because rusqlite is sync and single-threaded here |
| view | from `declare` to the next `declare` with different rules | sqlite_ivm arrangements |

`evaluate(u, program, fx)` (`src/_6_eval/_5_evaluate.rs`) keeps its signature
as a wrapper: open `:memory:`, `declare`, `apply` every seed, `read` every
product.

## 5. Storage layout, then reads and writes, then uniqueness

Layout: the dictionary tables `sym`, `term`, `term_arg`; one source table per
base product (syntax products, served-answer tables, seeds); one `unit` table;
one sqlite_ivm view `program` with output `(product INTEGER, c0 .. cK
INTEGER)`, where K is the widest derived arity.

Sequence for `dl8 compile fixtures/openapi/todo.dl7`:

1. `open(":memory:")`: pragmas, extension, `dl_*` functions bound to the arena.
2. `BEGIN`; the emitted DDL; `COMMIT`.
3. Macrotime wave 0: `BEGIN`; `insert_*` for every syntax product; `CREATE
   VIRTUAL TABLE program USING sqlite_ivm(...)` over the macro rule cone,
   which is fixed for the whole expansion (`src/_1_macrotime/_4_expand.rs:109`);
   `SELECT` the claim rows; flush arena terms past the watermark into
   `term`/`term_arg`; `COMMIT`.
4. Wave n: `BEGIN`; `delete_*` the rewritten rows, `insert_*` the new rows;
   `SELECT` claims; flush the arena; `COMMIT`. It stops on no claim, a repeated
   row set, or `WAVE_LIMIT` (`_4_expand.rs:16`), as today.
5. Comptime round: `BEGIN`; if the assembled rules changed, `DROP` and
   `CREATE` the view; `insert_answers`; `select_pending_effects`; flush;
   `COMMIT`. It stops as today, capped by `COMPILER_ROUND_LIMIT`
   (`src/_4_comptime/_2_rounds.rs:99`).
6. `read`: one `SELECT product, c0.. FROM program`, decoded and sorted by
   `Universe::cmp`, so the output bytes match today's closure sort.

Every statement runs inside `sql()` and emits one span.

Uniqueness conditions:

| what | unique by | enforced by |
|---|---|---|
| `sym.text` | text | `UNIQUE` |
| `term.id` equals `TermId.0` | hash-consing in the arena `IndexSet` (`_0_term.rs:28-33`) | the arena; the db mirrors it past the watermark |
| a source product row | every column | `UNIQUE(c0..cn)` plus `INSERT OR IGNORE` |
| a view output row | `(product, c0..cK)` | the lowering emits UNION per product, so the output is a set |
| `effect` row | `(relation, application)` | same |
| a `TermId` across reopen | `load_arena` runs before the first view maintenance | `SqliteEvaluate::open` order |

Rollback leaves arena entries that no committed row references. The next
flush writes them anyway. The ids stay stable, and no row points at the
unreferenced terms.

## 6. What this page does not settle

- Whether F3b's single view keeps comptime rounds cheap when rules change
  every round. Step 6 measures the view `CREATE` cost per round in spans.
- The `dl_order_key` byte layout. Step 4 freezes it with a COUNT test over
  `oracle/eval/*` `any.lt` rows.
