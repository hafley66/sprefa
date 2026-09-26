# Reuse DD dataflows with equal rules

The DD evaluator now retains two worker/dataflow states on the calling thread. A state is reused when its effective rules, served relations, and seeded relation set match; its term arena is a prefix of the current arena; and the new seed set only adds rows. New seed rows enter the existing input at the next timestamp. The settled output rows are updated from DD diffs. A retraction or changed graph builds a fresh state. The two-entry bound follows the macrotime/comptime alternation in `_7_sqlite_eval.rs`.

Three serial release direct-binary `compile --trace` samples per engine with the existing target directory. DD JSON and exit codes matched Rust for every paired run. Before values are from `10_stored_kernel.md`.

| fixture | DD release before → after s | Rust release s | DD/Rust after | operators built before → after | reused DD calls |
|---|---:|---:|---:|---:|---:|
| `0_fs_json` | 0.526 → 0.421 | 0.119 | 3.5× | 24,207 → 17,330 | 1/6 |
| `_8` `0_primitive_defaults` | 0.339 → 0.326 | 0.106 | 3.1× | 16,276 → 16,276 | 0/4 |

The operator counts are sums from one `DL8_DD_PROFILE=1` debug compile per fixture. Each reuse builds zero operators. `5_curry` reused 26/34 evaluator calls; its release median was 1.383 s DD versus 0.413 s Rust. The DD gate including `_23_comptime_effect` and `_8_compile_oracle` passed 17/17. SQLite `_15_fold` passed 2/2.

The target remains unmet. In one current debug `5_curry` trace, 34 DD evaluations took 1,326 ms: 849 ms stratum stepping and 189 ms dataflow building. Levels 2 and 5 accounted for 303 ms and 297 ms of stepping. The cache removes repeated graph builds and most repeated row processing when rule and seed conditions permit; the remaining step work is concentrated in those two strata.
