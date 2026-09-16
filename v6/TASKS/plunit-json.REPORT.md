# plunit-json report

Branch `feature/plunit-json`, pushed, no PR (per brief). Worktree
`~/projects/sprefa-worktrees/plunit-json`, rebased onto `0bf43e111`
(origin/main after PR #390 dropped the battery from 12.6s to 5.2s).

## Scope change mid-task

Original brief was JSON-only, sibling to PR #389's hand-rolled junit XML
writer. A scope change arrived mid-task after the coordinator re-ran
build-vs-buy on #389's writer: PR #389's check (`pack_list(junit)` only) was
too thin. New deliverable, all landed in this branch:

1. `PLUNIT_JSON=<path>` -- as originally briefed.
2. `PLUNIT_TAP=<path>` (or `-` for stdout) -- TAP version 13, bought nowhere
   because there is nowhere to buy it from; this file is the only thing that
   knows plunit's own result facts, so emitting the small standard TAP
   grammar is the one irreducible bespoke part.
3. `PLUNIT_JUNIT=<path>` -- rewritten to pipe the TAP stream from (2) through
   the `tap-junit` npm package instead of a hand-rolled XML writer. The old
   `plunit_junit_write/1` writer (`v6/prolog/compile/test/run_plunit.pl`,
   PR #389) is deleted outright, not kept as a fallback -- the measurement
   below showed no reason to keep it.

## Build-vs-buy: junit XML

| candidate | verdict | evidence |
|---|---|---|
| SWI `plunit:test_report/1` | rejected | accepts only `fixme`; not a general reporter (coordinator's read of its clauses, confirmed by inspecting `library(plunit)` at `/opt/homebrew/Cellar/swi-prolog/10.0.2/lib/swipl/library/ext/plunit/plunit.pl`; no `test_report(junit)` or similar clause exists) |
| SWI pack registry, junit/xunit reporter | rejected | `pack_list('junit')` and `pack_search(junit)` return no matching packages (coordinator, confirmed) |
| `pack tap@1.0.3` | rejected | a TAP test *framework* replacing plunit outright -- adopting it means rewriting all 936 tests. Cost rejected. |
| hand-rolled XML writer (PR #389's own choice) | superseded | worked, byte-valid, but re-derives a fixed 3-level schema plunit itself has no opinion on; every future junit-schema nuance (e.g. `<skipped>`, CDATA edge cases) becomes this repo's problem to keep correct |
| `npm tap-xunit@2.4.1` | tested, rejected | prefixes every testcase name with its own ordinal: `#1 acyclic_guard:a_cross_rel_option_mints_no_guard` instead of `acyclic_guard:a_cross_rel_option_mints_no_guard`. Mangles the `unit:name` spelling the JSON/TAP names carry, breaking the cross-format name diff the gates require. Measured on a 3-case, 1-failure sample TAP file (see command below) piped through both converters side by side. |
| `npm tap-junit@5.0.4` | **chosen** | keeps the TAP test description verbatim as the XML `testcase` name (`name="acyclic_guard:..."`), so JSON/TAP/JUNIT name sets diff clean. Single `<testsuite>` (not one per plunit unit, unlike the old hand-rolled writer) -- acceptable since nothing downstream reads per-unit `<testsuite>` grouping; the gate only checks total `testcase`/`failure` counts and the name set. |

Sample command used for the side-by-side (both packages installed as
devDependencies during evaluation, `tap-xunit` removed afterward since it
lost):

```
cat sample.tap | ./node_modules/.bin/tap-xunit   # -> "#1 acyclic_guard:..."
cat sample.tap | ./node_modules/.bin/tap-junit   # -> "acyclic_guard:..."
```

`tap-junit@5.0.4` is now pinned in `v6/tsv2/package.json` devDependencies
(`pnpm install` resolves it offline via the repo's own lockfile; no bare
`npx` fetch at test time). The binary is invoked at
`v6/tsv2/node_modules/.bin/tap-junit`, resolved from `run_plunit.pl`'s own
load-time directory (`prolog_load_context(directory, _)`, 3 levels up to
`v6/`), so it does not depend on the caller's working directory.

## Architecture: one case table, three writers

`v6/prolog/compile/test/run_plunit.pl` refactor, cheaper-shape call: the
Unit:Name row-folding PR #389 wrote (`plunit_junit_row/6` ->
`plunit_junit_case/2` -> `plunit_junit_cases/1`) is now the ONE shared
collection layer, renamed off the `junit` prefix (`plunit_case_row/6` ->
`plunit_case/2` -> `plunit_cases/1`), reading straight off plunit's
`passed/5`, `failed/5`, `timeout/5` facts (kept alive by `cleanup(false)`,
unchanged from #389). `plunit_cases/1` returns one `case(Unit, Name, Line,
Time, Status, Detail)` per DECLARED test, sorted by `Unit-Name`, and every
writer consumes that same list:

- `plunit_json_write/3` maps each `case/6` to a flat dict.
- `plunit_tap_string/2` renders the same list as TAP, in the same sort order
  (so JSON test order and TAP case numbering line up 1:1).
- `plunit_junit_write/2` never touches plunit facts directly -- it takes the
  TAP string `plunit_reports_maybe_write/1` already built and pipes it
  through `tap-junit`.

Failure-text rendering (`phrase(plunit:failure(E), Tokens)` +
`print_message_lines/3`, #389's approach, reused unchanged) and the
one-line fold (`plunit_failure_oneline/2`, was `plunit_junit_oneline/2`)
are shared helpers too: JSON keeps the raw (possibly multi-line) rendered
text since JSON strings carry newlines fine; TAP's YAML block uses the
folded one-liner, double-quote-escaped for YAML (`plunit_yaml_dquote/2`:
backslash escaped before quote, so the escaping backslash is never
re-escaped).

`plunit_case_file/2` is new: plunit's `passed/5`/`failed/5`/`timeout/5`
facts carry a source `Line` but no file. The file comes from
`plunit:unit_file/2` (public plunit predicate, confirmed by reading
`plunit.pl`), made relative to the caller's working directory via
`relative_file_name/3`; JSON's dict-mode null is the plain atom `null`
(NOT `@(null)` -- that spelling is for `library(json)`'s non-dict
`json(Pairs)` reader; confirmed by testing both against
`json_write_dict/3`, since the default null representation differs between
the two reader modes in `library(json)`, `default_json_dict_options/1` at
`json.pl:139-140`).

`library(http/json)` kept (matches this file's own prior import and the
majority of the repo -- `grep -rn "library(http/json)"` under
`v6/prolog` hits `compile_messages.pl`, `emit_rust.pl` (imports
`library(json)` directly, the minority spelling), `6_profile.pl`,
`compile/5_emit_openapi.pl`, `diag.pl`, `compile/typegen_export.pl`,
`compile/6_isolated_compiler_dd.pl`, `compile/4_emit_jsonschema.pl`, and
more -- `library(http/json)` is the dominant spelling). SWI 10 prints
`Library was moved: library(http/json) --> library(json)` once at load;
this is the same warning every other file above already emits.

## GATES verbatim

### Gate 1: JSON alone

```
$ cd v6 && PLUNIT_JSON=/tmp/p2.json just plunit
...
FAIL catalog_plane_rail:level_plane_family_corpus_counts
FAIL json_merge_patch:json_patch_lowers_with_the_null_stand_in_guard
FAIL json_merge_patch:merge_patch_stops_on_a_nested_json_null_stand_in
FAIL json_merge_patch:merge_patch_stops_on_the_json_null_stand_in
FAIL module_path_decls:a_zero_column_childs_name_used_as_a_value_is_not_rewritten
FAIL rel_template_and_is_clause:a_relation_arrow_prints_the_equivalent_explicit_declaration
FAIL rel_zero_arity:a_root_rel_zero_still_has_no_storage
FAIL subscribe_cone:golden_flex_cone_invariants
PLUNIT jobs=12 declared=936 results=982 passed=974 failed=8 timeout=0 wall=4.9Xs
error: recipe `plunit` failed on line 58 with exit code 1

$ python3 -c "import json;d=json.load(open('/tmp/p2.json'));print(len(d['tests']), d['run'])"
936 {'jobs': 12, 'declared': 936, 'results': 982, 'passed': 974, 'failed': 8, 'timeout': 0, 'wall_seconds': 4.9..., 'started_ts': 1787246...}
```

### Gate 2: JSON failing-name set == stdout FAIL set

```
$ diff <(sort stdout_FAIL_names) <(sort json_failing_names)
(empty)
```
Measured empty on two separate runs.

### Gate 3: TAP plan + not-ok set

```
$ PLUNIT_TAP=/tmp/p.tap just plunit
$ grep -c '^not ok' /tmp/p.tap
8
$ grep '^1\.\.' /tmp/p.tap
1..936
$ diff <(sort stdout_FAIL_names) <(grep '^not ok' /tmp/p.tap | sed -E 's/^not ok [0-9]+ - //' | sort)
(empty)
```

### Gate 3b: all three at once, JUNIT via the tap-junit converter

```
$ PLUNIT_JSON=/tmp/plunit.json PLUNIT_TAP=/tmp/plunit.tap PLUNIT_JUNIT=/tmp/plunit.xml just plunit
...
PLUNIT jobs=12 declared=936 results=982 passed=974 failed=8 timeout=0 wall=4.91s

$ python3 -c "
import xml.etree.ElementTree as ET
t = ET.parse('/tmp/plunit.xml'); r = t.getroot()
print(r.tag, len(r.findall('.//testcase')), len(r.findall('.//testcase/failure')))
"
testsuites 936 8

$ python3 -c "
import xml.etree.ElementTree as ET, json
xml_names = sorted(tc.get('name') for tc in ET.parse('/tmp/plunit.xml').getroot().findall('.//testcase'))
d = json.load(open('/tmp/plunit.json'))
json_names = sorted(f\"{t['unit']}:{t['name']}\" for t in d['tests'])
print(len(xml_names), len(json_names), xml_names == json_names)
"
936 936 True
```

### Gate 4: none of the three set, stdout unchanged

```
$ cd v6 && just plunit
...
FAIL catalog_plane_rail:level_plane_family_corpus_counts
FAIL json_merge_patch:json_patch_lowers_with_the_null_stand_in_guard
FAIL json_merge_patch:merge_patch_stops_on_a_nested_json_null_stand_in
FAIL json_merge_patch:merge_patch_stops_on_the_json_null_stand_in
FAIL module_path_decls:a_zero_column_childs_name_used_as_a_value_is_not_rewritten
FAIL rel_template_and_is_clause:a_relation_arrow_prints_the_equivalent_explicit_declaration
FAIL rel_zero_arity:a_root_rel_zero_still_has_no_storage
FAIL subscribe_cone:golden_flex_cone_invariants
PLUNIT jobs=12 declared=936 results=982 passed=974 failed=8 timeout=0 wall=4.97s..5.04s
error: recipe `plunit` failed on line 58 with exit code 1
```
Same 8-line FAIL set, same `declared=936 ... failed=8 timeout=0` shape, wall
matches PR #390's new ~5s baseline (NOT the ~13s the original brief quoted --
that number is stale post-#390, per the coordinator's note). No
`/tmp/gate4.{json,tap,xml}` files were created when the three env vars were
unset, confirmed with fresh, previously-nonexistent target paths.

## Sample JSON head

```json
{
  "run": {
    "declared":936,
    "failed":8,
    "jobs":12,
    "passed":974,
    "results":982,
    "started_ts":1787246506.744047,
    "timeout":0,
    "wall_seconds":4.910843133926392
  },
  "tests": [
    {
      "failure":null,
      "file":"test/plunit_tests.pl",
      "line":10741,
      "name":"a_cross_rel_option_mints_no_guard",
      "status":"passed",
      "time_seconds":0.006217002868652344,
      "unit":"acyclic_guard"
    },
    ...
```

A failing entry:

```json
{"failure": "failed\n", "file": "test/plunit_tests.pl", "line": 1841, "name": "level_plane_family_corpus_counts", "status": "failed", "time_seconds": 4.571897983551025, "unit": "catalog_plane_rail"}
```

`file` is relative to the caller's working directory (`v6/prolog/compile`,
where `just plunit` cd's before running); `failure` carries the raw
(unfolded) rendered text, `null` for passed rows; `line` and `file` are
present for every row, not only non-passed ones, so the array stays one flat
schema throughout (uniform columns for `json_each` / a SQLite import, no
per-row shape branching).

## Sample TAP

```
TAP version 13
1..936
ok 1 - acyclic_guard:a_cross_rel_option_mints_no_guard
ok 2 - acyclic_guard:the_guard_ddl_walks_the_companions_unique_index
...
not ok 92 - catalog_plane_rail:level_plane_family_corpus_counts
  ---
  message: "failed"
  severity: fail
  status: failed
  ...
```

## Not done / left for follow-up

- No `<skipped>` / TAP `not ok ... # SKIP` handling: same as PR #389's
  note, nothing in the current battery uses plunit's `blocked` option
  (`SUMMARY` dict shows `blocked:0`), so there is no observed case to test
  against, in any of the three formats.
- `tap-junit` emits one flat `<testsuite>` rather than one per plunit unit
  (the old hand-rolled writer's shape). Nothing downstream was found
  reading per-unit `<testsuite>` grouping; flagged here in case a consumer
  surfaces later.
