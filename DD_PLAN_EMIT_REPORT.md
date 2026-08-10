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
