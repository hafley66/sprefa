# fix/boop-registry-kinds: registry rows get a kind, pane-less rows stop reading as corpses

## Ruled by user 2026-08-11 (fix the bugs from the prune smoke test).

## The defect, measured
`boop beep lane prune --dry-run` on the live mailbox wants to prune
`sprefa-coordinator projects:0.0 tmux session gone, no pid recorded` — the
registry row every lane routes its completion hail to. Coordinator rows are
pane-less by nature; so are native (in-harness) subagents. Also measured:
`boop hail --to sprefa-coordinator` appends its ledger row fine, then errors
`tmux send-keys failed: can't find session: projects` — delivery failure to a
pane-less row prints as an error when it should be a normal outcome.

## The fix
1. Registry rows gain a `kind` field: `lane` (default, backward compatible
   with rows that lack the field), `coordinator`, `native`. Serde default so
   existing registry.json files parse unchanged.
2. `lane prune` only considers rows with kind `lane` (the two-layer liveness
   from PR #147 stays as is for those). Dry-run and real-run line output now
   include the kind.
3. `hail` to a row with no live pane and kind coordinator/native: append the
   ledger row, print `queued <id> -> <name> (no pane)`, exit 0. Keystroke
   injection is only attempted for rows that have a pane. For kind lane with
   a dead pane the current error behavior stays.
4. New verb `boop beep agent register <name> [--kind coordinator|native]
   [--parent <name>] [--mail-dir <d>]`: writes a pane-less registry row so
   native harness subagents and coordinators become visible to `lane list`
   and the strip. `boop beep agent done <name> [--rc <n>]` appends the same
   result-row shape lanes emit (`lane <name> done rc=<n>`) and removes the
   row. Keep both thin: registry + ledger writes only, no process handling.
5. Tests, fail-first, following the crate's existing test conventions:
   (a) prune skips a kind=coordinator row whose tmux session is gone;
   (b) old registry.json without kind fields still parses and prunes;
   (c) hail to a registered pane-less native row exits 0 and appends;
   (d) agent register/done round-trip shows up in and leaves the registry.

## Files you own
v6/boop/ only. Read src/main.rs (the prune verb from PR #147 is the pattern
for verb structure), src/tmux.rs, the bus/registry module. Follow existing
style exactly.

## Gate
```bash
cd <worktree>/v6/boop && cargo test && cargo build --release
```
Plus a manual smoke against a temp --mail-dir showing 2 and 3, output pasted
into the commit message or report.

## Commit rail (commit-or-report)
Up to 2 commits, prefix `boop:`. Blocked -> FAILURE-REPORT-BOOP-KINDS.md,
exact command + output, exit nonzero. NEVER --no-verify. Pre-commit hook
fails >2 consecutive comment lines in any touched hunk; one-line comment
edits only.

## Style
Comments state only constraints the code cannot show. Banned words, prose
and identifiers: provenance, substrate, load-bearing, regime, refusal.
6. Validate --brief at create time: refuse a path that does not exist or is not absolute (measured 2026-08-11: a relative brief path spawned a lane whose prompt cat failed silently, rc=1, self-cleaned registry row, classic silent death).
