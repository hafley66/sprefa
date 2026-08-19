---
created: 2026-08-19
updated: 2026-08-19
type: task
status: open
priority: high
epic: productionize-rust-door
size: M
---

# dl6 build: one Rust binary per program from the emitted IR

## Description

## Description
One binary per program. The runtime already interprets `PROGRAM_JSON` over bundled rusqlite; a bin that `include!`s `<prog>.program.rs` + `<prog>.types.rs` and calls `run_schedule` / `serve` is what `emit_rust_harness.rs` and `tests/source_bind/_0_runtime.rs:63` do by hand. No dyloading; "all Rust, no IR" is NOT this card.
## Acceptance Criteria
- [x] `dl6 build <prog>.dl6 [--out <bin>]` (a subcommand on the engine crate or a `just` recipe; pick one, say why): runs `dl6c`, writes a cargo bin crate from a template under `target/dl6-build/<prog>/`, `cargo build --release`, copies the binary out.
- [x] The binary: `<prog> serve --socket <path>` (UDS, `serve.rs` routes `/rel/{name}`, `/rel/{name}/deltas`, `/arrive`), `<prog> run <schedule.json>` (tick log on stdout, byte-identical to `emit_rust_harness` on the same input, test), `<prog> --version` (program name + dl6c sha + ir_version).
- [x] `.adapters.json` sidecar is embedded at build (include_str) with `--adapters-dir` still able to override.
- [x] One test builds `golden-flex` and `resident-coroutine` end to end and runs each; wall time reported; 10-second law per step except the cargo build.
## Landed
Subcommand on the engine crate (`v6/sprefa-engine-rs/src/bin/dl6.rs`), with
`just dl6-build` as the wrapper: the builder has to read the template, the
engine path and the compiler sha, and a recipe would have to re-derive all
three in shell. The compiler is the `grade.sh:36` swipl call behind one
function (`Dl6Compiler`), not `dl6c`: `dl6c` has no `.types.rs` target and
needs `just build-dl6c` first, so a fresh checkout would not build.
`clap` was NOT a direct dependency of the engine crate; it is now (it was
already in the lock through soopy).

`golden-flex` is NOT one of the two programs the test builds. At
`de8e2c0a2` it stops at `unsupported_construct: column_type_unknown` on
BOTH doors with the emitters untouched, so it cannot reach a build;
`door-handwritten` stands in. Filed separately.

## Implementation Notes
Template = the harness bin minus the arg parsing it no longer needs. Clap already in tree. Cargo offline build must work (vendored path dep on `sprefa-engine-rs` by absolute path for now; crates.io publish is a later card).
