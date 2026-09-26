# Skip absent stored-kernel joins

Positive kernel goals previously joined against stored rows as well as calling the kernel solver. A stored-row join is now built only when that relation can have stored rows: it is seeded, headed by a rule, or is one of the four evaluator-generated relations (`nil`, `intern`, `effect`, `intern_snapshot`). The kernel solver and its deduplication still run for every positive kernel goal.

Three serial direct-binary DD `compile --trace` samples per fixture with the existing target directory. Current JSON matched Rust for both fixtures in debug and release. Before values are from `9_side_rows.md`.

| fixture | debug DD before → after s | release DD before → after s | release Rust s | release DD/Rust after |
|---|---:|---:|---:|---:|
| `0_fs_json` | 0.692 → 0.648 | 0.537 → 0.526 | 0.119 | 4.4× |
| `_8` `0_primitive_defaults` | 0.448 → 0.448 | 0.401 → 0.339 | 0.105 | 3.2× |

With `DL8_DD_PROFILE=1`, total operators built fell from 27,498 to 24,207 on `0_fs_json` and from 18,494 to 16,276 on the `_8` fixture. Per-stratum rounds and net new rows matched `8_release_strata.md`. The 2× target remains unmet.
