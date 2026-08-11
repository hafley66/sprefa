# FAILURE-REPORT: golden-flex GENERICS section blocked by pre-existing decl-ordering bug

Lane `feature/generic-text-door`. Blocked sub-deliverable: the `GENERICS`
section in `v6/dl/fixtures/golden-flex.dl6` (one decl per list constructor,
arrivals + a retraction).

The text-door itself is complete and green: `parse_dl.pl` spells the four
constructors, `print_dl.pl` round-trips them, the term-door-only skips in
`roundtrip.sh` and `text_door_receipt.pl` are removed, and five new
`conformance/TEXT_DOOR` fixtures compile byte-identical on both doors.
All validation gates pass without the GENERICS section
(conformance / plunit / text-door / roundtrip / sweep / typecheck /
tsv2-test / golden-flex).

The GENERICS section cannot be added because any program that mixes a
relational list constructor (`list(T)`, `list_entity_dense_sequence(T)`,
`list_interned_set(T)`, `list_entity_linked_sequence(T)`) with a struct-typed
ref column read in a rule fails to compile with a false
`relation_column_type_conflict`.

## Reproduce

`just golden-flex` after adding any relational list column to
`golden-flex.dl6` (the file's `tree_label` rule reads the struct column
`tree.site`). Minimal standalone program through `compile_dl6`:

```
rel plot(row: int, col: int).
rel patch(label: text, at: plot).
rel tree(tree_id: int, species: text, site: patch).
rel tree_label(tree_id: int, label: text).
tree_label(TreeId, Label) <- tree(TreeId, _Species, Site), decode(Site, {label: Label}).
rel box_list(tree_id: int, items: list(text)).
```

Output (exact):

```
ERROR: unsupported_construct(at('<prog>',5,relation_column_type_conflict(tree/3,site,patch,tree_label/2,label,text)))
```

## Root cause

`v6/prolog/0_generic_expand.pl:generic_artifact_order/3` runs `msort(Decls, _)`
as soon as one generic instance is minted (any relational list column). `msort`
scrambles the per-rel column order of every `col_type/3` entry: `rel
tree_label(tree_id, label)` becomes `[label, tree_id]` and `rel tree(...)` 
becomes `[site, species, tree_id]` (alphabetical, not declaration order).

`v6/prolog/0_type_plane.pl:relation_columns_and_types/5` reads column order
from the `col_type/3` decl list position, so it sees the scrambled order. For
the rule above that makes the `tree_id` variable land in `tree/3.site`
(type `patch`) and in `tree_label/2.label` (type `text`) simultaneously, which
`v6/prolog/0_program_check.pl:program_violation(relation_column_type_conflict,...)`
mistakes for a real type conflict. The two columns are unrelated; it is a false
positive.

The bug is pre-existing and only reachable now because the text door makes the
list constructors spellable (previously bare `list(T)` hit
`removed_word(list)`). Fixing it is design work on core files the lane does not
own (`0_generic_expand.pl` ordering semantics, `0_type_plane.pl` column-order
resolution), which conflicts with the lane's "implements, does not design"
mandate.

## Notes for the fix

- A plain stable partition in `generic_artifact_order` removes the false
  conflict but breaks `expansion_order:generic_e2e_declaration_permutation_is_
  byte_deterministic` (plunit), which asserts the expanded program is
  independent of input decl order including within-rel ordering.
- The correct layer is column-order resolution: `relation_columns_and_types`
  must not trust the msort-scrambled decl order; declaration order is already
  captured by `parse_dl.record_column_order/2` and by the `col_type/3`
  column identity. `json_list(T)` and other non-relational types are unaffected.

The rel-element fixture (`list(some_rel)`) is also skipped in the
conformance/TEXT_DOOR fixtures: the rel-element engine path is still refused
on main (`list_of_relation_refs_still_refused`), so only the nested spelling
`list(list(text))` is exercised.
