# Seed retraction reuse

Three serial release `dl8 compile --trace` samples per engine and fixture. Before values and counts are from [equal-rules reuse](../2026-09-26-dd-eval-perf/12_equal_rules_reuse.md). After samples are in [release_samples.tsv](release_samples.tsv). DD and Rust stdout hashes and exit codes matched in every pair.

| fixture | DD before → after median s | Rust after median s | cache hits / rebuilds before → after |
|---|---:|---:|---:|
| `fixtures/comptime_effect/0_fs_json.dl7` | 0.421 → 0.373 | 0.128 | 1 / 5 → 4 / 2 |
| `fixtures/declare/0_primitive_defaults.dl7` (`_8`) | 0.326 → 0.308 | 0.109 | 0 / 4 → 2 / 2 |

The new `_29_eval_random` sequence test ran 900 seed sets in 1,367 ms at the default 200 generated programs and 2,253 seed sets in 3,701 ms with `DL8_RANDOM_SEEDS=501`. Every cached step matched fresh DD and Rust. The existing generator test also passed at both counts.

Gate: `_0_eval_oracle` 1/1, `_28_eval_dd` 2/2, `_27_eval_sqlite` 1/1. Under `DL8_ENGINE=dd`: `_23_comptime_effect` 4/4, `_15_fold` 2/2, `_14_host_effect` 3/3, `_8_compile_oracle` 3/3, `_13_store` 4/4, `_17_reconcile` 9/9. Clippy emitted no warnings in changed files. CI coverage adds the `_29_eval_random` metamorphic test.
