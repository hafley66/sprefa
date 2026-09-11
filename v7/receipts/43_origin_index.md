# Origin index candidate rejected

Date: 2026-09-11. The current main commit was `aae9e7d29`. Only a temporary
checker candidate was applied and then removed. The final source and focused
test paths are byte-identical to current main. No parser, evaluator, kernel,
type, binding, or phase files were changed.

## Attribution

The arena inventory measured nearest-shadow `memberchk/2` at 94,438 calls and
13,545,263 scanned list entries. A fresh caller-scoped probe on the current
checkout observed one wrapper-induced extra call, 94,439 total, and classified
the semantic callers as follows:

| Caller | Collection | Calls | Scanned entries |
| --- | --- | ---: | ---: |
| `dl7_checker:edge_origin/5` | origins | 4,166 | 4,378,888 |
| `dl7_lowerer:scoped_reservation/5` | reservations | 5,024 | 1,681,450 |
| `dl7_checker:resolve_name/6` | pending edges | 3,560 | 1,573,742 |
| `dl7_evaluator:relation_level/3` | strata | 18,921 | 1,272,768 |
| `dl7_lowerer:callable_slot/4` | pending edges | 1,748 | 1,026,774 |
| `dl7_checker:goal_origin/4` | origins | 642 | 662,100 |
| `dl7_checker:rule_origin/3` | origins | 528 | 544,224 |
| `dl7_lowerer:deferred_alias_promotion/5` | pending edges | 568 | 327,989 |
| `dl7_lowerer:deferred_alias_promotion/5` | origins | 568 | 302,858 |
| `dl7_module_lowerer:source_edge_node/5` | origins | 224 | 224,000 |

The checker origin lookups total 5,336 calls and 5,585,212 scanned entries.
The remaining lowerer/module origin lookups total 792 calls and 526,858
scanned entries. `seed_origin/3` had no nearest-shadow calls because the
fixture has no seeds. Modes are `memberchk(+Key,+Origins)` deterministic,
with first-list-match behavior and `none` fallback on failure.

## Candidate signature

The candidate built one immutable store per `check_datalog_body/4` call:

```prolog
origin_store(+Origins, -origin_index(Assoc))
origin_lookup(+origin_index(Assoc), +Key, -NodeId)
```

`keysort/2` plus first-per-key grouping preserved duplicate-origin first-match
behavior. `get_assoc/3` preserved deterministic success and `none` fallback.
The tagged store was threaded through existing checker predicates without
changing phase-boundary terms or public predicate arities.

## Rejection

Canonical output stayed byte-exact:

| Fixture | Rows | Diagnostics | SHA-256 |
| --- | ---: | --- | --- |
| nearest-shadow | 810 | `[]` | `f42d4b6a73ce4cdd97594ff9710377c499a8c78e2ff8987517793cbbca29a99a` |
| 2_partial | 910 | `[]` | `d47c0778343e976d0fc648324cbb4003c643543ee00b61e817c407ae4ce69519` |

The clean current-main nearest-shadow baseline measured 1,899,633 inferences
and 411.30 ms in one fresh process. The candidate measured 1,929,769
inferences and 333.16 ms. Inference delta was `+30,136`, so the candidate was
rejected despite removing the checker origin scans. The source and focused
test paths were restored with no remaining diff. No source edit remains from
this candidate.
