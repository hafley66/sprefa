# DD evaluator final status after equal-rules reuse

Three serial release `compile --trace` samples per engine on the two representative fixtures, with the existing target directory. DD JSON matched Rust for every paired sample. The before column is `11_status.md`; the cached-dataflow operator counts are from `12_equal_rules_reuse.md`.

| fixture | DD release before → after s | Rust release s | DD/Rust after | DD operators built before → after |
|---|---:|---:|---:|---:|
| `0_fs_json` | 0.526 → 0.421 | 0.119 | 3.5× | 24,207 → 17,330 |
| `_8` `0_primitive_defaults` | 0.339 → 0.326 | 0.106 | 3.1× | 16,276 → 16,276 |

The existing limits pass: `_23_comptime_effect` 4/4 and `_8_compile_oracle` 3/3; its parity test took 7.619 s under the 60 s limit. The required six-file DD gate passed 10/10. SQLite `_15_fold` passed 2/2. [13_oracle_cases.tsv](13_oracle_cases.tsv) has three serial debug samples per engine for every committed `_8` case, with JSON and exit-code parity. **0/16** cases met 2× using the unrounded median ratios; the nearest two measured 84/41 ms and 86/42 ms.

Equal-rules reuse cut `5_curry` from 6.532 s in the earlier single debug sample to a 1.762 s three-sample debug median. Its release median was 1.383 s DD versus 0.413 s Rust. It reused 26/34 DD calls. In one current debug trace, its 34 DD evaluations took 1,326 ms: 849 ms in stratum stepping, 189 ms building eight dataflows, and the remainder in setup and extraction. Levels 2 and 5 used 303 ms and 297 ms of step time. A current `0_fs_json` trace spent 234 ms stepping and 79 ms building five dataflows across six calls.

The 2× target remains unmet. The remaining measured cost is chiefly execution of the rule-specific operator graphs at levels 2 and 5, plus builds when the effective rules or seeded relation set change. No CI coverage was added, changed, or removed.
