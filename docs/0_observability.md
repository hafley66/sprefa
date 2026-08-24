# Observability knobs

Every Rust executable uses `hafley-observe` for its stderr formatter and
process identity event.

| Variable | Values | Scope |
|---|---|---|
| `RUST_LOG` | `tracing-subscriber` filter expression | Every executable |
| `HAFLEY_LOG_FORMAT` | `human`, `text`, or `json` | Every executable |
| `DL_LOG` | `tracing-subscriber` filter expression | Sprefa rolling `dl.log` only |
| `DL_TRACE_CHROME` | Output path | Sprefa Perfetto export only |
| `DL_TRACE_SUMMARY` | `1` | Extract and engine aggregate summaries |

`DL_TRACE` remains a Sprefa CLI compatibility fallback when `RUST_LOG` is
unset. New commands and automation should set `RUST_LOG`.

Every JSON startup event contains:

```text
service.name
service.version
process.pid
log.format
```

Application-specific durable layers remain attached to the same subscriber:

- Sprefa `dl.log` and `error.log` rolling writers;
- Sprefa daemon event trail;
- Sprefa Chrome/Perfetto export;
- sprefa-extract summary aggregation;
- sprefa-engine summary aggregation.

Examples:

```bash
RUST_LOG=debug HAFLEY_LOG_FORMAT=json dl --help
RUST_LOG=sprefa_engine_rs=trace HAFLEY_LOG_FORMAT=json dl6 run program.dl6
RUST_LOG=sprefa_extract=debug HAFLEY_LOG_FORMAT=human extract file.rs
```
