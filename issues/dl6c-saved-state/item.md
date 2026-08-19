---
created: 2026-08-19
updated: 2026-08-19
type: task
status: open
priority: high
epic: productionize-rust-door
size: S
---

# dl6c: the compiler as one executable (SWI qsave_program stand_alone)

## Description

## Description
Ship the compiler as one executable. `qsave_program/2` with `stand_alone(true)` on SWI-Prolog 10.0.2 embeds the runtime; zero uses in tree today. Entry to wrap: the `compile_dl6/3` call `grade.sh:36` makes, with both emitters loadable (`emit_rust:emit_program`, `emit_ts`).
## Acceptance Criteria
- [ ] `just build-dl6c` writes `target/dl6c` (saved state, stand_alone); `dl6c <in.dl6> --target rust|ts --out <dir>` writes the same bytes `compile_dl6.sh` writes (byte-diff test on 3 fixtures incl. one with `use`, one with `sh`, one with anonymous types).
- [ ] `dl6c --version` prints the git sha it was built from (the `install-boop` stamp pattern, `crates/boop/scripts/install.sh` in hafley-rs).
- [ ] Foreign libs named: `library(crypto)` (SHA-256 in `0_type_ids.pl`), `http/json`; the state runs from a directory with no `v6/prolog` checkout (test copies it to a temp dir).
- [ ] `just install-dl6c` into `~/.cargo/bin` or `~/.local/bin`, documented.
## Implementation Notes
Build-vs-buy: `qsave_program` IS the bought tool; no custom launcher script. Autoload must be resolved before saving (`autoload(true)` or explicit `use_module`s); list what was missing.
