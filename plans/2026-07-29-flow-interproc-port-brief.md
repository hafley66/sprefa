# flow-interproc port brief (codex terra, DOING lane)

Objective: port `examples/flow-interproc.dl` to v6 as far as today's facts
allow, graded against v5's own output through the flagship rig pattern, with
honest named stops where facts are missing. A parallel luna lane is scouting
the full gap map (plans/2026-07-29-flow-interproc-scout-brief.md); you do NOT
wait for it and you do NOT write its doc. Real decisions are yours: port
scope cuts, closure spelling, corpus choice — that is why this lane is terra.

## What exists
- `examples/flow-interproc.dl` (74 lines) + `std/flow.dl`: flow_edge =
  df_edge ∪ positional arg->param hop ∪ ret->call_res hop; flow_reach =
  closure(flow_edge); typed views over type_sig + df_node/df_param;
  call_edge_bare for callee binding.
- v6 extractor: `v6/sprefa-extract`, contract = `extract --schema`
  (build: `cargo build --release --features cli --bin extract`). Families
  cst/type/call; records def/edge/site/sig/const/specifier; NEW:
  `extract --resolve PATH...` emits `resolved_edge` JSONL
  {caller_path, caller_name, callee_path, callee_name, kind} — cross-file
  resolved call edges, landed today (tests/1_resolve_cli.rs is the contract).
- The graded rig precedent: `v6/tsv2/scripts/flagship-callgraph.sh` +
  `flagship-classify.py` — pinned 13-file rust corpus in a scratch one-commit
  repo, v5 leg hermetic + DL_STATE_DIR-isolated, per-bucket diff
  classification, rule-fidelity legs (v5's rule bodies run against EACH
  engine's own inputs). Copy this shape, do not reinvent it.
- v6 recursion: recursive strata compile incrementally (P3, support-count
  retraction, cycle-reachable reseed). Transitive closure as two rules is
  expected to compile. `v6/prolog/compile/SCOREBOARD.md` + conformance
  fixtures show the graded recursion receipts.

## Steps (commit-worthy units, in order)
1. **v5 leg first.** Run `examples/flow-interproc.dl` hermetically
   (`SPREFA_CONFIG=/nonexistent/x.toml DL_NO_DAEMON=1 dl ... --db <scratch>`)
   over the SAME pinned corpus flagship-callgraph.sh builds. Capture the four
   query outputs. If df_* facts are empty on the corpus (possible: the lift
   may not cover the corpus's shapes), record counts anyway — the v5 leg's
   own numbers are the grading target, whatever they are.
2. **Port the portable rules.** A v6 program (fixture .pl term-form and/or
   .dl6 via the text door — follow how 3_flagship_callgraph.pl did it):
   - resolved call edges from `extract --resolve` output as EDB arrivals
     (the callgraph fixtures show the host/arrival shape).
   - `flow_reach` as direct recursion over the edge rel you can feed today.
   - `sink_callee` / `flow_param_type` over sig-record facts if the sig
     records carry what type_sig carried; else named stop.
   - `flow_node_type` and anything needing df_node/df_param/df_edge: expected
     NAMED STOP (the df value-lift plane has no v6 extractor family). Do NOT
     synthesize fake df facts.
3. **Grade.** `v6/tsv2/scripts/flagship-flow.sh` (NEW file, modeled on
   flagship-callgraph.sh): run both legs, classify every diff row into named
   buckets (extraction-input vs expression gap vs defect), exit nonzero on
   unclassified. Transitive closure output may explode on corpus scale —
   if row counts make byte-diff grading unreasonable, grade counts +
   spot-checked pairs and SAY SO in the script header.
4. **Fixtures.** Promote at least one oracle-graded conformance fixture for
   the recursive flow_reach shape if an equivalent doesn't already exist.

## Hard laws
- Worktree only; base sha stated at launch; your FIRST action is
  `git merge --ff-only <that sha>` — if it fails or the tree looks wrong,
  STOP AND REPORT. Never work around a blocked command via another mechanism.
- NO-COMMIT flow: do not commit; leave the tree dirty with a final summary
  (the coordinator reviews file-by-file and commits). If you try a commit and
  git metadata writes fail, that is expected in codex worktrees — just stop
  committing.
- DO NOT edit `v6/prolog/compile/*.pl` core, `v6/prolog/conformance/engine.pl`,
  or `v6/tsv2/runtime/*` — the struct-as-rows arc owns those files in flight.
  Additive files (new fixture, new script, new gen output) + minimal fixture
  registration edits only. If the port genuinely requires a compiler/runtime
  change, STOP that step and name it in the summary.
- Hermetic v5 runs only (flags above); never touch ~/.local/state/sprefa or
  the daemon. Never `--full-auto` anything.
- dl variable names descriptive, never single-letter. Vocabulary law: rxjs /
  prolog / SQL words only for any construct-adjacent naming.
- Smallest correct solution, standing user directive. No speculative layers.

## Validation (run before the summary; report exact counts)
- `swipl v6/prolog/conformance/go.pl` — zero findings, count stated.
- Sweep BOTH modes (`v6/tsv2/scripts/sweep.sh`, and
  `SPREFA_TSV2_EMITTER_MODE=naive`) — identical-count growth only, zero wrong.
- Your new flagship-flow.sh exit 0 with the classification table printed.
- Max 3 full sweep runs; targeted single-fixture runs otherwise.

## Required final summary
Base-sha verification line; per-step outcome; the four-query coverage table
(ported / approximated / named stop + reason); rig classification results;
every law you could not satisfy, named, with the step left undone.
