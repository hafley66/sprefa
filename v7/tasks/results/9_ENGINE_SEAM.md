# DL7 engine seam result

Date: 2026-08-29
Base inspected: `1f1a67a30456482624d15f8e28e3d6a8b75f7e8b`
Status: contract complete; no Rust engine source change is required.

## Exact existing door

The Rust serde input is `sprefa_engine_rs::types::ProgramJson`, defined at
`v6/sprefa-engine-rs/src/types.rs:716-764`. It is a single JSON object. Its
fields are copied into `sprefa_engine_rs::program::GenProgram` by
`GenProgram::from_checked_json` at `v6/sprefa-engine-rs/src/program.rs:98-130`.

The loader path is:

```text
run::load_program(path)                                  [FS]
  -> read_to_string(path)                                [FS]
  -> run::load_program_text(module_text)
       -> run::program_json_text(module_text)
          extracts the bytes between r#" and "#;
       -> serde_json::from_str::<ProgramJson>(json)
       -> GenProgram::try_from_json(document)
          -> GenProgram::from_checked_json(document)
```

The loader signatures are `load_program(&Path) -> Result<LoadedProgram>` at
`v6/sprefa-engine-rs/src/run.rs:59-63`,
`load_program_text(&str) -> Result<LoadedProgram>` at lines 51-57, and
`program_json_text(&str) -> Result<&str>` at lines 39-49. The input path is a
generated Rust module, not a standalone JSON file. The module contains
`pub const PROGRAM_JSON: &str = r#"..."#;`.

The existing command is `dl6 run <source>` in
`v6/sprefa-engine-rs/src/bin/dl6.rs`. `Verb::Run` dispatches to `run` at
lines 591-603. `prepare` compiles or loads the cached generated module,
invokes `run::load_program` at lines 466-512, and the command folds it through
`run::run_once` at lines 540-568. The current compile leg is V6-specific:
`Dl6Compiler::program` at lines 146-153 invokes
`v6/prolog/emit_rust.pl`.

The artifact writer is `emit_rust:emit_program/5` at
`v6/prolog/emit_rust.pl:598-706`. It assembles the JSON object at lines
655-684 and wraps it in the Rust module consumed by `run::load_program`.

## Version source

The runtime gate is `pub const IR_VERSION: u32 = 1` at
`v6/sprefa-engine-rs/src/program.rs:53-55`. `GenProgram::try_from_json` at
lines 80-88 rejects any `ProgramJson.ir_version` other than that value.

The existing emitter source is `ir_version(1)` at
`v6/prolog/emit_rust.pl:37-40`; it writes the `ir_version` field at lines
655-658. `emit_ts.pl` carries a second copy. The V7 adapter emits `1` while
the meaning of the existing fields stays unchanged. Bumping the value would
be a contract revision, not a required engine edit.

## Minimum ProgramJson document

Serde requires these top-level fields because they have no `serde(default)` on
`ProgramJson`:

```text
name
intern_mode
ddl
rel_columns
rel_column_types
arrival_targets
boot
final_select
arrival_templates
enum_ref_columns
relations
edges
levels
retentions
reconcile_every_tick
```

`ir_version` has a serde default, but it must be present with value `1` for
the runtime gate to pass. Therefore the minimum inert artifact has the 16
keys below, with empty maps or lists where shown:

```json
{
  "name": "inert",
  "ir_version": 1,
  "intern_mode": "direct",
  "ddl": [],
  "rel_columns": {},
  "rel_column_types": {},
  "arrival_targets": [],
  "boot": [],
  "final_select": {},
  "arrival_templates": {},
  "enum_ref_columns": {},
  "relations": [],
  "edges": [],
  "levels": [],
  "retentions": [],
  "reconcile_every_tick": false
}
```

The fields with serde defaults can be omitted from that inert JSON:
`text_intern_plan`, `struct_types`, `struct_ref_columns`, `enum_types`,
`pre_snapshot_rels`, `uses_tick`, `incremental_safe`, `host_plans`, and
`queries`. The canonical emitter may continue emitting them as empty values.
`bind_plans` is emitted by the V6 Prolog writer but is absent from the Rust
struct and is silently ignored by serde because unknown fields are allowed.

For one arrival row, the same top-level fields remain present. Add one
`rel_columns` entry and one matching `rel_column_types` entry, one relation
name in `arrival_targets`, one `arrival_templates` entry, and one
`relations` entry. The relation entry must carry every required
`IncrementalRelationPlan` field from
`v6/sprefa-engine-rs/src/types.rs:456-475`:

```text
rel, kind, table_name, delta_table_name, frontier_table_name,
next_frontier_table_name, departure_frontier_table_name, columns,
column_types, key_indices, arrival_add_sql, arrival_del_sql, boundary_sql
```

The `ddl` list must create the durable table and each transient table named by
the relation plan. `edges`, `levels`, and `retentions` may remain empty for a
single base relation. `final_select` needs a relation-to-SQL entry only when
the caller requests final rows. The engine probes the transient table names in
`incremental::TickWork::probe` and uses `arrival_add_sql`,
`arrival_del_sql`, and `boundary_sql` during the per-row fold.

## V7 adapter signature

The smallest V7-facing conversion signature is:

```prolog
checked_datalog_to_program_json(
    +checked_datalog(
        root_graph(GraphNodes, GraphEdges),
        datalog_program(Relations, Seeds, Rules),
        Depends,
        Strata),
    -ProgramJson
).
```

`ProgramJson` is the plain object with the field names above. A surrounding
writer can then produce the existing Rust module text containing
`PROGRAM_JSON`; that wrapper is an artifact-format step, not a V7 parser
boundary.

The signature imports only the V7 checked representation from
`v7/src/2_comptime/0_compiler.pl:327-358` and the output object contract. It
does not accept or expose V6 parser or compiler terms such as `prog/2`,
`plan/9`, `lowered/8`, `fixture/5`, `rel/5`, or the `<-` rule syntax.

The checked input alone carries logical graph rows, relation declarations,
ground seeds, rules, positive dependencies, and strata. It does not carry SQL
DDL, physical table names, column-type maps, arrival templates, final selects,
or incremental statement vectors. The adapter therefore needs a V7-owned
physical planning step inside its implementation before it can populate
`ProgramJson`. This is a compiler-side projection requirement.

## Call and data flow

```text
compile_dl7/4
  -> checked_datalog/4
       (graph, relation/arity rows, seeds, rules, dependencies, strata)
  -> checked_datalog_to_program_json/2                    [V7 adapter]
       (ProgramJson object plus V7-generated SQL plan fields)
  -> Rust module wrapper with PROGRAM_JSON
  -> run::load_program/1                                  [FS]
  -> ProgramJson serde decode
  -> GenProgram::try_from_json/1                          [ir_version gate]
  -> driver::run_schedule/4
       -> SqliteSeam::run_program_ddl/2                   [DB]
       -> run_boot/2                                      [DB]
       -> GenProgram::run_tick/2 per scheduled batch     [DB]
```

`compile_dl7/4` is defined at `v7/src/2_comptime/1_type_compiler.pl:11-25`.
Its successful runtime value is retained as
`checked_datalog/4` at lines 65-78. No V7 engine runner or ProgramJson emitter
exists in the inspected tree yet.

## Engine-change ruling and blockers

Required Rust engine source changes: 0. The existing engine accepts the
document through `ProgramJson`, applies the existing `ir_version` gate, and
executes the existing `GenProgram` plan. The V7 work belongs on the compiler
side of the seam.

The remaining implementation work is bounded to a V7 physical planner and
ProgramJson/module writer. The existing `dl6` CLI also has V6 compiler wiring,
so a V7 command path must either re-point that compile leg or invoke the
existing loader after a V7 artifact has been written. Neither item requires a
change under `v6/sprefa-engine-rs/src`.

## Verification

Read-only inspection commands only. No SWI, Rust, cargo, generated-corpus,
formatting, lint, or test suite command ran.

Acceptance criteria:

- [x] Exact Rust type, loader, command, and `ir_version` sources named.
- [x] Minimum inert and one-row ProgramJson fields named.
- [x] Zero Rust engine source changes confirmed; compiler and CLI integration
      work is recorded as the remaining boundary work.
- [x] V7-only adapter signature defined from `checked_datalog/4`.
- [x] This report written to `v7/tasks/results/9_ENGINE_SEAM.md`.
