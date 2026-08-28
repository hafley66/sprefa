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
