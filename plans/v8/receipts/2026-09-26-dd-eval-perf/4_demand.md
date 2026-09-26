# Demand rule reachability

Hidden demand rules enter a stratum only when a visible rule at that level reads them, transitively through hidden rule bodies. The existing first-safe-stratum condition remains.

| fixture | DD wall, s | median before, s | median after, s | build, ms | step, ms |
|---|---:|---:|---:|---:|---:|
| `0_fs_json` | 0.95, 0.88, 0.90 | 1.84 | 0.90 | 135 | 497 |
| `0_primitive_defaults` | 0.58, 0.60, 0.58 | 1.19 | 0.58 | 84 | 301 |

Three serial direct-binary samples per fixture. Build and step figures use the first sample. Output JSON equaled the Rust engine in every sample.

`_23_comptime_effect`: 4/4, slowest 1.018 s. `_8_compile_oracle`: 3/3, parity test 18.976 s, below its existing 60 s limit. The required six-file DD gate: 10/10. `cargo clippy --offline -j 4 --all-targets`: exit 0 with no new DD warnings.
