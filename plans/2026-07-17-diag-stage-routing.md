# R7: diag stage routing — per-repo harness with staged severity surfaces

Status: SPEC DRAFT for user review, written 2026-07-17 during the auto
session. Not implemented. The goal, in the user's words: "commands for
pre-commit check _only_ so that its not annoying while dev'ing but actually
saving will yell at you, vs agent_prev_turn messages, and agent_session, so
that i can finally have my own harness of tools per repo that are consistent
like the comment/todo/plans/architectural techniques".

## Current state (what exists)

- diag(path, line, severity, code, msg) is the one findings rel
  (docs/rails.md:72). Severity drives exit code: `dl --check` exits 2 only on
  error-severity; warnings print but never block.
- .githooks/pre-commit = `exec dl --check` — every .dl rail runs at commit,
  every rail runs live under the daemon, same set both places.
- Agent surfaces exist: src/agent.rs agent_edit/latest_touch built-ins, and
  `dl --hook` is already wired as the Claude Code hook entry.
- The four storm rails (792cc902) are warning-tier: visible everywhere,
  blocking nowhere. 48/25/40/1 findings would spam every agent turn if routed
  there naively.

## Design

### Types first

```
rel diag_stage(code: text, stage: text).
# stage in: "live" | "commit" | "agent-turn" | "agent-session"
# absence of a row for a code = default stage "commit" (see resolution).
```

```rust
// CLI:
//   dl --check                    -> stage "commit"  (unchanged surface)
//   dl --check --stage live       -> stage filter
//   dl --hook                     -> stage "agent-turn" (+ "agent-session"
//                                    on session boundary events)
fn stage_filter(diags: Vec<Diag>, stage: &str, routes: &HashMap<String, Vec<String>>) -> Vec<Diag>
// pseudo:
//   for d in diags:
//     stages = routes.get(d.code).unwrap_or(default_stages(d.severity))
//     if stage in stages: keep
```

### Resolution rules

1. A rail declares its own routing by heading diag_stage rows in the same .dl
   file (colocated, no central registry).
2. Default when a code has no diag_stage rows: severity error -> every stage;
   severity warning -> "commit" only. That single default fixes the current
   annoyance without touching any rail: warnings stop appearing on live
   ticks and agent turns unless opted in.
3. "agent-turn": only diags whose path intersects agent_edit/latest_touch
   rows from the current turn (the rel already exists) — an agent hears about
   what it just touched, never the whole audit list.
4. "agent-session": full set, delivered once per session start via the hook's
   session event, as a summary count per code + top N.

### Instance lifetimes

- diag_stage rows: derived rel, rebuilt with the program like any other; no
  new state.
- The stage filter: pure function at render time in --check/--hook; no
  storage. The db keeps ALL diags regardless of stage (querying `? diag(...)`
  stays complete; staging is a presentation concern).

### Storage/sequence

- No schema change. diag_stage is an ordinary derived rel; --check reads it
  with one query and filters in Rust. Uniqueness: (code, stage) key, dupes
  harmless.

## Per-repo harness template (the actual point)

A repo opts in by dropping .dl files following this shape:

```
.dl/
  comment-audit.dl     # todo/fixme -> diag warning, diag_stage commit
  plans-freshness.dl   # stale plans/ docs -> agent-session
  arch-measures.dl     # fan_in/cycle audits -> agent-session
  <storm rails>        # unchanged, default commit
.githooks/pre-commit   # exec dl --check   (already the pattern)
Claude hook            # dl --hook          (already the pattern)
```

Portable = copy the .dl files; the stage routing rides in them.

## Open questions for the user

1. Stage names ok? ("live" = every daemon tick surface, "commit" =
   --check/pre-commit, "agent-turn"/"agent-session" = hook events.)
2. Default for unroutd warnings = commit-only: confirm.
3. Should "save will yell at you" mean live-stage diags surface in the
   editor via the LSP/vscode path (Wave 4 tie-in), or a `dl --check
   --stage live --watch` terminal loop for now?

## Implementation order (post-approval)

1. diag_stage rel decl + default resolution in --check (1 file + test).
2. --stage flag on --check.
3. Hook routing (agent-turn intersection with agent_edit; session summary).
4. Port the comment/todo/plans rails in claude-research to declare stages.
