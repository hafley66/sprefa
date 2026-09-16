# chore/json-list-term-rename

## Ruled by user 2026-08-10 (lang-design law satisfied)
The inline-JSON array column type is spelled `json_list(T)` EVERYWHERE: text
door (already done), internal prolog term (this lane), emitted catalog strings
(this lane). `list(T)` is freed for the upcoming relational generics. The
parse_dl.pl comment claiming "the retained prolog term stays list(T)" is
overridden by this decision.

## The work (mechanical term rename, no design calls)
Rename the TYPE TERM `list(Element)` -> `json_list(Element)` wherever it means
the inline-JSON column type. Do NOT touch prolog builtins (`is_list`,
`maplist`, `atomic_list_concat`, `library(lists)`) or fixture NAMES containing
the word list (`list_of_json_documents_round_trips` stays).

Term-site counts, measured at dispatch (grep for
`list\((int|text|json|bool|Element|Inner|_\))` after excluding builtins):

| file | sites |
|---|---|
| v6/prolog/lower.pl | 19 |
| v6/prolog/compile/test/plunit_tests.pl | 16 |
| v6/prolog/conformance/fixtures/10_list_elements.pl | 8 |
| v6/prolog/0_type_plane.pl | 8 |
| v6/prolog/compile/parse_dl.pl | 5 (two `typed_column_type_base` clauses build the term at ~:706-722; both now build `json_list(Element)`; the removed_word(list) clause stays, message already names json_list) |
| v6/prolog/0_program_check.pl | 5 |
| v6/prolog/print_dl.pl | 2 (print side must print `json_list(...)`) |
| v6/prolog/emit_ts.pl | 2 |
| v6/prolog/conformance/fixtures/json_arm.pl | 2 |
| v6/prolog/conformance/fixtures/5_value_plane.pl | 2 |
| v6/prolog/ARCH.pl | 2 (narrative rows: update spelling only where the term appears) |
| v6/prolog/sweep.pl, analyze.pl, 0_rel_record.pl, conformance/rulings.pl, compile/scripts/0_json_arrival.pl | 1 each |

Also in scope:
- Emitted catalog strings: `local_name: "list(json)"` etc. and `kind: "list"`
  in compile/out/*.ts come from the emitter; after the term rename plus
  regeneration they must read `json_list(...)` / `json_list`. Find the emit
  site by grepping the prolog for the kind atom.
- v6/tsv2/runtime/types.ts:386 kind union member `"list"` -> `"json_list"`.
  Then grep v6/tsv2 AND v6/sprefa-store/js for `"list"` comparisons against
  the catalog kind and update those matches ONLY (the union has many other
  members; do not touch them).
- Comments/decision rows: rewrite the parse_dl.pl block at ~:700-711 (drop the
  "retained prolog term stays list(T)" sentence), update the
  0_type_plane.pl:89 spelling comment, and append a decision row in
  v6/prolog/conformance/rulings.pl following that file's existing row format:
  user 2026-08-10, json_list is the one spelling at every layer, list(T)
  freed for relational generics.

## Known trap
`bash scripts/sweep.sh` (run from v6/tsv2) DELETES
v6/prolog/compile/out/pokeapi_shape.ts (artifact outside the sweep fixture
set, known defect). After sweep: `git checkout -- v6/prolog/compile/out/pokeapi_shape.ts`,
then apply the same string rename inside that file by hand
(`"list(int)"` -> `"json_list(int)"`, `kind: "list"` -> `kind: "json_list"`)
so it matches the regenerated files.

## Setup (REQUIRED before any validation or commit; fresh worktrees lack all of this)
```bash
cd <worktree>/v6/tsv2 && pnpm install
cd <worktree>/v6/sprefa-store/js && pnpm install
cd <worktree>/v6/sprefa-extract && cargo build --release
```
Use absolute `cd` in EVERY shell command; cwd resets between calls.

## Validation gate (all must pass; each is budgeted, none may exceed its cap)
```bash
cd <worktree>/v6 && just conformance
cd <worktree>/v6 && just plunit
cd <worktree>/v6 && just text-door
cd <worktree>/v6 && just roundtrip
cd <worktree>/v6/tsv2 && bash scripts/sweep.sh   # then the pokeapi_shape.ts restore above
cd <worktree>/v6 && just typecheck
cd <worktree>/v6 && just tsv2-test
```
Manifest check after sweep: `git diff v6/prolog/compile/out/manifest.json`
must show ZERO bucket flips (reason strings like `list_of_relation_refs(span)`
are different atoms and stay unchanged).

## Commit rail (commit-or-report, non-negotiable)
- Commit ON THE BRANCH before exiting, up to 3 commits, subject prefix
  `prolog:`. Include regenerated compile/out files.
- If blocked, write FAILURE-REPORT.md at the worktree root with the exact
  failing command and its output, commit nothing broken, exit nonzero.
- NEVER pass --no-verify. A hook denial ends the approach; report it.

## Style laws
- Comments state only constraints code cannot show; no change-log narrative.
- Banned words in prose and identifiers: provenance, substrate, load-bearing,
  regime, refusal (say TODO / not built yet).
- Follow each file's existing style exactly.
