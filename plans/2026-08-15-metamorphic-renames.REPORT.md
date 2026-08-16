# Metamorphic rename pass — REPORT

One law: renaming every rel / variable / module-segment in a program
(camelCase, `__dunder__`, trailing_underscore_, ALLCAPS, max-length shapes
included) must produce emitted artifacts identical modulo the rename map.
Every mismatch is a name-sensitivity defect hypothesis with a cited compiler
site.

- Tool: `v6/prolog/compile/scripts/metamorphic_rename.pl` + `metamorphic_rename.sh`.
- Method: per compiled fixture, build a deterministic rename map over rel names,
  variable names, and module segments; apply it to the fixture term; compile the
  original AND the renamed variant through the sweep's own single-program chain
  (`program_plan -> lower_program -> boot_statements -> emit_program` + the three
  type-artifact emitters + schedule JSON), all captured IN MEMORY (nothing under
  `compile/out/` is touched); inverse-map the renamed artifacts; diff byte-for-byte.
- Seed: `20260815` (printed by the run). Five rel/segment shapes (snake, camelCase,
  trailing_underscore_, ALLCAPS, max-length) and six variable shapes (the five plus
  `__dunder__`).
- Scope: rel names (declared + derived + seeded), variable surface names, module
  path segments. NOT renamed: column names, primitive type names, pure struct/enum
  type names.

## Counts

| metric | run A | run B |
|---|---|---|
| swept | 341 | 341 |
| identical-modulo-map | 8 | 8 |
| findings | 314 | 314 |
| skipped | 19 | 19 |

Reproducible (two runs, same seed, identical):

```
RUN A  seed = 20260815   swept=341  identical-modulo-map=8  findings=314  skipped=19
RUN B  seed = 20260815   swept=341  identical-modulo-map=8  findings=314  skipped=19
```

## Fixtures swept and skipped

- Swept: 341 compiled fixtures (every `compiled` entry in `compile/out/manifest.json`).
- Skipped (19): fixtures whose rel names double as TYPE names in a
  `list(rel)` / `option(rel)` / `acyclic(option(rel))` / host-decl position. The
  rename reaches the rel position but the renamed program then fails silently in
  the type plane (a `harness_fail`, not a thrown refusal). This is the
  struct-as-rows rel/type name coupling, out of the rel/var/module rename scope:

  `list_entity_dense_sequence_end_to_end`, `list_interned_set_end_to_end`,
  `list_entity_linked_sequence_end_to_end`,
  `list_interned_set_dictionary_content_deduplicates`, `rel_element_list_round_trips`,
  `nested_rel_element_list_round_trips`, `option_list_of_rel_round_trips_absent_and_present`,
  `option_dense_sequence_of_rel_round_trips_absent_and_present`,
  `option_list_of_scalar_and_of_rel_in_one_rel`,
  `recursive_list_arg_parent_holds_child_node_values`, `a_self_loop_parent_is_rejected`,
  `a_two_row_parent_cycle_is_rejected`, `a_retracted_edge_frees_the_reverse_parent`,
  `extraction_fork_callgraph`, `extraction_fork_span_line`, `native_ts_query_term`,
  `struct_host_output_schedule_answer_interned`, `probe_output_comparison_guard`,
  `host_free_query_leaves_a_derived_rel_unsubscribed`.

## Findings

Two root causes, one issue each, filed:

| slug | root cause | site | smallest fixture |
|---|---|---|---|
| `snake-name-allcaps-mangling` | `snake_name/2` turns an ALLCAPS variable into a garbled column name | `analyze.pl:364` | `0_enum_variants.pl:81` `enum_name_is_a_column_type` |
| `type-name-non-injective` | `type_name/2` maps camelCase and snake_case rel names to one PascalCase type name | `compile/7_emit_ts_types.pl:172`, `compile/8_emit_rust_types.pl:172` | `10_list_elements.pl` `list_bare_column_round_trips` |

### 1. snake_name/2 mangles ALLCAPS variables (5 refusals)

`analyze.pl:364` `snake_name/2` runs `snake_codes/2`, which rewrites every
uppercase letter to `_lowercase` WITHOUT collapsing underscores already in the
name. `snake_name('VAR_CAPS_0') = 'v_a_r__c_a_p_s_0'` (each capital letter
becomes `_letter`; the pre-existing `_` between `CAPS` and `0` doubles into `__`).

The original `snake_name('G') = 'g'` matched the declared column
`col_type(picked/2, g, grade)`; the renamed `v_a_r__c_a_p_s_0` does not, so the
generated enum-companion column falls back to inferred type and the join type
check throws:

```
unsupported_construct(join_column_type_mismatch('b1."v_a_r__c_a_p_s_0"', text, 'b0."g"', int))
```

5 of 341 fixtures flip to this refusal under an ALLCAPS/camelCase variable
rename. The artifact diffs for the remaining fixtures show the same residue:
inferred columns named `v_a_r__c_a_p_s_0` instead of the declared column.

### 2. type_name/2 is non-injective (TS/Rust type collision)

`compile/7_emit_ts_types.pl:172` (and the Rust twin at `8_emit_rust_types.pl:172`)
split the rel name on `_`, drop empty parts, and upcase only the first character
of each part. Verified:

```
type_name('foo_bar')  = FooBar
type_name('fooBar')   = FooBar   (collision)
type_name('foo__bar') = FooBar   (collision)
type_name('REL_CAPS_3') = RELCAPS3
type_name('rel_tail_2_') = RelTail2
```

A program with a camelCase rel `fooBar` beside a snake_case rel `foo_bar` emits
two `interface FooBar` (two `struct FooBar` in Rust) with no diagnostic. This is
the same name-sensitivity class PR #262 fixed for the `__dunder__` empty-part
drop (`exclude(empty_atom, ...)`) and for the SQL-side `module_type_stem`; the
prolog camelCase collision remains.

### 3. rel-as-type name coupling (19 skipped, not filed)

The 19 skipped fixtures and the one `compound_pattern_on_arrival_rel` refusal are
one phenomenon: a rel name that also serves as a struct type (`list(rel)`,
`option(rel)`, `acyclic(option(rel))`, relation patterns, host columns) does not
survive a consistent rename. The type plane resolves those positions against the
rel declaration by name, and the renamed program fails silently rather than
throwing a named refusal. Left as a boundary of the rel/var/module scope; a
follow-up pass that renames the type plane in lockstep with the rel plane would
turn this from a limitation into coverage.
