# fix/boop-lane-wait-stale-rows: lane wait must not match a previous lane's result row

## The defect (bit the coordinator twice on 2026-08-10)
`boop beep lane wait <lane>` (and the `--wait` path of lane create) scans
bus.ndjson for a `kind=result` row addressed from the lane and returns its rc.
Result rows are NEVER purged (`lane delete --route-only` removes only the
route), so respawning a lane under the SAME name makes wait match the OLD
run's `lane <id> done rc=0` row and return instantly with a lie.
main.rs:1906 `wait_returns_rc_from_a_preexisting_result_row` currently pins
the buggy contract as intended behavior.

## The fix (design settled, implement exactly this)
Wait gains a since-boundary = the CURRENT spawn's registration timestamp:
1. `lane create` already registers a route row; make sure its registration
   timestamp is recorded on the lane's registry entry (registry.rs rows carry
   `from_timestamp`/`ts` — reuse the existing field if the lane row already
   has one, add it at registration if not).
2. Both wait paths (standalone `lane wait` and `lane create --wait`) resolve
   the lane's route row first. If a route row with a timestamp exists, ONLY
   result rows with row timestamp >= that boundary satisfy the wait; older
   rows are skipped.
3. If NO route row exists (lane finished long ago and its epilogue deleted
   the route), keep today's behavior: any result row satisfies. That is the
   legitimate after-the-fact read and must keep working (main.rs:1954 test).
4. Timestamps are the bus rows' existing string timestamps; compare them the
   way the codebase already orders bus rows (find the existing comparison
   helper before writing a new one).

## Tests (fail-first where the contract changes)
- REWRITE main.rs:1906 into the pair that states the new contract:
  (a) `wait_skips_a_result_row_older_than_the_current_spawn` — old result
  row + newer route registration -> wait times out (124), receipt in the
  test header showing the pre-fix behavior returned rc=0 instantly;
  (b) `wait_accepts_a_result_row_after_the_current_spawn`.
- Keep main.rs:1923, :1941, :1954, :1981 green (adjust :1954's setup to
  include a route row only if the new lookup requires it).
- One test for the no-route-row fallback (contract 3).

## Files owned
v6/boop/src/{main.rs,lane.rs,registry.rs,bus.rs} as needed — nothing outside
v6/boop.

## Validation gate
```bash
cd <worktree>/v6/boop && cargo test
cd <worktree>/v6/boop && cargo build --release
```

## Commit rail (commit-or-report)
- Commit ON THE BRANCH, up to 2 commits, prefix `boop:`.
- Blocked -> FAILURE-REPORT.md at worktree root, exact command + output,
  exit nonzero. NEVER --no-verify.

## Style laws
- Comments only constraints code cannot show; max 2 consecutive lines.
- Banned words prose+identifiers: provenance, substrate, load-bearing,
  regime, refusal.
- No eprintln! in src/**.
