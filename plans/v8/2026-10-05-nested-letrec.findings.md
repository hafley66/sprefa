# Findings: recursion inside one wave (nested LetRec)

Brief: `plans/v8/2026-10-05-nested-letrec.brief.md`. sprefa branch `feat/nested-letrec` from
`eac0cb9b9`; sqlite_ivm clone `sqlite_ivm/`, branch `feat/nested-letrec` from `aa72853`. Debug
dl8, `-j 4`, `--test-threads=1`. Counts only, no wall-clock numbers.

## Semantics

IR: `LetRec { ids, bodies, limit, nested: Vec<LetRec> }` (`crates/ivm-ir/src/0_ir.rs`;
`#[serde(default, skip_serializing_if = "Vec::is_empty")]`, so every existing program serializes
byte-identically). `LetRec::binds(rel)`: an id of the LetRec or of one nested in it.

| point | rule |
|---|---|
| evaluation | each round of the outer LetRec first evaluates its nested LetRecs in order, each from empty to its own fixed point, then the outer bodies. A nested body reads the outer variables' previous-round values (the same simultaneous order as the outer bodies) and the results of nested LetRecs before it in this round. |
| visibility | nested ids are bound inside the outer LetRec only: outer bodies and later nested bodies read them; later strata do not |
| inner limit | counts the nested LetRec's evaluations within one outer round. A change in evaluation `limit + 1` in any outer round fails the settle with `LetRecLimit(inner limit)`, `rel` = the nested LetRec's first id |
| outer limit | counts outer rounds, as before; `rel` = the outer first id |
| depth | one level. A nested LetRec with its own `nested` is `Unsupported("LetRec nested two deep")` on both engines; `ivm-rxjs` emit returns `nested LetRec unsupported` |
| limit 0 | `Unsupported("LetRec limit 0")` for the outer or a nested LetRec, at install |

## Prior art

| system | nested recursion |
|---|---|
| differential dataflow | `iterate` / `Variable` scopes nest; each scope extends the timestamp with one coordinate (`Product<Product<T, u64>, u64>`), ordered by the product order, so an inner scope iterates to its fixed point at each outer iteration and its output leaves accumulated over its rounds. `differential_dataflow::dynamic` (`PointStamp`) represents unboundedly deep nesting with one timestamp type; its module doc names Naiad and Materialize as the users |
| Materialize | `WITH MUTUALLY RECURSIVE` may appear inside a recursive CTE's body; the inner block is an expression evaluated inside each outer iteration, so it runs to its own fixed point per outer iteration; each block carries its own `RECURSION LIMIT` (error or `RETURN AT`). Rendering uses the `dynamic` / `PointStamp` timestamps (not checked against Materialize source in this lane) |

## Where each engine refused (base)

| site | base line | what |
|---|---|---|
| `crates/ivm-ir/src/0_ir.rs` | 226 | `LetRec { ids, bodies, limit }`: no way to express a nested LetRec; `bodies` are node ids and no `Op` holds a LetRec |
| `crates/ivm-dd/src/2_dd.rs` | 84, 92-99 | `Nest for Inner::letrec` returns `Unsupported("LetRec nested in LetRec")`; "Only the top level may open a LetRec scope" |
| `crates/ivm-sqlite/src/3_nodes.rs` | 1330, 1337-1339 | `letrec` with `self.active` set returns `unsupported("LetRec nested in LetRec")` |
| `crates/ivm-rxjs/src/2_emit.rs` | 217 | any `limit` refused (so every wave LetRec) |
| sprefa `src/_6_eval/_9a_ir_lower.rs` | 93 | `waves`: a cone relation left on a cycle is `unsupported_construct(recursion inside one wave)` |

At base neither engine refusal was reachable from a `Program`: `ivm_engine::lower` calls `letrec`
only for top-level strata.

## Engines

ivm-dd (`letrec_scope<T>`, `crates/ivm-dd/src/2_dd.rs`):

- One generic scope builder for `Time` (top level) and `Inner = Product<Time, u64>` (inside a
  LetRec); `Nested = Product<Inner, u64>` refuses (`Nest for Nested`). The monomorphized call
  chain is `letrec_scope::<Time>` -> `Inner::letrec` -> `letrec_scope::<Inner>` ->
  `Nested::letrec` (refusal); no function calls itself at one type.
- Hoisting: the outer scope hoists the nodes that read neither its variables nor any nested
  variables (bodies of all of them, `binds`); a nested scope hoists into the outer scope the
  nodes that read none of its own variables (`independent_inputs`, moved to `ivm-engine`).
- Limit cut as before at `t.inner >= limit` in each scope. A nested scope's over-limit rows also
  carry the outer round (`t.outer.split()`), so the outer loop's next-round correction (`-` of a
  row) cannot cancel the round that hit the limit; the rows leave both scopes into `rel.limits`.
  Depth and `limit 0` are checked before the scope opens (an error inside `scoped` leaves a
  timely operator slot unfilled and panics the worker).

ivm-sqlite (`crates/ivm-sqlite/src/3_nodes.rs`):

| piece | rule |
|---|---|
| path | a LetRec with nested LetRecs takes the recompute path (`nonmonotone || !nested.is_empty()`); `NodesPlan::recompute_nested` |
| inner path | monotone inner body without a limit: DRed (`fixpoint`), maintained across outer rounds from its copies' deltas (retractions included, when a wave shrinks the relation the walk reads). Otherwise recompute from empty (`recompute`) |
| ownership | `Active.parent`, `SccPlan.parent`. Inside a nested LetRec a node reading none of its variables is owned by the enclosing loop (`Owner::Loop(parent)`); nested copies (`Owner::Copy(inner)`) fill in the enclosing round; nested results are enclosing-loop nodes |
| inputs | `nested()`: every independent root of the nested bodies is lowered in the enclosing loop; a root outside both loops goes through the enclosing loop's copy first, so its delta is seen once per outer recompute |
| round | the enclosing round runs its fills up to the nested SCC's position (`SccSql.nested: (fill index, scc)`), runs the nested SCC when one of its external deltas is nonempty (`scc_reads`), marks its result deltas, clears its own targets, runs the remaining fills, then closes the round (`close_round`) |
| reset | the enclosing recompute empties every nested image, delta and private table |
| no recursion | `run` -> `recompute_nested` -> `fills` / `fixpoint` / `recompute` / `close_round`; `recompute` -> `round` -> `fills`, `close_round`. No cycle |

`ivm-sqlite` catalog split (`4_engine.rs` `output_program`) visits, keeps and remaps nested bodies
and ids.

## dl8

`waves` (`src/_6_eval/_9a_ir_lower.rs`): the cone is split into dependency components (mutual
`reachable` sets) in dependency order, each marked recursive when a relation reaches itself.
`lower_waves`: a non-recursive component is inlined as before (`threshold(seeds ∪ bodies)`); a
recursive component becomes one nested LetRec of the wave LetRec over fresh relations
`__ir_wave_rec_<rel>` (`seeds ∪ bodies`, bodies substituted over the same round's wave variables
and earlier cone values), `limit: None` (no dl8 syntax for an inner limit). After the wave LetRec
the cone relations lower as before over the final wave (a recursive one as its own LetRec
stratum). A negation inside one recursive component is still `strict_dependency_cycle`
(stratify, unchanged); a negation of an earlier component is accepted (`8_wave_reach`).

| fixture (`fixtures/next/`) | page 3 shape | final rows, both engines |
|---|---|---|
| `4_wave_recursion` (was `unsupported_construct`) | reach over the wave graph | `graph_item` (0 1) (1 2); `reach` 0 1 2 |
| `7_wave_walk` | `rewrite_walk`: frontier walk through unclaimed nodes, claimed nodes replaced | `graph_item` (0 1 0) (1 2 0) (1 6 1) (3 4 0) (9 5 0); `walk` 0 1 2 6; `hit` empty |
| `8_wave_reach` | `rewrite_candidate` + `reach`: two recursive components, the second negating the first | `graph_item` (0 1) (1 5) (1 7); `reach` 0 1 5 7; `below` empty |
| `9_wave_slot` | `slot_start`: successor recursion with a running sum | `item` (0 0 3) (0 1 2) (0 2 2); `slot_start` (0 0 0) (0 1 3) (0 2 5); `growing` empty |

Test `tests/it/_9k_next.rs` (`recursion_inside_one_wave`, `walk_inside_one_wave`,
`reach_inside_one_wave`, `slot_start_inside_one_wave`). Each nested LetRec seen in the
`DL8_IR_DUMP` of the compile: `4` 1 nested, `7` 1, `8` 2, `9` 1; `0_graph_carry` 0.

## Engine tests (sqlite_ivm)

- `crates/ivm-dd/oracle/18_nested_wave_reach.{sql,program.json}`: transitive closure `reach`
  nested in the wave LetRec over `g`, the relation each wave rewrites (drops the out-edges of
  reached `cut` nodes); SQLite oracle view is the closed form. Seven steps, through the script
  harness on both engines (`0_scripts::nested_letrec_wave_reach`,
  `1_scripts::nested_letrec_wave_reach`): identical marbles.
- `crates/ivm-sqlite/tests/14_nested_letrec.rs`, dd == sqlite: inner recompute path equals inner
  DRed over four frontiers; inner limit not hit (4) / hit (3) on a 4-chain; inner limit hit by a
  later frontier; outer limit (1) not hit / hit; two deep refused; sqlite rolls back an inner
  limit hit and settles after it; work-count pin.

## Counts

sqlite work of the oracle program, `Sqlite::install_counted`, install then the four settles of
`inner_recompute_path_matches_inner_dred` (pinned in `14_nested_letrec::work_counts`):

| inner path | install executions | settle executions | zero-row | vm_steps | written_d | written_i | written_scratch |
|---|---|---|---|---|---|---|---|
| DRed (`limit: None`) | 85 | 841 | 241 | 43,831 | 346 | 267 | 173 |
| recompute (`limit: 8`) | 85 | 1,042 | 328 | 50,167 | 420 | 347 | 215 |

New dl8 fixtures, `dl8 compile`, the user program's install and settle (`RUST_LOG=ivm_sqlite::work=info,ivm_dd=debug`):

| fixture | sqlite install prepared | install executions | settle executions | settle vm_steps | written_d | written_i | written_scratch | dd install operators | dd arrangements | dd arranged_records | dd steps |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 4_wave_recursion | 1,473 | 1,473 | 3,835 | 3,599,249 | 13,690 | 8,075 | 4,705 | 7,211 | 375 | 8,604 | 41 |
| 7_wave_walk | 1,509 | 1,509 | 4,177 | 3,721,102 | 14,232 | 8,409 | 4,898 | 7,450 | 378 | 8,850 | 87 |
| 8_wave_reach | 1,541 | 1,541 | 4,369 | 3,702,496 | 14,228 | 8,420 | 4,900 | 7,510 | 383 | 8,836 | 98 |
| 9_wave_slot | 1,536 | 1,536 | 4,688 | 3,855,436 | 14,414 | 8,390 | 4,854 | 7,599 | 399 | 8,936 | 109 |

Unchanged for programs without nested recursion:

- Census: the 128 `.dl7` under `fixtures/ examples/ book/src/probes/ std/` present on main
  (`dl8 compile`, both engines, 256 runs), main dl8 (`sqlite-perf-main` build of `cbc9024b8` +
  `aa72853`) against this branch, run in this worktree: exit code, stdout, non-counter stderr and
  every summed counter (sqlite work per phase, dd install operators / nodes / arrangements, dd
  settle changes / epochs / steps / arranged records and cells) identical in 256 of 256. Exit 0
  in 216, 1 in 40, on both.
- Registry: `tests/it/_9h_build.rs::tsi_registry_sqlite_work_counts` passes unchanged;
  `dl8 build std/tsi_registry_emit.dl7` `cmp` identical to
  `hafley-rs/crates/hafley_scm/src/read/tsi/registry.rs` on both engines.
- sqlite_ivm `13_work_counts::registry_install_work` passes unchanged.

## Validation

| command | result |
|---|---|
| `DL8_ENGINE=ir:sqlite cargo test -j 4 --workspace --no-fail-fast -- --skip _7_book --test-threads=1` | 177 passed (2 + 175), 0 failed; main 174 |
| `DL8_ENGINE=ir:dd`, same | 177 passed (2 + 175), 0 failed |
| sqlite_ivm `cargo test -j 4 --workspace -- --test-threads=1` (extension dylib rebuilt first) | 264 passed, 0 failed, 14 ignored; base `aa72853` 254 / 14 ignored (same command, separate worktree); the 10 new: 2 script cases, 8 of `14_nested_letrec` (`work_counts` is `cfg(not(feature = "image"))`, and the workspace build enables `image`) |
| `cargo test -j 4 -p ivm-sqlite --test 14_nested_letrec -- --test-threads=1` | 9 passed |

## Overlap with `feat/rule-interpreter`

Its worktree (`sprefa-wt/rule-interpreter`, `51ea9ac76`, uncommitted) modifies
`src/_6_eval/_2_stratify.rs`, `_9b_ir_eval.rs`, `mod.rs`, `src/bin/dl8.rs`; this branch modifies
`src/_6_eval/_9a_ir_lower.rs`, `tests/it/_9k_next.rs`, `fixtures/next/`. No shared file. Its
sqlite_ivm clone is unchanged from `aa72853`; this branch changes `ivm-ir` (`LetRec.nested`),
which every `LetRec { .. }` literal must name.

## Next

Macrotime B: lower `macrotime/3_rewrite.dl7` (page 3) to one wave LetRec with its walk / reach /
slot_start components nested, then the remaining blockers of the nonmonotone-letrec findings
(intern readback in a wave rule, one engine pass per unit).
