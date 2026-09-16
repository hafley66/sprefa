# concatmap gaps: round 2

Repo: hafley-rs. Branch `fix/concatmap-one-pass-windows`, worktree already exists.
Base sha: 80cab60 (round 1, already committed on that branch).
FIRST ACTION: `git merge --ff-only 80cab60`. If that fails, STOP AND REPORT.

Round 1 landed and its three receipt tests pass. Two gaps remain. Fix only these.
Do not restructure round 1's work. Do not revisit passes_until_fixed, --cap,
coalesce defaults, or the window `ts` column: all correct already.

Files you own: `crates/boop/src/concatmap.rs` and its tests only.
Touch NOTHING else. Forbidden: query.rs, main.rs, harness.rs, every other crate.

## Gap 1 (blocking): the done-marker recheck never reads the disk

`poll_once` in `crates/boop/src/concatmap.rs` now does, inside the retry ladder:

```rust
if done.contains(&key) {
    break;
}
```

`done` is an in-memory `BTreeSet` this process alone mutates (`write_done_marker`
is the only writer). The defect this was meant to close is a HUMAN planting a
marker file into `<state>/done/` while the resident is mid-flight on a poisoned
bundle. That file never enters `done`, so the check skips nothing it did not
already know.

Required:

- a helper that stats the marker path, e.g.
  `fn marker_planted(state_dir: &Path, session: &str, id: i64) -> bool` reading
  `state_dir.join("done").join(format!("{session}-{id}"))`.
- call it before EACH attempt in the retry ladder, and once before starting each
  job in the `for job in &jobs` loop, so a marker planted while an earlier job
  hangs skips the later jobs of the same tick.
- keep the in-memory `done` check as the cheap first test; the stat is the
  second test, and a planted marker also gets inserted into `done` so the loop
  stops re-statting it.

Receipt (new test, name it `a_planted_marker_skips_the_bundle_mid_flight`):
build the two-turn window fixture the existing
`poisoned_bundle_times_out_and_the_next_window_still_processes` test uses, and
have the fake harness's FIRST `one_shot` call write the marker file for the
SECOND window's id into `<state>/done/` before returning. Assert the harness saw
exactly ONE call and the second window's out file was never written.

## Gap 2 (small): the `--from-start` seed has no test

Receipt (a) in the original brief names `--from-start`. `seed_cursor` is
untested; `oneshot_window_maps_each_window_exactly_once` passes cursor 0 to
`poll_once` directly and never exercises the seed.

Required (new test, name it `from_start_and_explicit_cursor_beat_the_persisted_seed`):
write a `cursor` file holding a large ts into a temp state dir, then assert

| args | `seed_cursor` returns |
|---|---|
| `from_start: true` | 0 |
| `cursor: Some(7)` | 7 |
| neither | the persisted value |

## Validation, run and paste output

```bash
cargo test -p boop
cargo build --release -p boop
```

COMMIT YOUR WORK on the branch in the worktree before you exit. Round 1 exited
rc=0 having written every file and committed nothing; that counts as an
undelivered lane. Never commit to main. Never push.

Style: no `eprintln!` additions beyond the existing CLI-UX lines; comments state
only constraints the code cannot show.
