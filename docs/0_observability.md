# dl8 environment

`dl8` logs through `hafley-observe` (`src/bin/dl8.rs:182`). Every variable
below is one `src/` reads; `git grep -n 'env::var' -- src` lists them.

## Logging and limits

| Variable | Values | Read at |
|---|---|---|
| `RUST_LOG` | `tracing-subscriber` filter; unset is `warn`, or `dl8=debug` when the command asks for a trace | `src/bin/dl8.rs:177-182` through `Config::from_env` |
| `HAFLEY_LOG_FORMAT` | `human`, `text`, `json` | `Config::from_env` in `hafley-observe` |
| `HAFLEY_MEMCAP_GB` | integer GB of physical footprint; default 8; past it the process exits 137 | `hafley_observe::memcap::spawn`, `src/bin/dl8.rs:175` |

## Engine modes

| Variable | Values | Read at |
|---|---|---|
| `DL8_ENGINE` | unset or `ir:dd`; any other value is an error | `src/_6_eval/_9b_ir_eval.rs:1235` |
| `DL8_MACROTIME` | unset or `rust`, `engine` | `src/_1_macrotime/_8_waves.rs:24` |
| `DL8_ONE_ENGINE` | unset or `1`, `0` | `src/_6_eval/_9d_one_engine.rs:17` |
| `DL8_RULES` | unset or `compile`, `interpret` | `src/_6_eval/_9a1_rule_interp.rs:22` |
| `DL8_TERM_SWEEP` | `1` turns it on | `src/_8_driver/_7_one.rs:46` |

## Paths and dumps

| Variable | Values | Read at |
|---|---|---|
| `DL8_IR_DUMP` | directory; each installed IR program is written there | `src/_6_eval/_9b_ir_eval.rs:1066` |
| `DL8_READ_PATH` | path `dl8 read` reports instead of the input path | `src/_0_read/_3_cli.rs:18` |
| `SPREFA_EXTRACT_BIN` | extractor binary; unset falls back to `ryi` on `PATH` | `src/_9_runtime/_3_executors/extract.rs:160` |
| `SQLITE_IVM_LIB` | sqlite_ivm extension path; unset uses the one `just ivm-ext` builds | `src/_9_runtime/_1_sqlite.rs:73` |

## Examples

```bash
RUST_LOG=dl8::phase=info dl8 compile program.dl7
RUST_LOG=dl8::ir=info HAFLEY_LOG_FORMAT=json dl8 compile program.dl7
DL8_IR_DUMP=target/ir dl8 compile program.dl7
```

`dl8::phase` logs one line per phase with its `ms`; `dl8::ir` logs install,
bootstrap, settle and snapshot counts.
