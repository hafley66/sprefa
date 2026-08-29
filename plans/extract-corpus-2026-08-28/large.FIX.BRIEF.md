# Brief: large-file resource bound: ts 1 GB RSS, rust 29 MB grammar timeout (lane `fix-extract-large-files`)

Read `plans/extract-corpus-2026-08-28/COMMON.md` (in your tree) for the style
laws, the 10-second law, and the forbidden list. Then read the finding rows
named below in the report files under the same dir.

## First action
```
git merge --ff-only 99b8dc79f
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Failure: STOP, `boop beep --no-wait --as fix-extract-large-files sprefa-coordinator "<one line>"`.

## Method, every fix
Failing test FIRST (`cargo test --features cli <test>` red, paste the red
output into the commit body), then the fix, then green, one commit per fix.
Existing fixture files named below are your repro inputs; their header
comments state the expected fact. Never weaken an existing golden or parity
test to pass; if one blocks you, record the reason in the commit body and use
the waiver mechanism that test file documents.

## Files you own (the only src files you may edit)
`v6/sprefa-extract/src/lang/astgrep.rs`, `v6/sprefa-extract/src/lang/ts.rs`, `v6/sprefa-extract/src/lang/rust.rs`, `v6/sprefa-extract/src/wire.rs`
Tests: new `v6/sprefa-extract/tests/*.rs` and fixtures under
`v6/sprefa-extract/tests/fixtures/large_findings/`. Fixtures outside the scip ratchet
globs: the ts and rust lanes hit `golden_parity.rs` ratchet failures by adding
files under `tests/fixtures/ts/` and `tests/fixtures/rust/`; use a
`<lang>_findings/` sibling dir for repros.

## Finding A (ts.REPORT.md F4)
`/usr/bin/time -l extract --family cst <monaco-editor/dev/vs/assets/ts.worker-*.js>`
(12.7 MB input, find it under `~/projects/instant/node_modules` or install
`monaco-editor` into scratch) peaks at 1,087 MB RSS in 3.33 s, reached via
`src/lang/astgrep.rs` from `src/lang/ts.rs:3018`. Profile the allocation
(`cargo instruments -t Allocations` or `heaptrack`-equivalent; paste the
top 5 frames). Expected: RSS bounded to a small multiple of the input plus
the tree (target under 300 MB on that file), by streaming CST rows instead
of materializing the whole fact vector, or by dropping per-node owned
Strings. Output bytes stay byte-identical on 20 corpus files (paste the
empty diff receipt).

## Finding B (rust.REPORT.md timeout + rss rows)
`timeout 10 extract ~/.cargo/registry/src/*/nickel-lang-core-0.15.3/src/parser/grammar.rs`
(29,328,358 B generated) rc=124; unbounded 20.8 s, 13.6M lines, peak RSS
3,762,643,968 B. Re-measure first on the post-#529 binary (BufWriter
landed). Then the same treatment as A on the rust arm: stream rows, cut
peak RSS, and get the default run under 10 s or emit a named
`parse_skip`/`size_skip` row with the byte count and a `--max-bytes`
override, the way `scip_skip` names its reason. The user law: a silent
timeout is a defect, a named skip is a fact.

COUNT tests: a synthetic 5 MB generated `.rs` and `.js` in a tempdir;
assert peak RSS (read `/usr/bin/time -l` or `getrusage` from the test
process spawning the binary) under a fixed budget and row count unchanged
vs `--bench`.

## Deliverables
- Commits as above; last commit body carries the whole-crate
  `cargo test --features cli` passed/failed count.
- Append a "Fixes" table (finding / before / after / test) to
  `plans/extract-corpus-2026-08-28/ts.REPORT.md`.
- `gh pr create --base main`.
- `boop beep --no-wait --as fix-extract-large-files sprefa-coordinator "fix-extract-large-files: PR #N, <fix list>, gate <passed>/<failed>"`.

## Forbidden
Every other src file and language arm, `v6/prolog/**`, `v6/sprefa-engine-rs/**`,
`CLAUDE.md`. No subagents, no `--no-verify`, no push to main, no whole-crate
`cargo fmt` (fmt only the files you own).
