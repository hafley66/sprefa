# v6/prolog/compile.pl -> v6/prolog/compile_pl/

module head keeps lines 1..79 (79 lines): 28 directives, 0 stray clauses

| part | lines | span | clauses | predicates |
|---|---:|---|---:|---:|
| `0_fixtures.pl` | 58 | 80-137 | 2 | 2 |
| `1_program_plan.pl` | 219 | 138-356 | 18 | 14 |
| `2_reserved_namespace.pl` | 47 | 357-403 | 6 | 6 |
| `3_fixture_entry.pl` | 39 | 404-442 | 8 | 5 |
| `4_storage_names.pl` | 203 | 443-645 | 37 | 21 |
| `5_dl6_door.pl` | 124 | 646-769 | 17 | 12 |
| `6_program_phases.pl` | 99 | 770-868 | 9 | 7 |
| `7_phase_trace.pl` | 130 | 869-998 | 17 | 14 |
| **total** | **919** | | | |

parts over 700 lines: none

## clauses of one predicate landing in two parts

none

## directives sitting below the first anchor

| line | directive | part it falls in |
|---|---|---|
| 317 | `:- meta_predicate check_step(+,0)` | `1_program_plan.pl` |

Each one moves up into the module head file, above the includes.

## cross-part call edges

| from | to | callees |
|---|---|---|
| `1_program_plan.pl` | `2_reserved_namespace.pl` | `check_reserved_namespace/1` |
| `1_program_plan.pl` | `3_fixture_entry.pl` | `check_single_arity_per_name/1`, `check_world_shapes/3` |
| `1_program_plan.pl` | `4_storage_names.pl` | `relation_shapes/5`, `relation_storage_names/6` |
| `3_fixture_entry.pl` | `0_fixtures.pl` | `read_fixture_term/4` |
| `3_fixture_entry.pl` | `1_program_plan.pl` | `default_intern_mode/1`, `throw_as_compiler_unsupported/1` |
| `3_fixture_entry.pl` | `6_program_phases.pl` | `compile_program/7` |
| `4_storage_names.pl` | `1_program_plan.pl` | `throw_as_compiler_unsupported/1` |
| `4_storage_names.pl` | `2_reserved_namespace.pl` | `compiler_owned_contract/1`, `reserved_namespace_name/1` |
| `5_dl6_door.pl` | `1_program_plan.pl` | `default_intern_mode/1` |
| `5_dl6_door.pl` | `6_program_phases.pl` | `compile_program_phases/8`, `throw_text_door_error/2` |
| `5_dl6_door.pl` | `7_phase_trace.pl` | `parse_debug/2`, `run_compile_phase/4`, `write_compile_trace/2` |
| `6_program_phases.pl` | `1_program_plan.pl` | `default_intern_mode/1`, `program_plan/3` |
| `6_program_phases.pl` | `7_phase_trace.pl` | `boot_debug/2`, `emit_debug/2`, `lower_debug/4`, `run_compile_phase/4`, `with_emit_context/3`, `write_compile_trace/2` |

13 directed part pairs

## what each part owns

| part | owns |
|---|---|
| `0_fixtures.pl` | reading one fixture term out of a fixture file, and finding a fixture by name |
| `1_program_plan.pl` | program_plan/3, the one term lower.pl and emit_ts.pl both read, plus the compiler-type-rule partition, reference-target materialization and the plan debug dumps |
| `2_reserved_namespace.pl` | the compiler-owned __ namespace: which names are reserved and what a violation reads as |
| `3_fixture_entry.pl` | the compile_fixture entry points, world shape checks and the single-arity-per-name check |
| `4_storage_names.pl` | shape identity and storage naming: shape digests, declaring-module stems, ascii folding and unique suffix allocation |
| `5_dl6_door.pl` | the .dl6 text door: emitter and schedule options, arrival terms, seeded forms and the fact partition |
| `6_program_phases.pl` | compile_program and the phase pipeline that runs parse, lower, boot and emit, and writes the compiled output |
| `7_phase_trace.pl` | phase measurement, the per-phase debug hooks and the compile trace file |
