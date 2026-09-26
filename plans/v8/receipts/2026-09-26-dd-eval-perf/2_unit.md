# Shared nil environment source

Three serial DD samples after replacing each rule's `all.map(...).distinct()` environment source with one nil-row unit collection per stratum. Same direct binary commands and target as `0_baseline.md`.

| fixture | DD wall, s | median before, s | median after, s | build, ms | step, ms |
|---|---:|---:|---:|---:|---:|
| `0_fs_json` | 2.01, 1.95, 1.97 | 4.34 | 1.97 | 291 | 1329 |
| `0_primitive_defaults` | 1.27, 1.27, 1.29 | 2.50 | 1.27 | 205 | 791 |

Build and step figures use the first new sample. Output JSON equaled the Rust engine in each sample.
