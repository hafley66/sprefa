---
created: 2026-08-21
updated: 2026-08-21
type: feature
reporter: chris
assignee: chris
status: open
priority: normal
epic: cheap-fast-analysis
---

# Cross-repo crawl on the Rust door: crosswalk

## Description

Track cross-repository logic, flows and paths at a pinned revision, with the
entry points declared by hand rather than guessed, and watch the result in
background without unbounded RSS.

The four golden programs under `v6/tsv2/goldens/multirepo_crawl/` declare the
host families this needs and every one of them ends in `exit 3`: no executor in
`v6/sprefa-engine-rs/src/hosts.rs` answers `repo_grep_at`, `dep_crawl_*`,
`git_ref`, `git_tag`, `git_merge_base`, `git_ahead_behind`, `git_ancestor`,
`git_change`, `git_rename` or `git_changed_line`. The mechanics already exist
in-process (`src/dep_resolve.rs`, `src/change_facts.rs`); nothing routes them.

## Scope

| deliverable | where |
| --- | --- |
| four executors | `v6/sprefa-engine-rs/src/executors/{git_refs,git_history,repo_at,dep_crawl}.rs` |
| one routing hunk | `src/hosts.rs` `executor_for` |
| adapter sidecars | `v6/dl/crosswalk/adapters/` |
| synthetic gate | `v6/dl/crosswalk/gate.sh`, `just crosswalk-gate` |
| real org fixture | `v6/dl/crosswalk/fixtures/grafana.{tsv,sh}` |
| the program | `v6/dl/crosswalk/crosswalk.dl6` |
| watch RSS series | measured, reported in the PR |

## Out of scope

`v6/tsv2` (paused), `v6/prolog` (compiler lane), `src/serve.rs` (peer lane).
Registry rows and soopy signatures this needs go to their owners as requests.
