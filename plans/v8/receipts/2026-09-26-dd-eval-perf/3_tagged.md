# Shared tagged-row arrangement

Three serial direct-binary samples. A single `arrange_by_key` on relation ID supplies each relation collection within a stratum.

| fixture | DD wall, s | median before, s | median after, s | build, ms | step, ms |
|---|---:|---:|---:|---:|---:|
| `0_fs_json` | 1.87, 1.84, 1.84 | 1.97 | 1.84 | 310 | 1154 |
| `0_primitive_defaults` | 1.19, 1.20, 1.19 | 1.27 | 1.19 | 219 | 687 |

Build and step figures use the first new sample. The tagged arrangement adds build time and reduces step time.
