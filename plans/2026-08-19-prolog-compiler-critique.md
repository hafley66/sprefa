# v6 prolog compiler: critique of the type system, the IRs, and lowering

Every claim carries a `file:line`. Every quantity comes from a command named
beside it. Companion: `plans/2026-08-19-prolog-compiler-anatomy.md`.

## TOC

1. [Method and rig](#method-and-rig)
2. [Gate state at this HEAD](#gate-state-at-this-head)
3. [The type system](#the-type-system)
   1. [Where types live: four representations](#where-types-live-four-representations)
   2. [What is checked, what is trusted](#what-is-checked-what-is-trusted)
   3. [Recomputed instead of threaded](#recomputed-instead-of-threaded)
   4. [Known holes](#known-holes)
4. [The IRs](#the-irs)
   1. [Data smuggled past the argument list](#data-smuggled-past-the-argument-list)
   2. [Positional terms past six arguments](#positional-terms-past-six-arguments)
   3. [The same fact derived twice](#the-same-fact-derived-twice)
   4. [A phase reparsing another phase's output](#a-phase-reparsing-another-phases-output)
   5. [Documented arities that are wrong](#documented-arities-that-are-wrong)
5. [Lowering: mapping lower.pl](#lowering-mapping-lowerpl)
   1. [Section map](#section-map)
   2. [The seams](#the-seams)
   3. [Clause counts per exported predicate](#clause-counts-per-exported-predicate)
   4. [Copy-paste families: the transient DDL mints](#copy-paste-families-the-transient-ddl-mints)
   5. [Cross-reference: the shared-frontier lab](#cross-reference-the-shared-frontier-lab)
6. [Defect ledger](#defect-ledger)
7. [Design forks needing Chris](#design-forks-needing-chris)

## Method and rig

Worktree `sprefa-worktrees/prolog-compiler-anatomy`, HEAD `4e2c21a82`, swipl
10.0.2 arm64-darwin, Apple M2 Pro. Clause counts from an awk head-scanner over
column-0 clause heads. Term-site counts from a Python balanced-paren scanner
that strips whole-line comments and matches functor name plus exact arity
across line breaks. Profiles from `library(prolog_profile)` over
`compile_dl6('v6/dl/fixtures/self-map.dl6', ...)`.

## Gate state at this HEAD

Measured here, not read from a file:

| gate | command | result | wall |
| --- | --- | --- | --- |
| conformance | `cd v6/prolog/conformance && swipl -q -l go.pl -g go -g halt` | 461 PASS, 1 fail (`nested_zero_column_child_is_one_row_per_parent`) | 4.2 s |
| ARCH | `swipl -q -l v6/prolog/ARCH.pl -g go -g halt` | all PASS | 0.03 s |
| plunit | `cd v6/prolog/compile && swipl -q -l test/plunit_tests.pl -g run_tests -g halt` | 923 tests, 7 failed | 35.6 s |
| text door, `golden-flex.dl6` | `swipl -q -l v6/prolog/compile.pl -g "compile_dl6('v6/dl/fixtures/golden-flex.dl6', ...)"` | `unsupported_construct(column_type_unknown('CodecDocument'))` | |

All four match `.github/CI-KNOWN-RED.md` (measured 2026-08-19): group A for the
conformance failure, the 7-test plunit set, group C for golden-flex at
`0_type_plane.pl:151`. Nothing new is red.

## The type system

### Where types live: four representations

A column's type is spelled four different ways at four different points, and no
two are the same data structure.

| # | representation | shape | authoritative for | site |
| --- | --- | --- | --- | --- |
| 1 | declaration | `col_type(Name/Arity, Column, Type)` inside the flat `Decls` list | the author's spelling | parser, `parse_dl_dcg.pl` |
| 2 | type table | `type_def(Name, Columns, ColumnTypes)` list | rel-as-type lookups | `0_type_plane.pl:67` |
| 3 | storage kind | `int`/`text`/`bool`/`float`/`json`/`bytes`/`json_list(T)`/`list(T)`/`ref(N)`/`idref(N)` | DDL and SQL rendering | `0_type_plane:column_storage/3`, `0_type_plane.pl:120-151` |
| 4 | catalog row | `row/11` = `row(Id, Parent, Ordinal, Name, Kind, TypeId, Arity, ModuleId, HId, HSchema, HRule)` | emitted `.types.ts`, `.types.rs`, `.schema.json` | `lower:catalog_type_rows/6`, contract at `lower.pl:963-967` |

Representation 4 is literally the `__rel` catalog table's column list
(`catalog_ddl_contract('__rel', ...)`, `lower.pl:963`), so the type IR the
typegen emitters read IS the runtime catalog's row shape. That coupling is not
stated anywhere in the emitters; `compile/7_emit_ts_types.pl` matches `row/11`
22 times positionally with no accessor and no contract comment.

A fifth carrier exists but is a passenger inside the `Decls` list rather than a
slot: `semantic_type_rows(Rows)`, one term appended to `Decls` at
`0_enum_expand.pl:96-116` and `0_anonymous_expand.pl:307-309`, holding the same
`row/11` terms. It is read back by `member/2` at `0_enum_expand.pl:156` and
`0_anonymous_expand.pl:291`.

The wrapper set is `type_wrapper/2`, five rows, `0_type_plane.pl:167-171`:

```
option -> endpoint
list -> value
list_entity_dense_sequence -> value
list_interned_set -> value
list_entity_linked_sequence -> value
```

`unwrapped_column_type/2` (`0_type_plane.pl:176-181`) walks the nest;
`column_element_type_name/2` (`0_type_plane.pl:185-195`) recovers the name in
column position. Both are pure and both are correct as far as the five-row
table goes. The problem is above them: `json_list(T)` is NOT in the wrapper
table even though it is a parametric type (`0_type_plane.pl:127`), and neither
is a generic application like `CodecBox(CodecDocument)`. Those get their own
clause in `column_storage/3` or fall through to the throw at
`0_type_plane.pl:151`.

### What is checked, what is trusted

| plane | checked | trusted |
| --- | --- | --- |
| declaration syntax | 38 named classes, `program_violation/3`, `0_program_check.pl` | |
| column type resolution | `column_storage/3` throws `column_type_unknown/1` (`0_type_plane.pl:151`) and `list_of_relation_refs/1`, `list_element_not_scalar/1` (`:130-132`) | any type reaching `column_def/4` is assumed already resolved |
| join column agreement | `join_column_type_mismatch/4`, `lower.pl:383` | |
| comparison operands | `comparison_type_mismatch/3`, `lower.pl:2458` | |
| edge head columns | `check_edge_head_column_types/2`, `compile.pl:245` | |
| arrival row shape | `world_row_shape_violation/3`, `compile.pl:330` | |
| clocks | `check_clock_program/1`, `compile.pl:191` | |
| the `plan/9` term | nothing | every one of its 29 destructure sites |
| the `lowered/8` term | nothing | every one of its 10 destructure sites |
| every statement functor | nothing | `arrivalstmt/6`, `edgestmt/9`, `levelstmt/7`, `deltastmt/5`, `retentionstmt/3`, `bootstmt/2` and `/3` |
| `row/11` | nothing | 34 sites in jsonschema, 29 in rust types, 22 in ts types, 9 in typegen export |

CLAUDE.md's "no coercions" decision cites `lower.pl:2319` and `lower.pl:347`.
Both numbers have rotted: the live sites are `lower.pl:2458`
(`comparison_type_mismatch`) and `lower.pl:383` (`join_column_type_mismatch`).
CLAUDE.md's own opening rule is "No number lives in this file"; those two lines
are the exception it left in, and they are now wrong.

### Recomputed instead of threaded

`type_definitions/2` is a full `findall/3` over the `Decls` list. It is called
from 20 sites (`grep -rn "type_definitions(" --include=*.pl`, excluding tests
and fixtures), eleven of them in `0_program_check.pl` alone:

```
0_program_check.pl:226,332,344,370,425,476,521,533,731,800  (10)
0_type_plane.pl:444,453,575                                 (3, self-calls)
0_dot_expand.pl:624,631                                     (2)
compile.pl:186                                              (1, the threaded one)
0_relation_edge_expand.pl:31, 0_relation_pattern.pl:33, analyze.pl:1654  (3)
```

`compile.pl:186` computes `Types` once and the `plan/9` term carries it in slot
3 exactly so consumers stop re-deriving. The ten `0_program_check.pl` calls all
run inside `check_supported_subset_expanded/1`, which `compile.pl:190` calls
FOUR LINES AFTER `compile.pl:186` built the table, and they rebuild it each
time because `program_violation/3`'s signature takes `prog/2`, not `Types`.
Profiled on `self-map.dl6`: 12 calls, over a 501-element `Decls` list.

The wider version of the same shape: 246 `member/2` or `memberchk/2` calls take
a raw `Decls` list as their second argument
(`grep -rnE "member(chk)?\([^;]*, ?Decls[0-9]?\)"`, tests excluded):

| file | sites | file | sites |
| --- | --- | --- | --- |
| `0_generic_expand.pl` | 73 | `0_type_plane.pl` | 8 |
| `0_dot_expand.pl` | 20 | `lower.pl` | 7 |
| `0_program_check.pl` | 17 | `emit_rust.pl` | 7 |
| `compile/parse_dl_dcg.pl` | 16 | `use_resolve.pl` | 6 |
| `0_enum_expand.pl` | 16 | `print_dl.pl` | 6 |
| `0_option_expand.pl` | 12 | `analyze.pl` | 5 |
| `emit_ts.pl` | 9 | `1_host_expand.pl` | 4 |
| `compile.pl` | 9 | `0_seq_expand.pl` | 4 |
| `0_anonymous_expand.pl` | 9 | 6 more files | 12 |
| `compile/4_emit_jsonschema.pl` | 8 | **total** | **246** |

`Decls` has no index. Commit `4e2c21a82` (this HEAD) fixed exactly this class in
five places (`0_dot_expand:declared_flat_names/2`,
`0_generic_expand:plain_relation_specs/3`, `first_member_row_per_id/2` moved to
`library(assoc)`, and two `member` to `memberchk` promotions), taking pokeapi
compile from 526.4 s to 0.9 s (`v6/labs/shared_frontier/REPORT.md` F10 against
the trace measured here). 241 sites of the same shape remain.

Profile of one `self-map.dl6` compile, call counts:

| calls | predicate | scale |
| --- | --- | --- |
| 409,572 | `rel_record:relplan_parts/6` | 117 relplans, 3,500 calls each |
| 132,176 | `system:memberchk/2` | |
| 130,290 | `lists:member/2` | 543,185 redos through `member_/3` |
| 117,366 | `rel_record:rel_cols/4` | |
| 90,872 | `analyze:body_ref_uses/2` | tabled |
| 88,660 | `analyze:rule_body/2` | 220 rules |
| 86,680 | `analyze:rule_head/2` | 220 rules |

`relplan_parts/6` and `rel_cols/4` are the `rel/5` accessors. Half a million
calls to re-destructure a 5-tuple that never changes during a compile.

### Known holes

| hole | throw site | what it means |
| --- | --- | --- |
| `column_type_unknown('CodecDocument')` | `0_type_plane.pl:151` | a rel name used as a template argument (`golden-flex.dl6:28`, `CodecUse(value: CodecBox(CodecDocument))`) does not resolve. `column_storage/3`'s clause chain has no case for a generic application; the fall-through clause throws. Named in `.github/CI-KNOWN-RED.md` group C, introduced by `69ea4a37c` |
| second `column_type_unknown` site | `lower.pl:3582` | the same reason name thrown from a second place, inside relation-value lowering, with a different resolution path |
| `type_name/2` is not injective | `compile/7_emit_ts_types.pl:290-294`, `compile/8_emit_rust_types.pl:760` | `atomic_list_concat` on `_` then capitalize: `foo_bar` and `foo__bar` and `fooBar` all reach `FooBar`. `collision_type_names/2` at `7_emit_ts_types.pl:166` DETECTS the collision after the fact and the repo decision (CLAUDE.md) is to resolve by module prefix, so this is a designed non-injectivity with a detector, not a silent bug. The detector is duplicated verbatim in the Rust twin |
| the wrapper table does not cover parametric types | `0_type_plane.pl:167-171` | `json_list(T)` (`:127`) and generic applications get separate clauses; the wrapper walk `unwrapped_column_type/2` cannot see them |
| `json_list(T)` element typing is unenforced | `0_type_plane.pl:118-125` comment | "the storage kind collapses to `json`, so neither guard is emitted". The array-ness CHECK needs `json_list(T)` to survive to `lower:column_def/4` |
| option cannot spell key-absent | `compile/4_emit_jsonschema.pl:121-146` | already recorded in CLAUDE.md under "Open, needing the user" |

## The IRs

### Data smuggled past the argument list

Three channels carry data around the phase signature.

| channel | file:line | what it carries | why |
| --- | --- | --- | --- |
| `physical_storage_name/2` (thread_local) | `lower.pl:197`, installed `:200-208` | `Ref -> physical table name`, one fact per relplan | stated at `lower.pl:192-196`: SQL helpers take a semantic `Ref` "far below the `RelPlans` argument that owns the map". Installed TWICE per compile, once by `lower_program/2` (`:6659`) and once by `boot_statements/7` (`:6755`) |
| `dd_compile_context/2` (thread_local) | `compile.pl:34`, asserted `:713` | `Initial` and `Schedule` for the dd emitter | stated at `compile.pl:29-33`: the emitter seam is `/5` and one emitter needs seven things |
| `body_ref_uses/2` (`:- table`) | `analyze.pl:109`, reset `:111` | the whole body-use analysis, process-global | reset from exactly one place, `compile.pl:174`. Any caller that reaches `lower_program/2` or an emitter without going through `program_plan/3` first reads a stale table |

The parser adds five `nb_setval` keys and four dynamic fact tables, all listed
in the anatomy doc's global state register. One of them is a live defect:
`source_statement_fact/3` is retracted at the start of EVERY file parse
(`parse_dl_dcg.pl:107-109`) and `use_resolve.pl:119` parses the entry file LAST,
so after a multi-file program parses, the table holds only the entry's
statements. `parse_dl_line_for_reason/2` (`parse_dl_dcg.pl:197,200`) is the sole
source of the `file:line` in `throw_text_door_error/2` (`compile.pl:645-655`).
An error inside an imported module cannot be located.

`emit_ts:emit_program/5` also re-destructures `Plan` four times in one clause
(`emit_ts.pl:2563, 2575, 2593, 2655`), each binding a different subset of the
nine positions, which is the argument-list problem expressed inside a single
predicate.

### Positional terms past six arguments

Terms:

| term | arity | destructure sites | accessor layer |
| --- | --- | --- | --- |
| `plan/9` | 9 | 29 across 12 files | none |
| `lowered/8` | 8 | 10 across 9 files | none |
| `edgestmt/9` | 9 | 19 | none |
| `levelstmt/7` | 7 | 22 | `emit_ts:level_statement_head_ref/2` only |
| `row/11` | 11 | 94 outside tests | none |
| `measurement/12` | 12 | 4 | `phase_trace_measurement_values/3`, `compile.pl:840` |
| `rel/5` | 5 | 5 raw | `0_rel_record.pl`, 16 exported accessors |
| `arrivalstmt/6` | 6 | 10 | none |
| `deltastmt/5` | 5 | 11 | none |

`plan/9`'s 29 sites, by file: `emit_ts.pl` 11, `lower.pl` 4, `emit_rust.pl` 3,
`compile.pl` 2, `compile/6_isolated_compiler_dd.pl` 2, and one each in
`6_profile.pl`, `sweep.pl`, `compile/9_emit_type_artifact.pl`,
`compile/typegen_export.pl`, `compile/scripts/metamorphic_rename.pl`,
`compile/scripts/arm_census.pl`, `compile/scripts/text_door_receipt.pl`.
Adding a tenth slot means touching twelve files, three of which are gate
scripts.

Predicates:

| file | predicates with arity >= 7 |
| --- | --- |
| `lower.pl` | 34, topped by `seeded_pre_args/10`, `room_rows/10`, `metadata_one_generic_columns/9`, `level_positive_delta_arms/9`, `json_member_sql/9`, `dred_seed_from_part/9`, `boot_rows_statements/9`, `boot_row_statements/9` |

`rel/5` is the only IR in the compiler with a real accessor layer, and it is the
only one whose raw destructure count is single digit. That is the working model
the other IRs do not follow.

### The same fact derived twice

| fact | derived at | derived again at | evidence |
| --- | --- | --- | --- |
| host plans | `1_host_expand.pl:47` inside `prepare_program/5` | `emit_ts.pl:485`, `emit_rust.pl:563` | `compile.pl:130` calls `prepare_program(SugaredProg, HostProg, _, _, _)` and throws away all three plan lists |
| query plans | `1_host_expand.pl:47` (`compile_query/2`) | `emit_ts.pl:499` | same discard |
| bind plans | `1_host_expand.pl:52` | `emit_ts.pl` bind config section (`:604`) | same discard |
| generic expansion | `1_expansion.pl:86`, run on `prog(SurfaceDecls, [])` purely to build `enum_context/2` | `1_expansion.pl:31` phase 5, run on the whole program | both call the same 14-step pipeline |
| the whole generic pipeline | `0_generic_expand.pl:50-67` `expand_generic_program_with_bindings/3` | `0_generic_expand.pl:70-87` `expand_generic_program_raw/2` | the 14 goal lines are identical modulo `Bindings` vs `[]`; only caller of the second is `compile/test/plunit_tests.pl:5113`. The comment at `:68-69` says the copy exists "so the template and replacement logic cannot drift apart", which is what one shared clause guarantees and two copies do not |
| relation-value rewrite | `0_relation_pattern.pl` (102 lines, 6 predicates), used ONLY by `conformance/engine.pl:79` | `lower.pl:3212-3651` (440 lines), `expand_relation_pattern_rules/4` at `:3294` | the oracle and the compiler each carry their own implementation of the same source-to-object rewrite |
| type table | `compile.pl:186` (threaded into `plan/9` slot 3) | 10 sites in `0_program_check.pl` | see above |
| `row/11` type rows | `compile/9_emit_type_artifact.pl:14` `type_rows/3` | run once per artifact; `v6/prolog/examples/_1_emit_types.sh:12,16,20` invokes `compile_dl6/3` three separate times to write `.ts`, `.rs` and `.schema.json` from the same rows | three full parse+plan+lower+boot runs for one type IR |
| `with_storage_context/2` | `lower.pl:6659` for `lower_program/2` | `lower.pl:6755` for `boot_statements/7` | the whole storage map asserted and retracted twice per compile |

### A phase reparsing another phase's output

| site | what happens |
| --- | --- |
| `compile.pl:250` | `findall(QueryAtom, member(query(QueryAtom), Decls), Queries)`. The parser returned `Queries` as its own slot in `program/3` (`parse_dl_dcg.pl:128`); `prepare_program/5` folded them into the flat `Decls` list at `1_host_expand.pl:61`; the plan phase scans `Decls` to get them back |
| `compile.pl:680` | `Lowered = lowered(_, _, _, _, LevelStatements, _, _, _)` in `compile_program_phases/8`, so the boot phase reads a field out of the lower phase's output term rather than receiving it |
| `emit_ts.pl:2575` | `recursive_level_refs(SelfRefScanRules, ...)` re-runs recursion analysis over `plan/9`'s `Rules`, after `strat.pl` already computed `cyclic_head_groups/2` in the plan phase |
| `emit_ts.pl:2593` | `listened_departure_refs(PlanRules, DepartureRefs)`, the same call `lower.pl:6672` already made on the same rules |
| `emit_ts.pl:2596-2600` | `findall(LevelRef, member((LevelHead <- _), PlanRules), ...)` re-derives level-headed refs; `analyze:level_headed_refs/2` exists and `lower.pl:6665-6668` computed the same list |
| `0_generic_expand.pl:737-745` | `plain_relation_specs/3` reconstructs a rel's column spec list from scattered `col_type/3` decls because no phase carries a per-rel index |

### Documented arities that are wrong

`lower.pl:1-23` is the module header and the single documentation site for the
`lowered/8` slot shapes. Three of five are stale:

| slot | header says | code builds | code site |
| --- | --- | --- | --- |
| `EdgeStatements` | `edgestmt/7` | `edgestmt/9` | `lower.pl:6685`, matched at `emit_ts.pl:1301` |
| `LevelStatements` | `levelstmt/5` | `levelstmt/7` | `lower.pl:6704`, matched at `emit_ts.pl:1350` |
| `DeltaStatements` | `deltastmt/4` | `deltastmt/5` | `lower.pl:6735`, matched at `emit_ts.pl:1091` |
| `ArrivalStatements` | `arrivalstmt/6` | `arrivalstmt/6` | correct |
| boot | `bootstmt(Rel, Sql, Params)` | `bootstmt/2` until `lower.pl:6779` retags it to `/3` | two arities of one functor in one file |

The same header describes `plan/6` (`lower.pl:1`) for a term that has been
`plan/9` since the intern mode landed.

## Lowering: mapping lower.pl

6,800 lines, 429 distinct predicates, 659 clauses, 37 exported. 392 private
predicates in one module. `awk` head-scanner over column-0 clause heads.

### Section map

27 banner comments, by line range and size:

| lines | count | section |
| --- | --- | --- |
| 190-260 | 71 | identifiers |
| 261-309 | 49 | rule identity |
| 310-412 | 103 | pattern-argument compiler |
| 413-511 | 99 | positive body-atom compilation |
| 512-540 | 29 | negative body-atom compilation |
| 541-930 | 390 | head expression compilation |
| 931-959 | 29 | guard / bind goals |
| **960-2582** | **1623** | **"step g1 SCAFFOLD: the program catalog"** |
| 2583-2619 | 37 | interning |
| 2620-2698 | 79 | text constants in the id space |
| 2699-2750 | 52 | the decode view |
| 2751-2789 | 39 | the ingest door's intern plan |
| 2790-2907 | 118 | DDL |
| 2908-3152 | 245 | relation reference projection |
| 3153-3211 | 59 | decode/2 as a dictionary join |
| 3212-3651 | 440 | relation-value terms as dictionary joins |
| 3652-3728 | 77 | arrival statement templates |
| 3729-4040 | 312 | edge rule lowering |
| 4041-4114 | 74 | level rule lowering |
| 4115-4820 | 706 | group-scoped aggregate maintenance |
| 4821-5085 | 265 | in-place recursive-head maintenance |
| 5086-5606 | 521 | backend-neutral fixpoint IR |
| 5607-5886 | 280 | decode/2 over a json column |
| 5887-6212 | 326 | aggregate heads |
| 6213-6533 | 321 | delta statements |
| 6534-6654 | 121 | boot |
| 6655-6800 | 146 | top level |

The 960-2582 banner is wrong by inspection. It holds 111 distinct predicates,
68 of which are named `catalog_*`; the other 43 are guards
(`compile_guard_goal/4`, `check_comparison_types/4`, `compile_comparison/4`,
`compile_regexp_goal/4`), interning (`intern_write_sql/4`,
`intern_write_arm/4`, `list_intern_statements/4`), list storage
(`list_row_id/3`, `list_type_depth/2`, `list_subtypes/2`), DDL
(`set_rel_table_ddl/5`, `set_rel_pk_sql/6`, `option_some_table/5`,
`acyclic_guard_ddl/3`), module paths (`path_scope_id/6`, `path_nest_map/6`,
`room_rows/10`), and 13 `metadata_*` predicates that build the `row/11` type IR.
24% of the file sits under one mislabelled banner.

Two sections that are not lowering at all: `5086-5606` "backend-neutral fixpoint
IR" (521 lines) constructs a second IR the emitters consume, and the 13
`metadata_*` predicates inside the catalog section construct the typegen row
IR. Both belong beside their consumers, not inside the SQL text builder.

### The seams

Four real seams exist and one is enforced.

| seam | enforced how | leak |
| --- | --- | --- |
| plan to lower | `lower_program/2` takes `plan/9` and nothing else | none |
| lower to emit | `emit_program/5` takes `Name, Plan, Lowered, BootStatements, Text` | `Plan` is passed too, so the emitters read `plan/9`'s `prog/2` directly and re-analyze the rules (`emit_ts.pl:2575, 2593, 2596`) |
| lower to boot | `boot_statements/7` | takes `Decls`, `Types`, `RelPlans` and `LevelStatements` separately, four of `plan/9`'s nine fields plus one of `lowered/8`'s eight, unpacked by the caller at `compile.pl:679-684` |
| emitter to type artifact | `compile/9_emit_type_artifact.pl` calls `lower:catalog_type_rows/6` | the type IR builder lives inside `lower.pl` and is exported (`lower.pl` module list) alongside `catalog_type_relation_rows/3`, `catalog_type_transport_rows/4`, `catalog_decl_rows/6`, `catalog_all_rows/10`, `catalog_rows/4` |
| storage naming | `physical_storage_name/2` thread-local, `lower.pl:197` | `table_name/2` at `lower.pl:213` falls back to `Ref = Table/_` when the fact is absent, so a helper called outside `with_storage_context/2` silently uses the semantic name instead of failing |

`table_name/2`'s silent fallback is the sharpest one:

```prolog
table_name(Ref, Table) :-
    ( physical_storage_name(Ref, Table) -> true ; Ref = Table/_ ).
```

Absent context yields a plausible-looking wrong table name rather than an error.

### Clause counts per exported predicate

`lower.pl`'s exported surface, clause counts from the head scanner:

| predicate | clauses | predicate | clauses |
| --- | --- | --- | --- |
| `column_def/4` | 11 | `lower_program/2` | 1 |
| `ir_column_storage/5` | 11 | `catalog_type_rows/6` | 1 |
| `canonical_column_expr/3` | 10 | `struct_type_plans/3` | 1 |
| `compile_expr/7` | 1 | `compile_comparison/4` | 1 |

The largest clause families in the whole file:

```
11  ir_column_storage/5      11  column_def/4       10  canonical_column_expr/3
10  aggregate_select_expr/5   7  catalog_type_id/2   7  catalog_declared_column/2
 6  list_element_render/5     6  catalog_level_family/3
```

`ir_column_storage/5` and `column_def/4` are eleven clauses each and are the
two halves of one decision. `lower.pl:29-31` says they are exported together
"so one test can compare the DDL's answer against the IR's on ONE run", which
is a test for a divergence that a single source of truth would make
unrepresentable.

### Copy-paste families: the transient DDL mints

Eighteen distinct `__`-prefixed table-name minters, each a one-line
`format(atom(X), '__<prefix>_~w', [Table])`. `grep -n "'__[a-z_]*~w'" lower.pl`:

| prefix | name-minting line |
| --- | --- |
| `__delta_` | `lower.pl:218` |
| `__frontier_` | `:222` |
| `__next_frontier_` | `:226` |
| `__pre_` | `:230` |
| `__departure_frontier_` | `:241` |
| `__support_next_` | `:293` |
| `__txt_` | `:1333` |
| `__txt___delta_` | `:1339` |
| `__ref_` | `:1383` |
| `__avg_acc_` | `:4215` |
| `__agg_scope_` | `:4460` |
| `__new_` | `:4711` |
| `__expand_a_` / `__expand_b_` | `:4788` / `:4791` |
| `__ping_` / `__pong_` / `__cone_` | `:4826` / `:4830` / `:4834` |
| `__agg_` | `:5959` |

Fourteen `CREATE TEMP TABLE` format strings, `grep -n "CREATE TEMP TABLE" lower.pl`:

```
4581  4595  4600  6301  6316  6318  6332  6348  6358  6396  6411  6414  6429  6436
```

Four `CREATE INDEX` format strings over them: `lower.pl:6337` (`_sign`),
`:6343` (all columns), `:6353` (`_phase`), `:6443` (partial, `__refcount <= 0`).

`lower.pl:6301`, `:6348` and `:6358` are three copies of one string,
`'CREATE TEMP TABLE ~w ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, ~w)'`.
`lower.pl:4581`, `:4595`, `:6316`, `:6396`, `:6411` and `:6414` are six copies
of `'CREATE TEMP TABLE ~w (~w, PRIMARY KEY (~w)) WITHOUT ROWID'`.

Every family follows one shape: mint a name from `table_name/2`, build a
`CREATE TEMP TABLE` with the rel's columns plus one or two fixed columns,
optionally add one index. Nine of the fourteen DDL strings are duplicates of
one of two templates.

### Cross-reference: the shared-frontier lab

`v6/labs/shared_frontier/REPORT.md`, landed as `8ef2c6922` (PR #374), measures
what a shared frontier would delete. The referenced plan
`plans/2026-08-19-shared-sqlite-frontier.md` is NOT in this tree; only the lab
that graded it is.

Numbers from the lab, relevant to any lower.pl refactor:

| finding | number |
| --- | --- |
| F6 | pokeapi emits 966,907 of 1,682,616 DDL bytes as per-relation transient tables, 57.5%, replaced by 416 bytes of shared DDL |
| C8 | 780 `__frontier_`, 780 `__next_frontier_`, 780 `__delta_`, 4 `__support_next_`, 4 `__new_`; 2,348 transient tables and 2,348 indexes |
| F5 | boot at N=1024: 388.87 ms to 70.05 ms; 3,226 temp pages to 5 |
| F1 | arm B faster at every k >= N/8, B/A 0.814-0.863 |
| F4 | at k=1 arm B loses, B/A 1.094 |
| F9 | the plan's frontier PK column order `(relation_id, row_id, tick, sign)` is not sargable for the tick read; `(relation_id, tick, row_id, sign)` reads 22% faster |
| C1 | the plan's stated seam `compile.pl:701-721` had already drifted; `compile_program_phases/8` is `compile.pl:671` |

The `lower.pl` regions a shared-frontier implementation will rewrite:
`lower.pl:6213-6533` (the delta-statement section, 321 lines) and, inside it,
the DDL block `lower.pl:6293-6444`, plus the name minters at `lower.pl:218-241`,
`:293`, `:4711`, `:4788-4834`. Any refactor arc that touches those exact ranges
collides with it.

## Defect ledger

Ordered by cost, worst first. Every row has a site.

| # | defect | site | measured |
| --- | --- | --- | --- |
| D1 | `Decls` is an unindexed flat list scanned linearly by every phase | 246 `member`/`memberchk` sites over 23 files | `4e2c21a82` fixed 5 of them and took pokeapi from 526.4 s to 0.9 s |
| D2 | `lower.pl` is 429 predicates in one file with 392 private | `wc -l lower.pl` = 6800 | 24% of it sits under one banner (`:960-2582`) whose name covers 68 of its 111 predicates |
| D3 | `plan/9` and `lowered/8` are positional with no accessors | 29 and 10 destructure sites in 12 and 9 files | adding one field touches 12 files including 3 gate scripts |
| D4 | the `lowered/8` slot arities documented in the module header are wrong for 3 of 5 | `lower.pl:1-23` vs `lower.pl:6685, 6704, 6735` | `edgestmt/7` vs `/9`, `levelstmt/5` vs `/7`, `deltastmt/4` vs `/5` |
| D5 | host, bind and query plans computed then discarded, recomputed per emitter | `compile.pl:130` discards; `emit_ts.pl:485,499`, `emit_rust.pl:563` recompute | 2 to 3 derivations of one fact |
| D6 | `expand_generic_program_raw/2` is a verbatim 14-goal copy of `expand_generic_program_with_bindings/3` | `0_generic_expand.pl:50-67` vs `:70-87` | only non-test caller is `plunit_tests.pl:5113` |
| D7 | `type_definitions/2` rebuilt inside every `program_violation/3` clause that needs it | `0_program_check.pl:226,332,344,370,425,476,521,533,731,800` | 12 calls per self-map compile, each a `findall` over 501 decls |
| D8 | `source_statement_fact/3` holds only the entry file's rows, so an error in an imported module cannot report a line | written `parse_dl_dcg.pl:223`, retracted `:107`, entry-last stated `use_resolve.pl:119`, consumer `compile.pl:650` | golden-flex's own known-red row shows `rule-index unavailable` |
| D9 | `table_name/2` silently falls back to the semantic name when the storage context is absent | `lower.pl:213` | a helper called outside `with_storage_context/2` emits a wrong table name with no error |
| D10 | 18 name minters over 14 DDL strings, 9 of which are duplicates of 2 templates | `lower.pl:218-241, 293, 4711, 4788-4834, 6293-6444` | 57.5% of pokeapi's emitted DDL bytes, lab F6 |
| D11 | `body_ref_uses/2` is a process-global answer table reset from exactly one call site | `analyze.pl:109`, reset `analyze.pl:111`, called `compile.pl:174` | any entry that skips `program_plan/3` reads stale answers |
| D12 | the expansion phase order is nine table rows plus five hard-wired rewrites outside the table | `1_expansion.pl:31-65` vs `:82, 86, 97, 98, 99` | 3 of 9 phases carry an order constraint in a comment; nothing enforces any of them |
| D13 | `column_type_unknown` thrown from two sites with two resolution paths | `0_type_plane.pl:151`, `lower.pl:3582` | golden-flex red, CI-KNOWN-RED group C |
| D14 | the relation-value rewrite exists twice | `0_relation_pattern.pl` 102 lines (oracle only) vs `lower.pl:3212-3651` 440 lines (compiler) | two implementations of one language rule |
| D15 | `bootstmt/2` becomes `bootstmt/3` by a retag pass | built `lower.pl:6577,6603,6653`, retagged `lower.pl:6779-6781` | one functor, two arities, one file |
| D16 | `with_storage_context/2` asserts and retracts the whole storage map twice per compile | `lower.pl:6659` and `lower.pl:6755` | 834 `assertz/1` calls in one self-map compile |
| D17 | `emit_ts:emit_program/5` re-destructures `Plan` four times in one clause | `emit_ts.pl:2563, 2575, 2593, 2655` | |
| D18 | `type_name/2` is duplicated verbatim in the TS and Rust type emitters, along with its collision detector | `compile/7_emit_ts_types.pl:290` and `compile/8_emit_rust_types.pl:760`; detectors at `7_emit_ts_types.pl:166-185` | |
| D19 | CLAUDE.md's two remaining hard-coded line numbers have rotted | CLAUDE.md cites `lower.pl:2319` and `lower.pl:347`; the sites are `lower.pl:2458` and `lower.pl:383` | |
| D20 | three full compiles to write three type artifacts from one `row/11` table | `v6/prolog/examples/_1_emit_types.sh:12,16,20` | parse+plan+lower+boot run 3x |
| D21 | `1_expansion.pl:45` uses the word `load-bearing` in a comment | `1_expansion.pl:45` | banned identifier/prose word per CLAUDE.md |

## Design forks needing Chris

Repo law: no lane settles language or type-system design. Each row is a
question with its throw site, never a proposal.

| fork | throw site | the question |
| --- | --- | --- |
| F1. generic application in column position | `0_type_plane.pl:151`, `column_storage/3`'s fall-through clause | `golden-flex.dl6:28` writes `CodecUse(value: CodecBox(CodecDocument))`. `column_storage/3` has clauses for `json_list(T)` and `list(T)` and no clause for a user template application. Is a bound template application a column type, and if so does it resolve to the minted instance rel's `ref/1`, or is the correct answer a named stop with a better name than `column_type_unknown`? |
| F2. `json_list(T)` as a surviving storage kind | `0_type_plane.pl:118-125` comment states it collapses to `json` before `column_def/4` | keeping `json_list(T)` alive to the DDL would let SQLite enforce array-ness as a CHECK. That widens every `json` match in `lower.pl`. Language decision: is `json_list(T)` a distinct storage kind or a view? |
| F3. `type_name/2` non-injectivity | `compile/7_emit_ts_types.pl:290`, detector `:166` | the module-prefix rule is decided (CLAUDE.md). The open half: `type_name/2` is duplicated in the Rust emitter and the two could drift. Is one shared `type_name/2` allowed to live in a neutral module, or does each target language own its own naming? |
| F4. `table_name/2` fallback | `lower.pl:213` | should a missing storage context be an error rather than a silent semantic-name fallback? Changing it can turn a currently-compiling program into a throw, so it is a semantics change |
| F5. imported-module error locations | `parse_dl_dcg.pl:107` retract, `use_resolve.pl:119` entry-last | keeping every file's statement table means the fact tables become per-parse arguments or a keyed store. That changes what `bop check` prints for a program whose error is in a `use` target. User-visible output change |
| F6. the two surface program shapes | `parse_dl_dcg.pl:124-128` produces `prog/2` or `program/3` | collapsing to one shape (always `program/3`, queries as a real slot) removes the `compile.pl:250` re-scan and the `1_host_expand.pl:66-67` normalizer. It changes what `print_dl.pl` round-trips, which the `roundtrip` gate grades |
| F7. shared frontier vs per-relation transients | `lower.pl:6293-6444`, lab `v6/labs/shared_frontier/REPORT.md` | already Chris's call and already planned; named here only so the refactor arcs stay off those line ranges |
