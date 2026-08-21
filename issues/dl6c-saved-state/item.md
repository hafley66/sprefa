---
created: 2026-08-19
updated: 2026-08-21
type: task
status: done
priority: high
epic: productionize-rust-door
size: S
related: ['@cheap-fast-analysis']
closed: 2026-08-21
closed_by: chris
commits:
- hash: 1c1e6171e
  summary: dl6c.sh saved state, load 195 to 44ms
---

# dl6c: the compiler as one executable (SWI qsave_program stand_alone)

## Description

## Description
Ship the compiler as one executable. `qsave_program/2` with `stand_alone(true)` on SWI-Prolog 10.0.2 embeds the runtime; zero uses in tree today. Entry to wrap: the `compile_dl6/3` call `grade.sh:36` makes, with both emitters loadable (`emit_rust:emit_program`, `emit_ts`).
## Acceptance Criteria
- [x] `just build-dl6c` writes `target/dl6c` (saved state, stand_alone); `dl6c <in.dl6> --target rust|ts --out <dir>` writes the same bytes `compile_dl6.sh` writes (byte-diff test on 3 fixtures incl. one with `use`, one with `sh`, one with anonymous types).
- [x] `dl6c --version` prints the git sha it was built from (the `install-boop` stamp pattern, `crates/boop/scripts/install.sh` in hafley-rs).
- [x] Foreign libs named: `library(crypto)` (SHA-256 in `0_type_ids.pl`), `http/json`; the state runs from a directory with no `v6/prolog` checkout (test copies it to a temp dir).
- [x] `just install-dl6c` into `~/.cargo/bin` or `~/.local/bin`, documented.
## Implementation Notes
Build-vs-buy: `qsave_program` IS the bought tool; no custom launcher script. Autoload must be resolved before saving (`autoload(true)` or explicit `use_module`s); list what was missing.

## Landed

`v6/prolog/dl6c.pl` (`main/0` + `dl6c_save/1`), `justfile` recipes
`build-dl6c` / `install-dl6c`, plunit unit `compile/test/dl6c.test.pl` (14
tests), shell test `compile/scripts/dl6c_roundtrip.sh` (12 comparisons),
`v6/prolog/README.md`.

Byte-diff fixtures are `source-mutations.dl6` (`use` AND `sh`),
`type-name-module-prefix.dl6` (`use`), `resident-coroutine.dl6` and
`anonymous-type-syntax.dl6` (anonymous types), both targets each.
`golden-flex.dl6` is NOT among them: it `use`s `0_golden-flex-imported.dl6`
and `1_golden-flex-namespaced.dl6`, and neither file is in git, so it throws
`use_path_unresolved` for every caller at this base. Separate defect.

Autoload resolution forced ZERO new `use_module` lines: `qsave_program`'s
`autoload(true)` resolved every library the compiler reaches, and a state
saved with `autoload(false)` still compiled all four fixtures for both
targets. `library(crypto)`, `library(pcre)`, `library(http/json)`,
`library(uri)` and `library(files)` load as five foreign `.so` files from the
installed SWI at run time; embedding them with `foreign(save)` is a defect,
`docs/failure-modes.md` row 56.
