# DL7 shared architecture graph and deterministic D2 projection

Date: 2026-09-10
Base: `4cb1542e6`
Scope: new userland application `v7/applications/architecture/`, one focused
PLUnit file, and additive `v7/justfile` recipes. No compiler kernel change, no
V6 change, no `sqlite_ivm` change, no existing DL6 application change, no Rust.

## Status

Complete. One authored DL7 source (`0_system.dl7`) publishes the shared
architecture graph through the existing `emits/3` protocol. `1_generate.pl`
compiles that source, sorts every row by a stable id, validates references,
computes counts and fractions, and writes `2_system.d2` and `2_progress.md`.
Generation is byte-stable and the tracked outputs match a fresh derivation.

## Files

| file | role |
| --- | --- |
| `v7/applications/architecture/0_system.dl7` | authored graph and emitter |
| `v7/applications/architecture/1_generate.pl` | compile, emit, validate, render |
| `v7/applications/architecture/2_system.d2` | generated system map |
| `v7/applications/architecture/2_progress.md` | generated counts and fractions |
| `v7/applications/architecture/README.md` | reading order and invariants |
| `v7/test/21_architecture_generate.test.pl` | focused gate |
| `v7/justfile` | `architecture-generate`, `architecture-check`, `architecture-render`, `architecture-test` |

## Graph counts

| metric | count |
| --- | --- |
| components | 72 |
| typed attachment edges | 75 |
| milestones | 72 |
| acceptance item rows | 7 |
| evidence paths | 179 |
| phase axis rows | 3 |
| concrete flow steps | 19 |
| state implemented | 59 |
| state partial | 7 |
| state planned | 3 |
| state absent | 2 |
| state reference | 1 |

## Percentages

Completed acceptance item counts over declared counts, from the source named by
each milestone. Seven components declare a denominator; all are complete. The
other 65 render `unknown`; no denominator is invented.

| component | fraction | percent |
| --- | --- | --- |
| `dl7.kernel` | 10/10 | 100% |
| `dl7.userland` | 22/22 | 100% |
| `dl7.emit.dbsp` | 4/4 | 100% |
| `dl7.emit.sqlite` | 5/5 | 100% |
| `dl7.bench` | 17/17 | 100% |
| `ivm.sqlite` | 64/64 | 100% |
| `dl6.conformance` | 163/163 | 100% |

## Validation

- `just architecture-generate` writes both files; a second run is
  byte-identical to the first (`cmp` clean on both pairs).
- `just architecture-check` reports no drift on the tracked pair, and returns
  exit 1 with the tracked file unmodified when `2_system.d2` is tampered.
- `d2 validate v7/applications/architecture/2_system.d2` is valid; `d2`
  renders `index.svg` plus the `compiler`, `runtime_tick`, `reload`, and
  `reference` layers into a temporary directory.
- `just architecture-test` runs 10 of 10 cases in 1.9 s wall. Individual cases
  are under 0.01 s except the exact-row case at 0.005 s; the shared compile is
  1.1 s.
- `just -s test/17_dl6_interned.test.pl run_tests` still passes in 2.2 s.

## D2 model

`2_system.d2` quotes every generated id, edge label, and node label. Components
are grouped into `compile`, `algebra`, `runtime`, `hosts`, `effects`, and
`generated` containers. Each of `depends_on`, `emits`, `lowers_to`, `hosts`,
`reads`, `writes`, `watches`, `reloads`, `effects`, and `later_target` carries
its own styled edge class. The `runtime_tick` layer is the concrete flow
filesystem/Git/editor event to hosted `sprefa-extract`, signed TSI facts,
`sqlite_ivm` maintenance, DL7 query view, and LSP diagnostic or shell/HTTP
effect. The `reference` layer is DL6 Prolog to `ProgramJson` to
`sprefa-engine-rs`/SQLite, and the retained V7 successor mapping through
`rt.executor` to `rt.v7`.

## CI coverage change

Adds one focused SWI-Prolog test file and one `just` test recipe
(`architecture-test`). Adds `architecture-generate` and `architecture-check`
recipes; the check recipe is the drift gate. No existing test coverage is
removed or changed.

## Next

- Wire `just architecture-check` into the integration gate once the v7 suite
  runner names per-file SWI targets.
- Replace the count-weighted `acceptance_item` rows with one row per declared
  item if a source enumerates item identities.
