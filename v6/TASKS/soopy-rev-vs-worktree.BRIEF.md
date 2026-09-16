# Brief: git rev blob walk vs filesystem walk, one distinction, everywhere

Base sha: the spawner prints it. FIRST ACTION `git merge --ff-only <sha>`; failure = stop and
report. Never spawn subagents. PR against `main`. `export CARGO_BUILD_JOBS=3
RUST_TEST_THREADS=4`; `timeout` on every command.

## The user's ask (2026-08-21)
"fix the soopy and extract git rev blob walk vs fs walk distinction, thought we already
figured that out." Find every site where the engine or extract reads files and make the
revision explicit: `Revision::Worktree` (fs walk, `ContentId::Blake3`) vs
`Revision::Commit(sha)` (blob walk, `ContentId::GitBlob`). Never a silent default.

## Known sites (start here, then grep for the rest)
- `v6/sprefa-engine-rs/src/hosts.rs:292` `soopy_files` hardcodes `Revision::Worktree`; a
  demand that names a rev cannot be answered. Add the `rev` input column; `WORK` spells the
  worktree (`change_facts::parse_revision`, `change_facts.rs:76,189`).
- `hosts.rs:137-157` and `:242-259`: `AstRuleInput::Content` takes both ids; check every
  caller passes the one the demand's rev implies.
- `wip/dl6-run-watch-salvage` `executors/watch.rs:5-9`: the watcher emits `Blake3`, the
  enumeration keys by `GitBlob`; the note says only the wake is used. Confirm with a test that
  a worktree edit (no commit) is seen by a `Worktree` demand and NOT by a `Commit` demand.
- `v6/dl/crosswalk/fixtures/grafana.sh` prints `WARN no soopy on PATH; the rev is verified by
  git cat-file only`. A fixture script shelling to git is the exact thing the zero-shell
  decision forbids. The checkout is `soopy_checkout` (`executors/checkout.rs`) and `repo_at`
  (`executors/repo_at.rs`): make `grafana.sh` a thin call into the harness, or delete it and
  let `crosswalk.dl6` seed the three repos as facts. `soopy` binary: build it from
  `~/projects/hafley-rs` (`cargo build --release -p soopy`) and state the path in the PR;
  do not add a PATH dependency.
- `v6/sprefa-extract`: `extract --resolve` reads paths from the fs. Check whether it can take
  `(blob oid, path)` pairs from soopy instead of paths (`src/bin/extract.rs`,
  `src/project.rs:152 resolve_project`). If it cannot, the PR says so with the line and files
  an issue; do not build it here.
- `executors/{repo_at,git_refs,git_history,dep_crawl}.rs` (landed PR #404): audit each for
  the same default.

## Receipts
One test per site: a repo fixture with a committed file and a dirty worktree edit; the
`Commit` walk returns the committed blob, the `Worktree` walk the edit, with both content ids
asserted. `just crosswalk-gate` stays 10/10. `just feature-reach` PASS x3. `cargo test -q`
in the engine 144/0 plus yours.

## Ownership (disjoint)
Yours: `src/change_facts.rs`, `src/executors/{repo_at,git_refs,git_history,dep_crawl,checkout}.rs`,
`src/hosts.rs` soopy_files/AstRuleInput sections only, `v6/dl/crosswalk/**`,
`v6/sprefa-extract/src/project.rs` and `src/bin/extract.rs` (read-path only).
FORBIDDEN: `v6/prolog/**`, `src/run.rs`, `src/runtime.rs`, `src/executors/{clock,watch,pulls,fetch}.rs`,
`v6/tsv2/**`.

## Style laws
No em dashes; banned: provenance, substrate, load-bearing, regime, refusal, "ground truth".
tracing only. Comment budget: constraints only. Failure ledger entry for the silent default.
