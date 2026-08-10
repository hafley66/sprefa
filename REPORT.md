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
