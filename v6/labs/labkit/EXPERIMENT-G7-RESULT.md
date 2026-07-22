# G7 wire result

| engine | live | reach |
|---|---|---|
| RamZset | yes | excluded: live-set answer |
| CascadeZset | yes | excluded: live-set answer |
| SqliteTemporal | yes | excluded: live-set answer |
| SalsaRows | yes (`with-salsa`) | excluded: live-set answer |
| RamReach | excluded: all-pairs reach answer | yes |
| SqliteReach | excluded: all-pairs reach answer | yes |
| DdReach | excluded: all-pairs reach answer | yes (`with-dd`) |
| Reconciler | excluded: trait interface, no concrete experiment | excluded: trait interface, no concrete experiment |
| SqlReconciler | excluded: reconciliation-DAG digest | excluded: reconciliation-DAG digest |
| SalsaReconciler | excluded: reconciliation-DAG digest (`with-salsa`) | excluded: reconciliation-DAG digest (`with-salsa`) |
| DdBfs | excluded: single-source reachable-node digest (`with-dd`) | excluded: single-source reachable-node digest (`with-dd`) |

Added: CascadeZset to `live`, with source multiplicities tracked across its binary assert/retract transitions; SalsaRows to `live`; DdReach to `reach`.

Validation:

```text
$ cargo build
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.27s

$ cargo build --features with-dd,with-salsa
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.07s

$ cargo run --bin 0_unified
Finished `dev` profile [unoptimized + debuginfo]
Running `target/debug/0_unified`

$ cargo run --features with-dd,with-salsa --bin 0_unified
Finished `dev` profile [unoptimized + debuginfo]
Running `target/debug/0_unified`
```

`UNIFIED-REPORT.md` from the feature build has 14 executed cells; all report `correct=true`. Scale 100 and 1000 values differ for each executed engine's digest and timing columns.
