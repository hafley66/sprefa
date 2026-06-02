# datafrog as a `MemStore` backend behind a `RelStore` seam

Assessment of three cloned sources for an in-memory batch-fixpoint backend behind a future
sprefa `RelStore` trait. datafrog is the primary candidate; ascent is the richer alternative;
simple-graph is a SQLite schema idiom for comparison against sprefa's current SQL path.

Sources (all under `/Users/chrishafley/projects/ext/`):

- `datafrog/` — rust-lang-nursery, MIT/Apache, no runtime, no macros, no deps.
- `ascent/` — macro-based Datalog DSL, lattices, BYODS.
- `simple-graph/` — SQLite nodes/edges schema + recursive-CTE traversal.

---

## 1. datafrog: core API

License `Apache-2.0/MIT`, zero runtime deps (`Cargo.toml:5,16-17` — only `proptest` as a
dev-dep). No macros; you build and re-apply the rules by hand. Module surface
(`src/lib.rs:14-36`): `Iteration`, `Relation`, `Variable`, plus the `treefrog` leapjoin
machinery.

### `Relation<Tuple>` — `src/relation.rs:14-18`

```rust
pub struct Relation<Tuple> {
    /// Sorted list of distinct tuples.
    pub elements: Vec<Tuple>,
}
```

A fixed, sorted, deduped set of tuples (`from_vec` sorts+dedups, `relation.rs:78-82`). Static:
once built it does not vary across iterations. Relation-level combinators that return a new
`Relation` (for the non-recursive parts of a query):

- `from_join` — `relation.rs:50-56` → `join::join_into_relation`
- `from_antijoin` — `relation.rs:62-68` → `join::antijoin`
- `from_map` — `relation.rs:73-75`
- `from_leapjoin` — `relation.rs:39-45`

### `Variable<Tuple>` — `src/variable.rs:39-50`

```rust
pub struct Variable<Tuple> {
    pub(crate) distinct: bool,
    pub(crate) name: String,
    pub stable: Rc<RefCell<Vec<Relation<Tuple>>>>,   // converged tuples
    pub recent: Rc<RefCell<Relation<Tuple>>>,        // delta from last round
    pub(crate) to_add: Rc<RefCell<Vec<Relation<Tuple>>>>, // staged for next round
}
```

A monotonically increasing set with the three-stage lifecycle documented at
`variable.rs:22-33`: a tuple lands in `to_add`, is promoted to `recent` for exactly one round,
then folds into `stable`. This staging is what makes the loop **semi-naive**: a join only has
to consider `recent × stable` and `recent × recent`, not the full cross product
(`join::join_delta`, `src/join.rs:46-63`). The `Variable` operator methods mutate in place
(append to `to_add`):

- `from_join` — `variable.rs:112-119` → `join::join_into`. `input1` must be a `Variable`;
  `input2` may be a `Variable` or a `Relation` (the `JoinInput` trait, `join.rs:161-198`);
  joining two `Relation`s is rejected because the result can't vary.
- `from_join_filtered` — `variable.rs:156-163` (yields `Option<Tuple>`).
- `from_antijoin` — `variable.rs:194-201` (key absent in a fixed `Relation`).
- `from_map` — `variable.rs:232-234`.
- `from_leapjoin` — `variable.rs:267-274` (worst-case-optimal multi-relation join/antijoin).
- `insert` / `extend` — `variable.rs:305-321` (load initial values).
- `complete` — `variable.rs:329-336` (assert stable, flatten `stable` into one `Relation`).

### `Iteration` + the fixpoint loop — `src/iteration.rs:11-44`

```rust
pub struct Iteration {
    variables: Vec<Box<dyn VariableTrait>>,
    round: u32,
    debug_stats: Option<Box<dyn Write>>,
}
```

`iteration.variable::<T>(name)` registers a `Variable`; `iteration.changed()`
(`iteration.rs:24-38`) drives one round across every registered variable. Per variable
(`VariableTrait::changed`, `variable.rs:339-395`): fold `recent` into `stable`, merge `to_add`
into the new `recent`, dedup against `stable` if `distinct`, and report whether the new
`recent` is non-empty. The loop terminates when no variable produced new tuples.

### Smallest complete example — `examples/graspan1.rs:39-55` (transitive reachability)

```rust
// Create a new iteration context, ...
let mut iteration = Iteration::new();

// .. some variables, ..
let variable1 = iteration.variable::<(u32, u32)>("nodes");
let variable2 = iteration.variable::<(u32, u32)>("edges");

// .. load them with some initial values, ..
variable1.insert(nodes.into());
variable2.insert(edges.into());

// .. and then start iterating rules!
while iteration.changed() {
    // N(a,c) <-  N(a,b), E(b,c)
    variable1.from_join(&variable1, &variable2, |_b, &a, &c| (c, a));
}

let reachable = variable1.complete();
```

(The README, `datafrog/README.md:33-41`, is the same loop with the `edges` relation kept fixed.)

---

## 2. ascent: macro DSL, generated code, lattices, BYODS

ascent is a Datalog DSL embedded via the `ascent!{ relation ...; rule ...; }` proc-macro
(`ascent/README.MD:16-24`). Where datafrog makes you hand-write the staging and the join order,
ascent's macro **generates** that: from the `relation` decls it emits a struct (default name
`AscentProgram`) with one `Vec<tuple>` field per relation plus hidden index maps, and from the
`<--` rules it emits an SCC-stratified semi-naive fixpoint `run()` — the same delta-loop
datafrog asks you to write, synthesized. You fill input relations as plain `Vec`s, call
`prog.run()`, read output relations as fields (`README.MD:39-55`). Beyond plain Datalog it adds:
**lattices** (`lattice` keyword; last column joins instead of set-unions, e.g. `Dual<u32>` for
shortest paths, `README.MD:63-82`), **stratified negation + aggregation** (`agg`/`mean`/`count`,
`README.MD:154-172`), a parallel backend (`ascent_par!` over rayon, `README.MD:84-89`), and
**BYODS** (Bring Your Own Data Structures, `README.MD:91-119` + `BYODS.MD`). BYODS is the
relevant axis here: `#[ds(provider)]` on a relation swaps the storage/index backend
(`BYODS.MD:3-15`), where a provider is a set of macros (`rel_ind`, `rel_ind_common`,
`BYODS.MD:17-35`) producing types that implement read/write index traits (`RelIndexRead`,
`RelIndexReadAll`, `RelIndexWrite`, `RelIndexMerge`; `BYODS.MD:31-38`). So **ascent relation
storage can be backed by an external store** — but the contract is index traits with `Key`/
`Value` associated types over in-memory tuples (`BYODS.MD:32-33`), not an `insert_rows/scan`
row API, and the providers are compile-time macros, not runtime objects. Adapting sprefa's
`RelStore` to BYODS means writing a macro provider, not implementing a trait.

Smallest example (`README.MD:39-54`):

```rust
use ascent::ascent;
ascent! {
   relation edge(i32, i32);
   relation path(i32, i32);

   path(x, y) <-- edge(x, y);
   path(x, z) <-- edge(x, y), path(y, z);
}

fn main() {
   let mut prog = AscentProgram::default();
   prog.edge = vec![(1, 2), (2, 3)];
   prog.run();
   println!("path: {:?}", prog.path);
}
```

---

## 3. simple-graph: SQLite schema + recursive-CTE traversal

A two-table graph-in-SQLite idiom (`simple-graph/README.md:5-16`): nodes are JSON blobs keyed
on an extracted `id`, edges are `(source, target, properties)` triples with FKs back to node
ids.

Schema — `sql/schema.sql:1-18`:

```sql
CREATE TABLE IF NOT EXISTS nodes (
    body TEXT,
    id   TEXT GENERATED ALWAYS AS (json_extract(body, '$.id')) VIRTUAL NOT NULL UNIQUE
);
CREATE INDEX IF NOT EXISTS id_idx ON nodes(id);

CREATE TABLE IF NOT EXISTS edges (
    source     TEXT,
    target     TEXT,
    properties TEXT,
    UNIQUE(source, target, properties) ON CONFLICT REPLACE,
    FOREIGN KEY(source) REFERENCES nodes(id),
    FOREIGN KEY(target) REFERENCES nodes(id)
);
CREATE INDEX IF NOT EXISTS source_idx ON edges(source);
CREATE INDEX IF NOT EXISTS target_idx ON edges(target);
```

Traversal ships as a Jinja2-templated recursive CTE (`sql/traverse.template:1-9`); the
outbound, id-only instantiation:

```sql
WITH RECURSIVE traverse(x) AS (
  SELECT id FROM nodes WHERE id = ?
  UNION
  SELECT id FROM nodes JOIN traverse ON id = x
  UNION
  SELECT target FROM edges JOIN traverse ON source = x
) SELECT x FROM traverse;
```

This is the same fixpoint shape sprefa already lowers `closure(edge)` to — a `WITH RECURSIVE`
walk over an edge table — confirming the SQL path and the datafrog path compute the identical
relation. simple-graph is the idiom sprefa's current SQLite backend already embodies; it is the
baseline the `MemStore` would have to match.

---

## (a) Sketch: `MemStore` impl of sprefa's plural `RelStore`

sprefa's write seam today is `Db` (`v5/src/db.rs:64-116`): the plural chokepoint is
`insert_rows(table, cols, rows: &[Vec<Value>])` (`db.rs:97`), with `Value` = `Text(String) |
Int(i64)` (`v5/src/ast.rs:27`). The fixpoint that consumes those rows is `rebuild_derived`
(`v5/src/engine.rs:1798-1815`): wipe derived tables, then per stratum
`loop { delta += execute(lower_rule(...)); if delta == 0 break; }`. That loop is structurally
identical to datafrog's `while iteration.changed()` — stratified, delta-driven, run to a
zero-delta fixpoint. Only the join engine differs (SQL `INSERT ... SELECT` vs. sorted-merge
`from_join`).

Extract a trait the current `Db` already satisfies, with one in-memory peer:

```rust
// The plural seam (Db is the SQLite impl today; MemStore is the datafrog impl).
pub trait RelStore {
    fn insert_rows(&self, table: &str, cols: &[&str], rows: &[Vec<Value>]) -> Result<usize>;
    fn scan(&self, table: &str, cols: &[&str]) -> Result<Vec<Vec<Value>>>;
    fn count(&self, table: &str) -> Result<i64>;
}

// In-memory backend: each base relation is a sorted Vec of Value-tuples.
pub struct MemStore {
    rels: RefCell<HashMap<String, BTreeSet<Vec<Value>>>>, // base + derived, deduped+sorted
}
```

`Value` already derives `Ord` is the only precondition (it is `Text|Int`, trivially orderable),
so `Vec<Value>` is a valid datafrog `Tuple`. The solve path:

1. **Ingest.** `insert_rows` pushes each `Vec<Value>` into the named `BTreeSet` — same plural
   contract as `Db::insert_rows`, same N+1 discipline (one call per relation per tick, not
   per row). No SQL.
2. **Build datafrog inputs.** For each base relation referenced by the program, project its
   `BTreeSet<Vec<Value>>` into a typed datafrog `Relation<(K, V)>` keyed on the join column the
   lowered rule uses — the keying that `lower_rule` currently encodes as a SQL `ON` clause.
   `Relation::from_vec` (`relation.rs:78`) handles the sort+dedup.
3. **Run the fixpoint.** Mirror `rebuild_derived`'s stratum order. Per stratum build an
   `Iteration`, register one `Variable` per derived relation, seed the base `Relation`s, and
   run `while iteration.changed() { var.from_join(&var, &edges, ...) }` — one `from_join` /
   `from_antijoin` / `from_map` call per lowered rule body, mapped from the same rule AST that
   `lower_rule` consumes. Negation maps to `from_antijoin` against a completed lower-stratum
   `Relation` (datafrog antijoins take a fixed `Relation`, `variable.rs:194` — which is exactly
   why sprefa's strata must finish before a higher stratum's negation reads them,
   `engine.rs:1801-1803`).
4. **Materialize.** `var.complete()` (`variable.rs:329`) flattens each derived `Variable` back
   to a `Relation`; write its `elements` back through `insert_rows` into the `MemStore` table
   so `scan`/`count` see the derived rows. Result set is identical to the SQL backend's.

`closure(edge)` (sprefa's `WITH RECURSIVE` condensation, `engine.rs:1784`) is the graspan1 loop
verbatim (`examples/graspan1.rs:50-53`): `var.from_join(&var, &edge, |_b, &a, &c| (c, a))`. The
recursive-CTE in simple-graph's `traverse.template` and the datafrog `from_join` loop compute
the same transitive closure, which is the portability check.

Net: the same rule AST lowers either to SQL strings (SQLite `Db`) or to `from_join`/`from_map`/
`from_antijoin` calls (datafrog `MemStore`), behind one `RelStore` trait. ascent is the richer
alternative if lattices or worst-case-optimal joins become load — but its storage seam is
compile-time index-trait macros (BYODS), not the runtime `insert_rows/scan` shape, so it costs a
macro provider rather than a trait impl.

## (b) Honest limit: datafrog is BATCH, no retraction

datafrog `Variable`s are **monotonically increasing** by construction (`variable.rs:22`,
"monotonically increasing set"; `lib.rs:5-6`). The entire mechanism — promote `recent`, fold
into `stable`, dedup-forward — only ever adds tuples. There is no retraction, no negative delta,
no way to remove a tuple once `stable`. `complete()` even asserts the iteration has fully
settled (`variable.rs:330`) before handing back results.

So a `MemStore` over datafrog proves **backend portability for the static solve only**: given a
fixed set of base facts, it computes the identical derived relations sprefa's SQL fixpoint
produces. It does **not** model sprefa's incremental layer. sprefa's reactive path —
`--changed` edits, `retract_paths` pruning a file's rows (`db.rs` / engine), rev-aware
relation variants, the per-tick wake/subscribe machinery — requires removing facts and
recomputing only the affected closure. datafrog cannot retract; the analogue is **full rebuild**
(re-seed every `Relation` from the current `MemStore` and re-run the `Iteration` from scratch),
which is exactly what `rebuild_derived` does on a full tick but not what the incremental path
does. The incremental story needs differential dataflow (datafrog's successor, which carries
signed multiplicities) or a hand-rolled delta/retraction layer on top — neither is in datafrog.

Bottom line: datafrog validates that sprefa's lowered fixpoint is backend-agnostic for the
static case (cheap, dep-free, MIT/Apache), and is a fair in-memory oracle to differential-test
the SQL backend against. It is not a path to the reactive/incremental engine.
