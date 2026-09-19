# eval-on-sqlite-ivm step 4b: finish S1, then S2

Branch `feat/eval-sqlite-step4b` from `origin/main` `8def98650`. One PR.

- Coordinator: `sprefa-coordinator`.
- Spec: `plans/v8/2026-09-18-eval-on-sqlite-ivm.design.md` (on main). Read
  sections 2, 3 and 5 whole before writing code.
- Step 4 landed an S1 partial as PR #799. Its body lists what landed and what
  is missing: `gh pr view 799`.

## You implement. You do not redesign.

Some decisions the design does not answer:

- a kernel mode not in design section 2;
- a SQL shape sqlite_ivm rejects;
- a schema choice;
- a name or file the design does not name.

For each one, send one line to the COORDINATOR in the same turn, then keep
going with your choice:

```bash
boop beep --no-wait --as feat-eval-sqlite-step4b sprefa-coordinator "field decision: <what> | chose <X> | because <one clause> | site <path:line>"
```

An unreported choice fails grading.

## Rails (new; they bind harder than anything below)

1. Push by your own clock. Note `date +%s` at your first action. At 20
   minutes, and after EVERY slice item that turns a fixture green, run
   `git add -A <owned paths> && git commit -m "wip: <what>" && git push -u
   origin HEAD`, with no exception. A message sent to you mid-turn never
   reaches you. The last lane died holding an unpushed tree for 30 minutes.
2. Open the PR at your first push (`gh pr create --draft`), then keep pushing
   to it. At 25 minutes, stop coding, update the PR body (landed, missing,
   receipts), push, and report.
3. `FLOOR` in `tests/_27_eval_sqlite.rs:9` only rises. Set it to your measured
   pass count on every push, and paste the failing fixture list into the PR
   body.
4. Wall clock 30 minutes. Turn cap 150; push at 140.

## First action

```bash
date +%s
ln -s ~/projects/sqlite_ivm sqlite_ivm; ln -s ~/projects/hafley-rs hafley-rs
ps -eo pid,pcpu,rss,args | grep -E '[d]l8 '
timeout 600 cargo test --test _27_eval_sqlite > /tmp/s4b-27-before.log 2>&1; grep -E 'sqlite oracles|FAIL|fixture' /tmp/s4b-27-before.log | head -40
```

That prints the 28 failing fixtures and each one's `not_built_yet(<shape>)`
payload. Group them by shape, then work the largest group first.

## Scope

In scope:

- **S1 remainder:** every `oracle/eval` fixture whose only blocker is a
  positive, non-recursive shape. Examples are a constant cell, repeated
  variables, a multi-arity product, or an empty program.
- **S2:**
  - Linear `WITH RECURSIVE` per SCC.
  - A `product` discriminator column for an SCC of several products, with
    columns padded with the `TermId` of `none` (design section 3).
  - Negation as a correlated `NOT EXISTS` over an earlier stratum's CTE.
  - Every mode row of design section 2 except the three fold rows.
  - F4a: nonlinear recursion gives a `not_built_yet(nonlinear_recursion)`
    diagnostic, plus one `tracing::info!(target: "dl8::eval", nonlinear_sites
    = N)` event per `declare`.

Out of scope, reserved for step 4c:

- folds (`count_step`, `min_step`, `max_step`, program-step);
- `effect` rows for served goals;
- the F5a demand CTE;
- `std/store.dl7` and `dl8 emit store`.

A fixture blocked only by these stays red, and the PR body names it that way.

Code lives in `src/_6_eval/_7_sqlite_eval.rs` (`declare` at `:91`,
`not_built_yet` payload at `:141` and `:498`), `src/_6_eval/_8_functions.rs`
(`register` at `:62`), and the shared lowering in `src/_5_reify/_7_sqlite.rs`
(`program_plan_with`, `not_built_yet`). `dl8 emit sqlite` output must stay
byte-identical, and `tests/_18_sqlite_emit.rs` must pass.

## Validation, exact

```bash
timeout 600 cargo test --test _0_eval_oracle --test _2_macrotime_oracle --test _8_compile_oracle --test _18_sqlite_emit --test _26_bounded_loops --test _27_eval_sqlite > /tmp/s4b-targeted.log 2>&1; grep -E '^test result|sqlite oracles|FAILED' /tmp/s4b-targeted.log
timeout 600 cargo test --no-fail-fast > /tmp/s4b-battery.log 2>&1; grep -E '^test result: FAILED|FAILED' /tmp/s4b-battery.log
git diff --stat origin/main...HEAD -- oracle/
git diff --stat origin/main...HEAD
```

- The battery runs once, at the end. Compare its reds to
  `.github/CI-KNOWN-RED.md` "dl8 cargo battery": `_16_extract_tsi`,
  `_20_hosts` (one leg), `_22_book::probes`. Any other red is yours.
- The `oracle/` diff is empty.
- The full diff lists only owned files (plus `Cargo.lock`).

## Ownership

Owned: `src/_6_eval/_7_sqlite_eval.rs`, `src/_6_eval/_8_functions.rs`,
`src/_5_reify/_7_sqlite.rs` (emit output unchanged), `tests/_27_eval_sqlite.rs`,
`tests/_26_bounded_loops.rs` (`UNBUDGETED` only shrinks), `docs/failure-modes.md`.

Forbidden: every other path, named explicitly below. A needed sqlite_ivm
change is a stop-and-report that carries the exact SQL it rejected.

- `src/_6_eval/_5_evaluate.rs` (the Rust path stays byte-identical)
- `src/_0_read/`, `src/_1_macrotime/`, `src/_2_lower/`, `src/_3_check/`, `src/_4_comptime/`, `src/_9_runtime/`, `src/bin/`
- `prelude/`, `macrotime/`, `std/`, `oracle/`, `book/`
- `AGENTS.md`, `CLAUDE.md`, `sqlite_ivm/`, `Cargo.toml`

Never run `cargo fmt` on the crate. Run `rustfmt --edition 2021 <owned file>`
only.

## Laws, inline

- Every loop and every recursion is bounded. A `loop {}`, `while`, or
  recursive fn in `src/**` carries an explicit budget (`for _ in 0..LIMIT`, a
  depth counter, a row-set repeat check) and stops with a named diagnostic
  when the budget is hit. The bound is a constant with a comment saying what
  it protects. `tests/_26_bounded_loops.rs` enforces this.
  - The scanner reads a call to a free function that shares a method's name
    as self-recursion (`SqliteEvaluate::open` had to become `connect`). Name
    methods apart from the free functions they call.
- No join code in Rust. A join, a filter over rows, or a fixpoint is SQL. Rust
  moves rows in and out and implements the `dl_*` scalar functions only.
- N+1 never. Collect the set, then run one multi-row `INSERT` per chunk.
  Every statement runs inside `sql()` (`src/_9_runtime/_1_sqlite.rs`).
- Read `.claude/skills/sql-relational-design/SKILL.md` and
  `.claude/skills/sqlite-costs/SKILL.md` before any DDL. Use surrogate INTEGER
  keys. Natural TEXT keys live once, in a dictionary table.
- Every `dl8` run is `RUST_LOG=dl8=info timeout 10 <dl8> ... 2> <log>`. A
  timeout IS the result. Read the last 20 lines of the log and cite them.
  Never rerun longer, never bisect in a loop.
- Every `cargo test` is `timeout 600 ... > <log> 2>&1`, then read the log. One
  cargo at a time. Never override jobs.
- No `eprintln!` in `src/**`. Use `tracing` only.
- Comments state only what the code cannot show. No dates, arc references, or
  change-log narrative.
- Use descriptive names. Interfaces carry the `I` prefix. Say products and
  rows, never "rel" in prose.
- No em dashes. Banned in prose and identifiers: provenance, substrate,
  load-bearing, regime, "ground truth" (say oracle), refusal.
- An unbuilt construct is `not_built_yet`, with the throw site cited. It is
  never described as a language limit.
- No subagents. No background processes left running. Nothing may seize the
  machine.

## Report

To the coordinator, at the end or at 25 minutes, whichever comes first:

```bash
boop beep --no-wait --as feat-eval-sqlite-step4b sprefa-coordinator "step 4b: PR #<n>, sqlite oracles <before> -> <after>/<total> (FLOOR <n>), rust oracles <pass>/<total>, battery <pass>/<total>, nonlinear_sites <n>, red: <list or none>, missing: <list>"
```
