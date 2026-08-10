## codex-findings lane

- Commit: `3ea35633` (`boop: PASS: wire domain gap paths`)
- Schema: version 7. `sync_cursor` now stores dictionary-backed `record_id`,
  `turn`, and `timestamp`; existing version 6 stores migrate on open.
- Observation receipt: `sync_session()` records `live` or `idle` status and
  repeats fold without inserting another interval.
- Cursor receipt: the Claude fixture returns a non-empty record ID and nonzero
  turn and timestamp.
- Temporal edges: hail, result, retry, resume, and cancel messages call
  `add_edge_at()`.
- Typed rows: `LiveSpanRow` is returned by both live-span queries. `StatusRow`
  includes all Instant contract fields.

StatusRow fields with no source in the current store are `lane`, `rss_kb`,
`cpu_pct`, and `uptime_sec`; they return `None`. `state`, `pid`, `tmux_pane`,
`first_seen_ts`, `last_seen_ts`, and `died_ts` are sourced from stored live
state and intervals. `lane` has no lane-registry join in the store schema.

Receipts from `v6/boop`:

```text
cargo test
test result: ok. 91 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

cargo clippy -- -D warnings
Finished `dev` profile
```

The initial sandboxed test run could not create throwaway tmux sockets. The
full receipt passed with tmux access. No requested path resisted.

## boop-goal-edge lane

Job 1 (`e5f4a899`) `boop: lanes carry a goal on the route and dispatch mail`:
- `Route.goal: Option<String>` added in `v6/boop/src/bus.rs`; parsed in
  `route_from_value`, written in `route_to_json` (main.rs). Missing field loads
  as `None` via the manual `string_field` path (Route is not serde-derived; the
  brief's "serde default" is honored behaviorally).
- `--goal` wired into `lane create`, `dispatch --goal`, `adopt`, and
  `lane patch`; therefore into `DispatchArgs`, `LaneArgs`, `run_adopt`.
- Dispatch mail (kind=dispatch) embeds the goal in its body as
  `\n[goal] <text>`, so history states it without a registry lookup.

Job 2 (`86150f0d`) `boop: pstree prints the lane goal`:
- `LaneNode.goal` threaded from the route through `build_lane_nodes`.
- Text lane line gains a ` -- <goal>` suffix when present.
- `--format ndjson` rows gain `"goal": <string|null>`.

Tests (new, all pass): `route_goal_round_trips`, `a_route_round_trips_its_
goal_field`, `a_registry_without_the_goal_field_still_loads`, `pstree_carries_
the_goal`, `pstree_goal_null_when_absent`.

Receipts from `v6/boop`:

```text
cargo test
98 lib + 9 main + 2 bench = 109 ok (baseline 104, never drops)

cargo clippy -- -D warnings
clean (0 warnings / 0 errors; pre-existing linker_messages warning only)
```

Resisted note (corrected by the driver at harvest): the lane stalled on the
pre-commit comment-budget rail, which needs the `v6/sprefa-extract` release
binary at `<worktree>/v6/sprefa-extract/target/release/extract`
(`v6/tsv2/scripts/comment-budget-rail.sh:16,19-20`). The lane did NOT build or
bypass it and could not commit. The driver symlinked the main tree's existing
binary into the worktree; the rail then returned rc 0 and the lane's next
commit attempt succeeded. Nothing else resisted.

Driver verification in the worktree, independent of the lane's own run:

```text
cargo test          109 ok (98 lib + 9 main + 2 bench), 2.35s
cargo clippy --all-targets -- -D warnings    rc 0
comment-budget-rail.sh                       rc 0
```

Live pstree receipt against a throwaway `--mail-dir`:

```text
text:   demo-lane (-) [dead] -- prove the goal edge renders
ndjson: {"lane":"demo-lane",...,"goal":"prove the goal edge renders",...}
real registry: every row carries "goal":null
```

## boop-store-close lane

Agent key: `agent_pr`, StatusRow sources, sync cost gate + `agent_live.pid`,
`beep lane wait`. Head was `29071a5b` (boop-goal-edge beneath). All in v6/boop.

Job 1 (`22cdfaf8`) `boop: agent_pr PK collapse to (session_id, turn, pr_url_id)`:
- `agent_pr` PRIMARY KEY changed from `(session_id, turn)` to
  `(session_id, turn, pr_url_id)`: a turn mentioning two PRs keeps two rows.
- SCHEMA_VERSION 7 -> 8; the migrate-on-open path now runs per version step, so
  a v7 store that already applied the 6->7 record columns does not re-run those
  ALTERs. The 7->8 step rebuilds `agent_pr` (create new, copy, drop, rename).
- Tests: `two_prs_in_one_turn_survive_and_resync_dedups`,
  `a_v7_store_migrates_agent_pr_onto_the_three_column_key`.

Job 2 (`e21a951c`) `boop: beep ps and db status report tree-summed rss/cpu`:
- `proc.rs` gains `TreeSum`, `tree_sum_of`, and `uptime_secs`. `beep ps` and
  `measure` sum rss_kb/cpu_pct across the pane pid's descendant tree and print
  `now - start_time` (a duration) not the epoch start.
- `status_rows` (query.rs) replaces the hardcoded `NULL AS rss_kb/cpu_pct/
  uptime_sec` with the same tree-sum path when a live pid exists.
- Lane join: `StatusRow.lane` stays `None` in the store row; `db status`
  (main.rs) joins the lane by route session_id OR cwd, the clean join, never a
  guess. Verified against the goal-edge lane's machinery.
- Tests: `tree_sum_exceeds_pane_only_on_a_fixture_tree`,
  `uptime_is_a_duration_not_an_epoch_stamp`, `live_pid_status_row_carries_
  nonzero_rss`.

Job 4 (`16b254a0`) `boop: gate sync re-reads on cursor length and lane pane pid`:
- `run_sync_all` now skips any transcript whose length still equals its
  consumed cursor (metadata.len -vs- `sync_cursor.offset`), the run_follow
  freshness law. Cursor offsets load in one batched query
  (`all_cursor_offsets`). A shorter file keeps the truncation path.
- `sync_session_with_pid` stores the lane route's pane pid on `agent_live`;
  `session_route_pid` resolves it by session id or cwd. Test:
  `an_observed_live_lane_row_carries_its_pid`.
- Cost receipt, `boop db sync create`, release binary, same real store:

```text
before (brief, ungated):  11,331 ms  (24 events, ~2/s)
after  (gated warm):       2,456 ms  (21 events, ~8/s)
```

The residual ~2.3s is harness discovery: `first_record_context`
(v6/boop/src/harness/claude.rs:222) calls `read_complete_lines(file, 0)`,
which reads the whole 2.86 GB corpus on every run. That lives in
`src/harness/**`, where this lane is forbidden, so the gate alone cannot reach
the 0.023 s walk floor. Reported rather than improvised into harness.

Job 3 (`961bc5b6`) `boop: add beep lane wait`:
- `beep lane wait <lane>` polls the mailbox every second for a `kind=result`
  row from the lane, exits with the rc its body names (`lane <id> done rc=N`);
  `--timeout` seconds exits 124, a pre-existing row returns immediately. Reads
  the same bus.ndjson bus.rs writes, no new mailbox format.
- Smoke-verified exit codes 5 (result) and 124 (timeout).
- Tests: `wait_returns_rc_from_a_preexisting_result_row`,
  `wait_times_out_when_no_result_row_arrives`, `a_non_result_row_does_not_
  satisfy_the_wait`.

Receipts from `v6/boop`:

```text
cargo test          118 ok (104 lib + 12 main + 2 bench), baseline 109, never drops
cargo clippy --all-targets -- -D warnings   rc 0
```

Resisted: the compute cost of `beep ps`/`status_rows` by spawning a fresh
sysinfo snapshot per row (it is captured once and queried many times).
