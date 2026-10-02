# generic-rules: state and next step

Branch `feature/generic-rules`, base `d80bf99c5`. HEAD `c18e7e826`; M6 Extends work uncommitted in the tree and saved as `plans/v8/2026-09-30-generic-rules.m6-extends.patch`.

| commit | sha | content |
|---|---|---|
| 1 | `9c85ac285` | M3 + identity |
| 2 | `cd3856efb` | M1 + P1-P5 (`tests/it/_9e_generic_rules.rs`) |
| 3 | `e30a7af4d` | GeneratedSyntax (macrotime) |
| engine | `978327879` | sqlite UNION dedupe + `EXPANSION_BUDGET` rounds |
| 4 | `1e63008e1` | HeadCall, BodyCall, Variable (readers bind the position) |
| IR fix | `610509075` | adorned demand copies read their product under the IR bridge |
| 5 | `2582437d9` | Option, Partial |
| 6 | `624f66c9b` | Pick, Exclude (+ tuple positions for generic columns) |
| perf | `c18e7e826` | only sum readbacks keep a snapshot-reading rule demanded |

## Stopped: HistoryV1, Conforms, ConformsAll, Intersect (M6)

`git grep -n "(intern " -- prelude macrotime std`:
Key (2_constructor_rules.dl7:3), HistoryV1 (:16), Conforms
(4_type_algebra.dl7:155), ConformsAll (:193), Intersect (:227).

Today `(Conforms ?Source ?Contract ?Proof)` is a validated view: the builder
interns, then `valid_conformance` gates (4_type_algebra.dl7:154-160). User
code reads it as a check: fixtures/extract/main.dl7:16,
fixtures/openapi/todo.dl7:20, oracle/compile/sources/test/fixtures/
3_type_algebra.dl7:31 (`(Conforms MissingName Named ?Proof)` must fail),
tsi_project/0_contract.dl7:14. As a generic, that goal builds with no gate.
HistoryV1's gate is three goals (2_constructor_rules.dl7:13-15) with no
named relation; its readers are 3_derived_rules.dl7:149-188.

Fork for the user: where the gate lives once the builder rule is gone
(every caller adds the gate goal; or the generic carries it; or these
four stay rules).

## M6 attempt with kernel `field` (superseded below)

In the tree: `field/4` kernel relation seeded from declared checked-graph
edges (`src/_4_comptime/_1_api.rs` `field_seeds`); `Extends`/`ExtendsAll`
over `asked`/`candidate`/`fails_at`/`fails`/`undecided`
(`prelude/4_type_algebra.dl7`); HistoryV1 and Intersect keep their intern
builders (HistoryV1 interns before `Extends` and asks through
`intern_snapshot`); callers write `asked` from `field`; `tests/it/_9f_extends.rs`
cases 1-15 and 18 pass on all five engines; goldens refrozen.

Red: `_6_imports::todo_routes_params_and_conforms_match_the_frozen_rows`
(5 routes, 2 expected) and `_1_compile_oracles::every_committed_case_file_exists_and_every_case_is_indexed`
(3380498 bytes, budget 3145728).

| site | what is missing | fork |
|---|---|---|
| `fixtures/openapi/todo.dl7` `TodoShape (id: tsi.string)` | a dotted column target is written by a derived-bind rule, so no `field` row; the contract passes every source | (a) round seeds add `field` from last round's derived-bind `:` rows (`_2_rounds.rs` `compiler_round_seeds`); (b) derived binds write `field`; (c) dotted-column contracts unchecked |
| `dl8 eval` | no `field` rows at runtime; `Extends` answers every asked pair | ship `field` into the runtime program (D3 app mode), or gate; a `node` gate is `aggregate_dependency_cycle` through HistoryV1/Intersect readers |
| HistoryV1, Intersect | gate location once the builder goes | every caller adds the goal; readers carry a `*_valid` gate; stay rules |
| cases 16-17 | frontiers name relations by program name (`src/bin/dl8.rs:581`); `kernel(field)` has none | CLI/kernel naming |
| oracle byte budget | `field` rows per product column | raise `oracle/compile/status.json` `budget_bytes` or shrink |

## M6 landed: Extends over edge_snapshot (brief `plans/v8/2026-10-01-type-bind-swap.brief.md`)

- Kernel `field` removed; `src/` carries no M6 change (`git grep -n '"field"' -- src` empty).
- User pick (2026-10-01, option B): no `type_bind` relation. `candidate`, `has_field`,
  `fails_at`, `undecided` read `edge_snapshot` directly, so saved compile outputs carry no copy
  of `:` rows. `(not (primitive_contract ?Contract))` stops the walk at a primitive contract
  (a primitive's binds are its namespace members, `str.cons` etc.).
- Every check reads last round's edges: a program using Extends takes >= 2 comptime rounds
  (todo.dl7 and extends/0_cases.dl7: 2, same as with kernel `field`).
- `fixtures/call/*.dl7`: the checker is a `(checker_of Bound Checker)` row; a `check` edge on
  the contract is a bind the walk reads.
- HistoryV1 and Intersect still call `intern` (fork 3 in `plans/v8/2026-10-01-open-forks.md`).
- Later lane: shadowing of kernel names by user names (name resolution untouched here).
