# Agent eval harness: a falsifiable, scriptable measurement of dl's effect

## What this is and is not

This is a measurement instrument, not an idea list. The possibility mining
lives in plans/2026-07-10-fable-possibility-mining.md and stays there. This
plan exists to answer ONE preregistered question with a number that could
come out against dl, produced by a script anyone can rerun:

> Does a small model (Haiku) with dl's structured surfaces beat the same
> model with bash/grep, and how does it compare to a bigger model (Sonnet)
> with bash/grep, on accuracy and cost, on tasks whose ground truth does
> not come from dl?

If the answer is no, the harness has done its job. The results template
has a mandatory "where dl lost" section.

## Anti-grift protocol (the design constraints everything below serves)

1. **Ground truth independent of the tool under test wherever possible.**
   The PRIMARY task class is seeded faults (C2): a script mutates a
   throwaway worktree (break an import, orphan an export, introduce a
   duplicate symbol); the mutation coordinates ARE the answer key. dl
   never touches the key. Navigation QA (C1), where dl's own graph is the
   convenient key, is SECONDARY and only counts for task types covered by
   an independent oracle (rust-analyzer SCIP for Rust call/def facts,
   madge for TS imports) with the oracle agreement rate reported beside
   the score; task instances where the oracle and dl disagree are
   excluded and counted, not silently dropped.
2. **Symmetric cells.** Every cell gets the same prompt text, the same
   timeout, the same output contract, the same repetition count. The only
   difference is the tool inventory. The baseline cells get full
   bash/grep/read, not a crippled subset. No dl-flavored hints in the
   shared prompt; tool discovery is the agent's problem in every cell.
3. **Preregistration.** The primary endpoint, task filters, seeds, and
   scoring rules are committed in this repo BEFORE the first full run
   (this plan is that commitment; the harness config file freezes the
   numbers). Prompt or filter changes after that = a new experiment
   version, old results kept.
   - Primary endpoint: C2 locate rate (exact file+line-window match) per
     cell, with cost (tokens) per solved task.
   - Repetitions: 3 per task x cell (temperature nonzero reality), all
     reported, median scored.
4. **Deterministic task generation.** Corpus sampling is seeded (order by
   sym-hash, not random), filters are numeric and committed (e.g. fan-in
   in [3, 40]), and the generator emits the full task set before any
   model sees any task. No post-hoc task removal; a malformed task is
   scored against every cell equally or the whole task is voided with a
   logged reason.
5. **Foreign corpus.** Runs happen on this repo AND on at least two
   external repos from ~/orgs that dl's authors did not write. The
   headline number is the foreign-corpus one.
6. **Full-matrix reporting.** Results are a committed markdown + jsonl
   pair under bench/agent-eval/results/, keyed by git sha + model ids +
   experiment version. Never overwritten, never partially reported. Parse
   failures (model answered in prose) are scored wrong but tracked as a
   separate rate so contract problems don't masquerade as knowledge.
7. **One command.** `bench/agent-eval/run.sh <experiment.toml>` does
   generate -> mutate -> run cells -> score -> emit report. No manual
   steps between generation and report; a human choosing anything mid-run
   is a protocol violation.

## Task classes

| class | task | ground truth | role |
|---|---|---|---|
| C2 seeded faults | "a recent change broke something in this repo: find the file and line of the defect" over a mutated worktree; 4 mutation kinds x N sites | the mutation script's coordinates — fully dl-independent | PRIMARY |
| C1 navigation QA | who-calls X / where-defined Y / what-imports Z | dl graph, admitted ONLY where an independent oracle agrees (RA SCIP, madge); oracle-disagreement excluded + counted | secondary |
| C3 edits | "move file A to B and fix every reference" | `dl --move` output as reference diff + `cargo check`/tsc green; note the referee is dl-derived, so C3 is reported as exploratory, not headline | exploratory |
| C4 judgment | reachability/safety calls | needs labels or an LLM judge | out of scope until C1/C2 prove the instrument |

## The matrix

{haiku, sonnet} x {bash/grep/read, same + dl MCP tools}. Per cell x task x
rep: solved (per class rule), tokens in/out, wall time, tool calls, parse
failure. Runner = `claude -p` headless, per-cell `--allowedTools` and MCP
config, `--output-format json` for cost capture. Bounded parallelism
(N=4).

Prerequisite: the dl.what/dl.verb/dl.rows MCP tools (ledgered,
decided-unbuilt). Without them the +dl cells inject via raw CLI and the
measurement conflates dl's value with shell-parsing ability. S0 below.

## Harness anatomy (bench/agent-eval/, the bench/ scenario idiom)

1. `gen-tasks.dl` per class emits `task(id, class, prompt, expected_json)`
   (expected packed with json_group_array — the query-head aggs shipped
   2026-07-10). Sampling seeded and filter-gated as preregistered.
2. `mutate.sh` (C2): applies one mutation to a fresh `git worktree` per
   task, records coordinates into the task row, cleans up after scoring.
3. `run.sh`: the one command. Reads experiment.toml (model ids, cells,
   task filter, reps, timeout), executes the matrix, writes raw outputs
   to out/ (jsonl per task x cell x rep).
4. `score.py` (or rust bin): lenient JSON extraction (fenced block,
   trailing prose), class-specific scoring (C2 file+line-window; C1
   set-F1), aggregates, emits results/<date>-<sha>-<version>.md + .jsonl.
5. Post-mortem: the +dl cells run against a db accumulating `query_log` +
   `hook_event`; `diagnose.dl` joins failures to what the agent asked —
   wrong verb / right verb misread / tool answer itself wrong. The third
   bucket is a TOOL bug the harness just found; those feed fixes, and the
   fix triggers a new experiment version, not a rescore.

## Stages

- **S0 (prereq, S-M)**: dl.what / dl.verb / dl.rows MCP tools in the
  --mcp adapter. The harness is the forcing function for this ledger item.
- **S1 (M)**: C2 end to end on this repo + one foreign repo: mutation
  kinds (import-break, export-orphan, duplicate-sym, off-by-one span),
  ~40 tasks, full 4-cell matrix, 3 reps, first committed results. THE
  falsification gate: if haiku+dl does not separate from haiku-grep here,
  stop and say so.
- **S2 (S)**: C1 with oracle gating, second foreign repo.
- **S3 (S)**: diagnose.dl + the "where dl lost" and "tool bugs found"
  sections wired into the report template.
- **S4 (defer)**: C3 exploratory; C4 only with labels.

## Risks / honest limits

- An agent in a +dl cell answering C1 by running the generator's own query
  shape is measuring the intended thing (knowing to ask), but it is why C1
  can never be primary; C2's key never lived in the graph.
- dl graph errors on the foreign corpus depress the +dl cells (the tool
  answers wrong). That is signal, not noise — report it via the diagnose
  bucket rather than repairing tasks.
- Model version drift: model ids pinned per experiment version; a model
  update = new version.
- Cost: sonnet cells dominate spend; experiment.toml supports cell subsets
  and task caps, but a headline claim requires the full preregistered
  matrix.

## Critical files

- bench/ (idiom: ghcacher_vs_dl.sh, graph_diff/harness.sh)
- src/mcp.rs (S0 tools), src/cli/query.rs (verbs the tools wrap)
- examples/gen-doc-indexes.dl (generator idiom), tests/it/query_agg.rs
  (the agg contract the generator leans on)
- tests/oracle_rust.rs, tests/it/oracle_madge.rs (the independent oracles
  C1 admission rides on)
