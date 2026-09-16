# Lane: cass vs harness.rs, leg-by-leg swap assessment (RESEARCH ONLY)

Worktree /Users/chrishafley/projects/instant-lab-cassmap, branch lab/cassmap,
base 0e4e017. FIRST action: `git merge --ff-only 0e4e017` — failure = STOP and
write REPORT.md saying so. You change ZERO source files. Deliverables are
exactly two new files at the worktree root: REPORT.md (receipts) and
REPORT.visual.human.unga.md (plain words + ascii diagrams, zero citations).

## Question

`src-tauri/src/harness.rs` hand-parses four agent-history formats (claude
jsonl + subagents dirs, opencode sqlite db, codex rollout jsonl, kimi session
dirs) to feed a "who ran for whom, when, why" tree panel. The `cass` CLI
(installed at /opt/homebrew/bin/cass, "Unified TUI search over coding agent
histories") already parses agent histories. Which harness.rs legs collapse
into cass queries, and which must stay direct reads?

## Method (binding)

1. Read src-tauri/src/harness.rs COMPLETELY. List every leg and every field it
   extracts (session id, cwd, parent_id, parent_kind, status, last activity,
   harness name, tmux join inputs).
2. Interrogate cass, read-only commands only:
   `cass capabilities`, `cass robot-docs`, `cass introspect`, `cass stats`,
   `cass status`, `cass api-version`, and a handful of real `cass search`
   probes over this machine's histories (e.g. find a known opencode deepseek
   session, a claude subagent transcript). Record exact commands + trimmed
   outputs in REPORT.md.
3. Answer per leg, in one table: fields needed | cass exposes them? (cite the
   command/schema) | index freshness (does a session running RIGHT NOW appear,
   and how stale; measure via cass status/stats before+after touching a live
   history if possible read-only) | verdict: collapse into cass / keep direct /
   hybrid.
4. Separate section: what cass CANNOT answer structurally (live process state,
   tmux joins, anything not in transcripts) — verified, not assumed.
5. Every claim carries its receipt (command output or file:line). Claims you
   cannot verify get flagged UNVERIFIED. If cass's index is empty/stale on
   this box, run `cass status` and report what onboarding would require; do
   NOT run `cass index` or any writing command without recording it as a
   deviation first — prefer read-only throughout.

## Laws

- No commits. No source edits. Nothing outside this worktree except read-only
  cass invocations. Never `just dev`. No subagents.
- Deviations: STOP the item and record it in REPORT.md.
- Style: no em dashes; never the words provenance, substrate, load-bearing,
  regime; descriptive names. The unga doc is plain human words, short lines,
  ascii diagrams, zero citations.
