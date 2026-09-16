# plunit-junit report

Branch `feature/plunit-junit`, pushed, no PR (per brief). Worktree
`~/projects/sprefa-worktrees/plunit-junit`, based on `e5fcdf55a`.
Commit `e93b6599a`.

## Build-vs-buy check

`pack_list(junit)` against the default SWI pack server returns no matches
(`Search for "junit", returned no matching packages`). Base install has no
junit writer. `library(sgml)` supplies generic `xml_quote_attribute/2` and
`xml_quote_cdata/2` (used below for escaping) but nothing schema-shaped.
Verdict: hand-rolled writer over the 3-level fixed junit schema
(testsuites > testsuite > testcase), reusing SWI's own XML quoting rather
than a general XML-tree library.

## What changed

`v6/prolog/compile/test/run_plunit.pl`: `PLUNIT_JUNIT=<path>` env knob.
When set, `plunit_junit_write/1` runs after `run_tests/2` (facts still
alive under `cleanup(false)`) and writes junit XML to the given path.
Unset: the whole code path is skipped, stdout unchanged.

Key design points:
- Reuses `plunit:passed/5`, `plunit:failed/5`, `plunit:timeout/5` facts
  (same facts the existing slowest-test/unit tables read) instead of a
  separate collection pass.
- Failure text comes from `phrase(plunit:failure(E), Tokens)` +
  `print_message_lines/3` — plunit's own failure-rendering grammar, the
  same one the terminal report uses — so the XML message text matches
  what a human sees on a red run instead of a re-derived approximation.
- `forall(...)`-generated tests emit multiple result rows under one
  `Unit:Name` (measured: 982 result rows over 936 declared tests, 4 tests
  with >1 row). These fold into one `<testcase>` per declared test: status
  is worst-of (failed/timeout beats passed), time is the sum of the
  generated cases' wall times.
- Grouped into `<testsuite>` per plunit unit via `keysort` +
  `group_pairs_by_key`, matching the idiom already used by
  `plunit_report_slowest_units/1` in the same file.

## Gate receipts

```
$ cd v6 && PLUNIT_JUNIT=/tmp/plunit.xml just plunit
...
FAIL catalog_plane_rail:level_plane_family_corpus_counts
FAIL json_merge_patch:json_patch_lowers_with_the_null_stand_in_guard
FAIL json_merge_patch:merge_patch_stops_on_a_nested_json_null_stand_in
FAIL json_merge_patch:merge_patch_stops_on_the_json_null_stand_in
FAIL module_path_decls:a_zero_column_childs_name_used_as_a_value_is_not_rewritten
FAIL rel_template_and_is_clause:a_relation_arrow_prints_the_equivalent_explicit_declaration
FAIL rel_zero_arity:a_root_rel_zero_still_has_no_storage
FAIL subscribe_cone:golden_flex_cone_invariants
PLUNIT jobs=12 declared=936 results=982 passed=974 failed=8 timeout=0 wall=13.22s
error: recipe `plunit` failed on line 58 with exit code 1

real	0m13.750s
```

```
$ python3 -c "import xml.etree.ElementTree as ET; t=ET.parse('/tmp/plunit.xml'); r=t.getroot(); print(r.tag, len(r.findall('.//testcase')), len(r.findall('.//testcase/failure')))"
testsuites 936 8
```

Parses clean, 936 testcases (matches `declared=936`), 8 `<failure>`
entries (matches the 8 known-red `FAIL` lines), wall time unchanged
(~13s, same as the pre-existing `just plunit` gate).

## Byte-identical-without-the-knob check

`PLUNIT_JUNIT` unset, `PLUNIT_JOBS=1` (deterministic ordering) before vs.
after this patch: diffed clean except for per-test wall-clock numbers
(`passed (0.001 sec)` vs `(0.002 sec)`, `SLOW`/`UNIT` tables, and the final
`wall=` figure) — inherent run-to-run timing noise, not code-path change
(confirmed CLAUDE.md's own note that back-to-back whole-gate runs vary
under load). Test identities, order, the `FAIL` set, and all structural
lines matched exactly. The default `jobs=12` (unpinned) run additionally
reorders progress-line interleaving between runs, which is pre-existing
parallel-scheduler nondeterminism unrelated to this change.

## Sample XML head

```xml
<?xml version="1.0" encoding="UTF-8"?>
<testsuites>
  <testsuite name="acyclic_guard" tests="3" failures="0" errors="0" time="0.015">
    <testcase classname="acyclic_guard" name="a_cross_rel_option_mints_no_guard" time="0.001">
    </testcase>
    <testcase classname="acyclic_guard" name="the_guard_ddl_walks_the_companions_unique_index" time="0.002">
    </testcase>
    <testcase classname="acyclic_guard" name="the_guard_walk_searches_rather_than_scans" time="0.011">
    </testcase>
  </testsuite>
  <testsuite name="acyclic_surface" tests="7" failures="0" errors="0" time="0.006">
    ...
```

Sample failure entry:

```xml
<testsuite name="catalog_plane_rail" tests="2" failures="1" errors="0" time="13.175">
  <testcase classname="catalog_plane_rail" name="level_plane_family_corpus_counts" time="6.198">
    <failure message="failed" type="failed">failed
</failure>
  </testcase>
```

## Not done / left for follow-up

- No `<skipped>` entries: nothing in the current battery uses plunit's
  `blocked` option (`SUMMARY` dict shows `blocked:0`), so there is no
  observed case to test the skipped path against. `blocked/4` facts exist
  in plunit but this driver does not read them — add a
  `plunit_junit_row/6` clause for `plunit:blocked/4` plus a `<skipped/>`
  element if/when a test starts using `blocked(...)`.
