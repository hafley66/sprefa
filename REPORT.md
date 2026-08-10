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

## prolog-dead-trees lane

STOPPED, no deletions made, no commit. Head verified `0a2b42a6` before any
work. Receipts re-verified at that head:

| tree | verified | proof |
|---|---|---|
| v6/prolog/labs/** (4,635 lines, 18 .pl) | CONFIRMED dead | `grep -rn "use_module.*labs" --include=*.pl v6/prolog` empty; broad load-directive search (use_module/load_files/consult/ensure_loaded) empty; justfile labs refs point at `{{v6}}/labs/` (=v6/labs/, a separate dir that exists) |
| v6/prolog/src/emit_ts.pl (239) | CONFIRMED dead | superseded marker at ARCH.pl:194 (brief cited :195, one-line offset) + task emit_ts_direct at ARCH.pl:700; never loaded by any use_module(emit_ts) (compile.pl/sweep.pl/6_profile.pl/plunit resolve to the root emit_ts.pl with emit_program/5, not src/) |
| v6/prolog/src/checks.pl (42) | DEVIATION, blocked | brief proof "marked superseded at ARCH.pl:700" is FALSE: grep for `checks.pl` in ARCH.pl returns nothing; found a live loader: examples/ghcacher.pl:20 `use_module('../src/checks.pl')`, a self-documented runnable example (`swipl -q -l .../ghcacher.pl -g go -g halt`) that also imports kernel.pl and grader.pl |

Per the brief's STOP rule ("a new loader since the recon means STOP AND
REPORT, never delete anyway") and because the cited superseded marker does
not exist, I stopped and made no deletions, ran no gates, created no commit.

Note for resolution: the justfile ghcacher-golden gate runs the .dl6 golden
at v6/tsv2/goldens/ghcacher_tick_golden/6_gate.sh, not examples/ghcacher.pl,
so examples/ghcacher.pl may itself be an orphan. Resolving whether
examples/ghcacher.pl is live (and what checks.pl really is) is the gate to
retrying this lane.
