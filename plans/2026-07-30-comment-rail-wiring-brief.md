# comment rail wiring brief (codex luna, no-commit flow)

Base sha stated at launch. Work ONLY inside your worktree. First action:
`git rev-parse HEAD` compared against the sha in the launch prompt —
READ-ONLY verification; if it mismatches, STOP AND REPORT. Do not commit,
do not push; leave the tree dirty for coordinator review.

## Context

plans/2026-07-29-comment-node-verdict.md proved 7 comment_node techniques
zero-new-constructs with comment_node 745/745 byte-exact vs v5. The
receipt_folding ruling (chat_log/20260730.0.v6-2-ts-closeout.pl) requires
every passing receipt folded into production. ARCH row: comment_rail_wiring.

## Goal

The verdict's techniques become standing production rails: .dl6 rail
programs checked in under the existing rails/fixtures conventions, a
justfile recipe that runs them against the live extraction feed, graded
the same way the lab graded (v5 parity where the verdict claimed it).
Read the verdict FIRST and follow its technique list exactly.

## Hard laws

- NO new syntax. Every rail uses only constructs that already compile
  (check with the compiler; a refusal means STOP that technique and note
  it, never work around).
- The markdown extractor hole named in the verdict stays a named skip —
  do not touch sprefa-extract.
- No production compiler/runtime edits (v6/prolog/compile/**, v6/tsv2/src/**
  are off-limits). Fixtures, rails, scripts, justfile recipes only.
- Hermetic runs; never touch ~/.local/state/sprefa or the daemon.
- Line numbers in plans are stale; re-find by symbol.
- Test budget: full battery zero times (coordinator runs it); your gates =
  the rail runs themselves + conformance once at the end.

## Final summary shape

Per-technique: rail file path, grade command, parity number vs the
verdict's claim, skips with named reasons.
