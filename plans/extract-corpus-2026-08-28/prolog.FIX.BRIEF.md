# Brief: prolog meta-call closures and goal arguments (lane `fix-extract-prolog-metacall`)

Read `plans/extract-corpus-2026-08-28/COMMON.md` (in your tree) for the style
laws, the 10-second law, and the forbidden list. Then read the finding rows
named below in the report files under the same dir.

## First action
```
git merge --ff-only 99b8dc79f
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Failure: STOP, `boop beep --no-wait --as fix-extract-prolog-metacall sprefa-coordinator "<one line>"`.

## Method, every fix
Failing test FIRST (`cargo test --features cli <test>` red, paste the red
output into the commit body), then the fix, then green, one commit per fix.
Existing fixture files named below are your repro inputs; their header
comments state the expected fact. Never weaken an existing golden or parity
test to pass; if one blocks you, record the reason in the commit body and use
the waiver mechanism that test file documents.

## Files you own (the only src files you may edit)
`v6/sprefa-extract/src/lang/prolog/**`
Tests: new `v6/sprefa-extract/tests/*.rs` and fixtures under
`v6/sprefa-extract/tests/fixtures/prolog/`. Fixtures outside the scip ratchet
globs: the ts and rust lanes hit `golden_parity.rs` ratchet failures by adding
files under `tests/fixtures/ts/` and `tests/fixtures/rust/`; use a
`<lang>_findings/` sibling dir for repros.

## Finding (kotlin-prolog-markdown.REPORT.md, rows 4-5)
`prolog/_0_source.rs:264-265` treats only `once/1` arg 1 and `catch/3` args
1 and 3 as goals. `extract --family call tests/fixtures/prolog/corpus_1_meta_closures.pl`
shows `double` under `maplist/3` and `call/3` with no reference and no site,
and goals under `forall/2`, `findall/3` as `term_arg`. Swipl library
exposure (107396 sites): maplist/2..5 604, findall/3,4 317, forall/2 235,
call/1..8 251, foldl/4..6 42, include/exclude/partition 88,
aggregate_all/setof/bagof 56, ignore/1 40, not/1 1.

Build the meta-predicate table from SWI's own `meta_predicate` declarations:
argument spec `0` = goal, `N` = closure called with N extra args, `^` =
goal under `^`. Cover at least: call/1..8, once, ignore, not, \+, forall,
findall/3,4, aggregate_all/3,4, setof, bagof, maplist/2..7, foldl/4..7,
include, exclude, partition/4, catch, catch_with_backtrace, setup_call_cleanup,
call_cleanup, with_output_to/2, phrase/2,3, freeze/2, thread_create/3.
A closure `double` in a `2` slot mints a site `double/2` (name + added
arity) in `--family call`, and a `reference` with a new `position`
`closure`. A `0` slot's compound becomes a site the way `once/1` does today.
`^`-wrapped goals under setof/bagof unwrap. Also read the file's own
`:- meta_predicate` directives and extend the table per file.

Tests: unit half on the fixture pinning sites and reference positions in
clause order (copy the shape of `tests/1b_prolog_metacall.rs`); CLI half
with `--resolve` over two files asserting the go/1 -> double/2 edges.
Corpus receipt: `--resolve` over `/opt/homebrew/lib/swipl/library/*.pl`
resolved_edge count before/after in the Fixes table (before: 39751 over
library+boot).

## Deliverables
- Commits as above; last commit body carries the whole-crate
  `cargo test --features cli` passed/failed count.
- Append a "Fixes" table (finding / before / after / test) to
  `plans/extract-corpus-2026-08-28/kotlin-prolog-markdown.REPORT.md`.
- `gh pr create --base main`.
- `boop beep --no-wait --as fix-extract-prolog-metacall sprefa-coordinator "fix-extract-prolog-metacall: PR #N, <fix list>, gate <passed>/<failed>"`.

## Forbidden
Every other src file and language arm, `v6/prolog/**`, `v6/sprefa-engine-rs/**`,
`CLAUDE.md`. No subagents, no `--no-verify`, no push to main, no whole-crate
`cargo fmt` (fmt only the files you own).
