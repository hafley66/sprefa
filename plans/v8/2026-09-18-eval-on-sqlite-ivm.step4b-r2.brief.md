# eval-on-sqlite-ivm step 4b-r2: three hangs return, every recursion capped

Branch `feat/eval-sqlite-step4b-r2` from `720bdde96`. That commit is the head
of draft PR #801, branch `feat/eval-sqlite-step4b`, not `origin/main`.

- PR: push to your own branch and open a PR with base `feat/eval-sqlite-step4b`.
- Coordinator: `sprefa-coordinator`.
- Wall clock: 20 minutes.
- Scope: the four items below and nothing else.

## Defect

Run `DL8_ENGINE=sqlite RUST_LOG=dl8=info timeout 10 dl8 eval <program>` on
`oracle/eval/15_aggregate_tables_only`, `4_cons_lists` or `9_edge_ref`. None
of the three returns. The log's last line is a `dl8::sql` line for
`insert_seeds rows=3`. Either cause breaks the 10-second law and the
bounded-loop law. There are two candidate causes, and you must tell them
apart before you fix anything.

| candidate | receipt in `720bdde96` |
|---|---|
| A. arena mutex re-entry: a `std::sync::Mutex` guard is held across a `sql()` call whose statement runs a `dl_*` function, and that function locks the same mutex | `src/_6_eval/_7_sqlite_eval.rs:96` takes `guard` in `declare` and holds it through `declare_view` at `:190`; `:216` takes it in `read` and holds it through `read_view` at `:224`; `src/_6_eval/_8_functions.rs:54`, `:94`, `:106` lock the arena inside each function |
| B. a `WITH RECURSIVE` with no bound: a constructor function in a recursive step makes a new term each round | the recursive CTE text the lowering emits for these three programs |

## Step 0: tell A from B, in one command per fixture

Extract the program JSON first, the way `tests/_0_eval_oracle.rs:30-37`
does. Then:

```bash
DL8_ENGINE=sqlite RUST_LOG=dl8=info timeout 10 target/debug/dl8 eval /tmp/4_cons_lists.json 2> /tmp/r2-4.log & PID=$!; sleep 2; ps -o pid,pcpu -p $PID; sample $PID 1 2>/dev/null | grep -E 'pthread_mutex|__psynch_mutexwait|sqlite3VdbeExec|dl_' | head -8; wait $PID; tail -5 /tmp/r2-4.log
```

- **A:** about 0 % CPU, and the stack shows `__psynch_mutexwait`.
- **B:** about 100 % CPU, and the stack shows `sqlite3VdbeExec`.

Paste the result in the PR body. Send the coordinator one line naming A or B
for each fixture. A process left alive by `timeout` is killed and gets a row
in `docs/failure-modes.md`.

## Items

1. Fix the cause step 0 found.
   - For A: no arena guard is ever held across a `sql()` call. Take what you
     need from the arena, drop the guard, run the SQL, then lock again.
   - For B: item 2.
2. Every `WITH RECURSIVE` the lowering emits carries a `depth INTEGER` column.
   The anchor starts at 0 and the step adds `depth + 1` under
   `WHERE depth < RECURSION_DEPTH_LIMIT`.
   - `RECURSION_DEPTH_LIMIT` is a `const` in `src/_5_reify/_7_sqlite.rs`, and
     its comment names what it protects: a recursive step that constructs a
     new term every round never reaches a fixpoint.
   - At the cap, the view emits one output row with product tag
     `recursion_depth_exceeded`, and `read` turns it into a diagnostic that
     names the CTE.
   - The CTE's consumers project `depth` away with `SELECT DISTINCT`, so the
     cap never changes a closure below it.
   - `dl8 emit sqlite` output must still pass `tests/_18_sqlite_emit.rs`. If
     the depth column changes that output, gate it to the eval lowering
     (`KernelReach::Eval`), and send a field decision.
3. The three fixtures return in under 1 s each. Paste the last 5 log lines of
   each into the PR body.
4. `tests/_27_eval_sqlite.rs` finishes in under 10 s, with the wall time
   pasted. Raise `FLOOR` (`:9`) to the measured pass count. It never falls.
   Paste the failing fixture list.

## Rails

- Commit and push by event. Do it after step 0, after each fixture that stops
  hanging, and after each fixture that turns green:
  `git add <owned paths> && git commit -m "wip: <what>" && git push -u origin HEAD`.
  Open the PR (`gh pr create --draft --base feat/eval-sqlite-step4b`) at the
  first push. A message sent to you mid-turn never reaches you, and the last
  two lanes died holding unpushed trees.
- Field decisions go to the coordinator directly, one line, in the same turn,
  before you build on them:
  `boop beep --no-wait --as feat-eval-sqlite-step4b-r2 sprefa-coordinator "field decision: <what> | chose <X> | because <clause> | site <path:line>"`.
- Every `dl8` run is `RUST_LOG=dl8=info timeout 10 ... 2> <log>`. A timeout is
  the result. Read the last lines of the log. Never rerun longer.
- Every `cargo test` is `timeout 600 ... > <log> 2>&1`. One cargo at a time.
  Never override jobs.

## First action

```bash
date +%s
ln -s ~/projects/sqlite_ivm sqlite_ivm; ln -s ~/projects/hafley-rs hafley-rs
ps -eo pid,pcpu,rss,args | grep -E '[d]l8 '
timeout 600 cargo build > /tmp/r2-build.log 2>&1; tail -3 /tmp/r2-build.log
```

## Validation

```bash
timeout 600 cargo test --test _0_eval_oracle --test _18_sqlite_emit --test _26_bounded_loops --test _27_eval_sqlite > /tmp/r2-targeted.log 2>&1; grep -E '^test result|sqlite oracles|FAILED|finished in' /tmp/r2-targeted.log
git diff --stat 720bdde96...HEAD
ps -eo pid,pcpu,rss,args | grep -E '[d]l8 '
```

## Ownership

Owned: `src/_6_eval/_7_sqlite_eval.rs`, `src/_6_eval/_8_functions.rs`,
`src/_5_reify/_7_sqlite.rs`, `tests/_27_eval_sqlite.rs`, `docs/failure-modes.md`.

Forbidden: everything else, including:

- `src/_6_eval/_5_evaluate.rs`, `src/_9_runtime/`, `src/bin/`
- `oracle/`, `prelude/`, `macrotime/`, `std/`, `book/`
- `Cargo.toml`, `AGENTS.md`, `CLAUDE.md`, `sqlite_ivm/`

Never run `cargo fmt` on the crate. Run `rustfmt --edition 2021 <owned file>`
only.

## Laws, inline

- Every loop and every recursion is bounded. A `loop {}`, `while`, or
  recursive fn in `src/**` carries an explicit budget and stops with a named
  diagnostic at the cap. The bound is a constant with a comment saying what it
  protects. `tests/_26_bounded_loops.rs` enforces this. Name methods apart
  from the free functions they call, because the scanner reads a shared name
  as self-recursion.
- The 10-second law. Any single operation over 10 s is a defect to fix now.
  Nothing may seize the machine.
- No join code in Rust. Every statement runs inside `sql()`. N+1 never.
- No `eprintln!` in `src/**`. Comments state only what the code cannot show.
  Use descriptive names.
- No em dashes. Banned: provenance, substrate, load-bearing, regime, "ground
  truth", refusal, "rel" in prose.
- An unbuilt construct is `not_built_yet`, with the throw site cited.
- No subagents. No background process left running.

## Report

```bash
boop beep --no-wait --as feat-eval-sqlite-step4b-r2 sprefa-coordinator "step 4b-r2: PR #<n>, cause <A|B> per fixture, hangs 3 -> <n>, _27 <s> s, sqlite oracles <pass>/<total> (FLOOR <n>), red: <list or none>"
```
