# Unified G4v2 report

| engine | workload | scale | result |
|---|---|---:|---|
| ram-zset | live | 100 | RESULT engine=ram-zset workload=live scale=100 correct=true digest=7183135401265645600 oracle=7183135401265645600 ms=0.017 rust_peak=0.003 sqlite_hw=0.000 rss=2.719 |
| cascade-zset | live | 100 | RESULT engine=cascade-zset workload=live scale=100 correct=true digest=7183135401265645600 oracle=7183135401265645600 ms=1.855 rust_peak=0.064 sqlite_hw=0.205 rss=7.453 |
| sqlite-temporal | live | 100 | RESULT engine=sqlite-temporal workload=live scale=100 correct=true digest=7183135401265645600 oracle=7183135401265645600 ms=1.257 rust_peak=0.062 sqlite_hw=0.215 rss=7.531 |
| salsa-rows | live | 100 | RESULT engine=salsa-rows workload=live scale=100 correct=true digest=7183135401265645600 oracle=7183135401265645600 ms=0.079 rust_peak=0.036 sqlite_hw=0.000 rss=4.562 |
| ram-zset | live | 1000 | RESULT engine=ram-zset workload=live scale=1000 correct=true digest=-4159754040995164509 oracle=-4159754040995164509 ms=0.104 rust_peak=0.034 sqlite_hw=0.000 rss=2.828 |
| cascade-zset | live | 1000 | RESULT engine=cascade-zset workload=live scale=1000 correct=true digest=-4159754040995164509 oracle=-4159754040995164509 ms=4.519 rust_peak=0.163 sqlite_hw=0.454 rss=7.844 |
| sqlite-temporal | live | 1000 | RESULT engine=sqlite-temporal workload=live scale=1000 correct=true digest=-4159754040995164509 oracle=-4159754040995164509 ms=4.590 rust_peak=0.124 sqlite_hw=0.359 rss=7.812 |
| salsa-rows | live | 1000 | RESULT engine=salsa-rows workload=live scale=1000 correct=true digest=-4159754040995164509 oracle=-4159754040995164509 ms=0.359 rust_peak=0.081 sqlite_hw=0.000 rss=4.672 |
| ram-reach | reach | 100 | RESULT engine=ram-reach workload=reach scale=100 correct=true digest=-2221706294120251723 oracle=-2221706294120251723 ms=4.357 rust_peak=0.024 sqlite_hw=0.000 rss=3.188 |
| sqlite-reach | reach | 100 | RESULT engine=sqlite-reach workload=reach scale=100 correct=true digest=-2221706294120251723 oracle=-2221706294120251723 ms=20.879 rust_peak=0.628 sqlite_hw=0.792 rss=8.969 |
| dd-reach | reach | 100 | RESULT engine=dd-reach workload=reach scale=100 correct=true digest=-2221706294120251723 oracle=-2221706294120251723 ms=20.623 rust_peak=1.212 sqlite_hw=0.000 rss=9.141 |
| ram-reach | reach | 1000 | RESULT engine=ram-reach workload=reach scale=1000 correct=true digest=1165635720824132967 oracle=1165635720824132967 ms=397.005 rust_peak=0.245 sqlite_hw=0.000 rss=4.062 |
| sqlite-reach | reach | 1000 | RESULT engine=sqlite-reach workload=reach scale=1000 correct=true digest=1165635720824132967 oracle=1165635720824132967 ms=2984.838 rust_peak=52.275 sqlite_hw=63.910 rss=168.203 |
| dd-reach | reach | 1000 | RESULT engine=dd-reach workload=reach scale=1000 correct=true digest=1165635720824132967 oracle=1165635720824132967 ms=349.288 rust_peak=126.707 sqlite_hw=0.000 rss=263.500 |

| engine | workload | scale | result |
|---|---|---:|---|
| Reconciler | excluded | - | trait interface; no concrete experiment state |
| sqlite-reconciler | excluded | - | reconciliation-DAG digest, not live-set or all-pairs reach |
| salsa-reconciler | excluded | - | reconciliation-DAG digest, not live-set or all-pairs reach; requires with-salsa |
| dd-bfs | excluded | - | single-source reachable-node digest, not all-pairs reach; requires with-dd |
