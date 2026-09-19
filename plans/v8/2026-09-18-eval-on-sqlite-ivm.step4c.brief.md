# eval-on-sqlite-ivm step 4c: the four kernel-mode fixtures

Branch `feat/eval-sqlite-step4c` from `origin/main` `932ee1413`. One PR to
`main`. Coordinator: `sprefa-coordinator`. Wall clock: 20 minutes.

## Scope

Four fixtures, and nothing else. Each must pass under `DL8_ENGINE=sqlite`:

| fixture | kernels it exercises |
|---|---|
| `oracle/eval/4_cons_lists.json` | `nil`, `cons` |
| `oracle/eval/9_edge_ref.json` | `edge_ref` |
| `oracle/eval/16_intern_row_reuse.json` | `intern`, plus the `intern` output rows |
| `oracle/eval/12_repeated_vars_and_arity.json` | repeated variables, several arities of one product |

The spec is design section 2, `plans/v8/2026-09-18-eval-on-sqlite-ivm.design.md`
(the rows for `nil`, `cons`, `edge_ref` and `intern`). Some of this lowering
already exists:

| site | what it does |
|---|---|
| `src/_5_reify/_7_sqlite.rs:745-755` | admits the kernels under `KernelReach::Eval` |
| `src/_5_reify/_7_sqlite.rs` near `:983`, `:998`, `:1040` | lowers them to `dl_cons`, `dl_head`, `dl_tail`, `dl_edge_ref`, `dl_application` |
| `src/_6_eval/_8_functions.rs:62-110` | registers those functions |

Find why each fixture still differs and fix exactly that. Do not rewrite
working code.

Out of scope:

- the fold fixtures (`3_count`, `15_aggregate_tables_only`, `6_*`, `7_*`);
- demand (`14_demand_recursion`);
- every `c*` compiler fixture;
- `std/store.dl7`.

## First action

```bash
date +%s
ln -s ~/projects/sqlite_ivm sqlite_ivm; ln -s ~/projects/hafley-rs hafley-rs
ps -eo pid,pcpu,rss,args | grep -E '[d]l8 '
timeout 600 cargo build > /tmp/4c-build.log 2>&1; tail -3 /tmp/4c-build.log
```

For each of the four fixtures, extract `program` from the JSON into
`/tmp/4c-<name>.json` (as `tests/_0_eval_oracle.rs:30-37` does), then run:

```bash
DL8_ENGINE=sqlite RUST_LOG=dl8=info timeout 10 target/debug/dl8 eval /tmp/4c-<name>.json > /tmp/4c-<name>.out 2> /tmp/4c-<name>.log; echo rc=$?; tail -5 /tmp/4c-<name>.log
```

Diff `closure` in the `.out` file against the fixture's `expected.closure`.
Write one line per fixture naming the difference (missing rows, extra rows, a
`not_built_yet` payload, or a timeout) before you change any code.

## Rails

- After EVERY fixture that turns green:
  - raise `FLOOR` in `tests/_27_eval_sqlite.rs:9` to the new pass count;
  - `git add <owned paths> && git commit -m "wip: <fixture> green" && git push -u origin HEAD`.

  Open the PR (`gh pr create --draft`) at the first push. A message sent to
  you mid-turn never reaches you, and the last three lanes died holding
  unpushed work.
- Field decisions go to the coordinator directly, one line each, in the same
  turn, before you build on them:
  `boop beep --no-wait --as feat-eval-sqlite-step4c sprefa-coordinator "field decision: <what> | chose <X> | because <clause> | site <path:line>"`.
  A field decision is a mode not in design section 2, a shape sqlite_ivm
  rejects, a schema choice, or a name the design does not give.
- You implement. You do not redesign.
- Every `dl8` run is `RUST_LOG=dl8=info timeout 10 ... 2> <log>`. A timeout is
  the result. Read the last 5 lines and cite them. Never rerun longer.
- Every `cargo test` is `timeout 600 ... > <log> 2>&1`, then read the log. One
  cargo at a time. Never override jobs.
- At 17 minutes, stop coding. Update the PR body (green fixtures, still-red
  fixtures with their one-line cause, receipts), push, and report.

## Validation

```bash
timeout 600 cargo test --test _0_eval_oracle --test _18_sqlite_emit --test _26_bounded_loops --test _27_eval_sqlite > /tmp/4c-targeted.log 2>&1; grep -E '^test result|sqlite oracles|FAILED|finished in' /tmp/4c-targeted.log
git diff --stat origin/main...HEAD
ps -eo pid,pcpu,rss,args | grep -E '[d]l8 '
```

- `_0_eval_oracle` stays green (the Rust path).
- `_18_sqlite_emit` stays green (emit output unchanged).
- `_27` finishes in under 10 s and prints the failing fixture list.

## Ownership

Owned: `src/_5_reify/_7_sqlite.rs`, `src/_6_eval/_7_sqlite_eval.rs`,
`src/_6_eval/_8_functions.rs`, `tests/_27_eval_sqlite.rs`,
`docs/failure-modes.md`.

Forbidden: everything else, including:

- `src/_6_eval/_4_kernel.rs`, `src/_6_eval/_5_evaluate.rs`
- `src/_9_runtime/`, `src/bin/`
- `oracle/`, `prelude/`, `macrotime/`, `std/`, `book/`
- `Cargo.toml`, `AGENTS.md`, `CLAUDE.md`, `sqlite_ivm/`

Never run `cargo fmt` on the crate. Run `rustfmt --edition 2021 <owned file>`
only.

## Laws, inline

- Every loop and every recursion is bounded. A `loop {}`, `while`, or
  recursive fn in `src/**` carries an explicit budget and stops with a named
  diagnostic at the cap. The bound is a constant with a comment saying what it
  protects. `tests/_26_bounded_loops.rs` enforces this. Name methods apart
  from the free functions they call.
- Never hold the arena `Mutex` guard across a `sql()` call. The `dl_*`
  functions lock the same arena, so holding it self-deadlocks
  (`docs/failure-modes.md` row 115).
- No join code in Rust. Every statement runs inside `sql()`. N+1 never: one
  multi-row `INSERT` per chunk.
- The 10-second law: any single operation over 10 s is a defect. Nothing may
  seize the machine.
- No `eprintln!` in `src/**`. Comments state only what the code cannot show.
  Use descriptive names.
- No em dashes. Banned: provenance, substrate, load-bearing, regime, "ground
  truth", refusal, "rel" in prose.
- An unbuilt construct is `not_built_yet`, with the throw site cited.
- No subagents. No background process left running.

## Report

```bash
boop beep --no-wait --as feat-eval-sqlite-step4c sprefa-coordinator "step 4c: PR #<n>, green <list>/4, sqlite oracles <pass>/<total> (FLOOR <n>), _27 <s> s, red in scope: <fixture: cause>, battery not run"
```
