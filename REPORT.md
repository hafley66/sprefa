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
