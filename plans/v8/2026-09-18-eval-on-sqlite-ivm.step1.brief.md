# eval-on-sqlite-ivm step 1: stop the demand bleed in `_6_eval`

Parent brief: `plans/v8/2026-09-18-eval-on-sqlite-ivm.brief.md` (read its
"Machine laws" and "Laws inline" first; they bind this lane). Branch
`fix/eval-demand-bleed` from `origin/main`. One PR. Report to the driver lane
`plan-eval-on-sqlite-ivm` and the coordinator.

## Defect, with receipts

`src/_6_eval/_5_evaluate.rs` on `origin/main`:

| site | what it does | cost |
|---|---|---|
| `:250-256` `positive_solutions` | calls `demand` for EVERY positive goal with any bound arg on a relation that heads a rule | fires on relations bottom-up already derives in full |
| `:283-322` `demand` | top-down proof, plan `Range::All` (`:308`) | semi-naive delta defeated inside the demand tree |
| `:379` `fire` | `memo: HashMap::new()` per firing | every firing redoes its whole demand tree |
| `:412`, `:567` | same fresh memo in `aggregate_proofs`, `fold_step` | same |
| `:787-806` | round >= 1 fires a rule once per delta position | multiplies the above |

Measured: one firing of `ambiguous_return` (`macrotime/1_caret.dl7:146-148`,
7 goals with `int.lt`) over the prelude's 16151 syntax rows = 48353 `demand`
calls, 246 ms release.

## Fix, two parts

1. `demanded: HashSet<TermId>` computed once in `evaluate_into` after
   `rules_by_rel` (`:718-725`), stored in `Context`. A relation is in it when
   any of its non-aggregate rules needs bound head arguments: a body goal that
   is a kernel constructor (`Kernel::of(u, g.rel).is_some()`, see
   `src/_6_eval/_4_kernel.rs`) or a negative goal, whose args mention a head
   variable not bound by an earlier positive non-kernel goal. The condition at
   `:250-253` gains `&& self.cx.demanded.contains(&goal.rel)`. If the
   precise rule breaks an oracle, widen it (any kernel or negative goal in the
   body) before anything else, and say which you shipped.
2. Memo moves out of `Eval`: one `HashMap<(TermId, Pattern), Vec<Vec<TermId>>>`
   per round, owned by the round loop (`:774`), passed `&mut` into `fire`,
   cleared when the round's pending rows are inserted (`:809-830`). The store
   is immutable inside a round, so entries stay valid. Hazard: a re-entrant
   key returns the partial rows found so far (`:285-288`); an entry computed
   while any in-progress key was hit is incomplete and must NOT be kept past
   the outermost `demand` call that started the cycle. Track an in-progress
   set; keep only entries whose computation hit no in-progress key other than
   itself. `aggregate_proofs` and `fold_step` may keep their per-call memo.

`Context` is `Copy` today (passed by value at `:741`, `:777`); keep it `Copy`
by adding `demanded: &'a HashSet<TermId>`.

## Counter for the COUNT test

Add `Trace::Demand { calls: usize }` emitted once per round with the round's
`demand` call count (count entries into `demand`, memo hits included).
`tests/_9_tracing.rs` shows how `Trace` is collected.

## Receipts required in the PR body

1. `timeout 600 cargo test --test _0_eval_oracle --test _2_macrotime_oracle --test _8_compile_oracle` PASS, no oracle file changed (`git diff --stat origin/main...HEAD -- oracle/` empty).
2. New test `tests/_24_demand_count.rs`: macrotime over the prelude, sum of
   `Trace::Demand.calls` under 500. On `origin/main` the prelude may not flow
   through macrotime; `feat/prelude-macrotime` (`a3ebebf8b`, `src/lib.rs`,
   `src/_8_driver/_2_macro.rs`) does. If no main-side entry point reaches it,
   copy the smallest syntax fixture that reproduces the `ambiguous_return`
   blowup into `tests/fixtures/demand_count/` and drive `evaluate` directly.
   Paste the before (main) and after counts.
3. P1 on the `feat/prelude-macrotime` tree: in a scratch detached worktree at
   `a3ebebf8b`, `git cherry-pick` your commit, `cargo build --release`,
   `timeout 10 target/release/dl8 compile fixtures/openapi/todo.dl7` with
   `RUST_LOG=dl8=info`; paste P1 ms (target under 100). Remove the scratch
   worktree after. Never commit to `feat/prelude-macrotime`.
4. `timeout 600 cargo test --no-fail-fast` once; red legs compared against
   `.github/CI-KNOWN-RED.md` "dl8 cargo battery". `_22_book::chapter_executors`
   alone, three times, walls pasted.
5. `ps -eo pid,pcpu,rss,args | grep dl8` before and after, pasted.

## Ownership

Owned: `src/_6_eval/_5_evaluate.rs`, `tests/_24_demand_count.rs`,
`tests/fixtures/demand_count/`, `docs/failure-modes.md` (one row for this
incident: incident, RCA, the COUNT test, rail).
Forbidden: every other path. `git diff --stat origin/main...HEAD` lists only
owned files.

## Laws

No `eprintln!` in `src/**`; `tracing` only. Comments state only what the code
cannot show. Descriptive names. No em dashes. Banned: provenance, substrate,
load-bearing, regime, "ground truth", "rel" in prose (say products and rows).
`timeout 10` on every `dl8` run; a timeout is the result, never rerun longer.
One cargo at a time, never override jobs. No subagents. No background
processes left running.

## Stop and report

Any oracle byte change you cannot remove: stop, paste the diff, report. Do not
refreeze.

```bash
boop beep --no-wait --as fix-eval-demand-bleed plan-eval-on-sqlite-ivm "step 1: PR #<n>, oracles <pass>/<total>, battery <pass>/<total>, demand calls <before> -> <after>, P1 <ms> release, red: <list or none>"
```
