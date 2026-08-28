# plan: grade ratchet bisect, 345 -> 322

Bisect over `021ecad22..7be76330e` (177 commits) for the commit that dropped
the Rust grade ratchet from byte-clean=345 to 322 with 23 fixtures lost.

## Measurement notes

- Test: `bash v6/sprefa-engine-rs/grade.sh`, one full battery per commit,
  `timeout 900`, run from the worktree. Each run measured once; the culprit and
  the boundary commits reproduced the same numbers across independent runs
  (HEAD: 322/23 twice; culprit: 322/23 twice).
- Adjusted verdict rule (coordinator word, m-76212522): a step is BAD when the
  REGRESSION list contains any of `relation_depth2_chained_decode`,
  `struct_intern_order_a`, `json_patch_fold_rfc7396_clauses`. The baseline's
  standing 1-fixture loss counts green.
- Baseline `021ecad22` is NOT the briefed 345/0-lost. Measured twice,
  deterministic: `graded=445 byte-clean=340`, REGRESSION list of exactly one
  fixture, `callgraph_unused_inverts_with_the_call_set` (verdict `diff`,
  reason `missing-rel first-tick=4`). The ratchet was never true as briefed;
  the bisect proceeded on the adjusted rule.
- `graded` differs across the range (445 before #458 lands, 449 after): the
  fixture set itself grew mid-range. Compare `byte-clean`, not `graded`.

## Steps

| commit | date | subject | byte-clean | lost | verdict |
|---|---|---|---|---|---|
| 021ecad22 | 2026-08-23 | engine: gate recount on live retractions | 340 | 1 (standing) | good (baseline) |
| 5c0581b5b | 2026-08-24 | docs(ghcache): failure-modes entry 91, README account/retention sections | 341 | 0 | good |
| ef82c8194 | 2026-08-24 | Merge PR #458 (extract-prolog-throughput) | 345 | 0 | good |
| 519212fc0 | 2026-08-25 | extract: ast-grep Edit drains into soopy SourceAction (#473) | 345 | 0 | good |
| f9f51cb71 | 2026-08-25 | plan: extract move rule path back to the 0.12 s band (#479) | 345 | 0 | good |
| b1a7ab918 | 2026-08-25 | extract move: bounded rule, stem gate, parallel drain (#480) | 345 | 0 | good |
| **ecf8ec5f7** | **2026-08-25** | **extract move for TypeScript: corpus walk, specifier rows, bought resolution (arcs 1-3) (#481)** | **322** | **23** | **BAD, first bad** |
| 13e12ef02 | 2026-08-25 | extract move: the TypeScript arm and `--list` batch (arc 4) (#482) | 322 | 23 | bad |
| c942436fb | 2026-08-26 | extract move: re-aim the relative path constants (#485) | 322 | 23 | bad |
| 7be76330e | 2026-08-27 | sprefa-store: delete the unreferenced tasks.rs trait set (#506) | 322 | 23 | bad (HEAD) |

Single-step drop 345 -> 322 at the culprit. No intermediate step landed in the
330-339 band; there is no second stage.

## Culprit

`ecf8ec5f752464a4be5264f4cadbc060a7983a26` - "extract move for TypeScript:
corpus walk, specifier rows, bought resolution (arcs 1-3) (#481)".

    v6/sprefa-extract/Cargo.toml                       |   8 +
    v6/sprefa-extract/Cargo.lock                       | 167 +++++++++++-
    v6/sprefa-extract/src/lang/ts.rs                   | 303 +++++++++++++-------
    v6/sprefa-extract/src/lang/ts_resolve.rs           | 161 +++++++++++
    v6/sprefa-extract/src/lang/ts_walk.rs              | 100 +++++++
    ... (extract src/tests/fixtures; engine untouched)

The engine's own `Cargo.toml`/`Cargo.lock` are identical on the culprit and its
parent. What changes the grade is a dependency edge: the commit adds
`oxc_resolver = "11.24"` (`v6/sprefa-extract/Cargo.toml:53`), and
`sprefa-engine-rs` already depends on `sprefa-extract`
(`v6/sprefa-engine-rs/Cargo.toml:25`), so the new crate enters the harness
build graph. `oxc_resolver`'s default features turn on
`serde_json/preserve_order` (verified: `cargo tree -i serde_json -e features`
at the culprit shows `preserve_order <- oxc_resolver v11.24.3 default`).

## Lost fixtures and reasons

All 23 lost fixtures are verdict `diff` with cause `number-text`:

| cause | count |
|---|---|
| number-text first-tick=1 | 19 |
| number-text first-tick=2 | 3 |
| number-text first-tick=3 | 1 |

`diff_cause.py` assigns `number-text` when the oracle and the Rust output
lines are equal as parsed JSON but differ as bytes (the equal-as-JSON test
also passes for reordered object keys, so the category covers key-order byte
diffs). Three of the lost fixtures, from the culprit-run verdicts:

| fixture | verdict | reason |
|---|---|---|
| relation_depth2_chained_decode | diff | number-text first-tick=1 |
| struct_intern_order_a | diff | number-text first-tick=1 |
| json_patch_fold_rfc7396_clauses | diff | number-text first-tick=3 |

Full lost list: enum_variant_field_typed_as_rel_is_a_ref,
json_patch_fold_rfc7396_clauses,
one_colliding_ref_column_beside_a_disjoint_sibling,
recursive_list_arg_parent_holds_child_node_values,
relation_depth2_chained_decode, relation_depth2_construct_and_read,
relation_depth2_dot_read, relation_depth2_literal_leaf_selects_zero_and_one,
relation_depth2_many_rows_share_one_leaf, relation_depth2_member_dot_pattern,
relation_depth2_nested_decode_pattern, relation_depth2_two_leaf_shapes,
relation_depth3_chained_decode, relation_depth3_nested_struct_roundtrip,
struct_arrival_key_order_canonicalized, struct_column_renders_canonical_json,
struct_intern_order_a, struct_intern_order_b,
struct_nested_value_renders_whole_tree,
struct_shared_child_survives_one_release,
two_bounded_parameters_mint_one_instance,
variant_field_typed_as_struct_is_a_ref.

## Fix hypothesis (not implemented)

`serde_json/preserve_order` flips `serde_json::Map` from sorted (BTreeMap) to
insertion order (IndexMap) across the whole unified build graph. The tick
canonicalizer relies on the sorted side:

- Throw site: `v6/sprefa-engine-rs/src/ticklog.rs:146-156`,
  `canonical_json_value` iterates `serde_json::Value::Object(map)` in map
  order; its own comment states the assumption ("serde_json's Object is
  sorted when preserve_order is off").
- Trigger: `v6/sprefa-extract/Cargo.toml:53`, `oxc_resolver = "11.24"` with
  default features.
- The culprit commit already met this exact hazard on the extract side and
  fixed it there with a BTreeMap sort (the added comment: "Sorted through the
  BTreeMap, never through `Map`: a dependency turning
  `serde_json/preserve_order` on otherwise reorders every row"). The engine
  side got no such sort.
- One-line fix: in `canonical_json_value`, collect the object entries into a
  `BTreeMap` (or sort by key) before rendering, so the canonical text is
  order-independent regardless of the `preserve_order` feature. Alternative
  shape: `oxc_resolver.default-features = false` plus the minimal feature
  list, at the cost of re-deriving which defaults the resolver actually
  needs.

## Bisect log

```
git bisect start '7be76330e60a3281001153474e58edf9472d7ee3' '021ecad22'
# bad: [7be76330e60a3281001153474e58edf9472d7ee3] sprefa-store: delete the unreferenced tasks.rs trait set (#506)
# good: [021ecad2271bd97a3aed94dd32d4bbadd471800a] engine: gate recount on live retractions
git bisect start '7be76330e60a3281001153474e58edf9472d7ee3' '021ecad22'
# good: [5c0581b5b59ca7c6445211c528669b248e5fd491] docs(ghcache): failure-modes entry 91, README account/retention sections
git bisect good 5c0581b5b59ca7c6445211c528669b248e5fd491
# good: [5c0581b5b59ca7c6445211c528669b248e5fd491] docs(ghcache): failure-modes entry 91, README account/retention sections
git bisect good 5c0581b5b59ca7c6445211c528669b248e5fd491
# good: [ef82c8194e95c61a4b95eec513de42877619b2cc] Merge pull request #458 from hafley66/fix/extract-prolog-throughput
git bisect good ef82c8194e95c61a4b95eec513de42877619b2cc
# bad: [c942436fba531128738fa7db159d3798f90b0c6f] extract move: re-aim the relative path constants a moved TS file writes (#485)
git bisect bad c942436fba531128738fa7db159d3798f90b0c6f
# good: [519212fc0c1448399b17c25973e59ba541214703] extract: ast-grep Edit drains into soopy SourceAction, Act deleted (arc B) (#473)
git bisect good 519212fc0c1448399b17c25973e59ba541214703
# good: [f9f51cb71fcb096bdcbceacb9397bd7e5cb74980] plan: extract move rule path back to the 0.12 s band (move-rule-perf) (#479)
git bisect good f9f51cb71fcb096bdcbceacb9397bd7e5cb74980
# bad: [13e12ef02c789ccce9ad6fff735ee989689a8286] extract move: the TypeScript arm and `--list` batch (arc 4) (#482)
git bisect bad 13e12ef02c789ccce9ad6fff735ee989689a8286
# bad: [ecf8ec5f752464a4be5264f4cadbc060a7983a26] extract move for TypeScript: corpus walk, specifier rows, bought resolution (arcs 1-3) (#481)
git bisect bad ecf8ec5f752464a4be5264f4cadbc060a7983a26
# good: [b1a7ab918ce24ff1f79adbdc57991217f89e2e9b] extract move: bounded rule, stem gate, parallel drain, one parse per file (0.36 -> 0.18 s) (#480)
git bisect good b1a7ab918ce24ff1f79adbdc57991217f89e2e9b
# first bad commit: [ecf8ec5f752464a4be5264f4cadbc060a7983a26] extract move for TypeScript: corpus walk, specifier rows, bought resolution (arcs 1-3) (#481)
```

(The doubled `5c0581b5b` good entry is real: the first `git bisect good` was
aborted by an uncommitted `Cargo.lock` churn from the build, then re-run.)
