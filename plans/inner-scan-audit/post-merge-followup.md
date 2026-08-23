# inner-scan-audit: post-merge follow-up

`origin/main` moved under this PR (#427, one-tick-path landed, f6627d9da):
`incremental.rs` was rewritten and `ordered.rs` deleted. Re-running the
issue's DL_EXPLAIN line on the merged tree changed the statement set from
739 to 1232 distinct statements.

`scan_kind` (this PR's own rule, already landed and tested) reads the
refreshed run directly:

| scan_kind | count |
|---|---|
| json_each | 261 |
| ref_subquery | 92 |
| inner (scan=true) | 234 |
| none | 645 |

The 3 fixed rels (`pr_batch_response.status`, `rest_response.status`,
`checkout_task.fetch_pr_branches`) still emit their index and still resolve
to SEARCH where their statements appear in the merged run
(`plans/inner-scan-audit/explain-post-merge.log`).

A quick pass with the same non-leading-key-column heuristic used for the
original 3 surfaces roughly a dozen more candidate (rel, column) pairs
introduced by the one-tick-path rewrite (`ghcache_poll_endpoint`,
`ghcache_dirty_repo`, `ghcache_watched_repo`, `ghcache_org_owner`,
`ghcache_pull_request`, and several with only one occurrence). None of
these are verified: the original 3 fixes each needed a literal-plan-text
empirical check (`sqlite3 EXPLAIN QUERY PLAN` before/after `CREATE INDEX`,
plus a UNIQUE-key-order read) before being trusted, and the same rigor
hasn't been applied here. Classifying and fixing them is follow-on work,
sized similarly to this PR's item 3, not a same-day addition.
