# Lane `feat/extract-ts-checker` (opus): tsc type-checker tier behind the plain-data seam

First action: `git merge --ff-only 55214a200d3209faff734259007a00a180c095ce`. Failure = STOP, beep the coordinator.

## Goal, one sentence

A ts checker tier mirroring the rust one: the TypeScript compiler answers
call-target AND type_of per reference behind a plain-data seam, feature/flag
gated, three-way answer (Corpus / External / absent) with External suppressing
name-match guesses — measured against the ts5 oracles before any floor moves.

## Why

User want (2026-08-31): "non syntax tier working at codeql level", "can we get
type data and call flow data from compiler tier". ts5 call vs codeql sits at
92.07 / 71.15 — worst precision row on the board; the checker's exact answers
are the only clean fix. Pattern to copy: `src/lang/rust_checker_ra.rs` (#598)
+ `src/project.rs:291` (`load_rust_checker`) + the `RustCheckerIndex` seam.

## Design constraints

- Build-vs-buy first, in the PR body: candidate table for driving tsc from
  Rust. Candidates to price: (a) spawn `tsserver` over its protocol, (b) a
  small node script linking `typescript` npm package emitting jsonl answers
  once per project (snapshot, like a private scip+types), (c) swc/stc
  (rust-native, likely dead/incomplete — verify, cite). Pick the cheapest that
  answers `getSymbolAtLocation` + `getTypeAtLocation` batch-wise. The zero-
  shell law: the engine links executors; a node subprocess is acceptable in
  EXTRACT (extract already shells for scip indexers) — confirm the existing
  scip pattern and copy its process discipline (timeout, budget, tracing).
- Plain-data seam: `TsCheckerIndex` built once per `resolve_project`, plain
  structs only, no compiler types crossing the seam (mirror
  `RustCheckerIndex::build`).
- Answers: per reference site, `Corpus(path, span) | External | absent`.
  External SUPPRESSES the syntax tier's name-match answer (copy the rust
  suppression wiring). type_of answers flow into the Type family as
  `resolved_type_edge` with `resolution_origin: checker`.
- Gating: cargo feature `ts-checker` + CLI flag `--ts-checker --project-root`,
  exactly like rust-checker, INCLUDING the measure() rail (extend the existing
  assert in `tests/bench/mod.rs` if the ts leg's floors ever become
  checker-tier; for now the ts ratchet leg stays syntax and floors stay
  UNTOUCHED).
- New edges carry `resolution_origin` (enum landed in #619). No stringly
  columns. tracing only, no eprintln.

## Files you own

- `v6/sprefa-extract/src/lang/ts_checker.rs` (new), `ts.rs` (suppression
  wiring only), `src/project.rs` (load leg), `src/types.rs` (seam structs),
  `src/bin/extract.rs` + `src/schema.rs` (flag + docs), `Cargo.toml` (feature)
- `v6/sprefa-extract/tests/92_ts_checker.rs` (new, fail-first) + fixtures
  `tests/fixtures/ts_checker/`
- `plans/extract-crawl-2026-08-29/ts.REPORT.md` — one new section: measured
  delta on the TS-5.9 corpus, syntax vs checker, all four oracles' 3-bucket
- FORBIDDEN: RATCHET.tsv, rust/go/python lang files, tests/bench floors,
  corpus dirs (read-only), root src/.

## Receipts

1. Build-vs-buy candidate table with citations.
2. Fail-first `92_ts_checker.rs` red (no tier) then green.
3. Suite `cargo test --features cli,rust-checker,ts-checker` rc=0 direct.
4. Measurement (release binary, one process, nice -n 15, 15-min cap,
   background): ts5 corpus with and without `--ts-checker`, both scored
   against `ts5.oracle.call.tsv` + `ts.codeql2.call.tsv`, table in the report:
   recall / precision / 3-bucket, syntax vs checker, wall + rss with units.
   NO floor changes in this PR regardless of result.
5. `git diff --stat origin/main...HEAD` = owned files only.

## Laws

Banned words prose+identifiers: provenance, substrate, load-bearing, regime,
ground truth (say oracle), honest, signal. Numbers carry units. Commit per
logical step; push; `gh pr create`; then
`boop beep --no-wait --as feat-extract-ts-checker sprefa-coordinator "<PR number, syntax-vs-checker table>"`.
10-second law: builds/measures background with caps, never foreground-wait.
ONE ratchet at a time on the machine: you do NOT run just extract-ratchet at
all (floors untouched); the suite is your only gate.
