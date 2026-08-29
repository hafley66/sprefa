# Brief: resolve door: markdown doc_ref at the CLI, python callee_name null (lane `fix-extract-resolve-door`)

Read `plans/extract-corpus-2026-08-28/COMMON.md` (in your tree) for the style
laws, the 10-second law, and the forbidden list. Then read the finding rows
named below in the report files under the same dir.

## First action
```
git merge --ff-only 99b8dc79f
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Failure: STOP, `boop beep --no-wait --as fix-extract-resolve-door sprefa-coordinator "<one line>"`.

## Method, every fix
Failing test FIRST (`cargo test --features cli <test>` red, paste the red
output into the commit body), then the fix, then green, one commit per fix.
Existing fixture files named below are your repro inputs; their header
comments state the expected fact. Never weaken an existing golden or parity
test to pass; if one blocks you, record the reason in the commit body and use
the waiver mechanism that test file documents.

## Files you own (the only src files you may edit)
`v6/sprefa-extract/src/project.rs`, `v6/sprefa-extract/src/bin/extract.rs`, `v6/sprefa-extract/src/lang/markdown/**`
Tests: new `v6/sprefa-extract/tests/*.rs` and fixtures under
`v6/sprefa-extract/tests/fixtures/resolve_findings/`. Fixtures outside the scip ratchet
globs: the ts and rust lanes hit `golden_parity.rs` ratchet failures by adding
files under `tests/fixtures/ts/` and `tests/fixtures/rust/`; use a
`<lang>_findings/` sibling dir for repros.

## Finding A (kotlin-prolog-markdown.REPORT.md, markdown rows)
`extract --resolve --family type tests/fixtures/markdown/doc_node.md tests/fixtures/rust/*.rs tests/fixtures/ts/*.ts`
emits field/param/returns edges and zero `doc_ref` rows, while
`tests/22_doc_node.rs` proves `doc_ref` through the library `Resolve<TypeF>`
API. Find where the CLI resolve path drops or never runs the markdown
`Resolve<TypeF>` arm (start at `src/bin/extract.rs` resolve dispatch and
`src/project.rs` RESOLVE_ARMS). Expected: the same `doc_ref` rows the test
sees, as `resolved_type_edge` with `kind: doc_ref`. CLI test: run the binary
over that fixture set, assert `doc_ref` count > 0 and equal to the library
count.

## Finding B (python.REPORT.md F1)
`extract --resolve tests/fixtures/python/corpus_8.py tests/fixtures/python/corpus_9.py`
emits the `Widget()` edge with `callee_name: null` (`src/project.rs:889`).
Expected `callee_name: "Widget"`. Find why the callee's def name is not
carried for a class constructor call and fix at the resolve layer if the
cause is there; if the cause is in the python arm, STOP and hail the
coordinator with the throw site (the python arm is not yours).

## Finding C (ts.REPORT.md F6, F10, F9)
`--resolve` drops every `unresolved` record the schema advertises
(`--schema` line 26); `--resolve one.ts` with a single path exits 0 silent
though `--help` says two or more are needed; `--resolve <dir>` dies with a
Debug dump `Error: Read("src", Custom {...})`. Fix all three in
`bin/extract.rs` / `project.rs`: emit `unresolved` rows, exit 2 with the
help sentence on one path, and a plain message naming the path and "is a
directory" on a dir. One CLI test each.

## Deliverables
- Commits as above; last commit body carries the whole-crate
  `cargo test --features cli` passed/failed count.
- Append a "Fixes" table (finding / before / after / test) to
  `plans/extract-corpus-2026-08-28/kotlin-prolog-markdown.REPORT.md`.
- `gh pr create --base main`.
- `boop beep --no-wait --as fix-extract-resolve-door sprefa-coordinator "fix-extract-resolve-door: PR #N, <fix list>, gate <passed>/<failed>"`.

## Forbidden
Every other src file and language arm, `v6/prolog/**`, `v6/sprefa-engine-rs/**`,
`CLAUDE.md`. No subagents, no `--no-verify`, no push to main, no whole-crate
`cargo fmt` (fmt only the files you own).
