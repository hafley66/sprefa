# Agent resume ledger — 2026-07-11 hook-deadlock stoppage

All four in-flight agents were stopped mid-work because the PostToolUse
`dl --check` hook (error-severity tick-over-budget -> exit 2) blocked every
Edit/Write repo-wide. Their worktrees and transcripts are intact; each resumes
with full context via a message to its agent id (the orchestrator session has
the ids pinned; resuming = SendMessage "resume: hook unblocked, continue your
brief").

| agent | worktree branch | brief | state when stopped |
| --- | --- | --- | --- |
| Cross-harness arc 1 (a305bb99e664ff9b8) | worktree-agent-a305bb99e664ff9b8 | hooks dialect seam + .agents/skills unification; codex = claude-alias dialect, .codex/hooks.json wiring (schema verified against embedded binary schemas); opencode plugin | mid-build: "Now the opencode plugin asset" |
| Instrumentation (abe1076d17acbc772) | worktree-agent-abe1076d17acbc772 | _stmt_ms max->sum+passes fix + cache pragma measurement (NB: cache pragma since landed on main 33f549b — drop that half, rebase over main) | mid-design of the timing wrapper |
| Plans TODO index (a24dbf19dff474101) | worktree-agent-a24dbf19dff474101 | comment_node .md support + examples/gen-plans-index.dl -> PLANS.md + sprf-write-plan skill; generic-example amendment received | mid-wiring: markdown into comment_file_set/refresh_comment_rels |
| Slow-rule factoring (a12cb6c43c97a748c) | worktree-agent-a12cb6c43c97a748c | factor entry_reach_node_raw / call_node / flow_edge through intermediate rels, row-equivalence proof | unknown (stopped without a last note) |

Resume order when ready: instrumentation (rebase over 33f549b first; NB the
max-vs-sum _stmt_ms fix may be superseded by the semi-naive branch — check
before resuming) -> factoring (same check: semi-naive may deflate its targets)
-> plans-index -> cross-harness (LAST-BUT-DECIDED: resumes after perf lands so
its hook e2e runs at sane tick speed). All carry the no-subagents rule and the
sprefa-feedback closing section.

OWNERSHIP DECISION (Chris, 2026-07-11): the engine/mod.rs split
(plans/2026-07-11-engine-mod-split.md) goes to CODEX after the perf arc lands —
not to a Claude agent. The plan file is the codex brief.

## Never again (the hook deadlock class)

What happened: hook = bare `dl --check`; a perf diag at ERROR severity made it
exit 2 on every write; with the daemon wedged/stale each hook run ALSO paid a
cold engine, stacking 40s processes. Writes were blocked repo-wide for every
session and agent at once.

Guards to land (sequenced, first two are one-liners):
1. Hook command hardening: `.claude/settings.json` hook becomes
   `timeout 10 dl --check --quiet-on-timeout || true`-shaped — a PostToolUse
   ADVISORY must never be able to block writes on perf grounds; only true
   parse/type errors should exit 2. (settings.json currently DISABLED —
   .claude/settings.json.disabled; re-enable only with this shape.)
2. Severity policy: perf rails (tick-over-budget et al.) stay warning-severity
   in checked-in .dl (done, 33f549b); error severity is reserved for
   correctness findings. A rail wanting exit-2 goes in CI, not in the
   editor-write hook.
3. `dl --check` gets a `--max-wall <secs>` self-deadline (loud partial report,
   exit 0 with a `check-timed-out` warning diag) so NO caller can stack
   unbounded cold engines.

<!-- todo(bug): dl --check --max-wall self-deadline so hook callers can never stack cold engines -->
<!-- todo(feature): re-enable PostToolUse hook with timeout+advisory shape once derived is under budget -->
