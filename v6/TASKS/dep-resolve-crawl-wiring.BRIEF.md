# Lane brief: wire dep_resolve into the crawl path

First action: `git merge --ff-only 1b90a018`. Failure = STOP AND REPORT.

## Goal

`v6/sprefa-engine-rs/src/dep_resolve.rs` (596 lines, landed PR #286) is
referenced only from `lib.rs`. Wire it into the live crawl path so the
dependency-crawl frontier closure runs through the engine tick and shows up as
authored dl6 relations.

## What exists

- `DepResolveRelations::declarations()` returns 4 relations
  (`dep_resolve.rs:38`): local repos, visited, edges, unresolved.
- `DepResolveOutcome::arrivals()` (`dep_resolve.rs:220`) already produces
  engine `Arrival` rows.
- `LocalRepoRoster::scan_checkout_root` (`dep_resolve.rs:147`) scans a
  checkout root (proven on ~/orgs/grafana/repos: 630 coordinates, closed 0.11s).
- Host executor pattern: `SoopyFilesExecutor` in `src/hosts.rs` (name +
  execution tag match, no child spawn). Follow it exactly.
- Tick schedule: `src/driver.rs` (151 lines).
- The crawl golden: `v6/tsv2/goldens/multirepo_crawl/` (`0_multirepo_crawl.dl6`,
  `2_gate.sh`), pinned four-repo corpus.
- Bench: `v6/tsv2/scripts/crawl-bench.sh` (`--v6-leg`).

## Deliverable

1. A hosts.rs executor arm exposing dep_resolve through the SourceBind-style
   host path; driver.rs schedules it on the tick.
2. dl6 spelling: extend `goldens/multirepo_crawl/0_multirepo_crawl.dl6` (or a
   sibling golden dl6 + gate under the same dir) so the dep_resolve relations
   are declared and queried from authored dl6. No new language surface: use
   existing `rel` declaration forms only. If a construct you need does not
   compile, cite the manifest bucket (`v6/prolog/compile/out/manifest.json`)
   and stop that sub-piece with a written note.
3. A crawl-bench leg (or extension of `--v6-leg`) that exercises the wired
   path over the pinned corpus and prints row counts.
4. Rust tests in `tests/dep_resolve.rs` extended for the host arm (fake roster
   dir fixture, no network).

## Receipts (run each three times, never once)

```bash
cd v6 && just multirepo-golden
cd v6/sprefa-engine-rs && cargo test dep_resolve
```

`grade.sh` cannot run in slash-nested worktrees; cite diff-scope proof
instead. Commit with `COMMENT_RAIL_IDLE_MS=3000 git commit ...` and NEVER pipe
a commit through tail/head. Check `git log` before finishing.

## File ownership

OWNS: `v6/sprefa-engine-rs/src/dep_resolve.rs`, `src/hosts.rs`,
`src/driver.rs`, `src/lib.rs` (module wiring lines only),
`v6/sprefa-engine-rs/tests/dep_resolve.rs`,
`v6/tsv2/goldens/multirepo_crawl/**`, `v6/tsv2/scripts/crawl-bench.sh`,
`v6/justfile` (crawl leg lines only).

FORBIDDEN: `v6/sprefa-engine-rs/src/text_plane.rs`,
`src/source_bind/**`, everything under `v6/prolog/`, root `src/**`.
Another lane owns text_plane concurrently.

## Laws

- No `eprintln!` in src/**; `tracing` only.
- N+1: never a per-row write; collect the set, one insert call.
- Surrogate INTEGER keys; no composite TEXT PKs in any DDL.
- Comment budget: comments state only constraints code cannot show.
- Language vocabulary: rxjs/prolog/SQL words only; "support" banned.
- dl variable names descriptive, never single-letter.
- Working around a blocked command is a defect; a permission denial ends the
  approach. Report instead.
