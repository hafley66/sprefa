# Lane: add `emitter-rust-sqlite-ir` as the fourth exec_shootout engine

## Base
`git merge --ff-only e70417d9` is your FIRST action, then merge
`origin/feature/emit-rust-climb-3` (PR #226, grade 280) in.
Failure = STOP AND REPORT.
Worktree: `.boop-worktrees/lab/exec-shootout-emitter-rust-sqlite-ir`.

## What exists

`v6/labs/exec_shootout/` compares three Rust execution strategies on one
datalog workload. Read `CONTRACT.md` and `STANDINGS.md` before anything else.

| dir | engine | the layer it measures |
|---|---|---|
| `interp/` | IR interpreter, in memory | rules as DATA, generic tuple store, zero per-program types |
| `rxgraph/` | rx operator graph, in memory | boxed operators wired at startup, dynamic dispatch |
| `mono/` | **compiler-emitted Rust**, in memory | `v6/prolog/labs/emit_rust_shootout/emit_rust.pl` writes `mono/src/main.rs`; concrete u32, FxHashMap, semi-naive loop unrolled |

Measured, best of 3, derived rows/sec in the fixpoint phase:

| shape/scale | interp | rxgraph | mono |
|---|---:|---:|---:|
| chain 10k | 7,089,513 | 55,534,517 | **68,001,449** |
| chain 1M | 3,852,038 | **20,746,660** | 15,872,841 |
| grid 1M | 6,495,238 | 31,580,648 | **39,934,647** |
| layered 1M | 10,966,864 | 36,899,786 | **50,078,281** |

mono wins 8 of 9 cells. interp wins 0 and carries 5-11x the RSS.

## What is missing, and is your job

The PRODUCTION path is absent from this bench. It is a fourth strategy and
nobody has priced it:

```
.dl6 -> swipl lower.pl -> emit_rust.pl -> a .rs file whose whole body is
        a JSON string -> sprefa-engine-rs reads it as TEXT ->
        GenProgram::from_json -> run_tick walks the plan -> SQL -> SQLite
```

Add it as `emitter-rust-sqlite-ir/`. The name is the user's: it is an emitter
producing an IR that a fixed Rust runtime interprets **over SQLite**, and the
SQLite half is the point, not an implementation detail.

It differs from `interp/` on the axis that matters: `interp` holds tuples in
memory, this one holds them in SQLite. So the question it answers is whether
the plan-walk overhead that costs `interp` 4-10x is noise once SQL execution
dominates.

## Scope

1. New crate `v6/labs/exec_shootout/emitter-rust-sqlite-ir/` implementing the
   CLI and IO contract exactly:
   ```
   <engine-binary> --input <path>
   ```
   Input: first line `p <nodes> <edges>`, then `u v` per line, u32.
   stdout: exactly three JSONL events, nothing else.
   ```
   {"event":"loaded","edges":M,"ms":<int>}
   {"event":"fixpoint","derived":D,"ms":<int>}
   {"event":"done","checksum":"<16-hex>","peak_rss_kb":<int>}
   ```
2. The workload is the same two rules, and semi-naive is REQUIRED. Naive
   re-derivation disqualifies the number.
   ```
   reachable(x, y) <- edge(x, y).
   reachable(x, z) <- reachable(x, y), edge(y, z).
   ```
   Write it as `.dl6`, compile it through the REAL pipeline
   (`v6/prolog/emit_rust.pl`), and run it on `v6/sprefa-engine-rs`. Do NOT
   hand-write a SQLite engine. If the real pipeline cannot express or run this
   program, that is the finding: report the throw site and stop.
3. Single thread, matching the others.
4. Correctness gate: the harness only writes standings when every engine agrees
   on `(derived, checksum)`. Yours must agree.
5. Re-run the harness with all four engines at 10000, 100000, 1000000 and
   rewrite `STANDINGS.md`.

## Second defect, fix it while you are here

`STANDINGS.md` has a column headed **"cold build seconds"** reading 0.1 for
every engine. `harness/src/main.rs:369-391` `measure_build` runs
`cargo build --release` on an ALREADY-BUILT crate, so cargo checks freshness
and exits. It measured a no-op, and the column is false.

The user's live question is whether emitting real Rust and compiling it per
program takes ages. That column is supposed to answer it and does not.

Replace it with two honest numbers per engine:

| number | how |
|---|---|
| cold build | `cargo clean` then `cargo build --release`, wall seconds |
| warm rebuild | `touch src/main.rs` then `cargo build --release`, wall seconds |

Warm rebuild is the number that matters for reload, since a reload recompiles
one crate whose dependencies are already built. Report both, three runs each.

For comparison, already measured elsewhere: 2.07 s to compile the large
emitted `.rs` as a cdylib, but that file is almost entirely one string
literal, so it is not evidence about generated code.

## Anchors
- `v6/labs/exec_shootout/CONTRACT.md` the engine contract, read it first
- `v6/labs/exec_shootout/harness/src/main.rs:369` `measure_build`
- `v6/labs/exec_shootout/mono/Cargo.toml` deps are `fxhash` and `libc` only
- `v6/prolog/emit_rust.pl` 315 ln, the production emitter
- `v6/sprefa-engine-rs/src/program.rs:45` `GenProgram::from_json`, `:81` `run_tick`
- `v6/sprefa-engine-rs/src/sql.rs:21` `trait SqlRunner`, `:28` `SqliteSeam`
- `v6/sprefa-engine-rs/src/bin/emit_rust_harness.rs:82` uses `in_memory()`;
  decide and STATE whether your engine uses an in-memory or a file database,
  since that choice changes the number

## Laws
- Infra is bought, never built, and `mono` sets the bar at two dependencies.
- The 10-second law: anything over 10s is a defect to investigate, never a
  budget. If a build or a scale blows it, say so plainly with the number.
- No `eprintln!` in src/**, `tracing` only. The three JSONL events go to
  stdout by contract; mark them `@eprintln-ok` only if they are on stderr,
  which they should not be.
- Doubt yourself before asserting. Every number is best of 3.
- Comments state only constraints the code cannot show. No dates, no narrative.
- No em dashes. No negative parallelism. No sycophancy.
- Banned in prose AND identifiers: provenance, substrate, load-bearing, regime.
- Never claim the language does not support X without citing the throw site.

## Files you own
`v6/labs/exec_shootout/**`, plan doc
`plans/2026-08-12-exec-shootout-emitter-rust-sqlite-ir.md`.

## Files you must NOT touch
`v6/prolog/emit_rust.pl`, `v6/sprefa-engine-rs/src/**`, `v6/boop/**`,
`v6/prolog/lower.pl`, `v6/prolog/compile/7_emit_ts_types.pl`,
`8_emit_rust_types.pl`, `v6/justfile`. Four other lanes own those. You CONSUME
the emitter and the runtime; you do not change them.

## COMMIT YOUR WORK
Seven lanes today wrote their whole deliverable and exited rc=0 WITHOUT
COMMITTING. Commit on the branch before you exit.

## Report
The four-way STANDINGS table, the real cold and warm build seconds per engine,
whether your engine agreed on `(derived, checksum)`, and one sentence on
whether SQLite execution swamps the plan-walk overhead that costs `interp`
4-10x.
