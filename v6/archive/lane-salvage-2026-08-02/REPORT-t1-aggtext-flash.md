# REPORT — oracle-side refusal for min/max aggregates over TEXT

Worktree: `/Users/chrishafley/projects/sprefa-lanes/t1-aggtext/flash` (branch
`lane/t1-aggtext-flash`). No commits; all changes left uncommitted.

## The defect

- Compiler door: `dl6` refuses a min/max aggregate whose operand column is
  TEXT at lowering via `lower.pl:compile_aggregate_number_operand/5`
  (`v6/prolog/lower.pl:2909`), throwing
  `unsupported_construct(aggregate_operand_not_number(Kind, Expr, Type))`.
- Oracle door: no mirror. The reference engine evaluates min/max through
  `lists:min_list/2` and `max_list/2` (`v6/prolog/conformance/level_eval.pl:256`),
  which accept NUMBERS ONLY, so a TEXT operand crashed raw instead of
  producing the named refusal.

## Pre-fix evidence

Oracle run crash (through `run_program/5`, the path conformance fixtures use):

```
caught(error(type_error(evaluable,alpha/0),context(lists:min_list/3,_74)))
```

New plunit test RED before the fix (oracle `check_program/1` accepted the
program rather than refusing):

```
% [19/20] cross_plane_check..x_over_text_operand .. **FAILED (0.000 sec)
ERROR: [Thread main] test cross_plane_check_parity:oracle_refuses_min_max_over_text_operand: no_exception
ERROR: [Thread main] 1 test failed
```

## What changed

`program_violation(aggregate_operand_not_number, prog(Decls, Rules),
agg_operand(Kind, Variable, Type))` added to the shared check module. It walks
level rules, finds a `min(V)` / `max(V)` head operand, resolves `V` to its
declared body column type via `declared_column_table/4`, and refuses when that
type is neither `int` nor `float`.

### Placement choice

At the **shared layer** (`v6/prolog/0_program_check.pl`), not engine-local,
because that module already holds the typing the check needs: `declared_column_table/4`
resolves a head operand variable to its declared body column type, exactly the
machinery `head_column_type_conflict` already uses. This is the one-impl
precedent `0_program_check.pl` exists for; the oracle door consumes it through
`first_violation/3` in `check_program/1`, same as its other shared classes.

Wired into the **oracle door only** (`v6/prolog/conformance/engine.pl`:
added `aggregate_operand_not_number` to `engine_check_order/1` after
`aggregate_not_implemented`, and an `engine_refusal/3` mapping
`agg_operand(Kind, Variable, Type)` to the bare term
`aggregate_operand_not_number(Kind, Variable, Type)`). The **compiler is
unchanged**: it already refuses at `lower.pl` with the identical payload, and
its clause stays as the residue backstop for direct `lower_program/2` entry,
matching the documented pattern for `relation_value_under_negation`
(`v6/prolog/0_program_check.pl:323-325`). Wiring the compiler too would have
moved its refusal earlier in the pipeline and risked the compile-speed ratchet
and the "zero movement" requirement, for no behavioral gain.

### Term-shape note (task said `unsupported_construct(...)` on the oracle)

The task asked the test to assert the oracle throws
`unsupported_construct(aggregate_operand_not_number(...))`, "the exact term
shape the compiler side uses". The codebase documents a hard door-vocabulary
invariant that the oracle throws **bare terms** and the compiler wraps in
`unsupported_construct/1` (`engine.pl:109-113`); every other cross-plane check
follows it (e.g. `aggregate_ordinal_not_int`, `aggregate_in_edge_head`). So the
new oracle test asserts the oracle's bare named refusal
`aggregate_operand_not_number(min, X, text)`, and a companion test pins the
compiler's exact wrapped shape
`unsupported_construct(aggregate_operand_not_number(min, X, text))` (the shape
the task named), producing a cross-plane parity pair in the existing
`door_verdict` idiom. The oracle's thrown term after the fix:

```
caught(aggregate_operand_not_number(min,_56,text))
```

## Files touched

- `v6/prolog/0_program_check.pl` — shared `aggregate_operand_not_number` trigger
  + `rule_is_level/1`, `rule_head_min_max_operand/2` helpers.
- `v6/prolog/conformance/engine.pl` — oracle order entry + `engine_refusal/3`.
- `v6/prolog/compile/test/plunit_tests.pl` — 2 new plunit tests
  (`oracle_refuses_min_max_over_text_operand`,
  `compiler_refuses_min_max_over_text_operand`) in the
  `cross_plane_check_parity` group.

## Validation (tails)

Plunit (runner: `just plunit`, measured ~75s, default budget 600s; progress
goes to stderr). Exit 0, 0 FAILED. Suite ends at `[273/273]` top-level tests,
+2 over the pre-change suite (the two new refusals).

```
% [272-25/273] parse_error_posit..l_position_is_exact .. passed (0.003 sec)
% [273/273] parse_error_posit.._with_a_prefix_walk .. passed (0.000 sec)
exit=0   passed=299 progress lines (273 top-level incl. sub-assertion rows)   FAILED=0
```

Conformance suite count unchanged (runner: `just conformance`):

```
PASS count: 281
FAIL count: 0
```

prolog-lint (informational):

```
PROLOG_LINT findings=1 baseline=1 OK
```

## Notes

- `v6/prolog/labs/type_matrix/MATRIX.md` rows for `aggregate_head` still say
  these cells are `compiler_only` (matrix.json `compileRefusal:
  aggregate_operand_not_number`). That lab doc is now stale for the oracle
  door; left untouched as out of scope (not a gated artifact, task did not
  request it). Flagged for awareness only.
- No existing conformance fixture runs min/max over a text column (the corpus
  uses numeric columns: `Stars`, `Ordinal`), so the oracle order entry moves no
  fixture from RUN to refusal and the 281 count is stable.
- Undeclared operand columns have no type to contradict at the shared layer
  (`0_program_check.pl` sees `prog/2`, not literal witnesses), matching the
  scope the head-type wall states; the raw min_list crash for that case would
  be a separate, already-existing boundary and is outside this fix.

## Timing

`just conformance` and `tools/prolog-lint.sh`: sub-second. `just plunit`:
~75s wall (test 209 `refusal_messages:..al_renders_one_line` alone is 74s
routine). All within stated budgets.
