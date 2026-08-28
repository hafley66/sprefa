# Slice 3 audit: canonical lowering (`lower.pl`, `0_graph.pl`)

Contents:

1. Scope and shape of the slice
2. Entry points and callers
3. Predicate-family map (`lower.pl`)
4. `0_graph.pl` predicate map
5. Canonical term shapes in and out
6. Dynamic state, ordering, and module-state dependencies
7. Per-predicate report blocks (entry points and exported seams)
8. Counts, extraction boundary, first adaptation force, open rulings

## 1. Scope

`v6/prolog/lower.pl` is 7,908 lines, module `lower`, exporting 46 names
(module header lines 116-176). It compiles `compile.pl`'s `plan/9` term into
the `lowered/8` canonical program contract consumed by both emitters
(`emit_ts.pl`, `emit_rust.pl`) and described in the engine contract. It also
emits `bootstmt/3` boot statements through a separate door, `boot_statements/7`,
because the plan does not carry `Initial`.

`v6/prolog/0_graph.pl` (196 lines, module `graph`) is a pure
Kosaraju/topological-order utility over `library(ugraphs)`. It has exactly one
in-tree caller (`3_clock_check.pl:29`) and no dynamic state. Everything in it
is class `extract`.

## 2. Entry points and callers

| Entry point | Consumes | Produces | Callers |
|---|---|---|---|
| `lower_program/2` (`lower.pl:7575`) | `plan/9` from compile.pl | `lowered/8` | `compile.pl`, `6_profile.pl:27`, `sweep.pl:38` |
| `boot_statements/7` (`lower.pl:7859`) | Mode, Decls, Types, RelPlans, Initial, LevelStatements | `bootstmt(Rel, Sql, Params)` list | `compile.pl`, `6_profile.pl`, `sweep.pl` |
| `lowered_program_data/2,3` (`lower.pl:7756,7760`) | Plan + lowered/8 | `program_data/5` (relations/rules write-verb rows) | emitters, shared-frontier work |
| `plan_rule_level_statements/2` (`lower.pl:1456`) | `plan/9` | RuleLevelStatements (mirrors the lower_program pipeline) | catalog rail callers outside `lower_program/2` |
| `catalog_all_rows/10`, `catalog_rows/4`, `catalog_decl_rows/6`, `catalog_type_rows/6`, `catalog_type_relation_rows/3`, `catalog_type_transport_rows/4` | Rules/RelPlans/Decls | catalog `row/11` terms | `emit_ts.pl`, `emit_rust.pl`, typegen |
| `compile_expr/7`, `compile_comparison/4`, `canonical_column_expr/2,3`, `statement_rule_ids/3`, `query_order_by_map/3`, `departure_frontier_table_name/2`, `departure_read_sql/3`, `write_verb/1`, `fixpoint_round_cap/1` | expression/pattern terms | SQL text fragments | emitters, `query_order_tail.test.pl`, `plunit_tests.pl` |
| `intern_mode/2`, `interned_column/2`, `string_dictionary_table/1`, `program_text_intern_plan/3`, `intern_write_sql/4`, `struct_type_plans/3,4`, `dictionary_table_name/2`, `dictionary_render_expr/3`, `json_capture_json_type/2` | column types, struct values | intern plans / SQL | emitters, `type_relation_ir.test.pl` |
| `column_def/4`, `ir_column_class/4`, `uniform_text_encoding/1` | column types | DDL text vs IR class | `0_storage_projection.test.pl` |
| `level_ref_count_sql/5`, `level_dred_plan/5`, `semantic_generic/4`, `semantic_generic_instance/4` | levelstmt internals | refcount/dred plans | plunit units, rulings.pl |
| `frontier_mode/1`, `with_frontier_mode/2`, `shared_frontier_relation_id/2,3` | mode option | thread-local read of mode/relation ids | `shared_frontier.test.pl`, emitters |
| `audit_scan_index_pairs/5`, `audit_scan_index_ddls/5`, `audit_scan_index_ddl/3`, `catalog_ddl_contract/2`, `query_order_by_map/3`, `plan_rule_level_statements/2` | plan pieces | index DDL / map SQL | `6_isolated_compiler_dd.test.pl`, emit side |

Direct module callers: `compile.pl:51` (whole module), `emit_ts.pl:13`,
`emit_rust.pl:17`, `6_profile.pl:27`, `sweep.pl:38`,
`conformance/rulings.pl`, `0_program_check.pl`,
`compile/6_isolated_compiler_dd.pl`, `compile/typegen_export.pl`,
`compile/scripts/{metamorphic_rename,arm_census}.pl`.

Tests: `v6/prolog/compile/test/` (notably `shared_frontier.test.pl`,
`query_order_tail.test.pl`, `type_relation_ir.test.pl`, `0_storage_projection.test.pl`,
`6_isolated_compiler_dd.test.pl`, `emit_rust.test.pl`, `plunit_tests.pl`,
`2_subscribe.plt`, `compiler_relations.test.pl`,
`4_braced_nested_relations.test.pl`, `5_remove_rel_is.test.pl`,
`anonymous_{product,sum}_values.test.pl`, `run_sql_check.pl`,
`run_plunit.pl`), plus `conformance/fixtures/door_split_trigger_literal.pl`.

## 3. Predicate-family map for `lower.pl`

Families that consume **surface-shaped terms** (DL6 rule bodies, pattern
arguments, `decode/2` goals, `:=` guards, aggregates, `{}`/json literals):

| Family | Lines | Consumes | Emits |
|---|---|---|---|
| Rule-identity naming | 368-421 | head refs, rule order | `rule_id` text (`prog:name/arity#ordinal`) |
| Pattern-argument compiler | 423-524 | body atom args (vars, `bool_lit`, compounds) | where-part pairs, `Bound` = `Var-typed(Sql,Type,Encoding)` |
| Positive body-atom compilation | 526-767 | `use(Ref,Args,pos,Source)` | FROM/WHERE SQL, alias table `b<N>` |
| Negative body-atom compilation | 769-822 | `use(_,_,neg,_)` | `NOT EXISTS` SQL, `c<N>` markers |
| Head/expression compiler `compile_expr/7` | 824-1240 | head args, `:=` RHS, comparisons, aggregates, `{}`/`json_*` docs, arithmetic | typed SQL fragments |
| Guard/bind goals | 1242-1269 | `:=`, regexp, comparisons | WHERE text, `__tick` subquery |
| Catalog scaffold | 1271-2926 | `prog(Decls,Rules)`, RelPlans | `row/11` catalog rows + `INSERT` SQL |
| Interning / text constants | 2929-3140 | column types, literals | dictionary DDL, `__txt_` views, intern plans |
| DDL minting | 3142-3515 | RelPlans, column defs | `CREATE TABLE/VIEW/INDEX/TRIGGER` text |
| Relation-pattern + decode expansion | 3516-4016 | rules containing relation terms / `decode/2` | rewritten rule bodies (dictionary atoms) |
| Arrival statements | 4017-4122 | ArrivalRelPlans | `arrivalstmt/6` |
| Edge rule lowering | 4123-4493 | `EdgeRules` (surface `<+` rules, trigger atoms, samples) | `edgestmt/7` groups |
| Level rule lowering + aggregates | 4495-5109 | `DecodedRuleOrder` | `levelstmt/7` (+ `aggsql`/`avgsql`) |
| Refcount / expand / dred | 5110-5627 | levelstmt RefCountSql | `refcountsql/16`, `expandplan`, dred SQL |
| Backend-neutral fixpoint IR | 5628-6154 | same rule bodies | `ir_*` terms (the wavefront contract) |
| json decode lowering | 6155-6440 | `decode/2` goals | json1 guards, capture types |
| Aggregate heads | 6441-6971 | aggregate rules | scope/accumulator statements |
| Delta statements | 6972-7021 | RelPlans | `deltastmt/5` |
| `?` order tails | 7023-7446 | query decls, order tails | `query_order_by_map/3`, index DDL |
| Boot | 7447-7572, 7850-7908 | Initial rows, LevelStatements | `bootstmt/3` |
| Top level | 7573-7738 | `plan/9` | `lowered/8` |
| Six write verbs | 7740-7848 | lowered/8 | `program_data/5` |

Families that emit the **canonical program contract** (consumed by engines /
emitters, not surface): the top-level `lower_program/2` pipeline
(`lower_program_in_context`, lower.pl:7582), `boot_statements/7`,
`level_statement_groups/4` (which is also re-exported via
`plan_rule_level_statements/2`), `delta_statement/2`, `arrival_statement/2`,
`edge_statements_for_rule/3`, `statement_rule_ids/3`, `catalog_row_ddl/10`
family, `level_ref_count_sql/5` + `level_dred_plan/5` (fixpoint IR at
5628-6155), `lowered_program_data/3` write-verb map, `query_order_by_map/3`,
`departure_read_sql/3`, `fixpoint_round_cap/1`.

Naming note: the pipeline re-derives statement families twice, once for DDL
existence (rel/family DDL mint sites) and once for rows
(`catalog_all_rows/10` mirrors each mint site clause for clause). That is a
deliberate invariant ("a plane row cannot describe a table the lowering did
not create", lower.pl:1531) but it is a textual mirror, not a shared
computation; `plan_rule_level_statements/2` (1456) is the one shared
re-derivation.

## 4. `0_graph.pl`

Pure wrapper over `library(ugraphs)` plus a hand-written Kosaraju with
`library(assoc)` for O(log n) neighbour lookup. Ten predicates, no dynamic
state, no cuts beyond a single member-cut in `graph_component_of/3`. One
in-tree consumer: `v6/prolog/3_clock_check.pl:29`
(`graph_from_edges/3`, `graph_cyclic_components/2`). Fully extract; the
Kosaraju complexity note (ugraph as assoc) is the only preserved law that
matters (linear passes, never `memberchk` over the pair list).

## 4b. Canonical term shapes

Entering the slice:

```prolog
plan(Name, prog(Decls, Rules), LoweringTypes, RelPlans, ArrivalTargets,
     RuleOrder, EdgeRules, SubscribedRels, Mode)
RelPlan = relplan(Ref, Kind, Columns, KeyOrNone, ColumnTypes, StorageName)
Rule    = (Head <- Body) | (Head <+ Body)
use(Ref, Args, pos|neg, Source)
```

Leaving the slice:

```prolog
lowered(Name, Ddl, ArrivalStatements, EdgeStatements, LevelStatements,
        DeltaStatements, RelPlans, ArrivalTargets)
arrivalstmt(Ref, Kind, AddSql, DelSqlOrNone, IncAddSql, IncDelSqlOrNone)
edgestmt(HeadRef, TriggerRef, HeadColumns, KeyColumns, ProjectSql, WriteSql,
         DeltaProjectSql)
levelstmt(HeadRef, DeleteSql, InsertSqls, DeltaInsertSql, RefCountSql,
          AggregateSql, Extra)  % execution order = strat.pl:sql_rule_order/2
retentionstmt(Ref, Limit, DeleteSql)
deltastmt(Ref, SelectAllSql, DeltaTable, BoundarySql, ...)
bootstmt(Rel, Sql, Params)
catalog rows: row(Id, ParentId, Ordinal, LocalName, Kind, TypeId, Arity,
                 ModuleId, HId, HSchema, HRule)
```

The internal `typed(Sql, Type, Encoding)` binding and `refcountsql/16`,
`expandplan/8`, `ir_*` fixpoint terms are slice-internal but pinned by tests.

## 5. Dynamic state and ordering dependencies

Dynamic predicates (all thread-local, all scoped by `setup_call_cleanup`):

| Predicate | Installed by | Scope |
|---|---|---|
| `physical_storage_name/2` | `with_storage_context/2` (219) | around `lower_program/2`, `boot_statements/7`, `plan_rule_level_statements/2`, `catalog_all_rows/10`, `catalog_type_rows/6` |
| `frontier_mode_option/1` | `with_frontier_mode(shared, Goal)` (242) | dynamic mode; default `per_rel` |
| `shared_frontier_relation_id_fact/2` | `with_shared_frontier_ids/2` (258) | relation ids from RelPlans order; ids only valid inside the same `lower_program/2` call |

Ordering dependencies inside `lower_program_in_context` (7582):

1. DDL ordering is contract: dictionaries (`InternDdl`) and seed DDL come
   FIRST in the emitted list so storage planes exist before dependents
   (7666-7691); `catalog_row_ddl` must run after `level_statement_groups`
   because `RuleLevelStatements` is its input (7661).
2. Expansion order is fixed: `expand_relation_pattern_rules` BEFORE
   `expand_decode_rules` (comment 7625: reverse order makes the spellings
   non-composable), both against `BodyRelPlans` = DictionaryRelPlans ++
   RelPlans.
3. `boot_statements/7` needs `LevelStatements` from the SAME `lower_program/2`
   call (7850 comment, PHASE C2 RULING 2); a caller must keep the pair.
4. Statement ordinals (`statement_rule_ids/3`, 383) are 1-based in lowering
   order among statements sharing a head ref; reordering two arms changes the
   ids by design.
5. `catalog_all_rows/10` requires the storage context; outside it the catalog
   described tables the DDL never created (1483 comment).
6. `shared_frontier_guard/2` runs AFTER lowering and throws
   `frontier_shared_todo/1` for nine construct classes (7706-7738), so
   shared-mode failure is post-hoc, not per-step.
7. `run_compile_step/4` (from `compile/0_trace`) wraps every step for tracing;
   importing it from `0_trace` avoids the compile.pl cycle (202-204).

## 6. Per-predicate report blocks

Representative blocks; family members inherit the block's class unless noted.

```prolog
% File: v6/prolog/lower.pl:7575
% Existing comment: top level section; plan/9 -> lowered/8, six-field statement families
% Signature: lower_program(+Plan, -Lowered)
% Called by: compile.pl:51, 6_profile.pl:27, sweep.pl:38, lowered_program_data/2
% Calls: with_storage_context/2, with_shared_frontier_ids/2, rel_ddl/5,
%        arrival_statement/2, edge_statements_for_rule/3, level_statement_groups/4,
%        delta_statement/2, catalog_row_ddl/10, program_intern_ddl/3, ...
% Tests: v6/prolog/compile/test/plunit_tests.pl, shared_frontier.test.pl, sweep.pl
% V7 class: adapt
% Parser coupling: term-shape (plan/9, rule <-/<+, ops declared at 206-208)
% Preserved law: lowered/8 statement families are complete and in execution
%   order such that applying DDL then arrivals then edges then levels then
%   deltas reproduces the engine's tick semantics.
% DL7 seam: input stays a plan term over cons-tree rule bodies; output keeps
%   the lowered/8 field order because the engine consumes those fields.
```

```prolog
% File: v6/prolog/lower.pl:7859
% Existing comment: boot statements computed on demand; needs Initial which plan does not carry
% Signature: boot_statements(+Mode, +Decls, +Types, +RelPlans, +Initial, +LevelStatements, -BootStatements)
% Called by: compile.pl (directly, with fixture Initial), 6_profile.pl, sweep.pl
% Calls: boot_seed_statement_for/6, boot_level_recompute_statements/2, with_storage_context/2
% Tests: compile/test/plunit_tests.pl (head_move_flips_current_tree_in_one_tick fixture)
% V7 class: adapt
% Parser coupling: term-shape
% Preserved law: t=0 level views over Initial-seeded data equal one extra
%   level-closure pass, not empty.
% DL7 seam: unchanged; the caller must still supply Initial and the SAME
%   LevelStatements the lower_program/2 call produced.
```

```prolog
% File: v6/prolog/lower.pl:859
% Existing comment: the ONE expression compiler; mirrors engine.pl eval_expr clause for clause; three documented wrong-translations (/ integer-only, mod sign correction, non-int refusal)
% Signature: compile_expr(+Mode, +Demand, +Expr, +Bound, -Sql, -Type, -Encoding)
% Called by: compile_pattern_arg, head/compiler paths, compile_guard_goal, aggregate compilers
% Calls: registry:expression/5, sql_literal/2, demanded_sql/5, arithmetic_*, json_*, text_scalar_*
% Tests: conformance/expressions.pl, compile/test/run_sql_check.pl
% V7 class: adapt
% Parser coupling: term-shape (compound functors are the expression surface)
% Preserved law: SQL rendering never silently diverges from engine.pl eval_expr; every divergence is a named unsupported_construct.
% DL7 seam: expression terms become cons-tree forms; Bound stays typed(Sql,Type,Encoding).
```

```prolog
% File: v6/prolog/lower.pl:435 (family 423-522)
% Existing comment: pattern-argument compiler; Binding = bind|check; json1 compound encoding
% Signature: compile_pattern_arg(+Mode, +Arg, +ColumnExpr, +ColumnType, +Bound0, -Bound, -WhereParts, +Binding)
% Called by: compile_atom_args, compile_negative_atom_args, compile_coalesced_args, seeded_pre_args
% Tests: compile/test/*.test.pl (body-compilation units), run_sql_check.pl
% V7 class: adapt
% Parser coupling: term-shape (compound = destructuring pattern)
% Preserved law: a shared variable across two columns of different storage type is a named error, never an affinity join.
% DL7 seam: same; pattern args stay term-shaped, only the literal spellings change.
```

```prolog
% File: v6/prolog/lower.pl:383
% Existing comment: rule identity "<program>:<name>/<arity>#<ordinal>", ordinal is 1-based in lowering order
% Signature: statement_rule_ids(+Program, +HeadRefs, -RuleIds)
% Called by: emit_ts.pl, emit_rust.pl
% Tests: query_order_tail.test.pl, plunit_tests.pl
% V7 class: extract
% Parser coupling: none
% Preserved law: ordinal moves when two arms of one head are reordered; that is the honest answer.
% DL7 seam: unchanged.
```

```prolog
% File: v6/prolog/lower.pl:1274-2687 (catalog family, ~90 predicates)
% Existing comment: step g1 SCAFFOLD; column is a child row of its rel; ids positional for byte-stable recompile
% Signature: catalog_all_rows/10, catalog_rows/4, catalog_decl_rows/6, catalog_type_rows/6, catalog_row_ddl/10, plane row families
% Called by: lower_program_in_context, emit_ts.pl, emit_rust.pl, typegen_export.pl
% Tests: 6_isolated_compiler_dd.test.pl, type_relation_ir.test.pl, compiler_relations.test.pl
% V7 class: adapt (row shapes are the type artifact's contract; the id layout is positional and order-sensitive)
% Parser coupling: term-shape (rel/5, sh_decl/4, interface_decl/3)
% Preserved law: every plane row's existence condition mirrors its DDL mint site clause for clause.
% DL7 seam: rels/types stay edge-represented (owner/name/target/ordinal) per the V7 assumption; the row/11 shape survives.
```

```prolog
% File: v6/prolog/lower.pl:5628-6155 (fixpoint IR family)
% Existing comment: backend-neutral fixpoint IR; one number for every fixpoint walk (fixpoint_round_cap/1)
% Signature: level_fixpoint_ir/2, ir_* family, level_ref_count_sql/5, level_dred_plan/5, fixpoint_round_cap/1
% Called by: level_ref_count_sql/5, ref_count_ddl/3, emitters
% Tests: plunit_tests.pl, shared_frontier.test.pl
% V7 class: adapt
% Parser coupling: none (backend-neutral by design)
% Preserved law: the wavefront hop cap and the stratum-group outer-round cap are the same number.
% DL7 seam: this is the V7-keep IR; the sql-text families around it are the replaceable rendering.
```

```prolog
% File: v6/prolog/0_graph.pl:28-196 (all 10 exports)
% Existing comment: graph reachability, closure, SCC (Kosaraju), topological order, cycle detection; closure strictness confirmed by measurement
% Signature: graph_from_edges/2,3, graph_nodes/2, graph_closure/2, graph_reaches/3, graph_components/2, graph_cyclic_components/2, graph_component_of/3, graph_topological_order/2, graph_has_cycle/1
% Called by: 3_clock_check.pl:29
% Calls: library(ugraphs), library(assoc)
% Tests: none directly in-repo for this module (used via 3_clock_check)
% V7 class: extract
% Parser coupling: none
% Preserved law: components sorted, each member sorted; topological order fails on any cycle including self-loops.
% DL7 seam: pure From-To edge lists; no term-shape dependency on DL6.
```

## 5. Totals

Predicate definition sites in `lower.pl`: ~460 (many multi-clause); family
count above. By class, family-level: extract ~15% (identifiers, graph,
quoting, sql_literal), adapt ~65% (all pattern/expr/DDL/catalog/level/edge
compilers), oracle ~10% (fixpoint IR, catalog rows, json capture-type pin,
measured-mod/behaviour comments), drop ~10% (DL6 operator forms `<-`, `<+`,
`:=`, `decode/2` expansion once cons-tree syntax lands; `bool_lit`, `{}`
brace patterns).

## 6. Extraction boundary

Smallest self-contained boundary: module `lower` minus the catalog family
(1271-2926) minus the surface-expansion pair
(`expand_relation_pattern_rules`, `expand_decode_rules`, lines 3516-4016).
What remains needs only: `compile/registry` (expression/5, surface/5),
`0_type_plane` column storage, `0_rel_record` rel/5, and the two
thread-locals. That core is the SQL-statement producer; it has no surface
syntax beyond head/body atoms.

## 7. First dependency forcing adaptation

`compile_pattern_arg/8` and `compile_expr/7` read `Bound` entries as
`typed(Sql, Type, Encoding)` and branch on `Mode` (`split` vs `direct`
interning) plus `frontier_mode/1` thread state. Any DL7 term-shape change to
patterns or expressions forces these two signatures to change, and they are
called from every statement family, so the whole file adapts rather than
extracts. The catalog family's second force: `catalog_decl_rows/6` mints a
positional id layout (`ctx/4`) that the plane half re-walks; a DL7
edge-shaped type/relation representation changes the id layout, hence
`adapt`.

## 8. Unresolved questions for a V7 ruling

1. Does the `lowered/8` program contract survive as-is (engine plan fields
   preserved) or does the cons-tree source allow the Round-2 tick-number
   elimination to be pushed further (dropping `__tick`, `__pre`)?
2. Keep the json1 tagged-term compound encoding, or adopt the exemplar's
   raw-text encoding (`route_data(settings)` + LIKE/substr)? Both are legal
   IRow TEXT; the file keeps json1 on generality grounds (lower.pl:83-96).
3. Shared-frontier mode (`plans/2026-08-19-shared-sqlite-frontier.md`) still
   refuses eight construct families via `shared_frontier_todo/2`; is shared
   the V7 default (which forces closing those TODO sites) or does per_rel
   remain primary?
4. `boot_statements/7` exists outside `lower_program/2` because the plan does
   not carry `Initial`; V7 should rule whether Initial joins the plan term.
5. `statement_rule_ids/3` ordinals are lowering-order, not source order; the
   trace-line contract depends on that. Confirm for DL7.
6. Catalog id layout is positional and byte-stability-pinned
   (`catalog_row_ddl/10`); any DL7 IR change moves every id. Is byte-stable
   recompile still a requirement?

## 9. Per-predicate blocks: every exported name

Preceding-comment summaries are quoted or compressed from the comment
immediately above each definition; line numbers are the definition site.

```prolog
% File: v6/prolog/lower.pl:7575
% Existing comment: top-level section header "plan/6 term compile.pl builds into a lowered/8 term" (header comment lines 1-27)
% Signature: lower_program(+Plan, -Lowered)
% Called by: compile.pl:51, 6_profile.pl:27, sweep.pl:38, lowered_program_data/2:7756
% Calls: with_storage_context/2:219, with_shared_frontier_ids/2:258, rel_ddl/5:3151, arrival_statement/2:4019, edge_statements_for_rule/3:4154, level_statement_groups/4:4517, delta_statement/2:6987, catalog_row_ddl/10:1440, program_intern_ddl/3:2958, shared_frontier_guard/2:7699
% Tests: v6/prolog/compile/test/plunit_tests.pl, shared_frontier.test.pl, run_sql_check.pl, sweep.pl
% V7 class: adapt
% Parser coupling: term-shape
% Preserved law: DDL, then arrivals, then edges, then levels (execution order), then deltas, reproduces engine tick semantics.
% DL7 seam: input plan over cons-tree rules; output lowered/8 field order preserved for the engine.
```

```prolog
% File: v6/prolog/lower.pl:7859
% Existing comment: "boot statements, computed on demand (needs Initial, which plan/6 does not carry)" (7850)
% Signature: boot_statements(+Mode, +Decls, +Types, +RelPlans, +Initial, +LevelStatements, -BootStatements)
% Called by: compile.pl, 6_profile.pl:27, sweep.pl:38
% Calls: boot_seed_statement_for/6:7876, boot_level_recompute_statements/2:7897, with_storage_context/2
% Tests: compile/test/plunit_tests.pl (head_move_flips_current_tree_in_one_tick)
% V7 class: adapt
% Parser coupling: term-shape
% Preserved law: t=0 level views over Initial data equal one extra level-closure pass (PHASE C2 RULING 2), never empty.
% DL7 seam: unchanged; caller must pair it with the same lower_program/2 call's LevelStatements.
```

```prolog
% File: v6/prolog/lower.pl:2934
% Existing comment: "Threaded, never a flag: a runtime toggle cannot undo a declared column type." (2930); "A compile input, defaulted here and recorded in the emitted artifact." (2932)
% Signature: intern_mode(+Options, -Mode)
% Called by: compile.pl (resolves the compile option into the atom the plan carries)
% Calls: memberchk/2
% Tests: plunit_tests.pl
% V7 class: extract
% Parser coupling: none
% Preserved law: mode defaults to `dict` when no intern option is declared.
% DL7 seam: options list shape only.
```

```prolog
% File: v6/prolog/lower.pl:2939
% Existing comment: "json stays TEXT: json1 reads it in place." (2938)
% Signature: interned_column(+Mode, +DeclaredType)
% Called by: column_def/4, text_intern_plan, catalog_storage_rows, program_intern_ddl
% Calls: none
% Tests: 0_storage_projection.test.pl
% V7 class: extract
% Parser coupling: none
% Preserved law: only `dict` mode interns, only `text` columns.
% DL7 seam: unchanged.
```

```prolog
% File: v6/prolog/lower.pl:2941
% Existing comment: none (single fact under the interning header)
% Signature: string_dictionary_table(-Table)
% Called by: intern_write_sql/4:2845
% Calls: none
% Tests: plunit_tests.pl
% V7 class: extract
% Parser coupling: none
% Preserved law: the one string dictionary is `__str`.
% DL7 seam: unchanged.
```

```prolog
% File: v6/prolog/lower.pl:3128
% Existing comment: "none when no column in the program is interned, so a direct-mode module carries no plan, no import and no statement." (3126)
% Signature: program_text_intern_plan(+Mode, +RelPlans, -Plan)
% Called by: compile.pl, emitters
% Calls: text_intern_plan/3:3107, interned_column_flag/3:3123
% Tests: type_relation_ir.test.pl
% V7 class: extract
% Parser coupling: none
% Preserved law: `none` when RelColumns is empty.
% DL7 seam: textintern(InternSql, LookupSql, RelColumns) term survives.
```

```prolog
% File: v6/prolog/lower.pl:3216 (head at 3216; family 3216-3268)
% Existing comment: "PHASE C2 RULING 1: INTEGER storage for an int-typed column, TEXT for everything else" (3209-3214)
% Signature: column_def(+Mode, +QuotedColumn, +Type, -Def)
% Called by: rel_ddl/6:3151; the only reader of rel/5 storage in the module (header line 8)
% Calls: atomic_list_concat/3
% Tests: 0_storage_projection.test.pl
% V7 class: adapt
% Parser coupling: none
% Preserved law: `__unit` is INTEGER NOT NULL DEFAULT 1 CHECK; int is plain INTEGER NOT NULL; bool gets the 0/1 CHECK.
% DL7 seam: unchanged column-type -> DDL mapping.
```

```prolog
% File: v6/prolog/lower.pl:5711
% Existing comment: "The comparator, which the declared type does not give: bool and ref(_) both store INTEGER, json stores TEXT" (5709-5710)
% Signature: ir_column_class(+Mode, +Column, +Type, -colclass(Column, TypeName, StorageClass, Collation, Encoding))
% Called by: ir_rel_storage/4:5688, tests
% Calls: ir_column_storage/5:5718
% Tests: plunit_tests.pl
% V7 class: adapt
% Parser coupling: none
% Preserved law: text storage always carries binary collation, everything else none.
% DL7 seam: colclass/5 is the storage decision the DDL must agree with (header line 123-124).
```

```prolog
% File: v6/prolog/lower.pl:5699
% Existing comment: "INVARIANT, not a unsupported construct: two encodings on one program's text columns would put the two sides of a text join in different id spaces, silently empty. Unreachable while interned_column/2 is one clause; it exists so that the day a per-column waiver returns, it fires at compile time." (5693-5697)
% Signature: uniform_text_encoding(+ColumnClasses)
% Called by: ir_rel_storage/4:5688
% Calls: findall/3, sort/2, throw/1
% Tests: 0_storage_projection.test.pl
% V7 class: extract
% Parser coupling: none
% Preserved law: at most one text encoding per program; violation is a named compile-time throw.
% DL7 seam: unchanged.
```

```prolog
% File: v6/prolog/lower.pl:859
% Existing comment: "the ONE expression compiler, used for head arguments, `:=` right-hand sides, comparison operands and aggregate arguments alike. Mirrors engine.pl:eval_expr/2 clause for clause"; three documented wrong-translations: `/` int-only, `mod` sign-corrected, non-int refused (829-858)
% Signature: compile_expr(+Mode, +Demand, +Expr, +Bound, -Sql, -Type, -Encoding)
% Called by: compile_pattern_arg:435, compile_guard_goal:2692, compile_comparison:2745, head compilers, aggregate compilers
% Calls: registry:expression/5:193, sql_literal/2:410, demanded_sql/5:919, arithmetic_*, json_*, text_scalar_*, bound_lookup/3:515
% Tests: conformance/expressions.pl, compile/test/run_sql_check.pl
% V7 class: adapt
% Parser coupling: term-shape
% Preserved law: every divergence from engine.pl eval_expr is a named unsupported_construct throw, never a silent answer.
% DL7 seam: expression terms become cons-tree forms; Bound stays typed(Sql, Type, Encoding).
```

```prolog
% File: v6/prolog/lower.pl:2745
% Existing comment: "`==`/`\\==` are eval_expr then Prolog ==/2, term identity ... SQLite's `=` ... applies affinity and can answer TRUE. Both cases are refused by name" (2740-2743)
% Signature: compile_comparison(+Mode, +Goal, +Bound, -Text)
% Called by: compile_guard_goal/5:2692
% Calls: compile_expr/7, comparison_operator_sql/5:2775, content_comparison/5:2761, aligned_pair/6:464
% Tests: run_sql_check.pl, conformance units
% V7 class: adapt
% Parser coupling: term-shape
% Preserved law: cross-type comparison that SQLite would answer via affinity is a named throw.
% DL7 seam: comparison goal functor set may change with cons-tree syntax.
```

```prolog
% File: v6/prolog/lower.pl:2845
% Existing comment: "Statement one of §5.7.1: every built string the arm will produce, set-based, reusing the arm's own FROM and WHERE so the two see identical input." (2842-2843)
% Signature: intern_write_sql(+BuiltValues, +FromSql, +WhereSql, -InternSql)
% Called by: intern_write_statements/4:2839, level arms
% Calls: intern_write_arm/5:2853, string_dictionary_table/1:2941
% Tests: plunit_tests.pl
% V7 class: extract
% Parser coupling: none
% Preserved law: one INSERT OR IGNORE per arm group, UNION of per-value SELECT arms.
% DL7 seam: unchanged.
```

```prolog
% File: v6/prolog/lower.pl:7367/7370/7373/7376/7382/7385/7396/7401/7406/7431/7438 (family, export canonical_column_expr/2,3)
% Existing comment: "INTEGER columns cannot hold a json1 compound ... json1-encoded compound becomes \"F(A1,A2,...)\"; json_valid/1 plus json_type/1 = 'object' gates the compound branch" (7359-7366)
% Signature: canonical_column_expr(+Column, +Type, -Expr) and /3 over outer columns
% Called by: emit_ts.pl, emit_rust.pl (boundary/delta rendering)
% Calls: outer_column_expr/3:7443
% Tests: plunit_tests.pl
% V7 class: adapt
% Parser coupling: none
% Preserved law: TEXT columns render the canonical Prolog term of the json1 encoding; int columns quote directly.
% DL7 seam: unchanged.
```

```prolog
% File: v6/prolog/lower.pl:1961
% Existing comment: "! semantic_generic(+Rows, -Name, -Parameters, -Specs) is nondet. The surface view of one compile-time relation, read back off the graph." (1959-1960)
% Signature: semantic_generic(+Rows, -Name, -Parameters, -Specs)
% Called by: catalog_type_metadata_rows/7:1922, rulings.pl
% Calls: semantic_constraint_surface/3:1989, semantic_surface_type/3:1981, keysort/2, pairs_values/2
% Tests: compiler_relations.test.pl, type_relation_ir.test.pl
% V7 class: adapt
% Parser coupling: term-shape (semantic graph rows)
% Preserved law: parameters and columns come out in declared ordinal order.
% DL7 seam: rows become edge tuples; ordinal ordering stays the contract.
```

```prolog
% File: v6/prolog/lower.pl:1998
% Existing comment: "! semantic_generic_instance(+Rows, -Concrete, -Generic, -Arguments) is nondet." (1997)
% Signature: semantic_generic_instance(+Rows, -Concrete, -Generic, -Arguments)
% Called by: catalog_type_metadata_rows/7:1922
% Calls: semantic_application_arguments/4:2005
% Tests: type_relation_ir.test.pl
% V7 class: adapt
% Parser coupling: term-shape
% Preserved law: a materialized rel deriving from a compile-time application yields (concrete, generic, arguments).
% DL7 seam: unchanged row vocabulary.
```

```prolog
% File: v6/prolog/lower.pl:5110
% Existing comment: "The delta and both frontier copies are written by SQL that reads the same predicates the head mutation reads, so no derived row crosses the JS seam." (5108-5109)
% Signature: level_ref_count_sql(+Mode, +RelPlans, +HeadRef, +Rules, -refcountsql/16)
% Called by: ref_count_ddl/3:7279, catalog level planes, emitters
% Calls: level_expand_plan/5:5300, level_dred_plan/5:5391, level_fixpoint_ir/2:5632, support_count_plan/4:5197
% Tests: plunit_tests.pl, shared_frontier.test.pl
% V7 class: adapt
% Parser coupling: none
% Preserved law: the 16-field refcountsql is the counting retract contract; expand/dred/IR ride inside it.
% DL7 seam: keep refcountsql/16; it is the engine-facing plan.
```

```prolog
% File: v6/prolog/lower.pl:5391
% Existing comment: none directly above (preceded by dred table-name helpers 5366-5379); the dred_plan_admissible/2 comment "Bounds ..." chain governs admissibility (5380-5390)
% Signature: level_dred_plan(+Mode, +RelPlans, +HeadRef, +Rules, -dredplan/24)
% Called by: level_ref_count_sql/5:5110
% Calls: dred_* SQL builders:5469-5627
% Tests: plunit_tests.pl
% V7 class: adapt
% Parser coupling: none
% Preserved law: dred is only planned when level_dred_plan's admissibility checks (no negative use, no pre use, no dictionary use, positive uses present) hold (5380-5390).
% DL7 seam: unchanged.
```

```prolog
% File: v6/prolog/lower.pl:345
% Existing comment: "Last tick's net -delta rows of a rel some rule binds with finalize/1 ... Emitted ONLY for those rels (analyze:listened_departure_refs/2), which is what keeps a program with no finalize byte-identical" (338-344)
% Signature: departure_frontier_table_name(+Ref, -DepartureTable)
% Called by: emit_ts.pl:13, emit_rust.pl:17, delta_ddl/3:7193, departure_read_sql/3:359
% Calls: table_name/2:309, format/3
% Tests: emit_rust.test.pl
% V7 class: extract
% Parser coupling: none
% Preserved law: name is `__departure_frontier_<table>`; emitted only for listened rels.
% DL7 seam: unchanged.
```

```prolog
% File: v6/prolog/lower.pl:359
% Existing comment: "The naive referee's read of that table: the departed rows in staged order, one occurrence each. Built HERE and not in emit_ts.pl because ... the emitter builds identifiers, never SQL." (354-358)
% Signature: departure_read_sql(+Ref, +Columns, -Sql)
% Called by: emitters
% Calls: departure_frontier_table_name/2:345, storage_row_columns/2:316, quote_ident/2:404
% Tests: emit_rust.test.pl
% V7 class: extract
% Parser coupling: none
% Preserved law: SELECT columns FROM departure frontier ORDER BY "_phase", "_sequence".
% DL7 seam: unchanged.
```

```prolog
% File: v6/prolog/lower.pl:383
% Existing comment: rule-identity header 368-382: ordinal is 1-based in lowering order among statements sharing a head ref; "it moves when two arms of one head are reordered, which is the honest answer"
% Signature: statement_rule_ids(+Program, +HeadRefs, -RuleIds)
% Called by: emit_ts.pl, emit_rust.pl
% Calls: statement_ordinals/3:390, rule_id/4:387
% Tests: query_order_tail.test.pl, plunit_tests.pl
% V7 class: extract
% Parser coupling: none
% Preserved law: "<program>:<name>/<arity>#<ordinal>" with lowering-order ordinals.
% DL7 seam: unchanged.
```

```prolog
% File: v6/prolog/lower.pl:239-247
% Existing comment: none directly (frontier section, plans/2026-08-19-shared-sqlite-frontier.md, header line 137-139)
% Signature: frontier_mode(-Mode); with_frontier_mode(+Mode, :Goal)
% Called by: old_state_relation_sql/4:672, lowered_program_data/3:7760, shared_frontier_guard/2:7699, tests
% Calls: thread-local frontier_mode_option/1 (233)
% Tests: shared_frontier.test.pl
% V7 class: adapt
% Parser coupling: none
% Preserved law: default is per_rel; `shared` is thread-local dynamic state, never a flag.
% DL7 seam: unchanged, but mode must become an explicit input if the thread-local goes.
```

```prolog
% File: v6/prolog/lower.pl:250-270
% Existing comment: "Relation ids are RelPlans order; every door numbers the same way." (249)
% Signature: shared_frontier_relation_id(+Ref, -Id) / (+RelPlans, +Ref, -Id); with_shared_frontier_ids/2:258
% Called by: shared_frontier_view_ddl/3:287, old_state_relation_sql/4:672, relation_write_verbs/6:7784
% Calls: relplan_parts/6, nth0/3
% Tests: shared_frontier.test.pl
% V7 class: adapt
% Parser coupling: none
% Preserved law: id = index in RelPlans; facts valid only inside with_shared_frontier_ids/2.
% DL7 seam: unchanged.
```

```prolog
% File: v6/prolog/lower.pl:7756/7760
% Existing comment: write-verb header 7740-7748: "Every transient write a tick makes is one of six verbs"
% Signature: lowered_program_data(+Plan, -program_data/5); /3 re-entry
% Called by: emitters, shared-frontier consumers
% Calls: lower_program/2, relation_write_verbs/6:7784, rule_write_verbs/3:7823
% Tests: shared_frontier.test.pl
% V7 class: adapt
% Parser coupling: none
% Preserved law: read_staged text is identical in both frontier modes; per-rel names resolve to TEMP views under shared.
% DL7 seam: program_data/5 is the engine's relation/rule write-verb map.
```

```prolog
% File: v6/prolog/lower.pl:7749-7754
% Existing comment: "A relation row carries five of them, a rule row carries recount" (7741-7742)
% Signature: write_verb(?Verb)
% Called by: emitters validating verb sets
% Calls: none
% Tests: shared_frontier.test.pl
% V7 class: extract
% Parser coupling: none
% Preserved law: exactly six verbs: arrive, stage, read_staged, recount, publish, clear.
% DL7 seam: unchanged.
```

```prolog
% File: v6/prolog/lower.pl:3281
% Existing comment: "`__ref_<name>` is a TEMP view used by decode and boundary rendering. It exposes `__id`, typed columns, and a computed `__rendered` expression." (3276-3278)
% Signature: dictionary_table_name(+TypeName, -Table)
% Called by: dictionary_render_expr/3:3331, list_entity_id_lookup/3:1014
% Calls: physical_storage_name/2 (thread-local), atomic_list_concat/3
% Tests: type_relation_ir.test.pl
% V7 class: extract
% Parser coupling: none
% Preserved law: outside a storage context the storage name falls back to the type name.
% DL7 seam: unchanged.
```

```prolog
% File: v6/prolog/lower.pl:3331
% Existing comment: "EXPLAIN receipt (v6/tsv2/tests/structPlane.test.ts): the inner query plans as `SEARCH d USING INTEGER PRIMARY KEY (rowid=?)`, never a SCAN." plus failure-modes entry 52 note (3325-3330)
% Signature: dictionary_render_expr(+TypeName, +Column, -Expr)
% Called by: emitters
% Calls: dictionary_table_name/2:3281, quote_ident/2:404
% Tests: type_relation_ir.test.pl
% V7 class: extract
% Parser coupling: none
% Preserved law: rendering is a correlated subquery on the dictionary view's __rendered, keyed by t.<col>.
% DL7 seam: unchanged.
```

```prolog
% File: v6/prolog/lower.pl:3449/3452
% Existing comment: "The per-type plan the emitter hands the runtime, in TOPOLOGICAL order: children before parents ... Two statements per type per tick, both set-based over one json_each(?) parameter, so the statement count is FLAT ... The N+1 law is structural here" (3441-3447)
% Signature: struct_type_plans(+Decls, +Types, -Plans); /4 with RelPlans
% Called by: emitters
% Calls: type_topological_order/2 (0_type_plane), struct_type_plan/5:3460
% Tests: type_relation_ir.test.pl, plunit_tests.pl
% V7 class: adapt
% Parser coupling: term-shape
% Preserved law: plans are topologically ordered; two set-based statements per type per tick.
% DL7 seam: structtype/7 shape kept.
```

```prolog
% File: v6/prolog/lower.pl:6354-6361
% Existing comment: "The capture types, clause-for-clause with body.pl:json_capture_type/2 (the agreement is pinned by the json_typed_capture plunit unit and, ultimately, by the byte-identical tick-log grade)." (6351-6353)
% Signature: json_capture_json_type(+Type, -JsonTypeName)
% Called by: json capture-type guards:6363
% Calls: throw/1 for unknown types
% Tests: plunit_tests.pl (json_typed_capture unit)
% V7 class: oracle
% Parser coupling: none
% Preserved law: int/float/text/bool map to integer/real/text/boolean; anything else throws.
% DL7 seam: unchanged.
```

```prolog
% File: v6/prolog/lower.pl:1274/1282
% Existing comment: "step g1 SCAFFOLD: the program catalog (rulings.pl:613 catalog_universe)"; "rel_id is dense and positional by construction (catalog_rows/N)" (1280-1281)
% Signature: catalog_ddl_contract(-Name, -Columns); catalog_ddl_key(+Name, -KeyPositions)
% Called by: compile.pl (reads the column contract), set_rel_has_key/5:1335
% Calls: none
% Tests: 6_isolated_compiler_dd.test.pl
% V7 class: adapt
% Parser coupling: none
% Preserved law: `__rel` is the catalog's column contract with rel_id as surrogate key.
% DL7 seam: contract survives; edge-shaped type nodes may add columns.
```

```prolog
% File: v6/prolog/lower.pl:1874
% Existing comment: "! catalog_rows(+ModuleName, +Rules, +RelPlans, -Rows) is det. The relplan carries each column's full type ... The decl half only; the plane half is appended by catalog_all_rows/10." (1871-1873)
% Signature: catalog_rows(+ModuleName, +Rules, +RelPlans, -AllRows)
% Called by: emit_ts.pl, typegen
% Calls: catalog_decl_rows/6:1882 with empty Decls
% Tests: compiler_relations.test.pl
% V7 class: adapt
% Parser coupling: term-shape
% Preserved law: decl-half rows only, byte-stable id layout.
% DL7 seam: row/11 kept.
```

```prolog
% File: v6/prolog/lower.pl:1486
% Existing comment: "catalog_all_rows ... mirror of the lower_program/2 call-site derivations so a plane row exists exactly where its DDL mint site emitted ... outside it the catalog described tables the DDL never created" (1475-1485)
% Signature: catalog_all_rows(+Mode, +ModuleName, +Rules, +RelPlans, +DepartureRefs, +PreRefs, +Types, +RuleLevelStatements, +Decls, -AllRows)
% Called by: catalog_row_ddl/10:1440, emitters
% Calls: project_storage_relplans/3, with_storage_context/2, catalog_decl_rows/6:1882, catalog_plane_rows/9:1535
% Tests: 6_isolated_compiler_dd.test.pl
% V7 class: adapt
% Parser coupling: term-shape
% Preserved law: every plane row's existence condition mirrors its DDL mint site clause for clause (1529-1534).
% DL7 seam: unchanged; requires storage context.
```

```prolog
% File: v6/prolog/lower.pl:1506
% Existing comment: "The public type artifacts do not need executable plane rows, but an idref and a ref deliberately share the target relation's type_id. Their storage child is therefore the semantic discriminator" (1502-1505)
% Signature: catalog_type_rows(+Mode, +ModuleName, +Rules, +RelPlans, +Decls, -Rows)
% Called by: typegen_export.pl, emitters
% Calls: project_storage_relplans/3, catalog_decl_rows/6, catalog_storage_rows/7:1834
% Tests: type_relation_ir.test.pl
% V7 class: adapt
% Parser coupling: term-shape
% Preserved law: storage rows carry the idref-vs-ref discrimination beside the type rows.
% DL7 seam: unchanged.
```

```prolog
% File: v6/prolog/lower.pl:1518
% Existing comment: "Target-independent schema metadata is a parallel catalog stream. The existing row/11 catalog remains the runtime-artifact stream; callers that need authored roles request these normalized rows explicitly." (1514-1517)
% Signature: catalog_type_relation_rows(+ModuleName, +Decls, -Rows)
% Called by: emitters, typegen
% Calls: 0_generic_expand:type_relation_rows/2:188
% Tests: type_relation_ir.test.pl
% V7 class: extract
% Parser coupling: none
% Preserved law: parallel stream; never mutates the row/11 stream.
% DL7 seam: unchanged.
```

```prolog
% File: v6/prolog/lower.pl:1524
% Existing comment: "The artifact transport view is derived from the target-independent rows and catalog column IDs, without changing the existing catalog rows." (1521-1523)
% Signature: catalog_type_transport_rows(+ModuleName, +CatalogRows, +Decls, -Rows)
% Called by: typegen
% Calls: type_relation_rows/2, schema_member_transport_rows/3:189
% Tests: type_relation_ir.test.pl
% V7 class: adapt
% Parser coupling: term-shape
% Preserved law: transport rows derive from the parallel stream joined on catalog column ids.
% DL7 seam: unchanged.
```

```prolog
% File: v6/prolog/lower.pl:1882
% Existing comment: "Rows are the byte-stable decl blocks; Context carries the id layout the plane half needs (module id/hash, the rel and list id maps, and FinalId, the id one past the last decl row)." (1877-1881)
% Signature: catalog_decl_rows(+ModuleName, +Rules, +RelPlans, +Decls, -AllRows, -Context)
% Called by: catalog_rows/4:1874, catalog_all_rows_in_context/10:1494, sweep.pl:38
% Calls: project_catalog_relplans/4:186, rule_bodies_map/2:1404, catalog_path_tree/7:2546, catalog_column_rows/7:2620
% Tests: compiler_relations.test.pl, 6_isolated_compiler_dd.test.pl
% V7 class: adapt
% Parser coupling: term-shape
% Preserved law: ids positional for a byte-stable recompile (1438-1439).
% DL7 seam: ctx/4 layout is the coupling; keep it or re-lay it explicitly.
```

```prolog
% File: v6/prolog/lower.pl:1456
% Existing comment: "Mirror of the lower_program/2 pipeline (dictionaries, the two expands, then level_statement_groups/4) so a caller outside lower_program/2 can plan the same level rows the DDL minted. Faithful because every step is the same predicate." (1451-1455)
% Signature: plan_rule_level_statements(+Plan, -RuleLevelStatements)
% Called by: catalog rail callers outside lower_program/2 (module export, line 165)
% Calls: with_storage_context/2, dictionary_relplans/2:3567, expand_relation_pattern_rules/4:3659, expand_decode_rules/3:3877, level_statement_groups/4:4517
% Tests: plunit_tests.pl
% V7 class: adapt
% Parser coupling: term-shape
% Preserved law: identical steps to lower_program/2, so its level rows match the DDL mint sites exactly.
% DL7 seam: unchanged.
```

```prolog
% File: v6/prolog/lower.pl:5296
% Existing comment: "Bounds HOPS, not rows: a closure is bounded by graph depth, a growing measure passes any depth. Both doors read this number out of the plan." (5293-5295)
% Signature: fixpoint_round_cap(-Cap)
% Called by: level_expand_plan/5:5300, level_dred_plan/5:5391, emitters
% Calls: none
% Tests: plunit_tests.pl
% V7 class: extract
% Parser coupling: none
% Preserved law: one number (1000) for the wavefront hop cap and the stratum-group outer-round cap (header 167-169).
% DL7 seam: unchanged.
```

```prolog
% File: v6/prolog/lower.pl:7027
% Existing comment: "deltastmt's SelectSql is ALSO the tick path's snapshot read, so each emitter appends this onto final_select alone and SelectSql stays byte-identical." (7024-7026)
% Signature: query_order_by_map(+Decls, +RelPlans, -Pairs)
% Called by: emit_ts.pl, emit_rust.pl (both emitters append one definition)
% Calls: query_decl/3 (1_host_expand:192), relplan_shape/6, order_by_sql/3:7038
% Tests: query_order_tail.test.pl
% V7 class: adapt
% Parser coupling: term-shape
% Preserved law: the `?` order tail has one definition; final_select is its only consumer.
% DL7 seam: query decls keep the order-column shape.
```

```prolog
% File: v6/prolog/lower.pl:7112
% Existing comment: "A stored rel's non-leading-key column some rule body compares by identity (==) against a literal or bound var: the composite UNIQUE key can't seek it." (7109-7110); "An inline literal argument compiles to the same WHERE equality as a `== Literal` guard" (7141-7142)
% Signature: audit_scan_index_pairs(+RelPlans, +Rules, +EdgeHeadedRefs, +ArrivalTargets, -Pairs)
% Called by: audit_scan_index_ddls/5:7145, lower_program pipeline:7678
% Calls: audit_scan_index_pair/6:7119, set_rel_key_positions/6:1315
% Tests: plunit_tests.pl (issues/inner-scan-audit pin, header 173-175)
% V7 class: adapt
% Parser coupling: term-shape
% Preserved law: derived (rel, column) pairs are sorted and deduplicated.
% DL7 seam: unchanged.
```

```prolog
% File: v6/prolog/lower.pl:7145/7152
% Existing comment: see audit_scan_index_pairs block above
% Signature: audit_scan_index_ddls(+RelPlans, +Rules, +EdgeHeadedRefs, +ArrivalTargets, -Ddls); audit_scan_index_ddl(+Ref, +Column, -Ddl)
% Called by: lower_program pipeline:7678, emitters
% Calls: audit_scan_index_pairs/5:7112, table_name/2, quote_ident/2
% Tests: plunit_tests.pl
% V7 class: adapt
% Parser coupling: none
% Preserved law: index name is `<table>__scan_<column>`.
% DL7 seam: unchanged.
```

```prolog
% File: v6/prolog/lower.pl:435 (family head; exports none, but every statement family depends on it)
% Existing comment: "EXPRESSION LIFT: a Bound entry is now typed(Sql, int|text), not bare Sql" (427-434)
% Signature: compile_pattern_arg(+Mode, +Arg, +ColumnExpr, +ColumnType, +Bound0, -Bound, -WhereParts, +Binding)
% Called by: compile_atom_args:761, compile_negative_atom_args:816, compile_coalesced_args:609, seeded_pre_args:731
% Calls: column_encoding/3:2950, compile_sub_args/8:501, where_text/3:518, bound_lookup/3:515
% Tests: run_sql_check.pl, body-compilation units in compile/test/
% V7 class: adapt
% Parser coupling: term-shape
% Preserved law: a shared variable across two columns of different storage type is the named throw join_column_type_mismatch (470-496); the '1'-vs-1 affinity join is refused (471-487, measured sqlite 3.45.1).
% DL7 seam: pattern args stay term-shaped; literal spellings change.
```

```prolog
% File: v6/prolog/lower.pl:4517 (level_statement_groups/4; head 4517, group 4532)
% Existing comment: "acyclic-by-construction level recompute (UNCHANGED from round 1)" header 107-114: engine re-derives a stratum GROUP to a joint fixpoint; sql_rule_order/2 topo-sorts within it; a genuine positive cycle is refused at strat.pl:topo_order_group/2
% Signature: level_statement_groups(+Mode, +BodyRelPlans, +DecodedRuleOrder, -RuleLevelStatements)
% Called by: lower_program_in_context:7635, plan_rule_level_statements_in_context:1462
% Calls: group_adjacent_by_head/3:4521, level_statement_group/3:4532, level_aggregate_sql/6:4602, level_ref_count_sql/5:5110
% Tests: plunit_tests.pl, shared_frontier.test.pl
% V7 class: adapt
% Parser coupling: term-shape
% Preserved law: levelstmts are in execution order (strat.pl:sql_rule_order/2), already ordered for the emitter.
% DL7 seam: levelstmt/7 kept.
```

```prolog
% File: v6/prolog/lower.pl:6987 (delta_statement/2, section header 6972)
% Existing comment: "delta statements (round 2: one plain \"read every row\" query per rel" (6972); "SelectAllSql preserves the recompute referee. DeltaTable and BoundarySql carry P1's tick-local change stream." (17-20)
% Signature: delta_statement(+Mode, +RelPlan, -deltastmt/5)
% Called by: lower_program_in_context:7671
% Calls: table_name/2, delta_table_name/2:322, storage_row_columns/2
% Tests: run_sql_check.pl, emit tests
% V7 class: adapt
% Parser coupling: none
% Preserved law: one plain read-every-row SELECT per rel; tick numbering never appears.
% DL7 seam: unchanged.
```

```prolog
% File: v6/prolog/lower.pl:4019 (arrival_statement/2 family, section 4017)
% Existing comment: "arrival statement templates (round 2: Log rel drops tick/seq params)" (4017)
% Signature: arrival_statement(+RelPlan, -arrivalstmt/6) (mode/shape clauses 4019-4080)
% Called by: lower_program_in_context:7602
% Calls: set_arrival_sql_parts/5:4081, incremental_arrival_add_sql/3:4102, placeholders/2:4121
% Tests: 2_subscribe.plt, run_sql_check.pl
% V7 class: adapt
% Parser coupling: none
% Preserved law: keyed arrival targets use a replace insert and stage explicit minus rows from the keys read before the write (98-105).
% DL7 seam: unchanged.
```

```prolog
% File: v6/prolog/lower.pl:4154 (edge_statements_for_rule/3, section 4123)
% Existing comment: edge-rule lowering header: ProjectSql reuses compile_head_expr/head_select_list UNCHANGED with a numbered-placeholder Bound; UpsertSql is INSERT ... ON CONFLICT ... DO UPDATE SET ... = excluded.... (58-72)
% Signature: edge_statements_for_rule(+Mode, +EdgeHeadedRefs, +RelPlans, +EdgeRule, -EdgeStatementGroup)
% Called by: lower_program_in_context:7611
% Calls: edge_statement_single/6:4238, compile_trigger_bound/2:4481, edge_delta_project_sql/3:4372, check_edge_decode_sources/2:4417
% Tests: 2_subscribe.plt, door_split_trigger_literal.pl fixture
% V7 class: adapt
% Parser coupling: term-shape
% Preserved law: one edgestmt per arm; the keyed(Ref, Positions) latent bug class stays fixed by indexing KeyColumns off HeadColumns (74-81).
% DL7 seam: unchanged.
```

```prolog
% File: v6/prolog/lower.pl:3677-4016 (relation-pattern + decode expansion pair)
% Existing comment: "Relation TERMS become dictionary atoms before decode/2 does, so a variable this pass introduces is already a legal decode source" (7625-7628)
% Signature: expand_relation_pattern_rules/4:3659; expand_decode_rules/3:3877
% Called by: lower_program_in_context:7629,7632; plan_rule_level_statements_in_context:1468,1470
% Calls: dictionary_relplans/2:3567, rewrite_relation_goals/3:3759, decode_pattern_atoms/3:3966, check_edge_decode_sources/2:4417
% Tests: 4_braced_nested_relations.test.pl, anonymous_{product,sum}_values.test.pl, 5_remove_rel_is.test.pl
% V7 class: adapt (drop-side: decode/2 and relation-pattern spellings are DL6 surface)
% Parser coupling: term-shape
% Preserved law: the two expands are order-fixed and composable only in this order.
% DL7 seam: cons-tree source may replace the rewrite with direct lowering; the dictionary-atom invariant is what to keep.
```
