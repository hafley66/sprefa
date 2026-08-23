---
created: 2026-08-23
updated: 2026-08-23
type: improvement
reporter: hafley66
status: done
priority: normal
labels: [engine, performance]
closed: 2026-08-23
commits:
- hash: 5209db5c8
  summary: 'Merge PR #429 inner scan audit, 19 guard-column indexes'
---

# Inner SCAN audit: DL_EXPLAIN shows 215 distinct statements with a SCAN inside the join on ghcache

_Source: v6/sprefa-engine-rs/src/sql.rs explain_once; v6/prolog/lower.pl_

## Description

Run: compile ghcache.dl6 through emit_rust, then DL_ADAPTERS_DIR=v6/dl/ghcache DL_EXPLAIN=1 RUST_LOG=sprefa_engine_rs::explain=info emit_rust_harness <rs> v6/dl/ghcache/ghcache.schedule.json --final. 739 distinct statements, 215 scan=true. Classify every scan=true plan into: (a) json_each virtual table scans (expected, the spread IS a scan of the array), (b) correlated scalar subqueries over __ref_* dictionaries that SEARCH by rowid (fine), (c) a real inner SCAN of a base or frontier table after a join (a missing index or a join order defect). For (c): the emitted DDL index set in lower.pl is the fix site; each fix carries an additive EXPLAIN test asserting SEARCH (tests/shared_frontier.rs:119 is the pattern). Report the (a)/(b)/(c) counts in the PR body; if (c) is zero, the PR is the report plus the classification rule baked into explain_once (scan=true only for (c)).
