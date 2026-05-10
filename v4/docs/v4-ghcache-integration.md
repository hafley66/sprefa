# V4 ghcache Integration

## Implemented Slice

V4 can read PR rows from a ghcacher SQLite database through `gh.prs(...)`.

The database path comes from:

```toml
[daemon]
ghcache_db = "/path/to/gh.db"
ghcache_interval_ms = 500
```

`sprefa-daemon` already accepts the same path through `--ghcache-db`.

## Current PR Query Shape

```sprf
rule(:open_prs, REPO?, NUMBER?, TITLE?, STATE?, HEAD?, BASE?, UPDATED_AT?);

gh.prs(
  REPO: REPO?,
  NUMBER: NUMBER?,
  TITLE: TITLE?,
  STATE: `open`,
  HEAD: HEAD?,
  BASE: BASE?,
  UPDATED_AT: UPDATED_AT?
)
  > open_prs(REPO, NUMBER, TITLE, STATE, HEAD, BASE, UPDATED_AT);
```

Columns:

| Column | ghcacher source |
| --- | --- |
| `REPO` | `repo.owner || '/' || repo.name` |
| `NUMBER` | `pull_request.number` |
| `TITLE` | `pull_request.title` |
| `STATE` | `pull_request.state` |
| `HEAD` | `pull_request.head_ref` |
| `BASE` | `pull_request.base_ref` |
| `UPDATED_AT` | `pull_request.updated_at` |

Keyword args are the supported surface for now. `TERM?` binds the column from
ghcache into the output cursor. A literal like ``STATE: `open` `` filters the
query and still stamps `STATE` on emitted cursors.

## Change Wake Path

Existing ghcache watcher flow:

```text
ghcacher.change_log
  -> poll_ghcache_changes()
  -> dispatch_ghcache_change()
  -> dirty bus
  -> parked query rows
```

PR changes now publish:

| Dirty domain | Key |
| --- | --- |
| `git/repo` | repo slug |
| `git/pr` | domain sweep for all PR query mounts |
| `git/pr` | repo slug + PR number |

`gh.prs` parks its query mount on `git/pr`. A domain sweep wakes the mounted
query, reruns it, diffs the new output set, and emits only new output cursors.

## Retraction

`gh.prs` uses the mounted-query support tables:

```text
mounted_query_mount
mounted_query_dep
mounted_query_output
mounted_query_cursor
mounted_query_support
```

When a PR row changes:

```text
old output cursor hash disappears
  -> old support id removed
  -> downstream rule/fact row removed when support_count reaches zero
  -> new cursor emitted
  -> downstream rule/fact row inserted
```

This is currently covered by `v4/tests/ghcache_query_smoke.rs`.

## Remaining Work

- Add `gh.pr(REPO, NUMBER, ...)` for a single PR.
- Add `gh.reviews`, `gh.comments`, `gh.checkouts`, and `gh.branches`.
- Route `git/pr(repo, number)` keyed wakes when a mounted query is fully bounded
  by repo and number.
- Add LSP completion metadata for `gh.*` column names.
- Decide whether ghcache rows should remain read-only external rows or be
  snapshotted into V4 store tables for offline replay.
