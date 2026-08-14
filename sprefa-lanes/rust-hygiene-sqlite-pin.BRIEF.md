# Lane: SQLite pin bump, grade.sh into the gate, one eprintln waiver

Three small independent defects, all mechanical, no design decisions.

## Base
`git merge --ff-only 0b672fc1` is your FIRST action. Failure = STOP AND REPORT.
Worktree: `.boop-worktrees/fix/rust-hygiene-sqlite-pin`.

## Defect 1: five crates bundle SQLite 3.46.0, inside the WAL-reset bug range

`bundled` compiles SQLite from source and ignores the system copy. Read from the
lockfiles 2026-08-12:

| file | pin | bundles |
|---|---|---|
| `Cargo.toml:84` | `rusqlite = "0.32.1"` | libsqlite3-sys 0.30.1 |
| `v6/sprefa-engine-rs/Cargo.toml` | `rusqlite = "0.32"` | 0.30.1 |
| `v6/dd-runner/Cargo.toml` | `rusqlite = "0.32"` | 0.30.1 |
| `v6/labs/exec_shootout/sqlite_baseline/Cargo.toml` | `rusqlite = "0.32"` | 0.30.1 |
| `v6/labs/exec_shootout/intern_bench/Cargo.toml` | `rusqlite = "0.32"` | 0.30.1 |
| `v6/boop/Cargo.toml` | `rusqlite = "0.40"` | 0.38.2, ALREADY FINE |

The WAL-reset bug fires only with two or more connections on one file
(sqlite.org/wal.html section 11). v6 opens one connection today, so nothing is
currently broken. This is a pin bump ahead of the Rust-plus-TS design, not an
incident.

**VERIFY THE VERSION CLAIM YOURSELF.** The affected range (3.7.0 through 3.51.2,
fixed 3.51.3) comes from a research doc, not from a page anyone read this
session. Read sqlite.org/wal.html section 11 and report what it actually says.
If the range is wrong, say so and stop; do not bump on a claim you could not
confirm.

Bump the five to `0.40`, matching boop. Report the resulting SQLite version from
`select sqlite_version()` in each crate.

## Defect 2: `grade.sh` is not in any `just` leg

`v6/sprefa-engine-rs/grade.sh` prints `RUST-GRADE graded=392 byte-clean=230`.
No `just` leg runs it, so the ratchet is invisible to CI. Add a leg. It is 14s
cold and ~9s warm and builds one crate total, so it is inside the 10-second law
warm and just over it cold; note which you measured.

Wire it as a ratchet with the current number pinned, so a regression below 230
fails. Additive only.

## Defect 3: one unwaived eprintln

`v6/sprefa-engine-rs/src/bin/emit_rust_harness.rs:53` carries an `eprintln!`
with no `@eprintln-ok` marker. The law is: no `eprintln!` in src/**, `tracing`
only, rare CLI-UX lines carry `@eprintln-ok`. Decide which this is. A harness
diagnostic line is CLI-UX and takes the marker; anything else becomes `tracing`.

## Gates, three runs each, never from the whole gate
```
cargo build --workspace
cargo test --no-fail-fast
bash v6/sprefa-engine-rs/grade.sh     # must still say byte-clean=230
just conformance                      # 392 PASS / 0 FAIL
```
`just green-all` is RED by design. `.github/CI-KNOWN-RED.md` is the real gate and
is stale by 9 rows. Do not chase anything listed there.

## Files you own
The five `Cargo.toml` files and their lockfiles, `v6/justfile`,
`v6/sprefa-engine-rs/src/bin/emit_rust_harness.rs`, `v6/sprefa-engine-rs/grade.sh`,
plan doc `plans/2026-08-12-rust-hygiene-sqlite-pin.md`.

## Files you must NOT touch
`v6/prolog/**`, `v6/boop/**`, `v6/sprefa-engine-rs/src/**` except the one binary
named above. Other lanes own those.

## Laws
- Infra is bought, never built.
- Doubt yourself before asserting. Cite what you read.
- Comments state only constraints the code cannot show. No dates, no narrative.
- No em dashes. No negative parallelism. No sycophancy.
- Banned in prose AND identifiers: provenance, substrate, load-bearing, regime.

## Report
The sqlite_version() per crate, the new just leg name, the eprintln decision,
and the three gate outputs.
