---
created: 2026-08-19
updated: 2026-08-19
type: epic
owner: hafley66
status: open
priority: high
---

# Productionize the Rust door: dl6c binary, dl6 build single binary, green CI, fresh-machine page

## Description

## Description

Chris 2026-08-19: "i would like to start using this elsewhere and need it productionized, what is our gaps". Scope: the Rust door first (the TS door needs the sprefa checkout and a pnpm workspace; it is a later epic). Outcome: on a clean machine, `dl6c` compiles a `.dl6` to a program, `dl6 build` makes one binary from it, the binary serves over UDS, CI is green, one page gets a stranger from zero to a running program.

## Facts measured 2026-08-19

- Compiler entry today: `swipl -l v6/prolog/compile.pl -l v6/prolog/emit_rust.pl -g compile_dl6(...)` (`v6/sprefa-engine-rs/grade.sh:36`). Zero uses of `qsave_program` anywhere under `v6/prolog`. SWI-Prolog 10.0.2 arm64 installed.
- Emitted Rust program = one module holding `pub const PROGRAM_JSON: &str = r#"..."#` + `program()` (`v6/prolog/emit_rust.pl:594-608`); the runtime interprets the IR (`program.rs:49 GenProgram::from_json`) over bundled rusqlite. No dyloading. A single binary = a bin that `include!`s `<prog>.program.rs` + `<prog>.types.rs` (hand-done today in `tests/source_bind/_0_runtime.rs:63` and `src/bin/emit_rust_harness.rs`).
- `ProgramJson` carries no version field; `incremental_safe: true` is a fossil (`emit_rust.pl:587`).
- CI: `just green-all` red by design; `.github/CI-KNOWN-RED.md` allowlist; `1_extraction-clock-golden.sh 62 !== 59`, `just typecheck` (golden-flex.ts), `flagship-flow.sh` needs the v5 binary.
- Env coupling: `DL_EXTRACT_BIN`, `DL_ADAPTERS_DIR` (default repo-relative, `types.rs:631`), `SOOPY_BIN`, `$DL_*` in fixtures.

## Cards (children)

| card | size | blocked_by |
|---|---|---|
| dl6c-saved-state | S | - |
| dl6-build-single-binary | M | - |
| program-ir-version | S | - |
| ci-red-legs-green | M | - |
| engine-config-no-env | S | dl6-build-single-binary |
| fresh-machine-page | S | dl6c-saved-state, dl6-build-single-binary |
| host-executors-installable | M | dl6-build-single-binary |

Out of scope here, tracked elsewhere: `bind` deletion (Phase 4), TS door packaging, 110 `unsupported` fixtures.
