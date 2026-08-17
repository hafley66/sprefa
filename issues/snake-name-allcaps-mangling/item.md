---
created: 2026-08-15
updated: 2026-08-16
type: bug
reporter: fable
status: fixed
priority: normal
labels:
- bugmine
- area:compiler
closed: 2026-08-16
commits:
- hash: 5cf2e2fe
  summary: snake_codes word-boundary rewrite; 7-row pinning table; gates match base
---

# snake_name/2 mangles ALLCAPS variable names into garbled column names

## Description

## Description

Metamorphic rename pass (v6/prolog/compile/scripts/metamorphic_rename.pl) found: renaming a variable to an ALLCAPS shape changes the inferred column name via `snake_name/2`, which can flip a program from compiling to a `join_column_type_mismatch` refusal.

## Site

`analyze.pl:364` `snake_name(VarName, ColumnName)` — snake_codes turns every uppercase letter into `_lowercase` without collapsing the underscores that are already in the name.

## Repro (smallest fixture)

`conformance/fixtures/0_enum_variants.pl:81` `enum_name_is_a_column_type`. Rename the variable `G` (used in `picked(Id, G)` / `grade_tag(G, Tag)`) to `VAR_CAPS_0`:

`snake_name('VAR_CAPS_0') = 'v_a_r__c_a_p_s_0'` (each capital letter becomes `_letter`; the pre-existing `_` between CAPS and 0 doubles into `__`).

The original `snake_name('G') = 'g'` matched the declared column `col_type(picked/2, g, grade)`; the renamed `v_a_r__c_a_p_s_0` does not, so the generated enum-companion column falls back to inferred type and the join type check throws:

`unsupported_construct(join_column_type_mismatch('b1."v_a_r__c_a_p_s_0"', text, 'b0."g"', int))`

## Impact

5 of 341 compiled fixtures flip to this refusal under an ALLCAPS/camelCase variable rename. snake_name is a lossy CamelCase->snake_case transform (not injective: distinct names can collide, and interior structure is silently rewritten), which is exactly the name-sensitivity class the metamorphic pass exists to find.
