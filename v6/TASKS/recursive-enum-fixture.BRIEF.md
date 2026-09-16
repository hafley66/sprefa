# Brief: recursive-enum conformance fixture (pass 1 of 2)

You are pass 1 of 2 on a flash lane in the sprefa repo (v6 dl engine, Prolog
compiler + oracle). You WRITE a conformance fixture and MEASURE the oracle. You
do NOT touch compiler or oracle code. Diagnosis of what your measurement means
is the coordinator's job, not yours. Favor plain code. If reality deviates from
this brief, STOP and report; do not improvise.

## Repo and root

Repo root: `~/projects/sprefa`. This is the v6 engine: `v6/prolog` holds the
compiler (`compile.pl`, `0_program_check.pl`, `0_type_plane.pl`) and the oracle
reference interpreter (`v6/prolog/conformance/engine.pl`). Conformance fixtures
live in `v6/prolog/conformance/fixtures/*.pl`. The runner is
`v6/prolog/conformance/go.pl`, which auto-loads every `fixtures/*.pl` file.

## Context

A SELF-REFERENTIAL enum (a boxed recursive ADT) already compiles in the
compiler door. Probe receipts: `rel tree(leaf(value: int) ; branch(left: tree,
right: tree)).` compiles and emits `tree_node(left INTEGER, right INTEGER)`;
the recursive fields are surrogate ids pointing at other rows, never inlined.
Zero conformance fixtures exercise a recursive enum, so the ORACLE door's
semantics are ungraded. Your fixture pins them.

## Deliverable

Add one file: `v6/prolog/conformance/fixtures/15_recursive_enum.pl`.

Copy the three op declarations from `fixtures/0_enum_variants.pl` (lines 1-3).

### Fixture A (must end green)

A fixture/5 fact that arrives a small acyclic tree through the variant tables
and asserts finals on the variant tables and the tag view.

Program shape (the recursive enum):

    enum_decl(tree, (leaf(value:int) ; branch(left:tree, right:tree)))

Expected emitted rels (read `0_enum_expand.pl` to confirm names before
asserting): variant table `tree_leaf/2` (id, value), variant table
`tree_branch/3` (id, left, right), tag view `tree_tag/2` (id, tag).

Schedule (arrivals):

    [+tree_leaf(1, 5)],
    [+tree_branch(2, 1, 3)],
    [+tree_leaf(3, 7)]

Best-guess finals:

    final(tree_leaf/2,   [tree_leaf(1, 5), tree_leaf(3, 7)]),
    final(tree_branch/3, [tree_branch(2, 1, 3)]),
    final(tree_tag/2,    [tree_tag(1, leaf), tree_tag(2, branch), tree_tag(3, leaf)])

Add `deltas(...)` for the tag view and a `ticks(N)` matching the schedule plus
any trailing drain ticks (read FIXTURES.md: `deltas(...)` and `ticks(N)` must
include the trailing `[]` ticks after the last write).

### Probe B (cyclic arrival)

A second fixture/5 fact, or a second schedule in fixture A, that arrives a
deliberately cyclic branch to learn whether the oracle stores, throws, or
loops. Use a self-cycle:

    [+tree_branch(4, 4, 1)]

Rules for Probe B:
- If the run TERMINATES (stores or throws), assert the observed finals and keep
  it green.
- If the run LOOPS or hangs, do NOT leave it in the committed fixture (a hang
  breaks the go.pl gate). Remove it from the committed file, run it as a
  throwaway probe, and record the result in REPORT.md. Kill any hung run after
  about 60 seconds.
- Report in REPORT.md which of store / throw / loop the cyclic arrival produced.

## Procedure

1. Read `v6/prolog/conformance/FIXTURES.md` (the shared fixture contract), the
   op header and two example fixtures in `0_enum_variants.pl`, and
   `12_enum_rel_payload.pl`.
2. Read `0_enum_expand.pl` to confirm the exact emitted variant/tag rel names
   and arities before asserting finals.
3. Write the fixture file with best-guess expectations.
4. Run the gate:
   `cd ~/projects/sprefa/v6/prolog/conformance && swipl -g go -t halt go.pl`
5. Iterate on the FIXTURE's expectations until your fixtures report PASS and
   the gate ends with zero `fail` lines. The oracle (engine.pl) is ground
   truth; adjust the fixture to the oracle's observed behavior, never the
   reverse.
6. Confirm `git status --short` shows only your new fixture file (and no
   modifications to engine.pl, go.pl, 0_program_check.pl, compile.pl, or any
   compiler/oracle/source file).

## Report (REPORT.md at worktree root)

List:
- The exact go.pl PASS/FAIL line for each of your fixtures.
- Which of store / throw / loop the cyclic arrival produced (Probe B).
- The exact emitted rel names and arities you confirmed from `0_enum_expand.pl`.
- Any door disagreement observed (compiler accepts a program the oracle throws,
  or the reverse), cited to the throw site.
- The final `git status --short` and `git diff --stat`.

Do not commit unless asked. Your deliverable contract is REPORT.md.

## Style laws (from repo CLAUDE.md, non-negotiable)

- No em dashes. Banned words in prose and identifiers: `provenance`,
  `substrate`, `load-bearing`, `regime`, `ground` as a verb, `ruling(s)`,
  `honest(ly)`, `distill`.
- No code comments unless they carry a fail-pre-fix sabotage note (see the
  fixtures for the pattern).
- Prolog variables are descriptive (`Id`, `Value`, `Left`), never `X`/`Y`.
- Follow the existing fixture style exactly.

## Package manager

None. Prolog runs via `swipl`. No npm, no pnpm, no installs.
