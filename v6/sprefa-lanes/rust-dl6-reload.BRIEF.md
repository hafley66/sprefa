# Lane: how does Rust load and RELOAD a .dl6 program

## Base
`git merge --ff-only 0b672fc1` is your FIRST action. Failure = STOP AND REPORT.
Worktree: `.boop-worktrees/plans/rust-dl6-reload`.

## The hole

`plans/2026-08-12-rust-async-load-ipc.RESEARCH.md` (715 lines) answered two
questions and skipped the one that matters here.

| question | answered? |
|---|---|
| is IPC the bottleneck on program load | yes, no; swipl is 96.3% of a 10.38s worst case |
| what async spine should Rust use | yes, tokio + futures, reject rxRust |
| **how does Rust load a compiled program at all** | **no** |
| **how does Rust RELOAD one without restarting** | **no** |

Nothing in those 715 lines mentions dylib loading, `libloading`, an interpreter
tier, or a process-swap. Grep for `hmr`, `reload`, `hot` returns three hits, none
about code loading.

## Why it matters

TypeScript reloads by `import(module_path)` on a freshly written file. Measured:
398.2 ms for a 2,956,190-byte emitted module, resident set 185.0 MB
(`plans/2026-08-12-rust-async-load-ipc.RESEARCH.md` section 5). Rust has no
equivalent. `emit_rust` emits Rust SOURCE, which must be compiled before it runs.

## Question to answer

Given a `.dl6` file that changes while the engine is running, what does Rust do?

## Deliverable 1: candidate table

Research each, present a written candidate-by-candidate analysis. No one-line
dismissals. Build-vs-buy law applies at every level: never assert "write our own"
without library research first.

At minimum cover:

| candidate | the shape |
|---|---|
| `libloading` + cdylib | rustc compiles the emitted source to a .dylib, engine dlopens it |
| `hot-lib-reloader` | the maintained crate over that pattern |
| subsecond / dioxus hot-patch | binary patching, no dlopen |
| process swap | new process, new program, hand over the SQLite file |
| an interpreter tier | Rust reads the plan JSON and walks it, no codegen |
| `cranelift` JIT | emit IR instead of source |
| wasm module + `wasmtime` | emitted program as a wasm guest |

For each: maintenance date, download count, what it costs on THIS system, and
which of the repo's laws it breaks or satisfies.

Note the interpreter row is not exotic here. The compiler already emits a plan
and `lowered/8` statements; walking them is closer to what v6 already does than
codegen is.

## Deliverable 2: measurements, not opinions

Measure on this machine, three runs each, report the numbers:

- `rustc` wall time to build ONE emitted program as a cdylib, for a small
  fixture and for `gen_served/ea699faefe33603f03451984a1f13665.dl6`
  (107,856 source bytes, 2,956,190 emitted TS bytes, 10.38s in swipl)
- `dlopen` + first call latency for that dylib
- process spawn + SQLite handover latency for the process-swap candidate
- what an interpreter tier would cost per tick against the compiled path

The 10-second law binds: any operation over 10s is a defect to investigate, not
a budget to normalize. If a candidate lands over 10s, say so plainly.

## Constraints you must carry

- The user's word: "I DO NOT WANT TO RUN V5 ANYTHING ANYMORE". No design may end
  in keeping a v5 binary alive.
- `v6/sprefa-engine-rs/src/program.rs:69` says edge rules are not ported, so the
  Rust engine is mid-port. Design against that, and say what parity it assumes.
- `v6/sprefa-engine-rs/src/sql.rs:25-27` holds a bare `Connection`, which is not
  `Sync`. The prior doc's recommendation was one thread owning it behind a
  request channel. Reloading interacts with that; say how.
- Two processes on one SQLite file is the WAL-reset trigger. `Cargo.toml:84`
  pins `rusqlite 0.32.1`, bundling SQLite 3.46.0. Lane
  `fix/rust-hygiene-sqlite-pin` owns that bump; do not touch the pins, but state
  which candidates depend on it landing.
- RUST-GRADE is 230 byte-clean of 392 as of `feature/emit-rust-climb-2`. 106 are
  unsupported, 50 diff. Reload work assumes the emitter keeps climbing.

## Deliverables, both required
1. `plans/2026-08-12-rust-dl6-reload.RESEARCH.md`, receipts and citations, TOC.
2. `plans/2026-08-12-rust-dl6-reload.RESEARCH.visual.human.unga.md`, plain words,
   ascii or mermaid, ZERO citations, TOC.

A plan without the second doc is undelivered. Output form is lists, tables and
mermaid. Prose is a caption under a diagram, never the medium.

## Files you own
Those two plan docs ONLY. Do not edit any `.pl`, `.rs`, `.ts`, `Cargo.toml`, or
`justfile`. Three other lanes are live and own all of those.

## Laws
- Never assert "write our own" without a written candidate analysis first.
- Doubt yourself before asserting. Cite or say you did not check.
- Language and design decisions belong to the user. Report cited forks.
- A compiler error for an unbuilt construct is "TODO", never "refusal".
- No em dashes. No negative parallelism. No sycophancy.
- Banned in prose AND identifiers: provenance, substrate, load-bearing, regime.
- Construct names use rxjs, prolog or SQL vocabulary only.

## Report
The candidate table, the measured numbers, and your single recommended path with
the number that justifies it.
