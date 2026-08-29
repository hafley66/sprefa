# Brief: kotlin call sites for infix, operator, invoke (lane `fix-extract-kotlin-calls`)

Read `plans/extract-corpus-2026-08-28/COMMON.md` (in your tree) for the style
laws, the 10-second law, and the forbidden list. Then read the finding rows
named below in the report files under the same dir.

## First action
```
git merge --ff-only 99b8dc79f
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Failure: STOP, `boop beep --no-wait --as fix-extract-kotlin-calls sprefa-coordinator "<one line>"`.

## Method, every fix
Failing test FIRST (`cargo test --features cli <test>` red, paste the red
output into the commit body), then the fix, then green, one commit per fix.
Existing fixture files named below are your repro inputs; their header
comments state the expected fact. Never weaken an existing golden or parity
test to pass; if one blocks you, record the reason in the commit body and use
the waiver mechanism that test file documents.

## Files you own (the only src files you may edit)
`v6/sprefa-extract/src/lang/kotlin.rs`
Tests: new `v6/sprefa-extract/tests/*.rs` and fixtures under
`v6/sprefa-extract/tests/fixtures/kotlin/`. Fixtures outside the scip ratchet
globs: the ts and rust lanes hit `golden_parity.rs` ratchet failures by adding
files under `tests/fixtures/ts/` and `tests/fixtures/rust/`; use a
`<lang>_findings/` sibling dir for repros.

## Finding (kotlin-prolog-markdown.REPORT.md, row 1)
`extract --family call tests/fixtures/kotlin/corpus_1_infix_operator.kt`
emits sites `Box Box Box Box` only. `kotlin.rs:773` mints sites from
`call_expression` alone. Expected additional `site` records:
- `1 plus2 2` -> callee `plus2` (tree-sitter-kotlin `infix_expression`).
- `Box(1) + Box(2)` -> callee `plus` (`additive_expression` operator to the
  Kotlin operator-function name: `+` plus, `-` minus, `*` times, `/` div,
  `%` rem, `..` rangeTo, `in` contains, `[]` get/set, `==` equals,
  `<` `>` `<=` `>=` compareTo, `+=` plusAssign etc., unary `-` unaryMinus,
  `!` not, `++` inc, `--` dec).
- `Box(3)()` -> callee `invoke`.
Each new site keeps the same span discipline as the existing call arm (the
span of the operator token or the infix name, so `--resolve` joins it to the
`operator fun`/`infix fun` definition by name). Add the `--resolve` COUNT
test: a two-file fixture where the resolved_edge count includes the three
new edges. Corpus receipt: rerun `--resolve` on the okio commonMain dir
(clone `https://github.com/square/okio` shallow into scratch) and put the
resolved_edge count before/after in the Fixes table.

## Deliverables
- Commits as above; last commit body carries the whole-crate
  `cargo test --features cli` passed/failed count.
- Append a "Fixes" table (finding / before / after / test) to
  `plans/extract-corpus-2026-08-28/kotlin-prolog-markdown.REPORT.md`.
- `gh pr create --base main`.
- `boop beep --no-wait --as fix-extract-kotlin-calls sprefa-coordinator "fix-extract-kotlin-calls: PR #N, <fix list>, gate <passed>/<failed>"`.

## Forbidden
Every other src file and language arm, `v6/prolog/**`, `v6/sprefa-engine-rs/**`,
`CLAUDE.md`. No subagents, no `--no-verify`, no push to main, no whole-crate
`cargo fmt` (fmt only the files you own).
