# DL7 one-relation layout planner blocker

Date: 2026-08-29

Status: blocked; `issues/dl7-layout-planner` remains open.

## Boundary correction

The target-neutral layout planner owns relation selection, artifact roles,
ordered columns, semantic types, encoded representations, and keys. ProgramJson
owns target table names, `RowColumnType`, DDL, add/delete/boundary statements,
and boot versus arrival seed placement. The latter fields remain blocked, but
they block `@dl7-program-json-writer` through
`@dl7-program-json-rulings`, rather than expanding the layout graph with SQLite
policy.

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

| Owner | Required field | Exact missing V7 row or signature | Competing choices left unresolved |
| --- | --- | --- | --- |
| Layout | Stored-relation selection and `kind` | `relation_storage_kind(+Relation, -Kind)` | `set`; `log`; a V7 declaration classifier distinct from every product and kernel relation |
| Layout | `key_indices` | `relation_key_indices(+Relation, -KeyIndices)` | `[]`; all ordered columns; explicit authored key positions |
| Layout | artifact role | `layout_artifact(+Relation, -ArtifactRole)` | current state; event/log; history; transient frontier |
| Layout | encoded representation | `layout_column_representation(+SemanticType, -Representation)` | target-neutral scalar, reference, list, and type-ID forms |
| ProgramJson | `column_types` | `program_json_column_type(+Representation, -RowColumnType)` | mappings among `text`, `int`, `float`, `bool`, `json`, `ref`, `relation_id`, `list`, and `bytes` |
| ProgramJson | `rel` and table names | `program_json_relation_name(+LayoutRelation, -Name)` | quoted semantic spelling; delimiter encoding; deterministic generated name |
| ProgramJson | DDL and statements | `program_json_relation_ddl/2`; `program_json_relation_statements/4` | durable/transient SQL and arrival add/delete/boundary protocols |
| ProgramJson | authored seed placement | `program_json_seed_placement(+LayoutRelation, +Seeds, -BootOrArrivalPlan)` | `boot`; arrival DTO/template rows; both |

The semantic-plan contract permits a target adapter to derive SQL DDL from a
target-neutral layout graph (`v7/design/0_KERNEL_RECONCILIATION.md:346-349`),
but does not settle any row in this table. Its decisions list explicitly leaves
the first V7 plan schema and ProgramJson mapping required
(`v7/design/0_KERNEL_RECONCILIATION.md:395-401`).

## Stop condition

No production module or test arm was added. No SWI test command ran because
the blocker was reached before source or test edits. No V6, Rust, TypeScript,
prelude, parser, compiler, ProgramJson writer, or engine-command file changed.
