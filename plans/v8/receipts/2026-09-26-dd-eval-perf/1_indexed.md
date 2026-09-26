# Shared relation collections and arrangements

Three serial direct-binary samples after indexing, same commands and target as `0_baseline.md`.

| fixture | DD wall, s | median before, s | median after, s | build, ms | step, ms |
|---|---:|---:|---:|---:|---:|
| `0_fs_json` | 4.52, 4.34, 3.68 | 5.04 | 4.34 | 499 | 3416 |
| `0_primitive_defaults` | 2.28, 2.50, 2.61 | 3.06 | 2.50 | 240 | 1666 |

Build and step figures use the first indexed sample. They cover six and four DD calls respectively. The relation collection cache is keyed by relation and arity within one stratum. The arrangement cache additionally keys by joined column positions.
