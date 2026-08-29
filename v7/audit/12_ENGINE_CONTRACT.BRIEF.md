# Slice 12: canonical program to engine contract

Read `0_SHARED.md` first.

Primary files:

- canonical schedule/program production in `v6/prolog/compile.pl`
- `v6/sprefa-engine-rs/src/program.rs`
- `v6/sprefa-engine-rs/src/incremental.rs`
- `v6/sprefa-engine-rs/src/types.rs`
- engine tests that deserialize schedules or compile `.dl6`

Treat the Rust engine as retained. Inventory every serialized field and semantic
assumption that V7 must continue emitting. Identify `.dl6` filename or compiler
invocation coupling separately from schedule semantics.

Write `v7/1_AUDIT/results/12_ENGINE_CONTRACT.md`.
