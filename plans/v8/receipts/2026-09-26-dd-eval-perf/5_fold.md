# Declared fold snapshot sharing

DD puts each stored row once into the reducer snapshot, groups proof values by output key, and builds one `Store` and rule index for all groups. The Rust evaluator's single-group entry point calls the same shared helper with one group.

| fixture | DD wall, s | median before, s | median after, s |
|---|---:|---:|---:|
| `fixtures/fold/1_program_step.dl7` | 0.40, 0.35, 0.36 | 0.39 | 0.36 |
| `fixtures/comptime_effect/0_fs_json.dl7` | 0.85, 0.85, 0.84 | 0.90 | 0.85 |

Three serial direct-binary samples. Output JSON equaled the Rust engine in every sample.

DD gate: 10/10. `_23_comptime_effect`: 4/4. `_8_compile_oracle`: 3/3, parity test 19.731 s. SQLite `_15_fold`: 2/2. `cargo clippy --offline -j 4 --all-targets`: exit 0 with no new evaluator warnings.
