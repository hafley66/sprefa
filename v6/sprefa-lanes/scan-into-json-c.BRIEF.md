# feature/scan-into-json-c: un-refuse json_object/2 as an aggregate head

## Why this brief is version 2
Version 1 named the surface `json_group_object/2`. That name is the
SQLite function, NOT the dl6 surface. The prior lane STOPPED correctly on
the mismatch, per its rail. Real names, verified by the coordinator
2026-08-11 against the code (not docs):

| thing | where | state today |
|---|---|---|
| surface row | `v6/prolog/compile/registry.pl:171` | `surface(json_object/2, aggregate, no_refs, head(refuse(aggregate)), refused).` |
| compiler classifier | `v6/prolog/analyze.pl:1678-1679` | EXISTS: `classify_head_arg(Arg, agg(json_object, KeyExpr-ValueExpr))` |
| oracle classifier | `v6/prolog/conformance/level_eval.pl:36-37` | EXISTS |
| oracle value fold | `level_eval.pl:230` | EXISTS: `head_arg_value(agg(json_object, KeyExpr-ValueExpr), contrib(Key-Value))` |
| oracle compute + duplicate-key guard | `level_eval.pl:294-298` | EXISTS: `agg_compute(json_object, Pairs, obj(Object))`, throws `json_object_dup_key(Keys)` |
| lowering arm | `v6/prolog/lower.pl` | MISSING — this is your work |

So the oracle side, both classifiers, and the duplicate-key semantics are
already built. The compiler-side TODO is the registry row plus one
lowering arm. This is implementation of a design the user ruled today
(scan-into-json candidate C, `plans/2026-08-09-scan-into-json-research.md`
section 7); do NOT implement candidates A or B.

## Do
1. `registry.pl:171`: flip the `json_object/2` row from
   `head(refuse(aggregate)), refused` to `head(lower), live`, matching
   the shape of the `json_group_array/1` and `/2` rows on the next lines.
2. `lower.pl`: add the aggregate lowering arm next to its siblings at
   `lower.pl:5015-5025`. Copy-shape from
   `aggregate_select_expr(Mode, agg(json_group_array, Expr), Bound, Sql, direct)`
   and its `json_group_array_value_sql/3` helper (`lower.pl:5042+`). The
   emitted SQL function is SQLite's `json_group_object(Key, Value)`. Keep
   the ORDER BY treatment consistent with the proven array arm.
3. Duplicate keys: the oracle THROWS `json_object_dup_key(Keys)`. SQLite's
   `json_group_object` does not throw; it emits both keys. Make the
   compiled path agree with the oracle. Read `level_eval.pl:294-298`
   first, then choose the cheapest agreeing shape (a compile-time refusal
   is NOT agreement; the oracle throws at RUN time on the actual data).
   If the only way to agree requires changing the oracle, STOP and report
   the fork with both call sites cited — do not change the oracle.
4. Fixtures: conformance fixture(s) with descriptive test names covering
   group-to-object happy path, ORDER BY determinism, and the duplicate-key
   case. Regenerate: `cd <worktree>/v6/tsv2 && bash scripts/sweep.sh`, and
   quote your fixtures' manifest bucket in the commit message.
5. plunit coverage in `v6/prolog/compile/test/plunit_tests.pl` following
   neighboring tests' style.

## Out of scope
The brace-literal `:=` document TODO (`lower.pl:559`
`json_value_expression`) stays. No parser changes. No oracle changes.

## Setup (required, absolute cd, pnpm never npm)
cd <worktree>/v6/tsv2 && pnpm install
cd <worktree>/v6/sprefa-store/js && pnpm install
cd <worktree>/v6/sprefa-extract && cargo build --release --features cli --bin extract

## Gates (all green before final commit; quote outputs verbatim)
cd <worktree>/v6 && just conformance && just text-door && just roundtrip
cd <worktree>/v6/prolog && swipl -g run_tests -t halt compile/test/plunit_tests.pl

## Rails
- rc=0 with dirty tree, no commits, or red gates is a DEFECT. Blocked ->
  FAILURE-REPORT-SCANJSON.md, exact command + output, exit NONZERO.
- NEVER git merge/pull/rebase. NEVER --no-verify. Up to 5 commits, prefix
  `prolog:`. No push, no PR; coordinator judges. Lanes never spawn
  subagents.
- If reality deviates from the table above, STOP and report which row is
  wrong with the real file:line. Do not improvise.

## Style
Banned words, prose and identifiers: provenance, substrate, load-bearing,
regime, refusal. Comments state only constraints the code cannot show. dl
variable names descriptive, never single-letter.
