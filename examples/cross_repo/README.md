# cross_repo

dl8 queries that span more than one checkout. A root is a fact row, so a
second repo is one more `Repo` or `Crate` row.

```bash
dl8 query 0_commits_per_repo.dl7 --repo .
dl8 query 1_calls_across_crates.dl7 --repo .
dl8 query 2_who_touches_std_fs.dl7 --repo .
```

`dl8` comes from `cargo build` in the sprefa root. `1_` and `2_` need `ryi` on PATH.

## 0_commits_per_repo

```
(Commits "hafley-rs" 2412)
(Commits "sprefa" 6807)
(Commits "sqlite_ivm" 404)
```

`git rev-list --count HEAD` gives 2415, 6808, 406. The gap is root commits,
which have no parent edge.

## 1_calls_across_crates

```
(Calls "ghcache" 607)
(Calls "soopy" 1985)
```

`CallsOpen` lists 190 call sites of functions named `open` across both crates.

## 2_who_touches_std_fs

Functions that call `std::fs` or `std::path`, or call something that does.
A sprefa-extract function that calls `soopy::...::name` counts when that soopy
function touches.

```
(TouchCount "soopy" 416)
(TouchCount "sprefa-extract" 1411)
```

A function's extent comes from `resolved_edge.callee_start`/`callee_end`, so a
function nothing calls has no extent, and its direct `std::fs` calls are missed.
