# BRIEF: recon, the whole differential-dataflow line and both rust emitter targets

## Base
- Read-only recon on `91c5ea6e` (main). Verify with `git log --oneline -1`.
- You WRITE exactly two new plan files. You EDIT no source file.

## The user's question, verbatim (2026-08-11)
"no we want all rust rust(dd style) and rust sqlite(the production one), i want
to see if i can get close to dd with a compiler for its logic. so i want to know
where were and left off on the whole dd stuff. did we imrpvoe sqlite raw algos?
we have all the bench's for it known and understood and just hwot o run yea? and
where didw e leave off on the emitter for rustxrust and rustxsqlite vs tsv2
(ts+sqlite). i asked if we had labbed/explored with recursive in the 1.6sec cycle
dred/etc. and making sure we have dense/compact btree ops"

Read that as SIX questions. Answer each as its own section, in this order, each
with citations. Where the repo says nothing, write "no prior work found" and
move on; do not fill a gap with reasoning.

## The standing frame
The user wants THREE emitter targets alive, not two:

| target | what it is |
|---|---|
| rust x rust | differential-dataflow style, resident memory, the speed reference |
| rust x sqlite | the production one |
| tsv2 | ts + sqlite, what ships today |

The goal is stated plainly: "i want to see if i can get close to dd with a
compiler for its logic". So the question behind every section is what a
COMPILER would have to emit, not what a runtime would have to do.

## Q1: where did the DD line stop
The closeout is `plans/2026-08-10-dd-source-hunt.CLOSEOUT.md`, research commit
`7d2418b5b84bfa0dddf616fb85c71168ac8519fc`. Read it, plus `.RECON.md` and its
unga twin. Also read `plans/2026-08-10-dd-dance-recon.PLAN.md`,
`plans/2026-08-10-dd-payload-grain.PLAN.md`, and their unga twins.

The measured table to carry forward and re-verify against the docs:

| path | storage | retraction ms | ratio to DD |
|---|---|---:|---:|
| sqlite-count-scc | memory | 1705.019 | 9.86x |
| sqlite-dred-loop | memory | 1697.397 | 9.82x |
| differential dataflow | resident | 172.923 | 1.00x |

State, with citations: what landed on main, what stayed in a lab, what was
never started. The closeout names four ranked transfer forks; say which of the
four, if any, has code anywhere in the tree today.

Also: that closeout records `git commit -n` was used because the comment-budget
rail could not start in a worktree lacking `rxjs`. Note whether the commit that
bypassed the rail is on main and whether its content ever got rail-checked.

## Q2: did we improve the raw SQLite algorithms
The closeout's decomposition, to be confirmed or corrected against the code:
over-delete init and rounds 871.04 ms (51.8%), rederive base and rounds
807.70 ms (48.0%), remainder 3.58 ms. Logged SQL is 99.79% of wall time.
Moving the DB to memory buys 3.203% on count-scc and 6.120% on dred-loop.

Find the SQLite algorithm implementations themselves. `sqlite-count-scc` and
`sqlite-dred-loop` are named engines; locate them, cite file:line, and report
whether either has changed since the closeout was written. `git log` on those
paths answers it.

Say plainly whether any measured improvement to the raw algorithms landed after
2026-08-10, or whether the 1.7s number still stands unimproved.

## Q3: the benches, are they known and is it just "how to run"
`v6/labs/exec_shootout/` and `v6/labs/BENCHMARKS.md` are the starting points,
plus `v6/labs/exec_shootout/dl6/FACTS.dredland.md`. The `v6/justfile` declares
`bench`, `bench-cli`, `dl6-bench`, `dl6-bench-full`, `dl6-budget`,
`dl6-dred-bench`, `perf-all`, `perf-all-deep`. Read each recipe body.

Deliver ONE table: bench name, what it measures, the exact command to run it,
its expected receipt line, and its measured runtime if the justfile comment
states one. If a bench is stale or its baseline is old, say so with the date.
`dl6-budget` reads `dl6/budget.json` and ratchets DOWN only; report the current
ceilings.

The user's real question is "can I just run these". Answer yes or no per bench,
and name what is missing for any no.

## Q4: the emitter, rust x rust and rust x sqlite versus tsv2
Read `plans/2026-08-10-rust-emit-recon.PLAN.md` and its unga twin,
`plans/2026-08-06-rust-emitter-modes.md`, `plans/2026-08-06-dred-emit-lab-header.md`
and its unga twin, and `plans/2026-08-06-dred-shapes.d2`.

Deliver one table with a row per emitter target and columns: what exists in
code today (file:line), what is designed but unbuilt, what is undecided, and
the named blocker. `tsv2` is the live one, so its row is the reference the
other two are measured against.

Cross-check every claim against `v6/prolog/ARCH.pl`. Quote the ARCH row
comments, do not paraphrase them.

## Q5: recursive CTE inside the 1.6-second dred cycle
The user says they already asked this once. Find whether it was ever answered.
Search the plans, the labs, `ARCH.pl`, and `conformance/rulings.pl` for any
probe, lab, verdict, or decision about using SQLite's `WITH RECURSIVE` for the
fixpoint instead of the round-by-round loop.

Three outcomes are acceptable: it was measured (give the number), it was
decided against (give the decision and its reason), or no prior work found.
Nothing else. Do not run a new experiment; this is recon.

If no prior work is found, say so and state in ONE sentence what a probe would
have to measure, so the user can decide whether to dispatch it.

## Q6: dense / compact btree ops
Two mandatory reads first: `.claude/skills/sqlite-costs` and
`.claude/skills/sql-relational-design`. The costs skill carries this machine's
MEASURED constants and a list of already-disproven optimizations. Anything you
report here must agree with it or explicitly cite where it disagrees.

Then report what the repo already has: `WITHOUT ROWID`, dense dictionary ids,
surrogate integer keys, interning. `ARCH.pl` has a `storage-diet 4a` item
listed as dispatchable in CLAUDE.md's open items ("WITHOUT ROWID junctions,
dense dict ids"); find its row and quote it.

Deliver one table: technique, is it in use today (file:line), is it measured,
what it bought. The surrogate-keys law records TEXT keys as 1.7-2.0x slower on
identical tables; confirm that number's source.

## What you must NOT do
- Do not edit any source file.
- Do not run a new benchmark or write a probe. This is recon. The one exception
  is running a bench command purely to confirm it still executes, and if you do,
  report the runtime you observed and label it as a liveness check.
- Do not design the DD-style compiler. The user is deciding that themselves.
- Do not research external libraries.
- Do not spawn subagents.
- Do not report a limit you have not traced to a line of code.

## Deliverable
Two new files, nothing else:
1. `plans/2026-08-11-dd-line-recon.md` — six sections in the order above, one
   per question, citations everywhere, opens with a table of contents and a
   one-sentence answer per question up front.
2. `plans/2026-08-11-dd-line-recon.visual.human.unga.md` — plain words,
   diagrams, ZERO citations, for a reader with no context. A plan without this
   second doc is undelivered.

## Style laws, inline so you need no judgment
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime`.
- "refusal" is banned in prose; unbuilt work is "TODO" or "not built yet".
- Tables, lists, and mermaid over prose. Prose is a caption under a diagram.
- Numbers come from tool output or a cited doc. No vague quantity claims.
- Construct names use rxjs, prolog, or SQL words only. "support" is banned.
- Never announce location in text ("here is", "below is", "the following").
  Point with file:line or a node name.
