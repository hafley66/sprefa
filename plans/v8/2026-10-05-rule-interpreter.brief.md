# Brief: rules as rows — an interpreter mode behind an env switch, default on

User (2026-10-05): fork the "run the compiled program" step into a config switch, default =
interpreter, so both can be tested.

## Context

- Compiler side is IVM already: module source enters as rows; macrotime and comptime settle row
  deltas on one installed program.
- The rules the compiler produces (comptime generated rules each round; the runtime program
  `dl8 run` / `query` / `build` evaluates) are installed as their own compiled engine program:
  a new rule set = new IR = reinstall (`_9b_ir_eval.rs` `IrSession::evaluate` fingerprint check;
  `_4_comptime/_2_rounds.rs`; `_9_runtime/_2_reconcile.rs` `run_live`).
- Interpreter mode: one fixed IR program, installed once, that evaluates any rule set given as
  rows. A rule change is then a row change, so module load / hot reload is IVM end to end.
- Compiled mode (today) stays; the extend/retire design (`design/engine-extend`) is its future.

## Do

1. Design section first in `plans/v8/2026-10-05-rule-interpreter.findings.md` (user planning
   order): type signatures; instance timelines (interpreter program, rule rows, binding rows,
   fact rows); storage and the sequence of reads/writes per settle; uniqueness.
   Shape to evaluate (prior art: Datalog meta-interpreters, Soufflé interpreter, egglog,
   "rules as data" in Materialize/DD discussions; library candidates before bespoke):
   - rule rows: `rule(R, head_rel, head_args)`, `goal(R, i, rel, args, polarity)`;
   - facts in one generic relation keyed by `(rel, args)` (comptime already uses `call(rel, args)`);
   - bindings walked goal by goal: `env(R, i, vars)` joined with facts on a key term built from
     the variables the goal shares with earlier goals (Mint a key list), negation as antijoin on
     the same key, aggregation and kernel ops (`int.add`, `str.cons`, ...) as their own goal kinds;
   - stratification: compute strata over rule rows; negation/aggregation across strata (or over
     `next` waves via the non-monotone LetRec now merged).
   State what the interpreter cannot express yet and fall back to compiled mode for those with a
   logged reason (no silent fallback).
2. Env switch `DL8_RULES=interpret|compile`, default `interpret`, read once in the binary config
   (not deep in eval). Applies to comptime generated rules and the runtime program.
3. Parity: both suites x both engines x both modes; registry `cmp`; macrotime oracle. Any output
   difference between modes is a bug in the interpreter.
4. Work counts (no wall clock): installs, creates, prepared, executions, vm_steps, rows written
   (sqlite work counter, `crates/ivm-sqlite/src/0_work.rs`) and dd arranged records/operators,
   interpret vs compile, registry and the 104-program census. Hot-reload case: one rule edit in a
   multi-module fixture, counts for the edit only in both modes.

## Laws

- Own worktree from sprefa main (current head); sqlite_ivm clone inside it from main `aa72853`,
  branch `feat/rule-interpreter`; build the extension dylib in the clone before suites.
- One other lane may run; `-j 4`; `--test-threads=1`. No caching. No wall-clock timing.
- No recursion in kernel Rust, no new kernel relation without stopping to report, no `json_*(`,
  no book edits, never `git commit -n`, no merge, no push.
- Commits end with
  `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_019jL2FmxduyQQ9kpmQYBKj7`.

## Receipt

```
status: done | blocked
sprefa branch + sha:
sqlite_ivm branch + sha + clone path:
what interprets / what falls back (with reasons):
parity: <suite x engine x mode result lines>
counts interpret vs compile: registry, census, hot-reload edit
next: <one action>
```
