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
- [ ] `dl6 build <prog>.dl6 [--out <bin>]` (a subcommand on the engine crate or a `just` recipe; pick one, say why): runs `dl6c`, writes a cargo bin crate from a template under `target/dl6-build/<prog>/`, `cargo build --release`, copies the binary out.
- [ ] The binary: `<prog> serve --socket <path>` (UDS, `serve.rs` routes `/rel/{name}`, `/rel/{name}/deltas`, `/arrive`), `<prog> run <schedule.json>` (tick log on stdout, byte-identical to `emit_rust_harness` on the same input, test), `<prog> --version` (program name + dl6c sha + ir_version).
- [ ] `.adapters.json` sidecar is embedded at build (include_str) with `--adapters-dir` still able to override.
- [ ] One test builds `golden-flex` and `resident-coroutine` end to end and runs each; wall time reported; 10-second law per step except the cargo build.
## Implementation Notes
Template = the harness bin minus the arg parsing it no longer needs. Clap already in tree. Cargo offline build must work (vendored path dep on `sprefa-engine-rs` by absolute path for now; crates.io publish is a later card).
