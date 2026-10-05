# Findings: non-monotone LetRec and `next`

Brief: `plans/v8/2026-10-05-nonmonotone-letrec.brief.md`. Base sprefa `296b64b53`, sqlite_ivm
`a9e4b37` (clone at `sqlite_ivm/`, branch `feat/nonmonotone-letrec`).

## Semantics

`LetRec { ids, bodies, limit }` evaluates like Materialize `WITH MUTUALLY RECURSIVE`: every
variable starts empty, each round recomputes every variable from the previous round's values,
and the loop stops when a round changes no variable. A body may negate, aggregate and retract.

| point | rule here | Materialize |
|---|---|---|
| order inside a round | simultaneous: every body reads the previous round's values (one DD `Variable` per id, feedback delayed one round) | sequential: "Update `$R_2` using the current values bound to `$R_1, ..., $R_n` ... this includes the new value of `$R_1` bound above" (docs, `sql/select/recursive-ctes`) |
| fixed point | no variable changes in a round | "The values bound to all CTEs have stopped changing" |
| `limit = n` | evaluations 1 ..= n may change a variable; evaluation n + 1 must change none, else the settle fails with `ErrorKind::LetRecLimit(n)` (`rel` = first id). The value the loop holds when cut is evaluation n. | `ERROR AT RECURSION LIMIT n` fails the query; `RETURN AT RECURSION LIMIT n` returns the values after n iterations (docs example: limit 1 returns one iteration's rows) |
| cut mechanics in DD | updates whose record time has `inner >= n` are filtered out of the feedback and the output; the same updates, tagged with their variable index, leave the scope, consolidate, and set the error flag (`crates/ivm-dd/src/2_dd.rs`, `Time::letrec`) | `render_recursive_plan` (`src/compute/src/render.rs`) partitions `oks` by the record's iteration index into in-limit and over-limit, maps over-limit to `EvalError::LetRecLimitExceeded` |
| `limit = 0` | install error `Unsupported("LetRec limit 0")` | - |

The simultaneous order is the one a time step needs: wave W + 1 is a function of wave W only.
For a monotone body both orders reach the same least fixed point.

Untagged over-limit rows cancel: `+9` and `-8` in the same round of one variable map to the same
empty row. The first DD cut did that and returned evaluation 8 with no error; the rows keep their
values now.

## Engines

| engine | monotone LetRec | non-monotone LetRec or any `limit` | maintenance under outer deltas |
|---|---|---|---|
| ivm-dd | unchanged (`iterate`, threshold per round) | same loop; DD retracts inside `iterate`; `limit` cut above | differential: rounds of every epoch are corrected in place, no recompute |
| ivm-sqlite | unchanged DRed round loop (`Sql::fixpoint`) | recompute path (`Sql::recompute`): old value into `acc` at -1, every loop-owned image and private table emptied, each copy's delta set to its input's whole post-frontier image, rounds from empty variables; each round fills, integrates, then `step` sets a variable to the support of its body's image over the rows the body changed. Result delta = `acc`. | the whole LetRec recomputes at every settle that changes one of its inputs; a settle that changes none skips it (the `Step::Loop` read check). Logged: span `frontier_recompute` with `rounds`, event `LetRec recomputed` with `evaluations`. |

A LetRec takes the sqlite recompute path when it has a `limit` or its body has a Negate,
Threshold or Reduce over a node that reads a variable, or an Antijoin whose right input reads one
(`nonmonotone`, `crates/ivm-sqlite/src/3_nodes.rs`). Inside that path every Antijoin is
`l - l ⋉ threshold(π r)`. DRed is not used there: a row deleted in the over-delete phase can come
back through a negation, so the over-delete set is not an upper bound.

## `next` in dl8

| piece | site |
|---|---|
| syntax: a head `(next (R args...))` | `src/_2_lower/_7_goals.rs` `next_head`, `lower_rule`: the head call is `call(next(R), Args)` |
| check | `src/_3_check/_2_resolve.rs` resolves the inner relation and keeps the wrapper; `_7_resolved.rs` checks the inner call's arity |
| stratification | unchanged code: `next(R)` is its own node in the dependency graph and no goal reads it, so a negation or aggregate read by a `next` rule closes no cycle (temporally stratified). A negation cycle inside one wave is still `strict_dependency_cycle` (`fixtures/next/3_same_wave_negation.dl7`). |
| IR lowering | `src/_6_eval/_9a_ir_lower.rs` `waves`, `lower_waves`, `substitute` |
| engine error | `src/_6_eval/_9b_ir_eval.rs` `engine_diagnostic`: `LetRecLimit` is the diagnostic `wave_limit(Limit)` |
| sqlite view emitter | `src/_5_reify/_7_sqlite.rs`: `unsupported_construct(next)` |

Lowering. Wave relations: every `R` with a `next` head. In-wave relations ("cone"): relations
ordinary rules derive from a wave relation, transitively. One LetRec per program:

```
ids    = [__ir_wave_started, R1, R2, ...]
bodies = [none-row,
          (seeds(R) ▷ started) ∪ π(next_bodies(R) × started), ...]
limit  = WAVE_LIMIT (64, the macrotime wave bound)
```

Round 1: `started` is empty, so every wave relation holds its seed rows (wave 0) and `started`
gets its row. Round k + 1 holds wave k: exactly the rows the `next` rules derive from round k.
A cone relation read by a `next` body is inlined as `threshold(seeds ∪ its rule bodies)` over the
same round's variables (the DAG is copied only where it reaches a cone `Get`), so it is evaluated
inside the wave. After the LetRec the cone relations lower as ordinary `Let` strata over the
final wave. The LetRec has a limit, so ivm-sqlite always takes the recompute path for it.

Not built (named diagnostics, `unsupported_construct`): an ordinary rule whose head is a wave
relation; recursion inside one wave (`fixtures/next/4_wave_recursion.dl7`); an intern readback,
a declared fold, or a served relation in a wave or cone rule. The `(: R (waves N))` declaration
of the page 3 proposal is not built; the limit is `WAVE_LIMIT`.

Fixtures (`fixtures/next/`, test `tests/it/_9k_next.rs`, compile and eval on each engine):

| fixture | page 3 example | final rows (both engines) |
|---|---|---|
| `0_graph_carry` | 1, graph carry | `graph_item` (0 20 0) (0 2 1); `removed_item`, `added_item` empty |
| `1_counter` | 2, counter | `cur_wave` 3; `rewrite_pending` empty |
| `2_origin` | 3, origin rows | `origin_claim` (1 10 0) (10 20 1); `cur_wave` 2 |
| `6_wave_count` | count inside a wave | `pile` 0 1 2; `pile_size` 3 |
| `3_same_wave_negation` | - | `strict_dependency_cycle` |
| `4_wave_recursion` | - | `unsupported_construct(recursion inside one wave)` |
| `5_wave_limit` | - | `wave_limit(64)` |

## Before / after

Debug binaries; before = `296b64b53` + sqlite_ivm `a9e4b37` (the `sqlite-perf-main` build),
after = this branch. Alternating runs, load 4.3-4.6. `cmp` of `target/tsi/registry.rs` against
`hafley-rs/crates/hafley_scm/src/read/tsi/registry.rs`: identical in all 12 runs.

| engine | before real s | after real s |
|---|---|---|
| ir:sqlite, `dl8 build std/tsi_registry_emit.dl7` | 1.435, 1.416, 1.414 | 1.422, 1.508, 1.412 |
| ir:dd, same | 0.404, 0.363, 0.415 | 0.376, 0.410, 0.369 |

The registry program has no `next` head and no LetRec limit, so every LetRec keeps the monotone
path on both engines.

| suite, `cargo test -j 4 --workspace --no-fail-fast -- --skip _7_book --test-threads=1` | before (brief) | after |
|---|---|---|
| ir:sqlite | 166/166, 189 s | 172/172 (2 + 170), 182.6 s wall |
| ir:dd | 166/166, 115 s | 173/173 (2 + 171), 117.9 s wall |

The dd run is one test larger: `count_inside_one_wave` landed between the two runs.

sqlite_ivm: `cargo test -j 4 -p ivm-dd -p ivm-sqlite -p ivm-engine -p ivm-ir -- --test-threads=1`
all green, new: `17_nonmonotone_walk` on both engines (oracle script), `12_nonmonotone_letrec`
5/5. `scripts/9_verify.sh`: nextest battery 86/86; its bench step stops on a missing
`../hafley-rs-wt` path dependency of `labs/20260923.0` outside this lane.

## What macrotime B needs now

| need | today | site |
|---|---|---|
| recursion inside one wave: `rewrite_up`, `rewrite_walk`, `reach`, `slot_start` successor recursion | `unsupported_construct(recursion inside one wave)`; both engines refuse a LetRec nested in a LetRec | `_9a_ir_lower.rs` `waves`; ivm-dd `Nest for Inner`, ivm-sqlite `letrec` |
| minted nodes read back in a later wave (rewrite outputs) | `intern readback in a wave rule` | `_9a_ir_lower.rs` compile loop |
| sqlite cost: the wave LetRec recomputes every wave of a unit at each settle touching it | first cut, measured above only for the fixtures | `Sql::recompute` |
| repeated-state stop (`expansion_cycle`) | a cycle of waves runs to `wave_limit(64)` | Rust `seen` list in `_4_expand.rs` |
| per-relation bound `(: R (waves N))` | `WAVE_LIMIT` for every program | - |
| one engine pass per unit instead of `read_wave` + `graph.apply` per wave | the Rust wave loop is unchanged | `src/_1_macrotime/_4_expand.rs` |
