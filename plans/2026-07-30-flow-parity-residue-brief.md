# flow parity residue brief (codex terra, no-commit flow)

Base sha stated at launch. Work ONLY inside your worktree. First action:
`git rev-parse HEAD` and compare against the sha in the launch prompt —
READ-ONLY verification; if it mismatches, STOP AND REPORT. Do not commit,
do not push; leave the tree dirty for coordinator review.

## Context (receipts from chat_log/20260730.0.v6-2-ts-closeout.pl)

Last graded state:
- flow_edge: v5 2772, v6 3114, matched 2654, v5-only 118.
- flow_node_type: v6 58, matched 33.
- flow_param_type: matched 35.
- blocker(flow_call_target_resolution): v5_targets 200, v6_targets 168,
  matched 113, v5_only 87, v6_only 55.

Rig: `v6/tsv2/scripts/flagship-flow.sh` + `flagship-flow-classify.py`.
The referee owns ALL coordinate/key translation (v5 `path:line:col:kind`
1-based-line/0-based-col vs v6 byte spans; v5 sym keys carry a `root::`
prefix and qualified type names). ARCH row: flow_parity_residue.

## Goal

Every unmatched row classified into exactly one bucket:
(a) extraction-input difference (one engine's extractor sees a node the
    other cannot — cite the source line),
(b) referee key gap (fix it referee-side in flagship-flow-classify.py /
    flagship-flow.sh ONLY),
(c) genuine engine defect (do NOT fix; report with a minimal repro).
Exit = 0 unclassified across all four query rels, or STOP with named
blockers and the partial table.

## Hard laws

- Engines untouched: no edits under v6/prolog/, v6/tsv2/src/, v6/tsv2/serve/,
  src/ (v5 rust). Referee scripts + this brief's plan doc are your only
  writable surface, plus a findings doc `plans/2026-07-30-flow-parity-residue-findings.md`.
- NO new syntax anywhere, no .dl6 edits.
- Hermetic v5 runs: `SPREFA_CONFIG=/nonexistent/x.toml DL_NO_DAEMON=1`,
  scratch `--db`; never touch `~/.local/state/sprefa` or the daemon.
- Line numbers in plans are stale; re-find by symbol.
- Test budget: the flow rig end-to-end at most 6 full runs.

## Final summary shape

Classification table per rel (v5-only and v6-only counts per bucket),
referee diffs made, defect repros if any, skipped items with reasons.
