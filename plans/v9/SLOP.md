# v9 slop log

Slop accepted at review: file, what, why accepted.

| punch | file | what | why accepted |
| --- | --- | --- | --- |
| P1 | `v9/src/_3_plan.rs` `check` | `Plan::hash` is `id_of(format!("{program:?}"))`; derived `Debug` text could shift across rustc versions and make a reopen report `Mismatch` | one impl, local use; replace with a canonical encoder when plans persist across toolchains |
| P1 | `v9/src/_3_plan.rs` | a Derived rel with no rules gets an empty stratum | harmless to the engine; filter when a reader minds |
| P1 | `v9/src/skeleton/` | walking-skeleton modules kept under `skeleton::` until P6 ports reach onto Core | removed in P6 |
| P5a | `v9/schema/0_wire.tsp` `Cell` | `text \| int` written as a model of two optional fields; JSON carries `{"text":"a","int":null}` | alloy-rs maps unions to `string` (`hafley-tsp/packages/rust/src/adapters/00_typespec-to-neutral.ts:111`); fix in the emitter, then make `Cell` a union |
| P5a | `v9/schema/0_wire.tsp` `Column` | field `col_type`, not `type` | emitter does not escape Rust keywords in model fields (`emitter/02_emit-model.tsx:48`) |
| P5a | `v9/src/_9_wire/ops.rs` | one process-wide `OnceLock<Arc<dyn Wire>>`; `impl FromStr for Change` only to satisfy a derived `clap::Args` | emitter has no axum state and always derives clap (`7_OpsTransports.tsx:76`) |
| P5a | `v9/src/_9_wire/http_auto.rs` | every `OpError` is HTTP 500, a check `Diag` included | generated error type is a string; status mapping waits for an error model in the contract |
| P5a | `v9/schema/package.json` | `@hafley66/alloy-rs` is `link:` to the hafley-tsp checkout, no version lock | published versions lack the HTTP emitter; publish a dev version and pin |
