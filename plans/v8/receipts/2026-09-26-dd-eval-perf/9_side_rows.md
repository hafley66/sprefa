# Omit empty side-row streams

`lower_rule` previously built an empty filtered collection and concatenated it to every rule head, even when the rule could not emit effect or `intern` rows. Side rows are now created only for those goals. The rule head and row generation are otherwise unchanged.

Three serial `compile --trace` samples per DD fixture with the existing target directory. Medians are compared with `7_status.md` for debug and `8_release_strata.md` for release. The current DD JSON matched Rust on both fixtures in both profiles.

| fixture | debug DD before → after s | release DD before → after s | release Rust s | release DD/Rust after |
|---|---:|---:|---:|---:|
| `0_fs_json` | 0.84 → 0.692 | 0.659 → 0.537 | 0.119 | 4.5× |
| `_8` `0_primitive_defaults` | 0.56 → 0.448 | 0.441 → 0.401 | 0.105 | 3.8× |

With `DL8_DD_PROFILE=1`, operator construction fell from 30,150 to 27,498 across the six `0_fs_json` evaluator calls and from 20,270 to 18,494 across the four `_8` calls. Per-stratum rounds and net new rows were identical to `8_release_strata.md`. The 2× target remains unmet.
