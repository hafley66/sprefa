# Lane: cass vs harness.rs, leg-by-leg swap assessment

Worktree: /Users/chrishafley/projects/instant-lab-cassmap
Branch: lab/cassmap. Base: 0e4e017.
Gate: `git merge --ff-only 0e4e017` -> "Already up to date", exit 0. Passed.

Mode: RESEARCH ONLY. No commits, no source edits, no `just dev`, no subagents.
All cass invocations read-only. No `cass index` or any writing command run.
Deviations: none.

## 0. Scope mismatch between the lane brief and the file (must read first)

The lane describes harness.rs as hand-parsing four agent-history formats to feed a
"who ran for whom, when, why" tree panel, listing fields: session id, cwd,
parent_id, parent_kind, status, last activity, harness name, tmux join inputs.

The actual file is 166 lines and does NONE of that. harness.rs only resolves the
newest-first list of RESUMABLE SESSION IDs for an exact cwd, so the UI can launch
`claude --resume <id>` / `opencode --session <id>`. It extracts exactly two things
per session: the id, and its recency ordering. See header comment harness.rs:1-7,
dispatch harness.rs:149-161, single-latest harness.rs:163-165.

The richer tree-panel fields (editor, id, cwd, title, updated, path) live in
ledger.rs, not harness.rs: `AiSession` struct ledger.rs:56-63, `list_ai_sessions`
ledger.rs:717-724. Fields the lane lists (parent_id, parent_kind, status, tmux
joins, harness name) do not exist in either harness.rs or ledger.rs. The four legs
assessed below are harness.rs:24-136; the verdict is scoped to those.

## 1. Every harness.rs leg and what it extracts

| Leg | fn, lines | Store | Match | Returns | Order key |
|-----|-----------|-------|-------|---------|-----------|
| claude | claude_sessions 24-55 | `~/.claude/projects/<cwd,-nonalnum>/` | dir = cwd encoded ('.','_','/',' '->'-') | jsonl file stem = uuid | file mtime desc |
| opencode | opencode_sessions 59-88 | `~/.local/share/opencode/opencode.db` (sqlite, read-only) | `WHERE directory = ?1 AND time_archived IS NULL` | session id | `ORDER BY time_updated DESC` |
| codex | codex_sessions 93-112 | `~/.codex/sessions/<Y>/<M>/<D>/**/*.jsonl` | first line `session_meta` payload cwd == cwd | rollout id (`meta.id`) | file mtime desc |
| kimi | kimi_sessions 117-136 | `~/.kimi-code/sessions/<ws>/session_<id>/state.json` | `state.json` workDir == cwd | `session_<id>` stem minus prefix | state file mtime desc |

All four return a plain `Vec<String>` of ids, newest first. Callers:
`harness_session`/`harness_sessions`, registered in lib.rs:875-876.

## 2. cass interrogation (exact commands + trimmed output)

Version: 0.6.22, api v1, contract v1 (`cass api-version`, exit 0).

`cass capabilities` (exit 0). Relevant connectors present: codex, claude_code,
kimi, opencode (plus 17 others). Commands present include: search, sessions,
resume, status, stats, introspect, timeline, context, export.

`cass status` (exit 0):
```
Index: Last indexed: 10 hours ago (stale)
Semantic: missing (consent required for model download)
Recommended: Run 'cass index' to refresh the index
```

`cass status --json --robot-meta` (trimmed):
```
status: unhealthy; index.status: stale
last_indexed_at: 2026-08-03T01:58:32.211+00:00
age_seconds: 37479          (~10.4 hours)
stale_threshold_seconds: 1800
semantic: absent
pending.sessions: 0 ; watch_active: false
```

`cass stats` (trimmed):
```
Conversations: 2439 ; Messages: 317602
By Agent: claude_code 1616, codex 663, opencode 155, kimi 5
Top Workspaces: .../sprefa 721, .../smashy 245, .../instant 129
```

`cass sessions --workspace /Users/chrishafley/projects/instant-lab-cassmap`
(exit 0). Asked for the exact worktree. Every row returned had `workspace:
/Users/chrishafley/projects` (the PARENT), e.g. claude
`.../-Users-chrishafley-projects/e213abd0-...jsonl`, codex rollouts, opencode
`ses_0453...`. No row was scoped to the worktree itself.

`cass sessions --current` run with cwd = this worktree (exit 0):
```
Sessions for /Users/chrishafley/projects/instant-lab-cassmap
 1. [2026-07-30T15:54:18.131+00:00] claude_code 95 msgs / 6 human
    workspace: /Users/chrishafley/projects
```
So resolved "current workspace" coalesces to the parent and returns a 2026-07-30
session (stale; today is 2026-08-03).

Exact-cwd scoping gun: `cass sessions --workspace
/Users/chrishafley/projects/sprefa/.claude/worktrees/agent-a0415b87430ff41e7`
-- that dir owns exactly 1 session (the subagent
`-Users-chrishafley-projects-sprefa/7d976dd8-.../subagents/agent-a0415b87430ff41e7.jsonl`).
cass returned that 1 PLUS the parent's main sessions (workspace
`/Users/chrishafley/projects/sprefa`, results 2-4). So `sessions --workspace` is a
parent-superset match, not exact.

`cass search "deepseek" --workspace <that exact worktree> --limit 3 --json`
-> `total_matches: 0`, even though the worktree owns a session. Inconsistent with
`sessions`: search's `--workspace` filter is stricter (returns 0 where
`sessions --workspace` returns the parent superset). Either way, neither matches
harness.rs's exact-cwd semantics.

`cass resume` (exit 0 each) -- reproduces the exact resume command the harness
caller builds:
- claude path -> `["claude","--resume","03191060-..."]`
- codex path -> `["codex","resume","019fa5ee-..."]`
- opencode path -> `["opencode","--session","ses_0453..."]`
(`--json` output, `command` array + `shell_command`, `agent` detected from path.)

`cass search "harness" --agent claude_code --workspace .../sprefa` -> hits with
`source_path`, `agent`, `workspace`, `created_at`, `score`. Search exposes
source_path (which embeds the id), workspace, timestamps. No dedicated `session_id`
field for search; `sessions` returns path+workspace+agent+title+modified+
message_count+human_turns.

kimi connector path mismatch: `cass search ... --agent kimi` returned source paths
under `~/.kimi/sessions/aea1ccc4.../.../wire.jsonl`. harness.rs kimi_sessions reads
`~/.kimi-code/sessions/<ws>/session_<id>/state.json` (harness.rs:118). Different
directory layout (`~/.kimi/` vs `~/.kimi-code/`, and `wire.jsonl` vs
`state.json`). cass's kimi connector does not read the tree harness.rs reads.

Live-freshness check for this worktree: `cass search "cassmap" --agent opencode`
-> `total_matches: 0`. No indexed opencode content exists for this worktree. The
current session running in this worktree is not visible to cass.

## 3. Per-leg verdict

Common need across all four legs: newest-first list of resumable ids for an EXACT
cwd, guaranteed to include the live session (fresh). cass constraints that break
substitution: (a) index staleness (age_seconds 37479, last_indexed 02:00 today)
means anything newer than ~10.4h, including a session running right now, is absent;
(b) `sessions --workspace` is parent-superset and `search --workspace` returns 0
for a dir that owns a session, so neither reproduces exact-cwd filtering; (c)
harness.rs needs no cass install and no index at all (direct disk reads; ledger.rs:80
notes the UI treats cass as optional, offering only an install hint).

| Leg | Fields needed | cass exposes them? | Freshness (live session now) | Verdict |
|-----|---------------|--------------------|------------------------------|---------|
| claude | id + recency, exact cwd | Partial. `resume` builds `claude --resume`; `sessions --workspace` parent-scoped, not exact | FAIL. index stale 10.4h; `--current` from this cwd returned 2026-07-30 | KEEP DIRECT |
| opencode | id + recency, exact cwd | Partial. `resume` builds `opencode --session`; search `total 0` for this worktree | FAIL. live opencode session invisible | KEEP DIRECT |
| codex | id + recency, exact cwd | Partial. `resume` builds `codex resume`; discovery parent-scoped | FAIL. stale index | KEEP DIRECT |
| kimi | id + recency, exact cwd | NO. cass reads ~/.kimi (wire.jsonl); harness needs ~/.kimi-code (state.json) | FAIL. path mismatch, stale | KEEP DIRECT |

Net: none of the four legs collapses cleanly into cass. cass's `resume` command is
a structural match for resume-command construction, and `search`/`sessions` expose
the right identifier/timestamp shapes, but the two blockers (index staleness
against a live session, and non-exact workspace scoping) apply to every leg, so all
four stay direct reads. cass is the right tool for the separate ledger.rs
discovery/search surface, which already talks to it (ledger.rs:88-130,
`cass_status`, `cass_swarm_status`).

## 4. What cass CANNOT answer structurally (verified)

- Live process state. A session open right now is not in the index until an index
  pass runs; `cass status` reports pending.sessions 0 / watch_active false and the
  lexical index is 10.4h stale. Verified: this worktree's live opencode session
  returns 0 search hits and `sessions --current` returns 2026-07-30 data.
- Anything not yet written to disk / not yet ingested. Structurally bounded by the
  last index run.
- Exact-cwd scoping. cass buckets by "workspace"; `sessions --workspace` is a
  parent-superset match and `search --workspace` disagreed (0 vs superset).
  harness.rs needs exact `directory = cwd` / `workDir == cwd` matching.
- The kimi tree harness.rs reads (`~/.kimi-code/sessions/<ws>/session_<id>/state.json`)
  is a different layout from cass's kimi connector (`~/.kimi/sessions/.../wire.jsonl`).
- For the richer panel (ledger.rs, outside this lane's binding): parent_id /
  parent_kind / status / tmux join inputs are not emitted by cass search or
  sessions (which expose title, message_count, human_turns, workspace, agent,
  timestamps, path). No parent-link field, no live tmux state.

## 5. Onboarding note

Index is stale and semantic is absent on this box. Refresh would require `cass
index` (and `cass models install` + `cass index --semantic` for semantic), which
are writing commands; per lane constraints these were NOT run. Lexical search is
usable for already-indexed history ("search fully correct for everything already
indexed"); only recent sessions lag and semantic refinement is absent. This
readiness gap is itself the disqualifying factor for the harness.rs legs, which
must be live-fresh.

## Receipts index

- `git merge --ff-only 0e4e017` -> "Already up to date", exit 0.
- `cass capabilities`, `cass api-version`, `cass introspect`, `cass robot-docs`
  (exit 0; capabilities listed under method 2).
- `cass status` -> last indexed 10h ago, stale.
- `cass status --json --robot-meta` -> age_seconds 37479, last_indexed 2026-08-03T01:58:32.
- `cass stats` -> 2439 conv / 317602 msgs; claude_code 1616, codex 663, opencode 155, kimi 5.
- `cass sessions --workspace <this worktree>` -> all rows workspace = parent /projects.
- `cass sessions --current` (cwd = this worktree) -> claude 2026-07-30, workspace /projects.
- `cass sessions --workspace <sprefa worktree>` -> exact 1 + parent superset.
- `cass search "deepseek" --workspace <sprefa worktree>` -> total 0.
- `cass resume` x3 -> claude --resume / codex resume / opencode --session.
- `cass search "harness" --agent claude_code --workspace .../sprefa` -> hits w/ source_path+workspace.
- `cass search ... --agent kimi` -> paths under ~/.kimi/sessions (mismatch with ~/.kimi-code).
- `cass search "cassmap" --agent opencode` -> total 0 (worktree absent from index).
- harness.rs:1-166 read complete; ledger.rs:56-63, 717-724, 88-130 read.

UNVERIFIED: exact "does a session running RIGHT NOW appear" before/after delta was
not directly measured by touching a live history (read-only constraint, lane method
2). Indirect evidence is decisive: `cass status` staleness + `sessions --current`
returning 2026-07-30 for a cwd that holds the currently running opencode session,
plus `search "cassmap" --agent opencode` returning 0.
