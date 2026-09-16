# Lane brief: study SQL pipe syntax, prolog vs datalog vs SQL evaluation order, rxjs-like state over time, and the existing projects that already do this

## First action

```bash
git merge --ff-only 13c98eced31baec16aad1d62d7b8572b6217d5f4
```

If that fails, stop and report with the error text.

## Why this study exists

Chris, verbatim (issues/pipe-sql-harness/item.md on main tree; copy is below):

> i want pipeline stuff too so i can do dumbshit sql queryies over time, like my sqlite_ivm, just monomorphize to another rust app. sql pipeline experiment from bigquery team is fucking hype love that shit so haromny towards that is rad buti have to learn the point free monadic scope rules/state ruels. also do dumb things like time since window function of boop db that has all my agent convos, make rules that saying that the agent is allowed 10k tokens before cancel for next user message etc. like make my own langauge that with bboop and ivm and rxjs like bindings (of time flow logic) we can make a programmable harness.

> if we can study sql pipeline syntax and how prolog vs datalog (thus sql) handle eval order and hwo that can be intertwined with rxjs lke state logic. and also there HAS to be an apache project liek this fucking lol

Read `AGENTS.md` on your branch, section `## Docs` and the `<human-notes>` block. Those laws govern the human doc you write. If AGENTS.md on your branch lacks that block, it is in the coordinator's uncommitted tree; the laws are restated here: no code citations in the human doc, no second person, no commas, intuition first, define a word before using it, diagrams beside lists never instead of them.

## Deliverables, two files, both required

1. `plans/v8/2026-09-15-pipe-sql-eval-order.study.md`: the auditor's doc. Every claim carries a URL or a path:line. Tables over prose.
2. `plans/v8/2026-09-15-pipe-sql-eval-order.study.visual.human.unga.md`: the human doc. Zero citations. Opens with a TOC. Mermaid diagrams next to lists. A worked example with real values before every rule. Written for a dev who has never read a datalog paper.

## Questions the study answers, in this order

### 1. SQL pipe syntax
- The BigQuery pipe syntax paper (Google, 2024, "SQL Has Problems. We Can Fix Them: Pipe Syntax In SQL") and the ZetaSQL grammar: every pipe operator, its input shape, its output shape.
- Which of those operators are order-sensitive and which commute. A table.
- Who else ships it: ZetaSQL, Databricks SQL pipe syntax, DuckDB FROM-first, PRQL, Malloy, KQL (Kusto). One row each: operator set, how it handles window functions, how it handles recursion.

### 2. Evaluation order: prolog vs datalog vs SQL
- Prolog: SLD resolution, top-down, left-to-right, cut, order matters, may not terminate. One worked query traced step by step.
- Datalog: bottom-up naive then semi-naive fixpoint, stratified negation, order of rules does not matter, always terminates. Same query traced.
- SQL: set semantics, optimizer reorders joins, WITH RECURSIVE is a fixpoint with an iteration order the standard leaves to the engine. Same query.
- Tabling (SLG, XSB, SWI `:- table`) as the bridge between the first two.
- Where dl8 sits today: read `v8/src/_6_eval/_5_evaluate.rs` and `v8/src/_3_check` for strata and rounds; cite the lines in the auditor doc. Read `sqlite_ivm/` README and its main source for how deltas propagate.

### 3. Time and state, rxjs shape
- What rxjs operators mean in relational terms: map is projection, filter is selection, mergeMap is join, scan is a fold with state, window and buffer are windows, switchMap is retraction of the previous inner stream, debounceTime and throttleTime are time-keyed filters. A table with one dl8-ish spelling per row if one exists in `v8/fixtures` or `v8/oracle/compile/sources/test/fixtures` (check; if none, say "no fixture").
- Point-free and monadic scope rules: what "point-free" means for a pipe, what state a stage may see (only its input vs the whole pipe scope), how Haskell, OCaml and rxjs each answer it. This is the part Chris says he has to learn; write it as a step trace with real values.
- Retraction: what happens to a stage's state when an input row is deleted. DBSP/Feldera z-sets, differential dataflow, Materialize, and `sqlite_ivm` deletes. Which of these dl8's append-only tables (`v8/src/_6_eval/_3_table.rs:1-3`) cannot express today.
- The concrete example Chris named: over the boop db (`~/.agent/boop.db`, plain SQLite; `boop db "<sql>"` runs a query; run `boop db ".tables"` and `boop db ".schema <table>"` for the conversation and turn tables) write the SQL for "time since the last user message per session" as a window function, then the rule "an agent may spend at most 10k tokens before the next user message or it is cancelled" in SQL, then sketch how the same thing reads as a pipe in the syntax from part 1. Run the SQL against the real db read-only and paste the output shape (columns and row count, no message bodies).

### 4. Existing projects
Build-vs-buy law: a candidate-by-candidate table, no one-line dismissals. Rows at least: Apache Flink SQL, Apache Beam, Apache Calcite (pipe-syntax planner and relational algebra), Apache Spark Structured Streaming, Apache Kafka Streams / ksqlDB, Apache Wayang, Materialize, Feldera (DBSP), RisingWave, Timely and differential dataflow, Noria, DuckDB, Datafusion/Ballista, SQLite itself with sqlite_ivm. Columns: what it is, eval model (batch, streaming, incremental), retraction support, embeddable in a Rust binary, monomorphize-to-app story (does it emit a program or run a server), license, why it does or does not fit "sqlite_ivm monomorphized into another rust app".

### 5. Forks for Chris
End the auditor doc with a table of decisions only Chris can make, each row: the fork, the two or three options, what each costs, which existing project answers it. Do not decide. Language design happens with Chris in the room.

## Laws
- Every claim in the auditor doc: URL or path:line. Where two sources disagree, both, one line each.
- Do not edit any file outside `plans/v8/`. Do not write code. Do not spawn subagents.
- Read-only against `~/.agent/boop.db`. No INSERT, UPDATE, DELETE, no schema changes.
- Banned words in prose and identifiers: provenance, substrate, load-bearing, regime, ground truth, refusal, honest, distill. No em dashes.
- No number in a sentence in the human doc; numbers live in tables and traces.

## Commit and PR

One commit, subject exactly:

```
plan(v8): pipe syntax, eval order, rxjs state over time, and who already ships it
```

Then push and open a PR against main with `gh pr create`, body = the human doc's TOC plus the forks table. End the PR body with the line `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.

## Done

```bash
boop beep --no-wait --as plan-pipe-sql-eval-order sprefa-coordinator "done: PR $(gh pr view --json number -q .number), study + human doc, forks table has $(grep -c '^|' plans/v8/2026-09-15-pipe-sql-eval-order.study.md) table rows"
```
