# extract eval plan: anti-overfit testing + next oracle classes (2026-08-31)

User word: "i clearly want more testing i really am worried we are overfitting
and not planning ahead" (2026-08-31, following the "src/dst of what" fork).

## TOC

1. The overfitting surface today
2. Arc A — held-out ratchet (train/held-out corpus split)
3. Arc B — trace oracle (run-based, python first)
4. Arc C — protocol forks (resolution-origin column, 3-bucket precision)
5. Arc D — ambiguity mutation battery
6. Arc E — flow closure over `flow_edge`
7. Rails and metrics
8. Sequencing and lane shapes
9. Forks needing the user

## 1. The overfitting surface today

Every accuracy number the lanes optimize is measured on ONE artifact per
language, and lane briefs hand the misses of that artifact to the lane:

| lang | tuning artifact | size | miss list the lanes grind |
|---|---|---|---|
| rust | ~/projects/rust-analyzer | 873 files (bench `enumerate`) | rust.REPORT.md census classes |
| go | ~/projects/typescript-go | 5,097 files | go.GAPS.md residual-six |
| ts | ~/projects/TypeScript-5.9 | 600 files | ts5 MISSES |
| python | PyCG micro suite | 137 cases, 236 edges (`python-oracle/SCORES.tsv`) | MISSES.tsv per class |

That is train-set optimization by construction. Two existing counterweights,
both partial:

- PR #606 re-measured the rust arms on 5 unseen repos (rust.REPORT.md sec 27,
  checker recall 91.04-99.39 at file-pair grain) — one-off, one language, not
  ratcheted.
- `corpus-stats/STATS.tsv` (PR #612, #614) covers 14 repos across 4 languages
  but carries volume columns only (rows, wall, rss), zero accuracy: no oracle
  exists on those repos.

The precision side has a structural guard (unique-candidate legs refuse on
ambiguity), so the overfit risk concentrates in RECALL legs tuned to shapes
that happen to appear in the tuning artifact.

## 2. Arc A — held-out ratchet

Give the 14 corpus-stats repos oracles, split them train/held-out, and track
the recall gap.

- Oracle generation, cheapest tool per language, on the pinned shas already in
  `corpus-stats/REPOS.tsv`:
  - go: `golang.org/x/tools/cmd/callgraph` vta (same tool as
    `go.oracle.call.vta.bare.tsv`), normalized by
    `plans/extract-bench-2026-08-29/normalize.py`.
  - ts/js: scip-typescript index -> call rows (the scip join already exists in
    the bench code).
  - rust: rust-analyzer scip (known coarse: the committed scip oracle row sits
    at 41.15 precision, so rust held-out rows are 3-bucket only, see Arc C).
  - python: PyCG the tool, run per repo (Apache-2.0, already vendored for the
    micro suite).
- Split: the 4 current tuning artifacts stay TRAIN (lanes keep their miss
  lists). All 14 corpus-stats repos are HELD-OUT: never named in a lane brief,
  never a miss list, measured by the coordinator at grade time only.
- Metric: `overfit_gap(lang) = recall_train - median(recall_heldout)` in pt,
  one row per language per oracle kind, appended to a new
  `RATCHET-HELDOUT.tsv` with the same 4-col protocol and the sha column
  `corpus-stats/run.py` already records.
- Gate mode: report-only first (the gap prints at every grade), hard-gate
  (a train bump that widens the gap past tolerance fails) after enough data
  exists to set the tolerance. Fork 9.1.

## 3. Arc B — trace oracle (run-based, python first)

Status 2026-09-03 (lane `grind-trace-oracle`): python pilot LANDED at
`plans/extract-bench-2026-08-29/python-oracle/trace/` (`run.py`, `TRACE.tsv`,
`RUNS.tsv`, `SCORES.tsv`, `REPORT.md`) and `pycg_score.py --oracle trace`;
105 of 119 mains ran, 220 executed edges all agreeing with PyCG,
recall-of-covered 87.27 vs static 86.86, 3-bucket 192/2/11. The pytest
extension and go/rust remain open.

Every oracle so far is a static tool; static-vs-static agreement cannot score
the shapes both sides miss. A run IS the oracle for exactly the classes with
written stops (OPEN-PROBLEMS row 2: dynamic 0%, builtins 25.00%, lambdas
35.71%, dicts 36.84% category recall in `python-oracle/SCORES.tsv`).

- Pilot: each PyCG case has a `main.py` entry
  (`python-oracle/suite/<case>/main.py`). Run it under `sys.monitoring`
  (3.12+) recording `(caller_file, caller_qualname, callee_file,
  callee_qualname)` per CALL event, filter to suite-local files, normalize to
  the 4-col row. Score ours against the trace exactly as against PyCG.
- The trace is per-run complete and per-program partial (only executed paths),
  so trace rows score RECALL of covered edges and never judge precision of
  uncovered ones — a natural fit for the 3-bucket protocol (Arc C).
- Extension after the pilot: pytest under the same tracer on the python
  corpus-stats repos (flask, click, requests) gives held-out trace oracles on
  real code, same normalization.
- go/rust trace oracles are a later fork (runtime tracing there is
  heavier); python proves the class.

## 4. Arc C — protocol forks (already queued, now funded)

- Resolution-origin column: every emitted edge carries which leg answered
  (`same_file | corpus_unique | module_plane | checker | alias_chain |
  param | decorator | subscript`). Two uses: (a) per-origin counts in the
  ratchet make a leg that suddenly answers 10x more visible before precision
  moves; (b) the overfit gap decomposes per leg, pointing at the fragile one.
- 3-bucket precision: ours-only rows split matched / contradicted (oracle has
  a different dst for the same src) / unjudged (oracle silent). Required for
  rust scip held-out rows (41.15 flat precision is mostly unjudged) and for
  trace oracles (uncovered paths are unjudged by definition).

## 5. Arc D — ambiguity mutation battery

Property tests on the resolver invariants, independent of any oracle:

- Duplicate-def injection: copy a def named `N` into a fresh file of the
  fixture; every `corpus_unique`-origin edge to `N` must flip to ABSENT.
  Flipping to a wrong dst is the failure the unique-candidate rule exists to
  prevent; this tests the rule instead of trusting it.
- Def relocation: move a def to another file (`extract move` where wired);
  edges must follow the def, path columns only.
- Shadow injection: introduce a same-named parameter or local above a call
  site; the edge must drop or re-point per the shadowing rule
  (`_0_source.rs` `shadowed`), never survive pointing at the module-level def.
- Mechanics: a small generator over the existing fixture dirs
  (`tests/fixtures/py_findings/`, `go_type_refs/`, `impl_owner/`), one test
  binary `tests/90_mutation_battery.rs`, resolve before/after in-process,
  assert the per-origin deltas. The rust checker arm is exempt (a compiler
  answer legitimately survives injected ambiguity).

## 6. Arc E — flow closure over `flow_edge`

`extract --resolve --family flow` emits `flow_edge` rows
(`src/bin/extract.rs:505`, PR #330); no closure program consumes them
(`v6/dl/dataflow/report_extract.dl6` is intra-procedural). Arc: a dl6 closure
program (Rust door only, per user 2026-08-21) + oracle rows from CodeQL's
dataflow library on the go tuning corpus. This is the first consumer of the
leaf relations and the first non-compiler-shaped oracle target.

## 7. Rails and metrics

| rail | where | asserts |
|---|---|---|
| RATCHET-HELDOUT.tsv | plans/extract-bench-2026-08-29/ | held-out recall per lang/oracle; report-only gap print at grade |
| per-origin counts | ratchet output table | origin distribution shifts past tolerance fail like a floor |
| mutation battery | tests/90_mutation_battery.rs | duplicate-def -> absent, relocation -> follows, shadow -> drops |
| trace scorer | python-oracle/trace/ | trace rows score recall-of-covered only, 3-bucket |

## 8. Sequencing and lane shapes

| order | arc | why first | lane shape |
|---|---|---|---|
| 1 | C origin column | Arcs A/B/D all read it | one lane, src/** column + ratchet table |
| 2 | D mutation battery | zero new oracles needed, pure tests | one lane, tests/** only |
| 3 | A held-out oracles | needs C landed; go+python first (cheap tools) | one lane per language pair |
| 4 | B trace pilot | needs C; PyCG suite is self-contained | one lane, python-oracle/trace/** |
| 5 | E flow closure | new plane, needs user in the room for the dl6 shape | plan lane first |

MAX 2 lanes concurrent stands. All lanes on native opus per the 2026-08-23
dispatch decision.

## 9. Forks needing the user

1. Held-out gate mode after the data exists: report-only forever, or hard-gate
   with a pt tolerance.
2. Which held-out repos, if any, may EVER graduate to train (rotate vs freeze).
3. go/rust trace oracles: fund after the python pilot, or drop the class.
4. Arc E dl6 closure shape (lang design, needs Chris in the room).
