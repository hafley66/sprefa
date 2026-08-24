# Unified row type IR report

## Status

The shared worktree has the implementation changes. No commit was created.

Historical report. `issues/remove-rel-is/item.md` removed relation conformance
suffixes and implementation rows on 2026-08-23.

## Normalized row signatures

```prolog
declaration(Id, Module, Name, Kind, Storage)
parameter(Id, Owner, Ordinal, Name)
member(Id, Owner, Ordinal, Name, Type)
constraint(Id, Subject, Interface)
application(Id, Constructor)
argument(Id, Application, Ordinal, Type)
implementation(Id, Subject, interface_application(Interface))
derived_from(Concrete, GenericApplication)
```

IDs are deterministic atoms derived from declaration labels, owner IDs,
ordinals, and canonical application arguments. Parameter references in member
types use parameter IDs.

## Old constructors

`generic_rel/3`, `interface/2`, and the old two-field
`implementation/2` semantic records are no longer constructed by
`generic_type_ir/2`. Source `rel_template/3`, `interface_decl/2`, and
`rel_is_implementation/2` remain accepted as parser/storage boundary terms.

## Files changed

- `v6/prolog/0_generic_expand.pl`
  - normalized semantic rows
  - stable IDs
  - normalized application and derivation graph
  - legacy storage provenance projection
  - semantic row transport through expansion
- `v6/prolog/lower.pl`
  - catalog metadata rows carry normalized semantic IDs in the existing
    `row/11` wire
- `v6/prolog/compile/test/plunit_tests.pl`
  - direct normalized-row tests
  - declaration permutation and parameter identity tests
  - equal-application reuse test
  - catalog semantic-ID projection test

## Verification

Focused command:

```text
run_tests([expansion_order,rel_template_and_is_clause,catalog_type_ids,emit_type_renderers])
```

Result: 77/77 passed.

Full command:

```text
swipl -q -s v6/prolog/compile/test/plunit_tests.pl -g "run_tests,halt"
```

Result: 654/660 passed. The six failures are the recorded baseline:

- `catalog_plane_rail:level_plane_family_corpus_counts`
- `expression_inventory:inventory_is_exactly_the_expected_rows`
- `rel_zero_arity:a_root_rel_zero_still_has_no_storage`
- `json_merge_patch:json_patch_lowers_with_the_null_stand_in_guard`
- `json_merge_patch:merge_patch_stops_on_the_json_null_stand_in`
- `json_merge_patch:merge_patch_stops_on_a_nested_json_null_stand_in`

`git diff --check` passes for the lane files.

## Remaining limits

- Catalog `row/11` remains the external wire. Semantic IDs are carried in its
  existing auxiliary field.
- Source generic provenance still projects to `generic_decl/3` and
  `generic_instance/3` for existing storage metadata consumers.
- Interface members and methods are deferred.
- Parameterized interface applications in bounds remain deferred.
- Emitter policy selection remains deferred.
