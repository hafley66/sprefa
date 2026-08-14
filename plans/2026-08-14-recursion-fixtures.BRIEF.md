# Brief: certify self-referential types with conformance fixtures

## Mission
Two spellings compile today with ZERO conformance coverage. Your job is to pin
their runtime semantics with fixtures, and to report exactly what happens on a
cyclic value. You measure and record; you never patch the compiler.

The two spellings, both probe-verified on this branch:

1. Recursive enum payload:
   `rel tree(leaf(value: int) ; branch(left: tree, right: tree)).`
   Emits `tree_leaf(id, value)`, `tree_node(id, left, right)` (left/right are
   INTEGER ids of other tree rows), `tree_tag(id, tag)`.
2. Self-referential generic argument:
   `rel node(name: text, children: list(node)) key(1).`
   Mints `__gen__list_node...__member(list_id, idx, value)` with value =
   node's surrogate id.

## First action, before anything else
```
git merge --ff-only 91da67816f00255d18d12094ea7cb11a9a896c70
```
If this fails or the worktree is missing: STOP AND REPORT. Do not archive,
copy, or `--no-verify` around any blocked command.

## Files you own (the only files you may create or edit)
- `v6/prolog/conformance/fixtures/17_recursive_enum.pl` (new)
- `v6/prolog/conformance/fixtures/18_recursive_list_arg.pl` (new)
- `plans/2026-08-14-recursion-fixtures.REPORT.md` (new; your findings)
- Files the gates regenerate on their own (`v6/prolog/compile/out/**`,
  `v6/sprefa-engine-rs/graded.tsv`) may be committed as the gates leave them.

FORBIDDEN, never edit: everything else. Specifically any compiler source
(`v6/prolog/*.pl`, `v6/prolog/compile/**`, `v6/prolog/conformance/*.pl`
outside fixtures/), `v6/sprefa-engine-rs/src/**`, `v6/tsv2/**`. If a fixture
fails because the compiler or oracle is wrong, that is a FINDING for the
report, not a thing you fix.

## Fixture format
Copy the exact shape of `v6/prolog/conformance/fixtures/12_enum_rel_payload.pl`
(enum arrivals spell as `+tree_leaf(Id, obj([value-5]))`-style variant rows;
finals/deltas/retractions shown there) and
`v6/prolog/conformance/fixtures/10_list_elements.pl` (list-column arrivals are
plain nested lists, e.g. `batch(1, [obj([a-1]), 42])`; the
`rel_element_list_round_trips` fixture there is the template for a rel-typed
element). Every fixture carries a one-line comment saying why it exists.
dl variable names are descriptive words, never single letters.

## Slices, in order, one commit each
1. `17_recursive_enum.pl`: an acyclic tree value. Arrive two leaves, then a
   branch whose left/right reference the leaf ids. Assert finals on
   `tree_leaf`, `tree_node`, `tree_tag`, plus one rule that reads
   `tree_tag(Id, Tag)` into a derived rel. Then retract one leaf and assert
   the deltas. If the oracle throws or the emitted program diverges, put the
   exact output in the report and mark the fixture with the failing
   expectation commented out and a `% FINDING` line pointing at the report.
2. `18_recursive_list_arg.pl`: `node(name: text, children: list(node))`.
   Arrive a parent whose children list holds child node values (spelling per
   `rel_element_list_round_trips`). Assert finals on `node` and the minted
   member rel. Same divergence protocol as slice 1.
3. Cyclic value probe, REPORT ONLY, no fixture unless behavior is stable:
   arrive a branch whose `left` is its own id (and a two-row mutual cycle).
   Wrap every run in a 60-second timeout (`gtimeout 60` if present, else
   `perl -e 'alarm 60; exec @ARGV' ...`). Record verbatim: loop, throw name,
   or stored-and-rendered. A hang past 60s is the finding "render does not
   terminate on a cycle"; kill it and write that.
4. Gates, run all four, paste the numbers into the report AND each commit
   message:
   ```
   cd v6/prolog/conformance && swipl -g go -t halt go.pl
   cd v6/tsv2 && bash scripts/sweep.sh
   cd v6/prolog && swipl -q -l compile/test/plunit_tests.pl -g run_tests -g halt
   bash v6/sprefa-engine-rs/grade.sh
   ```
   Baselines you must not regress: conformance 421 PASS / 0 FAIL before your
   fixtures (yours add to the total), sweep RUN wrong=0, plunit failing set is
   EXACTLY the 5 names in `.github/CI-KNOWN-RED.md`, RUST-GRADE
   graded=421 byte-clean=313 before yours. New fixtures that grade
   non-byte-clean are acceptable WITH a report line naming them. Never chain
   two grade.sh runs in one shell line; it races itself.

## Report file
`plans/2026-08-14-recursion-fixtures.REPORT.md`, committed. Contents: a table
(fixture, expectation, observed, verdict pass/diverged/blocked), the cyclic
probe transcript, final gate numbers. No narrative of what you tried first.

## Style laws in force
- Comments state only constraints code cannot show; two consecutive comment
  lines maximum.
- Banned words in prose and identifiers: provenance, substrate, load-bearing,
  regime, refusal (say TODO or "not built yet").
- No `eprintln!` anywhere (not expected to arise; stated for completeness).
