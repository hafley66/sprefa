# R7: diag stage routing — per-repo harness with staged severity surfaces

Status: IMPLEMENTED-WITH-DECISIONS 2026-07-18 (branch r7-diag-tracing).
Steps 1-3 landed; step 4 (porting the claude-research rails) is out of this
arc's scope. Decisions + the tracing build-vs-buy analysis + the eprintln
inventory are recorded at the end of this file. The goal, in the user's words:
"commands for pre-commit check _only_ so that its not annoying while dev'ing but
actually saving will yell at you, vs agent_prev_turn messages, and
agent_session, so that i can finally have my own harness of tools per repo that
are consistent like the comment/todo/plans/architectural techniques".

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

## Open questions for the user — ANSWERED

1. Stage names: ACCEPTED as spec'd — "live" | "commit" | "agent-turn" |
   "agent-session".
2. Default for unrouted warnings = commit-only: CONFIRMED.
3. Live surface: NO editor/LSP tie-in this arc (deferred to vscode Wave 4).
   The `--stage live` filter flag is enough for now.

## Implementation order (post-approval)

1. diag_stage rel decl + default resolution in --check (1 file + test). DONE.
2. --stage flag on --check. DONE.
3. Hook routing (agent-turn intersection with agent_edit; session summary). DONE.
4. Port the comment/todo/plans rails in claude-research to declare stages.
   OUT OF SCOPE for this arc.

## Decisions in force (2026-07-18 implementation)

### diag_stage is a builtin SINK, not a plain user rel

The spec sketch wrote `rel diag_stage(...)`, but the engine reads it by name in
the --check/--hook filter, so a plain user rel would be a MAGIC REL (an
un-catalogued literal the engine special-cases). Per `sprefa-v5-no-magic-rels`,
`diag_stage` is instead a first-class builtin sink mirroring `diag`:

- `diag_stage_rel_decls()` (src/engine/decls.rs), fixed 2-col schema
  (code: text, stage: text), group `diag`, chained into `all_builtin_decls()`
  so it shows in `rel_catalog` / `dl docs relations` and typecheck accepts it as
  a head.
- `DIAG_STAGE_RELS` const (src/engine/mod.rs) + a reserved-name bail
  (src/engine/declare.rs): a user `rel diag_stage(...)` decl is rejected ("head
  it directly, like diag"); heading it from a rule is the only way in.
- The engine reads it via `eng.rel_rows("diag_stage", 2)` — a parameterized
  read, not a `rels.get("diag_stage")` / `FROM rel_diag_stage` literal, so the
  magic-rel rail (.dl/magic-rel-audit.dl) stays green AND the name is
  catalogued regardless.

Staging is presentation-time only: the db keeps every `diag` row (`? diag(...)`
stays complete); the filter runs in Rust at render time in --check/--hook.

### Routing resolution (src/stage.rs, the pure filter)

- `routed_to(code, severity, stage, routes)`: an explicit `diag_stage` row set
  for a code is EXHAUSTIVE (overrides the default, not additive). A code with no
  row falls to `default_stages(severity)`: error -> every stage; warning (and
  anything not "error") -> "commit" only.
- Net effect on the historical surface: at the default `commit` stage every
  severity is still routed, so `dl --check` output is UNCHANGED until a rail
  opts a code onto other stages. `--stage live` / `--stage agent-turn` drop the
  unrouted warnings — the annoyance fix, no rail edit required.

### CLI

`--stage <live|commit|agent-turn|agent-session>` on --check (default `commit`);
an unknown name bails loudly (`unknown --stage ...`) rather than silently
surfacing nothing. `--diag-json` shares the same filter.

### Hook routing (src/hook.rs)

- The daemon `diag` RPC now bundles `{rows, stages, touch}` (the diags, the
  `diag_stage` [code,stage] rows, and the latest-turn `agent_touch` paths) so
  check and hook filter client-side in one round trip.
- Session-boundary events (`SessionStart`/`SessionEnd`) get the `agent-session`
  surface: per-code counts (descending) + a top-10 sample. Every other event is
  an `agent-turn`: the `agent-turn`-routed diags, THEN gated to the files the
  latest turn touched (`agent_touch`), so the agent hears about what it just
  touched, never the whole audit list. The routed context rides the hook's
  `additionalContext` alongside inject/inject_skill.
- KNOWN REQUIREMENT: the agent family (`agent_edit`/`agent_touch`) is a lazy
  built-in — refreshed only when the program references it. A harness `.dl` that
  wants agent-turn path gating must reference an agent rel (the hook's own
  template already heads inject over `agent_touch`/`agent_changed`). Kept lazy on
  purpose (standing law: nothing seizes the machine — no per-tick harness-store
  read for programs that never ask). The it-test references `agent_touch` to
  mirror real harness programs.

## The four storm rails at each stage

The four syntax storm rails (792cc902) are warning-tier with no `diag_stage`
rows, so under the new default resolution they route to `commit` only:

| stage         | storm rails visible? |
|---------------|----------------------|
| commit        | YES (pre-commit / --check, unchanged)     |
| live          | NO (dropped — no live spam while dev'ing) |
| agent-turn    | NO (unless a rail heads diag_stage(code,"agent-turn"), then gated to touched files) |
| agent-session | NO (unless a rail heads diag_stage(code,"agent-session"), then summarized) |

To route a storm rail onto an agent surface, its .dl heads e.g.
`diag_stage("storm-syntax", "agent-turn") <- ...` beside its `diag(...)` rule —
colocated, no engine change.

## Logging: tracing, per the user directive ("never eprintln")

New diagnostic/log lines added by R7 (the routing-decision debug lines in
lib.rs/hook.rs) go through the `tracing` crate at `debug` level — silent by
default, surfaced via `DL_TRACE`/`RUST_LOG` (CLI -> stderr, src/trace.rs) or
`DL_LOG` (daemon -> its subscriber, src/daemon.rs::install_daemon_tracing). The
product diag OUTPUT (the actual --check findings, the hook JSON) is not a log and
stays on its stream.

### Build-vs-buy: the logging/tracing crate (standing law)

The dependency was ALREADY in Cargo.toml (`tracing` 0.1 + `tracing-subscriber`
0.3, added in a prior arc) and wired to both targets, so R7 reuses it rather
than landing a new dep. Confirming the pick against the alternatives:

| candidate                     | fit for dl | verdict |
|-------------------------------|-----------|---------|
| tracing + tracing-subscriber  | spans carry per-phase durations (span CLOSE events already time tick phases); structured fields (`stage`, `kept`, `dropped`); EnvFilter gives per-target/per-module levels; the daemon and CLI install separate subscribers (stderr vs its log) off the same API; async-aware for the tokio daemon shell. | PICKED (already in tree, both subscribers wired) |
| log + env_logger              | simple facade, but no spans (so no per-phase span-timing, which the tick hot-path already relies on) and no structured fields; env_logger is CLI-only (no clean daemon-file target split). | rejected: loses spans + fields the engine already uses |
| slog                          | structured + fast, but its explicit-logger-threading model (or scope-guard globals) is a heavier API than the ambient `tracing::debug!` macros, and its ecosystem is smaller; would fight the tokio/axum stack that emits `tracing`. | rejected: API weight + ecosystem |
| fern                          | a thin formatting/dispatch layer over `log`; inherits log's no-spans/no-fields gap; only adds sink routing, which tracing-subscriber already covers via layers. | rejected: same gap as log |

Structured fields + spans + per-target subscribers + the tokio/axum ecosystem
alignment make tracing the fit; the alternatives each drop spans or fields the
engine already leans on.

### eprintln inventory (follow-up, NOT converted in this arc)

Per scope, R7 did NOT mass-convert existing eprintln. Current count over `src/`:
**223** call sites. Top files: lib.rs (32), scip_setup.rs (30), cli/daemon.rs
(24), setup.rs (13), engine/derive.rs (13), engine/tick.rs (12), engine/repo.rs
(10), cli/query.rs (10). Many are legitimate user-facing CLI OUTPUT (progress,
the --check finding lines themselves, setup prompts) rather than logs; a
conversion pass should first triage output-vs-log before routing the log subset
through tracing. Left as a standalone follow-up.
