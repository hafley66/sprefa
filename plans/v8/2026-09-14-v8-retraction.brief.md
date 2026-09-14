# Brief: retraction lab, Key as replacement identity

## 0. Contents
1. Job
2. Base and first action
3. Ownership
4. What exists, with lines
5. Design (decided) and the assumption
6. Deliverables in order
7. Validation
8. Style laws
9. Reporting

## 1. Job
A relation column marked `Key` (v6 `key(1)`) is a replacement identity: inserting a row whose key columns match an existing row replaces that row. Derived relations that depended on the old row lose the rows only it justified. This lab lands Key-as-replacement in the dl8 evaluator with delete-and-rederive per stratum (the v6 shape, store plan section 10.1), measures it against the signed-rows alternative on one fixture family at three sizes, and writes the numbers into the PR for Chris to pick the long-term shape. The measurement is the deliverable as much as the code.

## 2. Base and first action
- Base sha: `BASE_SHA` (`origin/main`). Branch `lab/v8-retraction-20260914`, worktree under `/Users/chrishafley/projects/sprefa-wt/`.
- FIRST command: `git merge --ff-only BASE_SHA`. Failure = stop and report.
- Commits end with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`. Commands run from `v8/`.

## 3. Ownership
You own: `v8/src/_6_eval/_3_table.rs`, `v8/src/_6_eval/_5_evaluate.rs` (the evaluation loop and a new `retract` path; NOT the aggregate functions, NOT the effect branch), `v8/src/_6_eval/_1_program.rs` (a `keys: Vec<usize>` on a relation declaration), `v8/src/_2_lower/**` and `v8/src/_3_check/**` ONLY for reading the existing `Key`/`key(` spelling into the program (grep first: `git grep -n "Key\b\|key(" v8/src`), `v8/tests/_19_retraction.rs` (new), `v8/fixtures/retraction/**` (new), `v8/bench/retraction/**` (new, the measurement harness).
Forbidden: `v8/src/_9_runtime/**` (reconciler lane), `v8/src/_5_reify/**` (emitter lane), `v8/oracle/**`, `v7/`, `v6/`, `sqlite_ivm/`. Never spawn subagents. Never `--no-verify`.

## 4. What exists
| thing | where |
|---|---|
| `Key` on a column = replacement identity, unrelated to requests, lands with retraction | `chat_log/20260914.1.dl8-effects-settled-design-reviews-extract-move.md:28` |
| store plan section 10: delete-rederive (v6, `lower.pl:4536`) vs signed rows; "decides nothing" | `plans/v8/2026-09-14-v8-store.PLAN.md:512-560` |
| astra: insert-only `Store` cannot remove old aggregate rows; define replacement before Fold optimisation | `plans/v8/2026-09-14-v8-design-review.astra.md:253`, `:321` |
| `Table`: append-only rows, `frontier`, per-column index | `v8/src/_6_eval/_3_table.rs` |
| `Store`, `mark_all` | `v8/src/_6_eval/_5_evaluate.rs:45-65` |
| strata and the fixpoint loop | `v8/src/_6_eval/_2_stratify.rs`, `_5_evaluate.rs` `evaluate_into` |
| v6 incremental engine: changed/shrank/grew tracking, boundary delta, aggregate retention | `v6/sprefa-engine-rs/src/incremental.rs:44`, `:1100`, `:1214`, `:1757`, `:1989` |
| ghcacher tick 3: keyed clock replacement retracts witness 1's demand; a `key(1)` latch survives the bucket moving | `v6/tsv2/goldens/ghcacher_tick_golden/README.md` table rows 3 to 5 |
| `sqlite_ivm` deletion = over-delete then rederive, semi-naive rounds | `sqlite_ivm/README.md:118-126` |

## 5. Design (decided) and the assumption
```rust
// _1_program.rs
pub struct Relation { pub rel: TermId, pub arity: usize, pub keys: Vec<usize> }   // keys empty = set semantics
// _3_table.rs
impl Table {
    /// Replace: rows whose key columns equal `row`'s are removed and returned; `row` is appended.
    pub fn replace(&mut self, keys: &[usize], row: Box<[TermId]>) -> Vec<Box<[TermId]>>;
    /// Removal keeps the per-column index consistent; `frontier` is clamped.
    pub fn remove(&mut self, row: &[TermId]) -> bool;
}
// _5_evaluate.rs
/// After seeds with keys are applied: for each stratum in order, if any input relation
/// lost rows, clear every derived relation of that stratum and rederive it from the
/// current lower rows (delete-and-rederive). Strata with no lost input keep their rows.
fn retract_and_rederive(store, u, program, strata, fx) -> Closure;
```
- Assumption, flagged to Chris in the PR: the surface spelling of a key column is whatever `v8/src` already reads (`grep -n "key(" v8/src/_2_lower`); if nothing reads one, the lane adds `key(<position>)` on the `rel` declaration in the dl7 reader ONLY as far as the lowerer's existing declaration row allows, and reports the throw site if it does not.
- Measurement harness `v8/bench/retraction/`: one fixture family (a keyed `watch(repo, tag)` feeding a two-stratum derivation with one aggregate), generated at 1k, 10k, 100k rows; one replacement per tick, 20 ticks; report wall ms per tick and rows rewritten per tick for (a) delete-and-rederive as built, (b) a throwaway signed-rows variant kept on a separate commit that the PR names and does not merge. Three samples each. The 10-second law applies per tick.
- Instance lifetime: `keys` lives in `Program`; `Table` rows live until replaced.
- Uniqueness: at most one row per key tuple per relation after every tick; a COUNT test asserts it.

## 6. Deliverables in order
1. `Relation.keys` read from the program; `Table::replace`/`remove` with index consistency tests (pure computation, unit tests allowed).
2. `retract_and_rederive` wired into the fixpoint; fixtures: keyed seed replacement retracts a derived row; a `key(1)` latch fed by a level relation survives (port of ghcacher README row 3 in miniature); aggregate over a keyed input recomputes; a non-keyed relation is untouched.
3. Real-binary test `_19_retraction.rs` over the fixtures.
4. The measurement harness and its numbers in the PR body as a table.
5. Forks section in the PR: signed rows (what changes in `Table`, what the v6 `incremental.rs` sites do), and per-stratum change-directed rederive.

## 7. Validation
```bash
cd v8 && cargo test --locked && cargo clippy --locked --all-targets -- -D warnings && grep -rn "eprintln!" src/ | wc -l
git diff --stat origin/main -- v8/oracle | tail -1     # empty
```
Batteries in the background, per-case cap 10 s.

## 8. Style laws
Comment budget. Banned words: provenance, substrate, load-bearing, regime, ground truth, refusal, support. No em dashes. Descriptive names. Language design stays with Chris: forks are written, not decided.

## 9. Reporting
PR title `lab(v8): Key as replacement identity with delete-and-rederive, measured`. Then:
```bash
boop beep --no-wait --as <your-lane-name> sprefa-coordinator "retraction: PR #<n>, cargo test <pass>/<total>, clippy 0, ms/tick at 100k: dred <x> signed <y>"
```
Blocked or brief wrong: same command, one line, stop. One lane, one task.
