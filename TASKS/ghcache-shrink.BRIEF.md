# ghcache-shrink

Goal: the ghcache program polls what a user OR an org account actually offers, backs off endpoints that keep failing, bounds its own logs, and drops every call the receipts say is wasted. Base: worktree at or past 641062112033005ae78f4bc6c068018b423cb2c8. Branch fix/ghcache-shrink. PR to main.

## Measured defects (fail-pre-fix receipts, from ~/.agent/dl6.db 2026-08-24)
1. `users/hafley66/events/orgs/hafley66` 404'd 1,422 times in 23.8 h, one per poll cycle, authenticated, rate pool decrementing. Cause: hafley66 is a USER account; that endpoint exists only for orgs. The repos poll already falls back (ghcache.dl6:265 org form, :270 user form, 24 org 404s then user path); the events poll (:286) has no user-account form.
2. Nothing backs off a permanently-failing endpoint: 404 -> re-demand next cycle, forever.
3. `log keep(all)` on telemetry: engine_tick_cost 132,571 rows AND its __host_response twin; change_log 31,811; call_log 15,280.

## Deliverables
1. Org/user account split for events: user accounts poll `users/<owner>/events` (exists for users), orgs keep the org form; same fallback shape as repos. Watched-set semantics unchanged: hafley66 stays watched WHOLE (CLAUDE.md user decision); only the endpoint spelling adapts to account type.
2. 404 backoff as rules (rx idiom retry/backoff): after K consecutive 404s on an endpoint, stop demanding it for a cool-off of buckets; a 200/304 resets the streak. Pick K and cool-off, state them in the PR body, encode as plain rels (no engine change).
3. Retention: keep(count(N)) on engine_tick_cost, change_log, call_log (keep(count) lowering exists, lower.pl:6610-6629). Pick Ns for roughly 24 h of history at current rates (state the arithmetic); pr_transition stays keep(all).
4. Call audit, numbers in the PR body: every endpoint x cadence from watched_endpoint/watched_global rules, calls/day each, which are stretchable (branches hourly instead of every minute? events unchanged?) with a recommendation table. IMPLEMENT only the two rows above; cadence changes are listed, not landed, unless trivially safe (say why).

## Receipts
- gate.sh ticks=14 pr_transition_open_merged=1 (schedule fixtures updated only if the demand shape forces it; say which lines).
- New conformance-style receipt or golden covering the user-account events path and the backoff (fail-pre-fix: the 1,422x404 numbers above go in the test header).
- Full gate: conformance 445/0, plunit (current)/0, grade 445 byte-clean=341, cargo /0, goldens 6, ARCH 7/0. Ledger entry (next free number). Live proof optional but welcome: restart the resident run and show the 404 stream stops.

## You own
v6/dl/ghcache/ghcache.dl6 + its schedule/goldens, docs/failure-modes.md, one ARCH row if named. Forbidden: v6/prolog/**, v6/sprefa-engine-rs/src/**, other v6/dl programs.

## Style laws (CLAUDE.md)
rxjs/prolog/SQL vocabulary only; no em dashes; banned words: provenance, substrate, load-bearing, regime, ground truth (oracle), refusal, support (refCount). dl variable names descriptive. Batteries in background with timeout; no foreground wait over 10 s. Commit per deliverable; PUSH before reporting.

Done: boop beep hail sprefa-coordinator --from <lane> --body "PR #<n>: numbers". Blocked: one line, stop.
