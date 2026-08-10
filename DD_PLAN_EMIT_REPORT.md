# DD plan emit report

## Goldens

`retraction_only_tick_retracts_level_view.dd.pl`

```prolog
dd_plan(retraction_only_tick_retracts_level_view,rels([rel(mirror/1,[item],set),rel(source_row/1,[item],set)]),arrangements([arr(arr_mirror_1,mirror/1,[item],[],signed),arr(arr_source_row_1,source_row/1,[item],[],signed)]),operators([op(map_1,map(mirror/1))]),wires([wire(source_row/1,map_1,delta),wire(map_1,mirror/1,delta)]),tick_order([phase(absorb_arrivals),phase(index_delta),phase(level_before_edges),phase(edge_arrivals),phase(edge_departures),phase(level_after_edges),phase(iterate),phase(consolidate),phase(retain),phase(boundary),phase(carry),phase(drain)])).
```

`float_exact_join_has_no_epsilon.dd.pl`

```prolog
dd_plan(float_exact_join_has_no_epsilon,rels([rel(left/2,[name,value],set),rel(matched/1,[name],set),rel(right/2,[name,value],set)]),arrangements([arr(arr_left_2,left/2,[name,value],[],signed),arr(arr_matched_1,matched/1,[name],[],signed),arr(arr_right_2,right/2,[name,value],[],signed)]),operators([op(map_1,map(matched/1)),op(join_1_1,join(left/2,right/2))]),wires([wire(left/2,map_1,delta),wire(right/2,map_1,delta),wire(map_1,matched/1,delta)]),tick_order([phase(absorb_arrivals),phase(index_delta),phase(level_before_edges),phase(edge_arrivals),phase(edge_departures),phase(level_after_edges),phase(iterate),phase(consolidate),phase(retain),phase(boundary),phase(carry),phase(drain)])).
```

`float_avg_is_grouped.dd.pl`

```prolog
dd_plan(float_avg_is_grouped,rels([rel(mean/2,[group,value],set),rel(score/2,[group,value],set)]),arrangements([arr(arr_mean_2,mean/2,[group,value],[],signed),arr(arr_score_2,score/2,[group,value],[],signed)]),operators([op(map_1,map(mean/2)),op(reduce_1,reduce)]),wires([wire(score/2,map_1,delta),wire(map_1,mean/2,delta)]),tick_order([phase(absorb_arrivals),phase(index_delta),phase(level_before_edges),phase(edge_arrivals),phase(edge_departures),phase(level_after_edges),phase(iterate),phase(consolidate),phase(retain),phase(boundary),phase(carry),phase(drain)])).
```

## Gates

```text
$ swipl -q -g run_tests -t halt v6/prolog/compile/test/plunit_tests.pl
% [566/566] schema_emit:opena.._checked_in_fixture .. passed (0.002 sec)
EXIT=0

real    0m6.390s
user    0m6.258s
sys     0m0.199s
```

```text
$ v6/tsv2/scripts/sweep.sh
RUN total=247 identical=246 wrong=0 emitted_crash=0 rejection=1 no_oracle_log=0
  REJECTION log_retraction_rejected retract from log rel 'event'
FINAL total=247 final_identical=246 final_wrong=0 no_oracle_final=1
  NO_ORACLE_FINAL log_retraction_rejected oracle threw on this schedule too; no final state to diff
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0 (informational)
EXIT=0

real    0m5.474s
user    0m4.893s
sys     0m0.871s
```

```text
$ git status --short -- v6/prolog/compile/out/ compile/out/
```

## Lower exports

None.

## Setup note

`v6/gen_emitted` is absent in this worktree, so its requested `node_modules`
symlink could not be created. `v6/tsv2/node_modules` was linked.

## Fix pass

The emitter derives arrangement key/value splits from rule bindings, records
the arrangement identifiers in join and reduce operators, and routes source
relations through every emitted operator before the head relation. The mirror
golden has no diff.

`float_exact_join_has_no_epsilon.dd.pl`

```prolog
dd_plan(float_exact_join_has_no_epsilon,rels([rel(left/2,[name,value],set),rel(matched/1,[name],set),rel(right/2,[name,value],set)]),arrangements([arr(arr_left_2,left/2,[name,value],[],signed),arr(arr_matched_1,matched/1,[name],[],signed),arr(arr_right_2,right/2,[name,value],[],signed),arr(arr_left_2_join_1_1_left,left/2,[name],[value],signed),arr(arr_right_2_join_1_1_right,right/2,[name],[value],signed)]),operators([op(map_1,map(matched/1)),op(join_1_1,join(left/2,right/2,arr_left_2_join_1_1_left,arr_right_2_join_1_1_right))]),wires([wire(left/2,join_1_1,delta),wire(right/2,join_1_1,delta),wire(join_1_1,map_1,delta),wire(map_1,matched/1,delta)]),tick_order([phase(absorb_arrivals),phase(index_delta),phase(level_before_edges),phase(edge_arrivals),phase(edge_departures),phase(level_after_edges),phase(iterate),phase(consolidate),phase(retain),phase(boundary),phase(carry),phase(drain)])).
```

`float_avg_is_grouped.dd.pl`

```prolog
dd_plan(float_avg_is_grouped,rels([rel(mean/2,[group,value],set),rel(score/2,[group,value],set)]),arrangements([arr(arr_mean_2,mean/2,[group,value],[],signed),arr(arr_score_2,score/2,[group,value],[],signed),arr(arr_score_2_reduce_1,score/2,[group],[value],signed)]),operators([op(map_1,map(mean/2)),op(reduce_1,reduce(arr_score_2_reduce_1))]),wires([wire(score/2,reduce_1,delta),wire(reduce_1,map_1,delta),wire(map_1,mean/2,delta)]),tick_order([phase(absorb_arrivals),phase(index_delta),phase(level_before_edges),phase(edge_arrivals),phase(edge_departures),phase(level_after_edges),phase(iterate),phase(consolidate),phase(retain),phase(boundary),phase(carry),phase(drain)])).
```

```text
$ LC_ALL=en_US.UTF-8 LANG=en_US.UTF-8 swipl -q -g run_tests -t halt v6/prolog/compile/test/plunit_tests.pl
% [69/568] emit_dd_plan:retr..retracts_level_view ... passed (0.001 sec)
% [70/568] emit_dd_plan:floa..join_has_no_epsilon ... passed (0.002 sec)
% [71/568] emit_dd_plan:float_avg_is_grouped ........ passed (0.001 sec)
% [72/568] emit_dd_plan:every_operator_has_a_wire ... passed (0.003 sec)
% [73/568] emit_dd_plan:join.._keyed_arrangements ... passed (0.002 sec)
EXIT=0
```

```text
$ LC_ALL=en_US.UTF-8 LANG=en_US.UTF-8 v6/tsv2/scripts/sweep.sh
RUN total=247 identical=246 wrong=0 emitted_crash=0 rejection=1 no_oracle_log=0
  REJECTION log_retraction_rejected retract from log rel 'event'
FINAL total=247 final_identical=246 final_wrong=0 no_oracle_final=1
  NO_ORACLE_FINAL log_retraction_rejected oracle threw on this schedule too; no final state to diff
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0 (informational)
EXIT=0
```

```text
$ git status --short -- v6/prolog/compile/out/ compile/out/
EXIT=0
```

## SQL payload pass

Each flow-IR node is now `op(Id, RelationalDescription, sqlite(Refs, Statements))`.
`Statements` retains the existing `lowered/8` `levelstmt/7` or `edgestmt/9` term, including the compiler-produced SQLite strings. `Refs` is the sorted set of the rule head and body relation references. This keeps the SQL payload on its operator node, preserves the lowerer as the SQL source, and gives the runner a checked public-relation boundary for each payload.

The three full goldens follow.

```prolog
/opt/homebrew/bin/bash: warning: setlocale: LC_ALL: cannot change locale (C.UTF-8): No such file or directory

### retraction_only_tick_retracts_level_view.dd.pl
dd_plan(retraction_only_tick_retracts_level_view,rels([rel(mirror/1,[item],set),rel(source_row/1,[item],set)]),arrangements([arr(arr_mirror_1,mirror/1,[item],[],signed),arr(arr_source_row_1,source_row/1,[item],[],signed)]),operators([op(map_1,map(mirror/1),sqlite([mirror/1,source_row/1],[levelstmt(mirror/1,'DELETE FROM "mirror"',['INSERT OR IGNORE INTO "mirror" ("item") SELECT b0."item" FROM "source_row" b0'],'INSERT OR IGNORE INTO "mirror" ("item") SELECT DISTINCT d0."item" FROM "__frontier_source_row" d0 WHERE d0."_phase" >= 0 RETURNING "item"',refcountsql('DELETE FROM "__support_next_mirror"','INSERT INTO "__support_next_mirror" ("item", "__refcount") SELECT "item", sum("__refcount") FROM (SELECT b0."item" AS "item", count(*) AS "__refcount" FROM "source_row" b0 GROUP BY b0."item") GROUP BY "item"','UPDATE "mirror" AS h SET "__refcount" = COALESCE((SELECT n."__refcount" FROM "__support_next_mirror" n WHERE n."item" = h."item"), 0)','INSERT INTO "__delta_mirror" ("_sign", "_sequence", "item") SELECT -1, row_number() OVER () - 1, "item" FROM "mirror" WHERE "__refcount" <= 0','DELETE FROM "mirror" WHERE "__refcount" <= 0','DELETE FROM "__new_mirror"','INSERT INTO "__new_mirror" ("item", "__refcount") SELECT n."item", n."__refcount" FROM "__support_next_mirror" n LEFT JOIN "mirror" h ON n."item" = h."item" WHERE h."item" IS NULL','INSERT INTO "__delta_mirror" ("_sign", "_sequence", "item") SELECT 1, "rowid" - 1, "item" FROM "__new_mirror"','INSERT INTO "__frontier_mirror" ("_phase", "_sequence", "item") SELECT ?, "rowid" - 1, "item" FROM "__new_mirror"','INSERT INTO "__next_frontier_mirror" ("_phase", "_sequence", "item") SELECT ?, "rowid" - 1, "item" FROM "__new_mirror"','INSERT OR IGNORE INTO "mirror" ("item", "__refcount") SELECT n."item", n."__refcount" FROM "__support_next_mirror" n',none,none,none,[]),none,[])]))]),wires([wire(source_row/1,map_1,delta),wire(map_1,mirror/1,delta)]),tick_order([phase(absorb_arrivals),phase(index_delta),phase(level_before_edges),phase(edge_arrivals),phase(edge_departures),phase(level_after_edges),phase(iterate),phase(consolidate),phase(retain),phase(boundary),phase(carry),phase(drain)])).

### float_exact_join_has_no_epsilon.dd.pl
dd_plan(float_exact_join_has_no_epsilon,rels([rel(left/2,[name,value],set),rel(matched/1,[name],set),rel(right/2,[name,value],set)]),arrangements([arr(arr_left_2,left/2,[name,value],[],signed),arr(arr_matched_1,matched/1,[name],[],signed),arr(arr_right_2,right/2,[name,value],[],signed),arr(arr_left_2_join_1_1_left,left/2,[name],[value],signed),arr(arr_right_2_join_1_1_right,right/2,[name],[value],signed)]),operators([op(map_1,map(matched/1),sqlite([left/2,matched/1,right/2],[levelstmt(matched/1,'DELETE FROM "matched"',['INSERT OR IGNORE INTO "matched" ("name") SELECT b0."name" FROM "left" b0, "right" b1 WHERE b1."name" = b0."name" AND b1."value" = b0."value"'],'INSERT OR IGNORE INTO "matched" ("name") SELECT DISTINCT d0."name" FROM "__frontier_left" d0, "right" b0 WHERE d0."_phase" >= 0 AND b0."name" = d0."name" AND b0."value" = d0."value" UNION ALL SELECT DISTINCT d0."name" FROM "__frontier_right" d0, "left" b0 WHERE d0."_phase" >= 0 AND b0."name" = d0."name" AND b0."value" = d0."value" RETURNING "name"',refcountsql('DELETE FROM "__support_next_matched"','INSERT INTO "__support_next_matched" ("name", "__refcount") SELECT "name", sum("__refcount") FROM (SELECT b0."name" AS "name", count(*) AS "__refcount" FROM "left" b0, "right" b1 WHERE b1."name" = b0."name" AND b1."value" = b0."value" GROUP BY b0."name") GROUP BY "name"','UPDATE "matched" AS h SET "__refcount" = COALESCE((SELECT n."__refcount" FROM "__support_next_matched" n WHERE n."name" = h."name"), 0)','INSERT INTO "__delta_matched" ("_sign", "_sequence", "name") SELECT -1, row_number() OVER () - 1, "name" FROM "matched" WHERE "__refcount" <= 0','DELETE FROM "matched" WHERE "__refcount" <= 0','DELETE FROM "__new_matched"','INSERT INTO "__new_matched" ("name", "__refcount") SELECT n."name", n."__refcount" FROM "__support_next_matched" n LEFT JOIN "matched" h ON n."name" = h."name" WHERE h."name" IS NULL','INSERT INTO "__delta_matched" ("_sign", "_sequence", "name") SELECT 1, "rowid" - 1, "name" FROM "__new_matched"','INSERT INTO "__frontier_matched" ("_phase", "_sequence", "name") SELECT ?, "rowid" - 1, "name" FROM "__new_matched"','INSERT INTO "__next_frontier_matched" ("_phase", "_sequence", "name") SELECT ?, "rowid" - 1, "name" FROM "__new_matched"','INSERT OR IGNORE INTO "matched" ("name", "__refcount") SELECT n."name", n."__refcount" FROM "__support_next_matched" n',none,none,none,[]),none,[])])),op(join_1_1,join(left/2,right/2,arr_left_2_join_1_1_left,arr_right_2_join_1_1_right),sqlite([left/2,matched/1,right/2],[levelstmt(matched/1,'DELETE FROM "matched"',['INSERT OR IGNORE INTO "matched" ("name") SELECT b0."name" FROM "left" b0, "right" b1 WHERE b1."name" = b0."name" AND b1."value" = b0."value"'],'INSERT OR IGNORE INTO "matched" ("name") SELECT DISTINCT d0."name" FROM "__frontier_left" d0, "right" b0 WHERE d0."_phase" >= 0 AND b0."name" = d0."name" AND b0."value" = d0."value" UNION ALL SELECT DISTINCT d0."name" FROM "__frontier_right" d0, "left" b0 WHERE d0."_phase" >= 0 AND b0."name" = d0."name" AND b0."value" = d0."value" RETURNING "name"',refcountsql('DELETE FROM "__support_next_matched"','INSERT INTO "__support_next_matched" ("name", "__refcount") SELECT "name", sum("__refcount") FROM (SELECT b0."name" AS "name", count(*) AS "__refcount" FROM "left" b0, "right" b1 WHERE b1."name" = b0."name" AND b1."value" = b0."value" GROUP BY b0."name") GROUP BY "name"','UPDATE "matched" AS h SET "__refcount" = COALESCE((SELECT n."__refcount" FROM "__support_next_matched" n WHERE n."name" = h."name"), 0)','INSERT INTO "__delta_matched" ("_sign", "_sequence", "name") SELECT -1, row_number() OVER () - 1, "name" FROM "matched" WHERE "__refcount" <= 0','DELETE FROM "matched" WHERE "__refcount" <= 0','DELETE FROM "__new_matched"','INSERT INTO "__new_matched" ("name", "__refcount") SELECT n."name", n."__refcount" FROM "__support_next_matched" n LEFT JOIN "matched" h ON n."name" = h."name" WHERE h."name" IS NULL','INSERT INTO "__delta_matched" ("_sign", "_sequence", "name") SELECT 1, "rowid" - 1, "name" FROM "__new_matched"','INSERT INTO "__frontier_matched" ("_phase", "_sequence", "name") SELECT ?, "rowid" - 1, "name" FROM "__new_matched"','INSERT INTO "__next_frontier_matched" ("_phase", "_sequence", "name") SELECT ?, "rowid" - 1, "name" FROM "__new_matched"','INSERT OR IGNORE INTO "matched" ("name", "__refcount") SELECT n."name", n."__refcount" FROM "__support_next_matched" n',none,none,none,[]),none,[])]))]),wires([wire(left/2,join_1_1,delta),wire(right/2,join_1_1,delta),wire(join_1_1,map_1,delta),wire(map_1,matched/1,delta)]),tick_order([phase(absorb_arrivals),phase(index_delta),phase(level_before_edges),phase(edge_arrivals),phase(edge_departures),phase(level_after_edges),phase(iterate),phase(consolidate),phase(retain),phase(boundary),phase(carry),phase(drain)])).

### float_avg_is_grouped.dd.pl
dd_plan(float_avg_is_grouped,rels([rel(mean/2,[group,value],set),rel(score/2,[group,value],set)]),arrangements([arr(arr_mean_2,mean/2,[group,value],[],signed),arr(arr_score_2,score/2,[group,value],[],signed),arr(arr_score_2_reduce_1,score/2,[group],[value],signed)]),operators([op(map_1,map(mean/2),sqlite([mean/2,score/2],[levelstmt(mean/2,'DELETE FROM "mean"',['INSERT OR IGNORE INTO "mean" ("group", "value") SELECT b0."group", avg(b0."value") FROM "score" b0 GROUP BY b0."group" HAVING count(*) > 0'],none,none,avgsql([group],[text],'DELETE FROM "__agg_scope_mean"',['INSERT OR IGNORE INTO "__agg_scope_mean" ("group") SELECT DISTINCT d0."group" FROM "__delta_score" d0 WHERE d0."_sign" IN (-1, 1)','INSERT OR IGNORE INTO "__avg_acc_mean" ("__group_1", "__sum", "__count") SELECT "group", 0.0, 0 FROM "__agg_scope_mean"','UPDATE "__avg_acc_mean" AS a SET "__sum" = "__sum" + COALESCE((SELECT sum(contributions."__sign" * contributions."__value") FROM (SELECT d0."group" AS "__group_1", d0."value" AS "__value", d0."_sign" AS "__sign" FROM "__delta_score" d0 WHERE d0."_sign" IN (-1, 1)) contributions WHERE contributions."__group_1" = a."__group_1" AND ("__group_1") IN (SELECT "group" FROM "__agg_scope_mean")), 0.0), "__count" = "__count" + COALESCE((SELECT sum(contributions."__sign") FROM (SELECT d0."group" AS "__group_1", d0."value" AS "__value", d0."_sign" AS "__sign" FROM "__delta_score" d0 WHERE d0."_sign" IN (-1, 1)) contributions WHERE contributions."__group_1" = a."__group_1" AND ("__group_1") IN (SELECT "group" FROM "__agg_scope_mean")), 0) WHERE ("__group_1") IN (SELECT "group" FROM "__agg_scope_mean")'],'DELETE FROM "mean" WHERE ("group") IN (SELECT "group" FROM "__agg_scope_mean") RETURNING "group", "value"',['INSERT OR IGNORE INTO "mean" ("group", "value") SELECT a."__group_1", a."__sum" / a."__count" FROM "__avg_acc_mean" a WHERE a."__count" > 0 AND ("__group_1") IN (SELECT "group" FROM "__agg_scope_mean") RETURNING "group", "value"'],['DELETE FROM "__avg_acc_mean"','INSERT OR IGNORE INTO "__avg_acc_mean" ("__group_1", "__sum", "__count") SELECT "__group_1", sum("__value"), count(*) FROM (SELECT b0."group" AS "__group_1", b0."value" AS "__value" FROM "score" b0) contributions GROUP BY "__group_1"','DELETE FROM "mean"','INSERT OR IGNORE INTO "mean" ("group", "value") SELECT a."__group_1", a."__sum" / a."__count" FROM "__avg_acc_mean" a WHERE a."__count" > 0 RETURNING "group", "value"']),[])])),op(reduce_1,reduce(arr_score_2_reduce_1),sqlite([mean/2,score/2],[levelstmt(mean/2,'DELETE FROM "mean"',['INSERT OR IGNORE INTO "mean" ("group", "value") SELECT b0."group", avg(b0."value") FROM "score" b0 GROUP BY b0."group" HAVING count(*) > 0'],none,none,avgsql([group],[text],'DELETE FROM "__agg_scope_mean"',['INSERT OR IGNORE INTO "__agg_scope_mean" ("group") SELECT DISTINCT d0."group" FROM "__delta_score" d0 WHERE d0."_sign" IN (-1, 1)','INSERT OR IGNORE INTO "__avg_acc_mean" ("__group_1", "__sum", "__count") SELECT "group", 0.0, 0 FROM "__agg_scope_mean"','UPDATE "__avg_acc_mean" AS a SET "__sum" = "__sum" + COALESCE((SELECT sum(contributions."__sign" * contributions."__value") FROM (SELECT d0."group" AS "__group_1", d0."value" AS "__value", d0."_sign" AS "__sign" FROM "__delta_score" d0 WHERE d0."_sign" IN (-1, 1)) contributions WHERE contributions."__group_1" = a."__group_1" AND ("__group_1") IN (SELECT "group" FROM "__agg_scope_mean")), 0.0), "__count" = "__count" + COALESCE((SELECT sum(contributions."__sign") FROM (SELECT d0."group" AS "__group_1", d0."value" AS "__value", d0."_sign" AS "__sign" FROM "__delta_score" d0 WHERE d0."_sign" IN (-1, 1)) contributions WHERE contributions."__group_1" = a."__group_1" AND ("__group_1") IN (SELECT "group" FROM "__agg_scope_mean")), 0) WHERE ("__group_1") IN (SELECT "group" FROM "__agg_scope_mean")'],'DELETE FROM "mean" WHERE ("group") IN (SELECT "group" FROM "__agg_scope_mean") RETURNING "group", "value"',['INSERT OR IGNORE INTO "mean" ("group", "value") SELECT a."__group_1", a."__sum" / a."__count" FROM "__avg_acc_mean" a WHERE a."__count" > 0 AND ("__group_1") IN (SELECT "group" FROM "__agg_scope_mean") RETURNING "group", "value"'],['DELETE FROM "__avg_acc_mean"','INSERT OR IGNORE INTO "__avg_acc_mean" ("__group_1", "__sum", "__count") SELECT "__group_1", sum("__value"), count(*) FROM (SELECT b0."group" AS "__group_1", b0."value" AS "__value" FROM "score" b0) contributions GROUP BY "__group_1"','DELETE FROM "mean"','INSERT OR IGNORE INTO "mean" ("group", "value") SELECT a."__group_1", a."__sum" / a."__count" FROM "__avg_acc_mean" a WHERE a."__count" > 0 RETURNING "group", "value"']),[])]))]),wires([wire(score/2,reduce_1,delta),wire(reduce_1,map_1,delta),wire(map_1,mean/2,delta)]),tick_order([phase(absorb_arrivals),phase(index_delta),phase(level_before_edges),phase(edge_arrivals),phase(edge_departures),phase(level_after_edges),phase(iterate),phase(consolidate),phase(retain),phase(boundary),phase(carry),phase(drain)])).
```

## Gates

```text
$ LC_ALL=en_US.UTF-8 LANG=en_US.UTF-8 swipl -q -g run_tests -t halt v6/prolog/compile/test/plunit_tests.pl
% [69/569] emit_dd_plan:retraction_only_tick_retracts_level_view ... passed
% [70/569] emit_dd_plan:float_exact_join_has_no_epsilon ... passed
% [71/569] emit_dd_plan:float_avg_is_grouped ... passed
% [72/569] emit_dd_plan:every_operator_has_a_wire ... passed
% [73/569] emit_dd_plan:every_operator_carries_sql_payload_for_plan_rels ... passed
% [74/569] emit_dd_plan:join_inputs_have_keyed_arrangements ... passed
% [569/569] schema_emit:opena.._checked_in_fixture .. passed
EXIT=0

$ LC_ALL=en_US.UTF-8 LANG=en_US.UTF-8 v6/tsv2/scripts/sweep.sh
RUN total=247 identical=246 wrong=0 emitted_crash=0 rejection=1 no_oracle_log=0
  REJECTION log_retraction_rejected retract from log rel 'event'
FINAL total=247 final_identical=246 final_wrong=0 no_oracle_final=1
  NO_ORACLE_FINAL log_retraction_rejected oracle threw on this schedule too; no final state to diff
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0 (informational)
EXIT=0

$ git status --short -- v6/prolog/compile/out/ compile/out/
EXIT=0

$ emit twice, compare bytes
byte-deterministic
EXIT=0
```

## Grain verdict

Option B is committed: `map_N` carries the rule SQL bundle and sibling join, filter, reduce, and iterate nodes carry `sqlite(Refs, owner(map_N))`. A payload-walking SQLite runner therefore executes one bundle per map without a per-tick statement-identity set; the pure-RAM kernel continues to ignore the SQLite field. Receipts, option comparison, and the future same-head clause amendment are in `plans/2026-08-10-dd-payload-grain.PLAN.md`; the plain-language diagram is in `plans/2026-08-10-dd-payload-grain.PLAN.visual.human.unga.md`.
