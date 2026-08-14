# Unified row type IR

## Goal

Refactor the current DL6 generic/interface implementation so the semantic type
system uses one row vocabulary from parsing through checking, monomorphization,
catalog emission, and target type emission.

Every type-system entity is a row. Every relationship is a row referencing
other row identities. Storage eligibility is data on a declaration row.

## Starting delta

The coordinator worktree contains the uncommitted implementation this lane is
refactoring:

`/Users/chrishafley/projects/sprefa`

Before editing, import only these tracked-file changes from that worktree:

```bash
git -C /Users/chrishafley/projects/sprefa diff -- \
  v6/prolog/0_generic_expand.pl \
  v6/prolog/lower.pl \
  v6/prolog/compile/parse_dl_dcg.pl \
  v6/prolog/print_dl.pl \
  v6/prolog/compile/7_emit_ts_types.pl \
  v6/prolog/compile/8_emit_rust_types.pl \
  v6/prolog/compile/test/plunit_tests.pl \
  v6/prolog/compile/test/emit_type_renderers.test.pl | git apply
```

Do not import or modify `compile/4_emit_jsonschema.pl`; its current coordinator
change belongs to the separate option-presence arc.

Read:

- `AGENTS.md`
- `plans/2026-08-13-generic-interface-type-ir.md` from the coordinator
  worktree, because it is untracked there
- `v6/prolog/0_type_plane.pl`
- `v6/prolog/0_generic_expand.pl`
- `v6/prolog/lower.pl`, especially `catalog_decl_rows/6`
- `v6/prolog/compile/parse_dl_dcg.pl`
- `v6/prolog/print_dl.pl`
- the focused tests changed by the imported patch

## Required semantic representation

Replace the parallel semantic constructors:

```prolog
generic_rel(...)
interface(...)
implementation(...)
```

with one normalized row vocabulary. Exact Prolog functor names may follow the
surrounding code, but the information model must contain:

```text
declaration(id, module, name, kind, storage)
parameter(id, owner, ordinal, name)
member(id, owner, ordinal, name, type)
application(id, constructor)
argument(id, application, ordinal, type)
constraint(id, subject, interface)
implementation(id, subject, interface_application)
derived_from(concrete, generic_application)
```

Declaration kinds in this slice:

```text
relation
interface
sum
primitive
```

Storage values in this slice:

```text
materialized
compile_time
```

The representation may use compact Prolog terms such as `type_row/…`, but it
must have stable IDs and explicit references. Names are labels, not identity.
Parameter references inside member types must resolve to parameter IDs before
substitution.

## Required behavior

Preserve the current accepted surface:

```dl6
interface json_encodable.

rel pair(T: json_encodable)(
  first: T,
  second: T
).

rel edge(value: pair(int)).
```

Preserve:

- generic application parsing in column types
- bound parameter round-trip
- marker interface declarations
- existing `rel ... is interface` spelling
- ground fixed-point monomorphization
- deterministic generated names
- reuse of equal applications
- wrong arity, duplicate interface, duplicate implementation, unknown
  interface, and unsatisfied-bound findings
- `json_encodable` closure over primitives, `option`, `json_list`, named
  records, enums, and explicit implementations
- concrete lowering into the existing `type_decl/2`, `col_type/3`, and
  storage `rel(...)` records
- catalog provenance rows
- TypeScript and Rust preserved generic declarations
- concrete generated type emission

## Boundaries

- Do not change `keyed/2`, `kind/2`, `keep/2`, or their surface syntax.
- Do not add interface members or methods.
- Do not add runtime polymorphism.
- Do not add higher-kinded types, default type arguments, variadics, generic
  rules, or Go emission.
- Do not change SQLite table semantics.
- Do not edit unrelated dirty or untracked files.
- Keep `row/11` as the external catalog wire for this slice. The unified
  semantic rows lower into it.
- Avoid a second generic graph model living only in `lower.pl`. Catalog rows
  must be a projection of the same normalized semantic rows used by bound
  checking and monomorphization.

## Required tests

Add direct tests for the normalized rows before testing their projections:

1. ordinary relation, generic relation, interface, parameter, member,
   constraint, application, argument, implementation, and concrete derivation
2. stable IDs under declaration-block permutation
3. parameter identity distinct from an equally spelled named type
4. one normalized application reused at two use sites
5. catalog rows link back to normalized semantic identities
6. TypeScript and Rust render from normalized rows rather than reconstructing
   generic structure from unrelated declaration terms

Run:

```bash
swipl -q -s v6/prolog/compile/test/plunit_tests.pl \
  -g "run_tests([expansion_order,rel_template_and_is_clause,catalog_type_ids,emit_type_renderers]),halt"

swipl -q -s v6/prolog/compile/test/plunit_tests.pl -g "run_tests,halt"

git diff --check
```

The coordinator observed six full-suite baseline failures before dispatch:

```text
catalog_plane_rail:level_plane_family_corpus_counts
expression_inventory:inventory_is_exactly_the_expected_rows
rel_zero_arity:a_root_rel_zero_still_has_no_storage
json_merge_patch:json_patch_lowers_with_the_null_stand_in_guard
json_merge_patch:merge_patch_stops_on_the_json_null_stand_in
json_merge_patch:merge_patch_stops_on_a_nested_json_null_stand_in
```

No additional failure is acceptable.

## Handoff

Commit the lane changes. Write `REPORT.md` with:

- commit SHA
- normalized row signatures
- old constructors removed
- files changed
- focused test result
- full-suite result with exact failures
- remaining limits

Then exit so Boop sends the completion hail.
