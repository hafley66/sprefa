# Lane: `boop beep lane create --await`

## Base
`git merge --ff-only e70417d9` is your FIRST action. Failure = STOP AND REPORT.
Worktree: `.boop-worktrees/feature/boop-lane-create-await`.

## The ask, from the user

"you should be able to make a boop lane create --await in 1 breath, and await
can be a subcommand but --await when combined with create is intended usage
with &".

Today a coordinator must run two commands:

```bash
boop beep lane create --branch x --brief b.md --goal g --preset sol
boop beep lane wait x &          # separate breath, easy to forget
```

Wanted:

```bash
boop beep lane create --branch x --brief b.md --goal g --preset sol --await &
```

`--await` makes `create` spawn the lane and then block on the same result row
`lane wait` blocks on, exiting with the lane's rc. The caller backgrounds it
with `&`. The existing `wait` subcommand stays exactly as it is.

## Why this matters, measured today

A coordinator spawned five lanes and never armed a single `lane wait`, so five
lanes finished silently and the user had to ask whether they had started. The
two-command shape is the defect; one command that already blocks cannot be
forgotten.

Worse, `~/.agent/mail/bus.ndjson` holds **171 messages addressed to
`sprefa-coordinator` and 0 delivered**, every one with `to_timestamp: null`.
`--await` is the path that works today without solving delivery.

## Scope

1. `--await` on `lane create`. Spawn, then block on the result row, then exit
   with the lane's rc.
2. Reuse the `wait` implementation. Do not write a second waiter.
3. `--timeout <seconds>` must work with `--await` the same way it works on
   `wait`, exiting 124.
4. `--dry-run` with `--await` prints the spawn line and does not block.
5. Print the lane name and tmux session to stdout BEFORE blocking, so a
   backgrounded caller sees the route immediately.
6. `--await` combined with `--detach`-style flags, if any exist, is an error
   with a named message rather than silent precedence.

## Anchors
- `boop beep lane --help` lists: list, create, run, get, patch, delete, prune,
  route, pane, message, wait
- `wait` today: "Wait for the lane's result row, then exit with the rc it names.
  `--timeout` seconds exits 124; a row that already exists returns its rc
  immediately"
- the store is plain SQLite at `~/.agent/boop.db`
- the spawn line `create` prints today ends with
  `boop hail --to '<parent>' ... --body "lane <name> done rc=$__rc"` then
  `boop beep lane delete '<name>' --route-only`
- presets live in `~/Library/Application Support/boop/config.json`:
  flash4, pro4, terra, luna, sol

## Laws
- boop NEVER reinvents SQLite or SQL. `boop db "<sql>"` is the query surface.
- Infra is bought, never built. No bespoke polling loop where a blocking
  primitive or an established crate exists.
- The 10-second law binds spawn, never the await itself.
- No `eprintln!` in src/**, `tracing` only. CLI-UX lines carry `@eprintln-ok`.
- Comments state only constraints the code cannot show. No dates, no narrative.
- No em dashes. No negative parallelism. No sycophancy.
- Banned in prose AND identifiers: provenance, substrate, load-bearing, regime.

## Files you own
`v6/boop/**`, plan doc `plans/2026-08-12-boop-lane-create-await.md`.

## Files you must NOT touch
`v6/prolog/**`, `v6/sprefa-engine-rs/**`, `v6/justfile`, any Cargo.toml outside
`v6/boop/`. Four other lanes are live and own those.

## Gates
`cargo test --no-fail-fast` in `v6/boop`, three runs. Two failures pre-exist:
`lane::tests::a_gpt_model_names_the_codex_harness` and
`lane::tests::an_unnamed_harness_never_guesses_opencode`.

Add a test that `create --await` returns the lane's rc, and one that
`--timeout` exits 124.

## COMMIT YOUR WORK
Six lanes today wrote their whole deliverable and exited rc=0 WITHOUT
COMMITTING. Commit on the branch before you exit. An uncommitted tree is an
undelivered lane.

## Report
The new `--help` text for `create`, the exact one-breath command line, and the
test counts.
