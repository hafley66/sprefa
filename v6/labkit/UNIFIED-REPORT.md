# Unified G4v2 report

| engine | workload | scale | result |
|---|---|---:|---|
| ram-zset | live | 100 | RESULT engine=ram-zset workload=live scale=100 correct=true digest=7183135401265645600 oracle=7183135401265645600 ms=0.008 rust_peak=0.003 sqlite_hw=0.000 rss=2.078 |
| sqlite-temporal | live | 100 | RESULT engine=sqlite-temporal workload=live scale=100 correct=true digest=7183135401265645600 oracle=7183135401265645600 ms=0.741 rust_peak=0.062 sqlite_hw=0.197 rss=5.891 |
| ram-zset | live | 1000 | RESULT engine=ram-zset workload=live scale=1000 correct=true digest=-4159754040995164509 oracle=-4159754040995164509 ms=0.060 rust_peak=0.034 sqlite_hw=0.000 rss=2.188 |
| sqlite-temporal | live | 1000 | RESULT engine=sqlite-temporal workload=live scale=1000 correct=true digest=-4159754040995164509 oracle=-4159754040995164509 ms=3.503 rust_peak=0.124 sqlite_hw=0.340 rss=6.391 |
| ram-reach | reach | 100 | RESULT engine=ram-reach workload=reach scale=100 correct=true digest=-2221706294120251723 oracle=-2221706294120251723 ms=3.971 rust_peak=0.024 sqlite_hw=0.000 rss=2.188 |
| sqlite-reach | reach | 100 | RESULT engine=sqlite-reach workload=reach scale=100 correct=true digest=-2221706294120251723 oracle=-2221706294120251723 ms=20.972 rust_peak=0.629 sqlite_hw=0.774 rss=7.375 |
| ram-reach | reach | 1000 | RESULT engine=ram-reach workload=reach scale=1000 correct=true digest=1165635720824132967 oracle=1165635720824132967 ms=437.753 rust_peak=0.245 sqlite_hw=0.000 rss=3.484 |
| sqlite-reach | reach | 1000 | RESULT engine=sqlite-reach workload=reach scale=1000 correct=true digest=1165635720824132967 oracle=1165635720824132967 ms=2778.680 rust_peak=52.275 sqlite_hw=63.891 rss=167.438 |

Store engines retained: CascadeZset, SqlReconciler, SqliteReachInc, SqliteReachDRed; optional engines retained: SalsaReconciler, SalsaRows, DdReach, DdBfs.
