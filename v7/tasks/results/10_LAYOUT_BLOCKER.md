# DL7 one-relation layout planner blocker

Date: 2026-08-29

Status: blocked; `issues/dl7-layout-planner` remains open.

## Checked input inventory

The only planner input signature on this card is:

```prolog
build_layout(+CheckedDatalog, -Layout, -Diagnostics).
```

The checked value retained by `compile_unit/3` is:

```prolog
checked_datalog(
    root_graph(Nodes, Edges),
    datalog_program(Relations, Seeds, Rules),
    Depends,
    Strata
)
```

`Relations` carries `relation(ref(Relation), Arity)`. `Edges` carries ordered
`':'(Owner, Name, Type, Index)` rows. `Seeds` carries ground
`call(ref(Relation), Arguments)` rows. The lowerer creates relation rows for
every product constructor, and the checker appends the kernel relation rows;
neither form has storage classification, key metadata, physical identity, or
durability metadata.

Sources: `v7/src/2_comptime/0_lowerer.pl:104-143`,
`v7/src/2_comptime/1_checker.pl:32-41,152-178,314-324`, and
`v7/src/2_comptime/2_compiler.pl:66-79`.

## Required engine fields

The existing `IncrementalRelationPlan` requires:

```text
rel, kind, table_name, delta_table_name, frontier_table_name,
next_frontier_table_name, departure_frontier_table_name, columns,
column_types, key_indices, arrival_add_sql, arrival_del_sql, boundary_sql
```

`kind` has the existing alternatives `set` and `log`; `column_types` has the
existing alternatives `text`, `int`, `float`, `bool`, `json`, `ref`,
`relation_id`, `list`, and `bytes`. The one-relation ProgramJson contract also
requires DDL for the durable table and every named transient table.

Sources: `v6/sprefa-engine-rs/src/types.rs:5-19,456-482,716-748` and
`v7/tasks/results/9_ENGINE_SEAM.md:113-131`.

## Missing rows and policy forks

| Required layout field | Exact missing V7 row or signature | Competing choices left unresolved |
| --- | --- | --- |
| Stored-relation selection and `kind` | `relation_storage_kind(+Relation, -Kind)` | `set`; `log`; a V7 declaration classifier distinct from every product and kernel relation |
| `key_indices` | `relation_key_indices(+Relation, -KeyIndices)` | `[]`; all ordered columns; explicit authored key positions |
| `column_types` | `physical_column_type(+SemanticType, -RowColumnType)` | V7 primitive-to-boundary mappings among `text`, `int`, `float`, `bool`, `json`, `ref`, `relation_id`, `list`, and `bytes`; the current `primitive(any)` and `primitive(type)` have no mapping |
| `rel` and all table-name fields | `physical_relation_name(+Relation, +Target, -Name)` | quoted semantic spelling; delimiter encoding; deterministic identity hash or generated name |
| DDL durability and physical SQL types | `physical_table_ddl(+RelationLayout, -DdlStatements)` | durable base table plus transient companions required by the engine, with backend SQL types and CREATE form derived from the unresolved name and type rows |
| arrival add/delete and boundary SQL | `physical_relation_statements(+RelationLayout, -ArrivalAddSql, -ArrivalDelSql, -BoundarySql)` | key-based upsert or row-set arrival; key-based delete or complete-row delete; boundary statement over the selected companion-table protocol |
| authored seed placement | `seed_placement(+Relation, +Seeds, -BootOrArrivalPlan)` | `boot` statements; arrival DTO/template rows; both with distinct engine fields |

The semantic-plan contract permits a target adapter to derive SQL DDL from a
target-neutral layout graph (`v7/design/0_KERNEL_RECONCILIATION.md:346-349`),
but does not settle any row in this table. Its decisions list explicitly leaves
the first V7 plan schema and ProgramJson mapping required
(`v7/design/0_KERNEL_RECONCILIATION.md:395-401`).

## Stop condition

No production module or test arm was added. No SWI test command ran because
the blocker was reached before source or test edits. No V6, Rust, TypeScript,
prelude, parser, compiler, ProgramJson writer, or engine-command file changed.
