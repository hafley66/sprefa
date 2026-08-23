# inner-scan-audit: classification and general-rule record

Labs die on landing; this file is the durable record, the raw DL_EXPLAIN logs
do not land. Reproduce with the issue's run line:
`DL_ADAPTERS_DIR=v6/dl/ghcache DL_EXPLAIN=1 RUST_LOG=sprefa_engine_rs::explain=info
emit_rust_harness <compiled ghcache.rs> v6/dl/ghcache/ghcache.schedule.json --final`.

## Pre-merge baseline (739 statements)

215 `scan=true`, classified by hand into three shapes:

| class | count | shape |
|---|---|---|
| json_each | 151 | `SCAN j0 VIRTUAL TABLE INDEX 1:` spreading a JSON array column |
| ref_subquery | 0 direct (169/215 co-occur) | `SEARCH s USING INTEGER PRIMARY KEY (rowid=?)`, a dictionary lookup, never itself a SCAN |
| inner | 64 | a real SCAN of a base/frontier/temp table after the driving table |

Of the 64 inner-SCAN statements, ~50 are structurally forced (recursive
CTE, `GROUP BY` aggregate co-routine, a bare `EXISTS(SELECT 1 FROM t)` with
no predicate to seek on, or a 1-2-row config/fact table) and stay
unindexable. 3 were real missing-index defects on a non-leading UNIQUE-key
column: `pr_batch_response.status`, `rest_response.status`,
`checkout_task.fetch_pr_branches`.

## `origin/main` moved mid-audit

PR #427 (one-tick-path, f6627d9da) rewrote `incremental.rs` and deleted
`ordered.rs`. Re-running the same line on the merged tree: 1232 statements.
`scan_kind` (this PR's own rule, landed in `sql.rs::explain_once`) reads the
refreshed counts straight off the log, no manual reclassification needed:

| scan_kind | count |
|---|---|
| json_each | 261 |
| ref_subquery | 92 |
| inner (scan=true) | 234 |
| none | 645 |

## The fix: a general rule, not three rel names

`lower.pl:audit_scan_index_pairs/5` derives (rel, column) pairs from the
program's own rules — no rel name lives in the compiler
(`.claude/skills/sprefa-v5-no-magic-rels`). A stored (`set`) rel's
non-leading-key column earns a dedicated index when some rule body filters
it by identity: either an `==` guard (`Status == 200`) or an inline literal
argument (`checkout_task(RepoSlug, DestRoot, WantSha, 1)`), both compiling
through `compile_atom_args` to the same `WHERE col = ?` shape a composite
UNIQUE key can't seek unless the column is its leading term.

Run against ghcache.dl6 (merged tree), the general rule finds **19**
`(rel, column)` pairs: the 3 the audit named by hand, plus 16 more the rule
finds structurally that a manual read of one schedule's EXPLAIN output
didn't surface (a column can be a real defect without that particular run
exercising the statement that filters it). All 19:

```
branch_pattern.pattern            notification.unread
candidate_branch.branch_name      org_config.sync_events
checkout_task.fetch_pr_branches   page_arrival.status
global_setting.sync_notifications page_response.status
poll_endpoint.endpoint_kind       pr_batch_response.status
pr_review.state                   pull_request.state
repo_event_seen.event_type        rest_response.status
watched_endpoint.endpoint_kind    watched_global.endpoint_kind
watched_repo.checkout_on_sync     watched_repo.sync_events
watched_repo.sync_prs
```

Verified safe: `grade.sh` stays `graded=444 byte-clean=340` and the ghcache
tick log stays `ticks=14 pr_transition_open_merged=1` with all 19 indexes
present — an index changes no row, only which access path reaches it.

## Test coverage

- `sql.rs::classify_plan` unit test: three literal plan texts pin json_each /
  ref_subquery / inner.
- `tests/explain_inner_scan_audit.rs`: 3 additive EXPLAIN tests assert
  SEARCH on `pr_batch_response.status`, `rest_response.status`,
  `checkout_task.fetch_pr_branches` (literal DDL + a synthetic query shape,
  independent of the general rule finding more columns in the real program).
- `plunit_tests.pl:audit_scan_index_ddl`: 4 tests against tiny inline
  programs — a guard-filtered non-leading column earns an index, an
  inline-literal-filtered one does too, a leading-key column earns nothing,
  an ordered (`>`) comparison earns nothing.
