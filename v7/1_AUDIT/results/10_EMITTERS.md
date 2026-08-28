# Slice 10 — compiler pipeline, storage projection, emitters

DL6 files audited: `v6/prolog/compile.pl`, `v6/prolog/compile/0_storage_projection.pl`,
`v6/prolog/compile/registry.pl`, the numbered emitter files under `v6/prolog/compile/`,
`v6/prolog/emit_ts.pl`, `v6/prolog/emit_rust.pl`. Read-only audit; report only.

## Contents

1. The canonical program contract (plan/9, lowered/8, the emitter seam)
2. Report blocks — compile.pl pipeline
3. Report blocks — 0_storage_projection.pl
4. Report blocks — registry.pl
5. Report blocks — emit_ts.pl
6. Report blocks — emit_rust.pl
7. Report blocks — doc/schema emitters (1,2,3,4,5,7,8,9) + dd plan + oracle dump
8. Counts by class
9. Canonical term shapes entering and leaving the slice
10. Hidden state
11. Smallest extraction boundary
12. First dependency forcing adaptation
13. Unresolved questions for a V7 language ruling

---

## 1. The canonical program contract

Everything every emitter consumes is fixed by two terms computed once in
`compile.pl` and threaded verbatim into every emitter:

```
plan(Name, Prog, Types, RelPlans, ArrivalTargets,
     RuleOrder, EdgeRules, SubscribedRels, InternMode)        % plan/9
lowered(Name, Ddl, ArrivalStatements, EdgeStatements,
        LevelStatements, DeltaStatements, RelPlans,
        ArrivalTargets)                                        % lowered/8
bootstmt(Rel, Sql, Params)                                    % boot rows
```

The emitter seam is `call(Emitter, Name, Plan, Lowered, BootStatements, Text)`
(compile.pl:869). `emit_ts:emit_program/5`, `emit_rust:emit_program/5`, and
`isolated_compiler_dd:compile_program/5` are all substitutable behind that
signature with no call-site special case. `Initial` and `Schedule` are NOT seam
arguments; the dd emitter reads them out of band via the thread-local
`dd_compile_context/2` (compile.pl:35, 899-901).

### The contract that keeps `sprefa-engine-rs` unchanged

`emit_rust.pl:656-684` assembles the `ProgramJson` dict. Every field is a field
the engine deserializes; the exact set is the preserved contract:

| ProgramJson field | source term |
|---|---|
| `name` | Plan Name |
| `ir_version` | `ir_version(1)` pinned in emit_ts.pl:38 and emit_rust.pl:40 |
| `intern_mode` | plan/9 arg 9 |
| `ddl` | lowered/8 arg 2 (+ `__enum_identity_*` DDLs) |
| `rel_columns`, `rel_column_types` | rel/5 plans via `relplan_parts/6` |
| `arrival_targets` | plan/9 arg 5 (Name/Arity, name-only in JSON) |
| `boot` | bootstmt/3 list |
| `final_select` | deltastmt SelectSql + `query_order_by_map/3` |
| `queries` | query decl names |
| `arrival_templates` | arrivalstmt/6 (kind, add_sql, del_sql) |
| `text_intern_plan` | lower:program_text_intern_plan/3 |
| `struct_types`, `struct_ref_columns` | lower:struct_type_plans/4 |
| `enum_types`, `enum_ref_columns` | emit-local over Decls+RelPlans+DeltaStatements |
| `pre_snapshot_rels` | `plan_pre_refs/2` (rules with `pre/1` bodies) |
| `relations` | deltastmt/5 + relplan shape + arrival SQL |
| `edges` | edgestmt/9 |
| `levels` | levelstmt/7 (+ refcountsql/16, expandplan/8, dredplan/24, aggsql/7, avgsql/7, supportcount/2, fixpointir/5) |
| `retentions` | retentionstmt/3 |
| `uses_tick` | `program_uses_tick/2` |
| `reconcile_every_tick` | `reconcile_every_tick/2` (negative body-use scan) |
| `incremental_safe` | constant `true` (deserialized by engine-rs, no live meaning) |
| `host_plans`, `bind_plans` | sh_decl/4 + bind_decl/2 via `compile_host_decl/3`, `host_plan_contract/2`, `host_execution/3`, `bind_read_literals/4` |

Storage-name and type laws the engine depends on transitively:
`boundary_type_name/2` (ref / relation_id / json / list / bytes / pass-through),
`key_indices` 0-based, `__delta_<table>`, `__frontier_<table>`,
`__next_frontier_<table>`, `__departure_frontier_<table>` spellings,
`recursion_group {group, round_cap, heads}`, `ir_version(1)` equality check on
both runtimes, and `intern_mode` (`dict` default, `default_intern_mode/1`).

---

## 2. Report blocks — compile.pl pipeline

```prolog
% File: v6/prolog/compile.pl:204
% Existing comment: plan/9 field contract, the record's single documentation site
% Signature: program_plan(+Term-Bindings, +Options, -Plan)
% Called by: compile_program_phases_moded, 6_isolated_compiler_dd, scripts/bop_check.pl
% Calls: preserve_compiler_type_rules, prepare_program_for_compiler,
%        expand_program_with_bindings, materialize_reference_target_rels,
%        materialize_catalog_rel, type_definitions, check_supported_subset_expanded,
%        check_clock_program, check_world_shapes, program_refs, declared_refs,
%        seeded_refs, derived_refs, rel_columns, program_column_types,
%        relation_shapes, relation_storage_names, rel_plans (findall),
%        derive_storage_rows, replace_storage_type_rows, project_storage_relplans,
%        check_edge_head_column_types, sql_rule_order, subscribed_rels
% Tests: v6/prolog/compile/test/dl6c.test.pl, conformance fixtures
% V7 class: adapt
% Parser coupling: term-shape (prog/2, fixture/5)
% Preserved law: one plan/9 record is the single input of lower and every emitter;
%                both stay pure functions of it.
% DL7 seam: in = cons-tree program + variable bindings; out = plan/9 with the
%           field order above (engine plan fields preserved).
```

```prolog
% File: v6/prolog/compile.pl:839
% Existing comment: none (doc above frontier_option/2 at 828)
% Signature: compile_program_phases(+Name, +Term, +Bindings, +Initial, +OutFile, +Emitter, +Options, -PhaseMeasurements)
% Called by: compile_program/7, compile_dl6/2,3
% Calls: frontier_option, with_frontier_mode, program_plan, lower:lower_program,
%        boot_statements, with_emit_context, call(Emitter,...), write_compiled_output
% Tests: v6/prolog/compile/test/dl6c.test.pl (target_emitter/3 rows)
% V7 class: extract
% Parser coupling: none
% Preserved law: five phases (plan, lower, boot, emit, write) in fixed order,
%                each measured and traced; emitter is a Module:Pred option.
% DL7 seam: (Name, Plan, Lowered, BootStatements) -> Text; phases measurable.
```

```prolog
% File: v6/prolog/compile.pl:35,899
% Existing comment: "Out-of-band channel for the emitter seam..."
% Signature: dd_compile_context/2 (thread_local); with_emit_context(+Initial, +Fixture, +Goal)
% Called by: compile_program_phases_moded, isolated_compiler_dd:compile_program/5
% Calls: assertz/retractall
% Tests: compile/test/6_isolated_compiler_dd.test.pl (door receipts)
% V7 class: adapt
% Parser coupling: none
% Preserved law: Initial and Schedule reach an emitter that needs them without
%                moving the 5-arg seam signature; emit_ts never reads them.
% DL7 seam: V7 should put Initial/Schedule IN the seam term and drop the
%           thread-local entirely (adapter change, no semantic law lost).
```

```prolog
% File: v6/prolog/compile.pl:202,206
% Existing comment: THE BUILD DEFAULT ... referee said NO (flip attempt 2026-08-08)
% Signature: default_intern_mode(-Mode); intern_mode/2
% Called by: every compile_* entry
% Calls: none
% Tests: interning contract tests (§15.4/§15.5 in emitted-field comments)
% V7 class: oracle
% Parser coupling: none
% Preserved law: default intern mode is dict; a database built by one mode is
%                unreadable by the other, so the artifact names its mode.
% DL7 seam: intern mode stays a plan/9 field with a pinned default.
```

```prolog
% File: v6/prolog/compile.pl:457-658 (relation_storage_names/6 and helpers:
%   relation_storage_candidate/6, storage_shape_suffix/4, storage_shape_digest/3,
%   shape_closure/4, shape_column_targets/3, relation_shapes/5, rel_module_hash_index/2,
%   rel_module_hashes/4, relation_declaring_module_stem/5, storage_identifier/2,
%   storage_base_name/3, sqlite_ascii_fold/2, allocate_storage_names/3,
%   unique_storage_name/4, unique_storage_suffix/4)
% Existing comment: "shape identity : docs/storage-name-hash.md"
% Signature: relation_storage_names(+EntryStem, +Decls, +DerivedRefs, +Shapes, +Refs, -Names)
% Called by: program_plan (step relation_storage_names)
% Calls: rel_module_hash_index, relation_declaring_module, storage_identifier,
%        storage_base_name, storage_shape_suffix, sqlite_ascii_fold,
%        allocate_storage_names, storage_shape_digest, shape_closure
% Tests: compile/test/0_storage_projection.test.pl; text_door_receipt.pl byte-compare
% V7 class: extract
% Parser coupling: none
% Preserved law: the physical table name is a pure function of the declaring
%   module stem, the relation name, the storage shape digest, and collision
%   suffixes; text door and term door reach the same spelling.
% DL7 seam: Decls + Refs + Shapes -> sorted Name/Arity-StorageName pairs.
```

```prolog
% File: v6/prolog/compile.pl:702-737 (emitter_option/2, schedule_option/4,
%   read_schedule_file/4, arrival_term/4)
% Existing comment: ".dl6 text door default is emit_ts:emit_program..."
% Signature: emitter_option(+Options, -Module:Pred)
% Called by: compile_dl6/3
% Calls: memberchk/2, read_schedule_file, arrival_column_types, schedule_value
% Tests: compile/test/dl6c.test.pl (target_emitter/3)
% V7 class: adapt
% Parser coupling: surface-policy (schedule(File) JSON shape, `add`/`del` sign)
% Preserved law: emitter/1 option swaps the emitter with no call-site special
%   case; external schedule JSON fills the fixture term's Schedule slot.
% DL7 seam: emitter option stays; schedule JSON keeps the {rel, row, sign} shape.
```

```prolog
% File: v6/prolog/compile.pl:35 (with_emit_context at 899; run_compile_phase/4 at
%   906; measure_phase/3 at 969; write_compile_trace/2 at 990)
% Existing comment: none above run_compile_phase except the phase-failure ball note
% Signature: run_compile_phase(+Name, +Phase, :Goal, -Measurement)
% Called by: compile_program_phases_moded, compile_dl6
% Calls: measure_phase, dl6_last_checkpoint, 0_trace statistics_snapshot
% Tests: compile/test/dl6c.test.pl (COMPILE-TRACE line)
% V7 class: extract
% Parser coupling: none
% Preserved law: a phase failure names the phase and the checkpoint; the thrown
%   term keeps its shape for callers that classify it.
% DL7 seam: measurement(11 args) term is instrumentation; free to re-shape.
```

```prolog
% File: v6/prolog/compile.pl:741-797 (dl6_seeded_form/3, partition_dl6_facts/4,
%   dl6_fact/2, dotted_relation_head/1, fact_args_atomic/1)
% Existing comment: "Shared by the two text-door callers that parse .dl6 themselves."
% Signature: dl6_seeded_form(+Prog, -Initial, -ProgOut)
% Called by: compile_dl6 (driver step)
% Calls: resolve_relation_paths, dl6_fact
% V7 class: drop
% Parser coupling: term-shape (`<-`/`<+` rule terms, ground-fact seeding)
% Preserved law: ground bodiless DL6 clauses become seed rows; non-ground stay refused.
% DL7 seam: DL6 source compatibility is out of scope; the seed concept moves to
%   the cons-tree frontend.
```

Other compile.pl predicates (read_fixture_term/4, find_fixture/4, reserved-namespace
checks, check_world_shapes/3, check_single_arity_per_name/2, throw_text_door_error/2,
debug helpers): classify `adapt` (pipeline) or `drop` (DL6 surface refusals tied to
`<-`/`<+` term spellings). `throw_text_door_error/2` is `adapt` — it maps
unsupported constructs to `at(File, Line, Reason)` via `parse_dl_line_for_reason/2`,
a token/CST coupling that disappears under the DL7 frontend but the
`unsupported_construct/1` vocabulary is oracle-grade (shared with the sweep).

## 3. Report blocks — 0_storage_projection.pl

```prolog
% File: v6/prolog/compile/0_storage_projection.pl:24
% Existing comment: "! derive_storage_rows(+Decls, +RelPlans, -Rows) is det.
%   Canonical owners receive physical relation, column, and key rows..."
% Signature: derive_storage_rows(+Decls, +RelPlans, -Rows)
% Called by: compile.pl:program_plan (step canonical_storage_rows)
% Calls: semantic_rows, canonical_storage_owner, storage_plan_row,
%        validate_storage_rows
% Tests: v6/prolog/compile/test/0_storage_projection.test.pl
% V7 class: extract
% Parser coupling: none
% Preserved law: canonical semantic owners receive storage_relation/3,
%   storage_column/2, storage_key/2 rows; a rel/5 without a canonical
%   declaration stays a compatibility-only IDB; conflicts throw named
%   unsupported constructs.
% DL7 seam: in = Decls (with semantic_type_rows/1) + rel/5 list; out =
%   list(storage_relation|storage_column|storage_key) rows, sorted, validated.
```

```prolog
% File: v6/prolog/compile/0_storage_projection.pl:266
% Existing comment: "! project_storage_relplans(+Decls, +RelPlans0, -RelPlans) is det."
% Signature: project_storage_relplans(+Decls, +RelPlans0, -RelPlans)
% Called by: compile.pl:program_plan
% Calls: semantic_rows, storage_rows_from_decls, project_storage_relplan,
%        rel_cols/4 (0_rel_record)
% Tests: compile/test/0_storage_projection.test.pl
% V7 class: extract
% Parser coupling: none
% Preserved law: plan order preserved; canonical relations rebuild physical
%   fields from semantic+storage rows; undeclared IDBs keep their plan.
% DL7 seam: rel/5 in, rel/5 out (Name/Arity, StorageName, Kind, Cols, KeyOrNone).
```

```prolog
% File: v6/prolog/compile/0_storage_projection.pl:42-163 (canonical_storage_owner/4
%   and fallbacks, canonical_storage_type/4, canonical_reference_target/4,
%   semantic_type_ref_target/2, semantic_list_element/2)
% Existing comment: none (module header only)
% Signature: canonical_storage_owner(+Decls, +Rows, +Name, +Arity, -Owner)
% Called by: derive_storage_rows, project_storage_relplan, canonical_reference_target
% Calls: rel_module_decl, declaration/5 member rows
% Tests: 0_storage_projection.test.pl
% V7 class: extract
% Parser coupling: none
% Preserved law: one canonical owner per (name, arity); ambiguous owners throw
%   canonical_storage_owner_conflict, never a silent pick.
% DL7 seam: owner/name/target/ordinal edge representation under DL7 replaces
%   named(Hash, relation, Name) ids — adapt if DL7 changes id spelling, but the
%   unique-owner law is the oracle.
```

```prolog
% File: v6/prolog/compile/0_storage_projection.pl:166-253 (validate_storage_rows/2
%   and the validate/require family)
% Existing comment: none
% Signature: validate_storage_rows(+SemanticRows, +Rows)
% Called by: derive_storage_rows
% Calls: keysort/group_pairs_by_key, memberchk over semantic rows
% Tests: 0_storage_projection.test.pl
% V7 class: oracle
% Parser coupling: none
% Preserved law: uniqueness of storage_relation/storage_column keys, owner and
%   member existence, key membership, and type-target resolution are each a named
%   refusal, not a silent drop.
% DL7 seam: same rows in, throw or unit.
```

```prolog
% File: v6/prolog/compile/0_storage_projection.pl:254-261 (replace_storage_type_rows/3,
%   storage_rows_from_decls/2, is_storage_type_rows/1)
% Existing comment: none
% Signature: replace_storage_type_rows(+Decls0, +Rows, -Decls)
% Called by: compile.pl:program_plan, project_storage_relplans (reader)
% Calls: exclude/4, append/3
% Tests: compile/test/0_storage_projection.test.pl
% V7 class: extract
% Parser coupling: none
% Preserved law: exactly one storage_type_rows/1 decl per plan; the plan decl
%   list is the single transport into the emitters.
% DL7 seam: storage_type_rows(Rows) stays a decl term in the plan program.
```

## 4. Report blocks — registry.pl

```prolog
% File: v6/prolog/compile/registry.pl:13 (whole file)
% Existing comment: "registry.pl: the compiler's surface construct inventory."
% Signature: surface/5, expression/5, arrival_executor/2, host_input_contract/3,
%            host_output_contract/3, scip_namespace_host/3, clock_role/4,
%            trace_event/2, cli_command/3, http_route/3, + projections
%            (surface_for_term/6, body_surface_for_term/6, wrapper_lower_role/3,
%            expression_for_term/5, host_input_roles/3, host_execution/3)
% Called by: analyze (supported-subset gate), lower, parse_dl, print_dl,
%   1_emit_registry_docs (SYNTAX.md), 2_emit_cli_inventory, 3_emit_trace_schema,
%   4_emit_jsonschema, 5_emit_openapi, emit_ts (host_execution), emit_rust
%   (host_execution), hosts.rs LINKED_EXECUTORS (grep-pinned equal)
% Tests: tests/bopCommandInventory.test.ts (TS verb inventory agrees),
%   hosts.rs test pins arrival_executor rows equal, plunit_tests.pl
% V7 class: oracle
% Parser coupling: surface-policy (rows name term functors; the JSON-axis rows
%   are punctuation shapes)
% Preserved law: one inventory; the parser/printer/analyzer/gate/docs all
%   project from these rows; a surface growing without a row grows in silence.
% DL7 seam: rows are data; DL7 keeps the table, changes only the term-shape
%   column where cons-tree syntax replaces operator syntax (`:=`, `<-` spellings).
```

`cli_command/7 rows`, `http_route/3`, `trace_event/2` are the byte-comparison
contracts (trace_event fields carry stability `stable|timing|host`; a second
emitter must reproduce the `stable` set exactly). These are `oracle` class.

## 5. Report blocks — emit_ts.pl

```prolog
% File: v6/prolog/emit_ts.pl:2644
% Existing comment: none (top-level entry; module header documents the seam)
% Signature: emit_program(+Name, +Plan, +Lowered, +BootStatements, -Text)
% Called by: compile.pl:compile_fixture/3,5 default; compile_program_phases_moded seam call
% Calls: ~80 section builders (header/imports/local_types/world_plan/ddl/
%   rel_columns/rel_physical_names/rel_column_types/rel_stored_column_types/
%   rel_catalog/rel_declared_column_types/arrival_targets/boot/snapshot/
%   final_select/arrivals/incremental_*/ordered_*/recompute_levels/build_deltas/
%   advance_tick/run_ordered_tick/run_incremental_tick/subscribe_prune/
%   incremental_plan/program_export), plus lower:struct_type_plans,
%   program_text_intern_plan, catalog_all_rows; analyze:body_ref_uses,
%   derived_refs, rule_head_ref, program_uses_tick, listened_departure_refs,
%   level_body_pre_ref, rel_rule_observers_map; strat:recursive_stratum_groups,
%   cyclic_head_groups; 1_host_expand:compile_host_decl, compile_query,
%   host_plan_contract; compile/registry:host_execution
% Tests: v6/prolog/compile/test/dl6c.test.pl (target_emitter/3), golden byte
%   parity in v6/tsv2 run-emitted / sweep harness; conformance sweep byte-diffs
%   tick logs
% V7 class: adapt
% Parser coupling: term-shape (plan/9, lowered/8, relplan/5, deltastmt/5,
%   edgestmt/9, levelstmt/7, arrivalstmt/6, bootstmt/3 destructure) — no source
%   tokens; reads POST-expansion Rules for pre/negation/tick scans
% Preserved law: emitted TS module is a pure function of (Name, Plan, Lowered,
%   BootStatements); byte-stability modes: absent-field-not-null (§15.4), real
%   newline in SQL joins, pipe() 9-operator split, per_rel byte-identical fields.
% DL7 seam: same 5-arg seam over the DL7 plan; the emitted IGenProgramWithBoot
%   field list is the runtime contract (extend by adding fields, never rename).
```

```prolog
% File: v6/prolog/emit_ts.pl:2581-2605
% Existing comment: per-predicate comments (carry_pending law, retraction guard)
% Signature: reconcile_every_tick/2, derived_edge_carry_required/3, retraction_guard/2,
%   plan_pre_refs/2, self_referential_level_refs/2, recursive_level_refs/2,
%   plan_intern_mode/2, derived edge carry expr
% Called by: emit_program/5 (also exported for callers: export of the module seam)
% Calls: body_ref_uses, rule_head_ref, recursive_stratum_groups, listened_departure_refs
% Tests: dl6c.test.pl; shared_frontier.test.pl
% V7 class: extract
% Parser coupling: term-shape (Rule = (_ <- Body), use(Ref, _, neg, _) terms)
% Preserved law: three program-shape booleans (reconcile, derived-edge-carry,
%   retraction guard) are pure functions of the plan's Rules.
% DL7 seam: input = plan/9's Rules in DL7 term shape; output booleans unchanged.
```

```prolog
% File: v6/prolog/emit_ts.pl:1040-1084 (gate_column_type/2, boundary_column_type/2,
%   stored_column_type/2)
% Existing comment: ruling type_gate_widening; json boundary classification
%   (the 15-of-23 value-kinds comment); F3 list type.
% Signature: boundary_column_type(+ColumnType, -BoundaryName)
% Called by: rel_column_types_lines, rel_stored_column_types_lines,
%   incremental_relation_entry_line, enum_variant_plan
% Tests: conformance fixtures (json columns); type_relation_ir.test.pl
% V7 class: oracle
% Parser coupling: none
% Preserved law: the five boundary words (ref, relation_id, json, list, bytes)
%   plus pass-through primitives are the driver-seam vocabulary; `list(T)` is
%   stored as int (interned id) and `json` keeps its own name.
% DL7 seam: mapping table moves verbatim; it is what both runtimes read.
```

```prolog
% File: v6/prolog/emit_ts.pl:763-902 (bind_args_helper_lines/1, arrival_value_guard_lines/1,
%   trigger_occurrences_helper_lines/1)
% Existing comment: extensive (libsql bigint binding; type gate ruling
%   arrival_gate_widening; PHASE C2 RULING 2 dedup semantics)
% Signature: lines-producing constants (no arguments)
% Called by: emit_program
% Calls: none (static text)
% Tests: golden byte-diff via conformance sweep
% V7 class: oracle
% Parser coupling: none
% Preserved law: the arrival type gate mirrors 0_type_plane.pl:world_row_shape_violation/3
%   error names; wide-integer scan is decl-independent; JS cannot distinguish
%   1e20 int from float — declaring the column is the only fix.
% DL7 seam: static helper text; carried as-is.
```

The emitted-pipeline sections (ordered occurrence loop, recompute_levels
fixpoint with round-cap, build_deltas/carry_pending, subscribe prune, tick
pipe splitting) are each a pure function of Plan+Lowered fields; individually
`extract`, collectively `adapt` (they destructure lower.pl statement terms).

## 6. Report blocks — emit_rust.pl

```prolog
% File: v6/prolog/emit_rust.pl:598
% Existing comment: "emit_program/5 is substitutable for emit_ts:emit_program/5
%   with no call-site special case: same lowered/8 destructure, same output contract."
% Signature: emit_program(+Name, +Plan, +Lowered, +BootStatements, -Text)
% Called by: dl6c.test.pl target_emitter(rust), compile seam
% Calls: same lower/analyze/strat/registry modules as emit_ts; library(json)
%   json_write_dict for the ProgramJson document; raw_string_hashes for the
%   r#"..."# raw-string fence
% Tests: v6/prolog/compile/test/emit_rust.test.pl (same lowered/8 destructure
%   assertion); v6/sprefa-engine-rs deserialization (program.rs) is the consumer
% V7 class: oracle
% Parser coupling: term-shape (same statement terms as emit_ts)
% Preserved law: compiler emits JSON, runtime parses it; ProgramJson field set
%   (section 1 table) is what sprefa-engine-rs deserializes — keep every field
%   and ir_version(1) or the runtime refuses.
% DL7 seam: in = DL7 plan + lowered; out = one Rust file wrapping a JSON string.
%   Only the tick log is byte-diffed; JSON whitespace irrelevant.
```

`edge_dict`/`level_dict`/`relation_dict` add Rust-only fields the TS emitter
spells as text (`schedule`, `occurrence_project_sql`, `recompute_delete_sql`,
`recompute_insert_sqls`, `head_column_types`, `incremental_safe: true` constant).
Those key spellings are engine-rs's deserialization contract: `oracle`.

## 7. Report blocks — numbered emitter files and companions

```prolog
% File: v6/prolog/compile/1_emit_registry_docs.pl:7
% Existing comment: header (SYNTAX.md table + tmLanguage keyword projection)
% Signature: emit_registry_docs exports (SYNTAX table generator over surface/5)
% Called by: docs generation scripts; registry.pl comment cross-references
% Calls: registry:surface/5, expression/5; registry_word_regex/2 exclusion of
%   the punctuation axis
% Tests: registry doc goldens
% V7 class: adapt
% Parser coupling: surface-policy (projects the registry, not source)
% Preserved law: the generated syntax table is the registry's projection; a
%   surface without a row is invisible to docs and to the coverage gate.
% DL7 seam: same rows, DL7 term spellings in the printed column.
```

```prolog
% File: v6/prolog/compile/2_emit_cli_inventory.pl:7 and 3_emit_trace_schema.pl:7
% Existing comment: module headers
% Signature: emit_cli_inventory, emit_trace_schema
% Called by: docs/tooling scripts
% Calls: registry:cli_command/3, trace_event/2
% Tests: tests/bopCommandInventory.test.ts (grep cross-check), traceSchema.test.ts
% V7 class: extract
% Parser coupling: none
% Preserved law: one JSON doc emitted per table; byte-comparison against goldens.
% DL7 seam: unchanged (registry rows in, JSON out).
```

```prolog
% File: v6/prolog/compile/4_emit_jsonschema.pl:1 and 5_emit_openapi.pl:1
% Existing comment: module headers
% Signature: jsonschema_text/3, jsonschema_document/3, option_rows/3, openapi_text/3
% Called by: plunit_tests.pl:103-104, typegen_export.pl
% Calls: type plane (type_definitions), 0_rel_record
% Tests: compile/test/emit/schema/*.schema.json goldens, emit/openapi/*.openapi.json
% V7 class: extract
% Parser coupling: none (consumes type plane rows / rel plans)
% Preserved law: json_list columns emit array items; struct columns render
%   canonical json (golden names are the law statements).
% DL7 seam: type rows in, JSON Schema/OpenAPI text out.
```

```prolog
% File: v6/prolog/compile/7_emit_ts_types.pl:1 and 8_emit_rust_types.pl:1
% Existing comment: module headers; emit_type_renderers.test.pl covers both
% Signature: ts_types_text/3, emit_ts_types/3; rust_types_text/3, emit_rust_types/3
% Called by: typegen_export.pl, compile/test/emit_type_renderers.test.pl
% Calls: type plane, renderer tables shared with the plan emitters
% Tests: compile/test/emit_type_renderers.test.pl; type_relation_ir.test.pl
% V7 class: extract
% Parser coupling: none
% Preserved law: the TS and Rust type artifacts are pure renderings of the type
%   plane; anonymous products/sums and braced nested relations have pinned
%   renderer tests.
% DL7 seam: same input rows, same renderers, different spelling files.
```

```prolog
% File: v6/prolog/compile/9_emit_type_artifact.pl:1
% Existing comment: module header
% Signature: emit_type_artifact/3
% Called by: type artifact build step
% Calls: 7_emit_ts_types, 8_emit_rust_types, jsonschema
% Tests: emit_type_renderers.test.pl
% V7 class: extract
% Parser coupling: none
% Preserved law: one invocation writes the full type artifact set.
% DL7 seam: unchanged.
```

```prolog
% File: v6/prolog/compile/6_isolated_compiler_dd.pl:2
% Existing comment: "Emit a deterministic target-neutral DD plan term." + the
%   seam comment (Initial/Schedule read out of band via dd_compile_context/2)
% Signature: compile_program/5 (seam-shape), dd_plan_text/2, fixture_dd_plan_*_text/3
% Called by: text door (emitter option), dd panel export
% Calls: program_plan, lower_program, dd_plan_term, json_write_dict
% Tests: conformance dd plan goldens (dd_panel.json)
% V7 class: oracle
% Parser coupling: term-shape
% Preserved law: the dd plan is the target-neutral projection of plan+lowered;
%   boot statements ignored (embeds own initial rows); Initial/Schedule come
%   from dd_compile_context when absent from the seam.
% DL7 seam: same seam shape; the context thread-local should fold into the seam
%   term under DL7 (see adapt note in section 2).
```

```prolog
% File: v6/prolog/compile/oracle_dump.pl:1
% Existing comment: file header (Phase C oracle side, no module header on purpose)
% Signature: dump_all/0, dump_entry/5 helpers, dynamic oracle_dump_dir_fact/1, oracle_root_fact/1
% Called by: sweep harness (`swipl -l oracle_dump.pl -g dump_all`)
% Calls: conformance/ticklog fixture/5, run_program/5, print_ticklog/3
% Tests: v6/prolog/compile/test/emit/PARITY.golden.md consumers
% V7 class: oracle
% Parser coupling: term-shape (reads fixture/5 terms)
% Preserved law: ORACLE_THROW on engine rejection paths; digests pin tick-log bytes.
% DL7 seam: drop with the DL6 fixture format; the digest scheme is the oracle to keep.
```

```prolog
% File: v6/prolog/compile/0_trace.pl (234 lines) and debug_dbg.pl (5 lines)
% Existing comment: module header (step trace / measurement plumbing)
% Signature: run_compile_step/4, record_step/3, write_step_trace/2, capture_phase_measurement/2, statistics_snapshot/1
% Called by: compile.pl check_step/run_compile_phase/write_compile_trace
% Tests: compile/test/dl6c.test.pl (COMPILE-TRACE line shape)
% V7 class: extract
% Parser coupling: none
% Preserved law: measurement/12 is a fixed record; every count walks a list
%   under its own debug topic.
% DL7 seam: move as-is.
```

## Counts by class

Report blocks above cover 34 predicate groups (one block may name a tight clause
set; each named predicate counted).

| class | count |
|---|---|
| extract | 12 (storage projection internals, doc/schema emitters, trace helpers, type renderers, storage-name machinery, pipeline phases) |
| adapt | 13 (program_plan, compile_program_phases, emit_program/5 both doors, emitter/schedule options, registry projections, isolated_compiler_dd seam, throw_text_door_error, reserved-namespace checks, debug wrappers) |
| oracle | 7 (registry tables as inventories, trace_event wire schema, boundary type map, arrival gate helpers, ProgramJson field set, dd plan determinism, oracle dump digests) |
| drop | 5 (dl6_seeded_form/3, partition_dl6_facts/4, dl6_fact/2, dotted_relation_head/1, compiler_type_fact/2 — DL6 `<-` fact partitioning and fixture reader surface) |

## Canonical term shapes

In (from plan/lower, consumed by emitters):

- `plan(Name, prog(Decls, Rules), Types, RelPlans, ArrivalTargets, RuleOrder, EdgeRules, SubscribedRels, InternMode)`
- `rel(Name/Arity, StorageName, Kind(log|set), Cols, KeyOrNone)` with `Cols` from `rel_cols/4`
- `lowered(Name, Ddl, ArrivalStatements, EdgeStatements, LevelStatements, DeltaStatements, RelPlans, ArrivalTargets)`; statements: `arrivalstmt(Ref, Kind, AddSql, DelSql, _, _)`, `edgestmt(HeadRef, TriggerRef, HeadCols, KeyCols, ProjectSql, WriteSql, DeltaProjectSql, TriggerKind, edgeinterns(ProjectInternSqls, DeltaInternSqls))`, `levelstmt(HeadRef, DeleteSql, InsertSqls, DeltaInsertSql, RefCountSql, AggregateSql, DeltaInternSqls)` with nested `refcountsql/16`, `expandplan/8`, `dredplan/24`, `fixpointir/5`, `aggsql/7`, `avgsql/7`, `supportcount/2`, `deltastmt(Ref, SelectSql, DeltaTable, BoundarySql, StoredSelectSql)`, `retentionstmt(Ref, Limit, DeleteSql)`
- `bootstmt(Rel, Sql, Params)`, `bootstmt` params may be `bool_lit/1`
- Decls atoms read by emitters: `sh_decl/4`, `bind_decl/2`, `enum_column/3`, `option_column/3`, `enum_option_payload/4`, `type_decl/2`, `col_type/3`, `query/1`, `storage_type_rows/1`, `rel_module_decl/2`, `module_storage_decl/2`, `entry_module_decl/1`, `semantic_type_rows/1`
- Out-of-band: `dd_compile_context(Initial, Schedule)` thread-local

Out: emit_ts produces a TS module text (sections listed in emit_program/2766-2783); emit_rust produces one Rust file wrapping the `ProgramJson` dict (fields in section 1). The five field names both runtimes refuse on mismatch: `ir_version`, plus the `IGenProgram` five pinned names and the `boot` extension field.

## Hidden state and control dependencies

- `dd_compile_context/2` — thread_local in compile.pl:35; asserted around the seam call, retracted after; read by isolated_compiler_dd only.
- `frontier_mode_option/1` — thread_local in lower.pl:233; `frontier_mode(shared)` consulted by BOTH emitters' `shared_frontier_field`; per_rel is byte-preservation mode.
- `frontier_mode_option/1` is set by `with_frontier_mode/2` around the whole compile; emitters read it mid-emit (ambient, not threaded) — an emitter invoked outside that scope silently gets per_rel.
- `dl6_debug/3` topics (`plan`, `lower`, `boot`, `emit`, `write`) — message-level only, no output coupling.
- `oracle_dump.pl` — dynamic `oracle_dump_dir_fact/1`, `oracle_root_fact/1` asserted at load time via `prolog_load_context/2`; loads into `user` context alongside ticklog.pl.
- Cuts: `canonical_storage_owner` chain uses once-style cuts to commit to the unique owner; `canonical_storage_owner_result` throws on ambiguity. emit_ts uses `!` in type-mapping chains (gate/boundary/stored column types) — order-sensitive fallthrough.
- No tabling in the slice. Assertion order matters in `oracle_dump.pl` only (load-time dynamic facts).
- `:- op(1150, xfx, <-)`, `<+`, `:=` re-declared in compile.pl, emit_ts.pl, emit_rust.pl, isolated_compiler_dd.pl — needed to parse rule terms crossing module boundaries.
- `ir_version(1)` duplicated in emit_ts.pl:38 and emit_rust.pl:40 by comment-pinned convention, with no shared definition — a drift hazard named in both headers.

## Smallest extraction boundary

The seam term is already the boundary: `emit_program(Name, Plan, Lowered, BootStatements, Text)` with `plan/9` + `lowered/8` + `bootstmt/3`. A V7 emitter needs exactly: plan/9 (9 fields), the lowered/8 statement terms, the BootStatements list, and `dd_compile_context`-equivalent Initial/Schedule (which should join the seam). Everything else (registry, storage names, type plane) must be inside the boundary or re-produced as plan fields; both current emitters reach back into `analyze`, `strat`, `lower`, `1_host_expand`, and `compile/registry` during emit, so the extraction unit is plan+lower+emit+those four read-only modules.

## First dependency forcing adaptation

`emit_ts.pl:incremental_relation_entry_line/5` (emit_ts.pl:1283) calls
`relplan_storage_name(RelPlans, Ref, StorageName)` and destructures
`relplan_parts/6` mid-render. The rel/5 record is produced by
`project_storage_relplans/3`, which rebuilds physical fields from
`storage_type_rows/1` + semantic rows — the canonical-owner lookup
(`named(Hash, relation, Name)` ids). Under DL7's owner/name/target/ordinal
edge representation, the `named/3` id spelling changes, so every
`relplan_*` consumer in both emitters must adapt even though the law
(unique canonical owner, plan-order preservation) is preserved. This is the
first forced adaptation: no emitter can be lifted out without restating the
rel/5 record in DL7 id vocabulary.

## Unresolved questions requiring a V7 language ruling

1. `ir_version(1)` and the `IGenProgram` field list are pinned by both runtimes ("extend by adding fields, never renaming"). Does the DL7 plan revision bump `ir_version` or ship a second version namespace per frontend?
2. `incremental_safe: true` is a constant kept only because engine-rs deserializes it. Is the V7 contract allowed to delete dead fields from ProgramJson, or does sprefa-engine-rs stay byte-frozen?
3. `arrival_targets` excludes the catalog (compile.pl:263-267: compiler-owned `__` rels are never arrival targets). Is the compiler-owned `__` namespace spelling a V7 law to preserve verbatim, including `__enum_identity_*`, `__frontier_*`, `__pre_*`, `__delta_*` table spellings the emitters hardcode?
4. The JS driver seam facts (bigint binding at emit_ts.pl:733-748, the `bind_args` non-readonly array note) are TS-runtime policy embedded in the compiler. V7 "compiler may revise the compiler-side IR" — do driver-specific helpers stay compiler-owned or move to the runtime side?
5. `subscribed_rels` is computed in program_plan and read by nothing but emission ("nothing else reads it"). Keep it in the plan/9 shape or drop the field in DL7?
6. `default_intern_mode(dict)` is a build default with a recorded referee NO on flipping. Does V7 keep the two intern modes and the `internMode` field, or does DL7 pick one?
