# Brief: expose sprefa-extract's phase-2 resolve through the CLI (codex terra)

USER WAIVER 2026-07-29 morning: the standing "extractor is FIXED" directive is
waived for exactly this: wiring the EXISTING resolve pass to the CLI surface.
No new extraction logic, no redesign. Smallest correct solution (user word).

## The gap (scouted receipts)

- The resolve machinery exists and is library-tested:
  `tests/0_prolog.rs:70-95` builds the whole recipe -- per-file
  `Source.extract`, `build_def_index(&[(BlobHash, &ExtractOutput)])` over ALL
  files, `IndexBag.def_index.set`, `ProjectCx { files, manifests, reader:
  None, digest, indexes }`, then `Resolve::<CallF>::resolve(&SourceImpl,
  &output, &cx)` PER FILE. Asserts `CallEdgeKind::NameResolve` edges.
- The CLI (`src/bin/extract.rs`, clap, no tokio, flat JSONL to stdout) runs
  phase 1 only and says so (`:176` "UNRESOLVED in phase 1"). No test pins a
  phase-2 CLI claim because the CLI never made one.
- Resolution is PROJECT-scoped (cross-file joins), so the per-file invocation
  shape cannot carry it; this is a new project-mode invocation.

## What to build

1. A project-mode CLI entry (your call: subcommand `resolve <paths...>` vs a
   flag; pick what fits the existing clap shape best and say why): reads N
   files, runs phase-1 extraction per file, builds the def index once, then
   resolves per file and streams resolved edges as JSONL.
2. Record shape: ONE new record kind with FLAT TOP-LEVEL FIELDS ONLY (the v6
   host decode projects top-level keys; nesting makes the output unusable --
   the span lesson). Suggested: caller path, caller name, callee path, callee
   name, edge kind. Deterministic order (sort before emit) so goldens are
   byte-stable.
3. `Resolve::<TypeF>` edges (`wire.rs:87`) ride along ONLY if the identical
   recipe reaches them with a few lines; otherwise leave them out and say so.
4. CLI-level golden test: a small fixture repo under `tests/fixtures/` (dir
   exists), run the real binary via the standard `env!("CARGO_BIN_EXE_extract")`
   mechanism (zero new dev-deps), assert the resolved-edge JSONL byte-equals a
   checked-in golden INCLUDING at least one cross-file edge. This test IS the
   point: it pins the CLI phase-2 contract that never existed.

## Hard constraints

- Existing behavior untouched: default (phase-1) output byte-identical; all
  existing tests (`cargo test` incl golden_parity, snapshot, 0_prolog) stay
  green; `--features cli` gate stays as-is (release build recipe is
  `cargo build --release --features cli --bin extract`).
- Zero new dependencies.
- Banned words in code and prose: provenance, substrate, load-bearing, regime.
  Descriptive names.
- DO NOT COMMIT (codex no-commit flow): leave a clean, verified working tree;
  the coordinator reviews and commits.

## Validation before your final report

`cargo test --features cli` green (or state the exact feature combo the test
needs), `cargo build --release --features cli --bin extract` clean, run the
new project mode against `src/` of this very crate and paste 5 sample edge
lines, plus the golden test output. State every file touched.
