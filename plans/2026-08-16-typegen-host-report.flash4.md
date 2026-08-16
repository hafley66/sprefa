# typegen-for-host-engines — investigation report (flash4 arm)

Date: 2026-08-16. Lane: `chore/typegen-host-report-flash4`, ff-only on `320464bf`.

Question: can the generated per-program types for TS and Rust be made to work
FOR the host engines themselves — `v6/tsv2` (TS runtime/serve/cli) and
`v6/sprefa-engine-rs` (Rust runtime)?

## TOC

1. [What the artifact is](#1-what-the-artifact-is)
2. [Q1 — tsv2 rel-row representation at its runtime seams](#q1--tsv2-rel-row-representation-at-its-runtime-seams)
3. [Q2 — sprefa-engine-rs Row/Value/Arrival and relation declarations](#q2--sprefa-engine-rs-rowvaluearrival-and-relation-declarations)
4. [Q3 — what already consumes a generated artifact](#q3--what-already-consumes-a-generated-artifact)
5. [Q4 — the shape question: per-program vs program-generic](#q4--the-shape-question-per-program-vs-program-generic)
6. [Q5 — forks needing Chris](#q5--forks-needing-chris)

## 1. What the artifact is

Per-program TS interfaces / Rust structs, one per relation of one compiled
dl6 program. Emitted by the prolog doors and mirrored by the dl6 doors, both
fed by the same `type_row/7` JSONL.

| producer | path | role |
|---|---|---|
| prolog TS emitter | `v6/prolog/compile/7_emit_ts_types.pl:15` | `emit_ts_types/3` writes `<name>.types.ts` |
| prolog Rust emitter | `v6/prolog/compile/8_emit_rust_types.pl` (head, `rust_types_text/3`) | writes `<name>.types.rs` |
| JSONL door | `v6/prolog/compile/typegen_export.pl:23` | `dump_type_rows/2` → `type_row/7` JSONL |
| dl6 TS door | `v6/dl/typegen/render_ts.dl6:211` | `rendered_type/4` reassembles the `.ts` |
| dl6 Rust door | `v6/dl/typegen/render_rust.dl6:215` | reassembles the `.rs` |
| sweep output | `v6/prolog/compile/out/*.types.ts`, `*.types.rs` | 338 each measured in `out/`; 700 tree-wide (gitignored) |

One sample of each, same program, `v6/prolog/compile/out/a_two_row_parent_cycle_is_rejected.types.ts:1`:

```
export interface Node {
  node_id: number;
  name: string;
}
```

```
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Node {
    pub node_id: i64,
    pub name: String,
}
```

Mapping table (TS / Rust):

| kind | TS (`7_emit_ts_types.pl`) | Rust (`render_rust.dl6`) |
|---|---|---|
| int / float | `number` (:118, :119) | `i64` / `f64` (:27, :29) |
| text | `string` (:120) | `String` (:27) |
| bool | `boolean` (:121) | `bool` (:30) |
| json | `unknown` (:122) | `serde_json::Value` (:31) |
| list / json_list | `Array<T>` (:124-131) | `Vec<T>` (:139-141) |
| option | `T | null` (:132-134) | `Option<T>` (:125-129) |
| ref → rel | target rel's PascalCase name (:135-136) | `emitted_type_name` (:32) |

## Q1 — tsv2 rel-row representation at its runtime seams

The host shuttles rows as **flat, untyped value arrays** everywhere. Column
names and column types ride as **data** on the program plan, never as
compile-time types.

| seam | representation | path:line |
|---|---|---|
| one row | `IRow = readonly IRowValue[]` | `v6/tsv2/runtime/types.ts:40` |
| one value | `IRowValue = IRowScalar | IRowValueArray`; scalar = `string|number|boolean` | `types.ts:17`, `:22` |
| column type | `IRowColumnType = "text"|"int"|"bool"|"float"|"ref"|"json"|"list"` | `types.ts:37` |
| program plan carries names+types as data | `IGenProgram.rel_columns`, `.rel_column_types` | `types.ts:511-512` |
| read seam shapes SELECT → `IRow[]` by declared order | `select_rows` in `v6/tsv2/runtime/rows.ts:75` | `rows.ts:75-92` |
| emitted module `tick(seam, arrivals)` | `IGenProgram.tick` | `types.ts:514` |
| emitted module imported at runtime | dynamic `import(module_path)` | `v6/tsv2/serve/0_compile.ts:111` |
| fold drives the generic tick | `TickFold.run` calls `program.tick(seam, arrivals)` | `v6/tsv2/runtime/tickLoop.ts:55` |
| host input/output columns | `IHostColumnPlan {name, type}` | `types.ts:621-626` |
| host output decode | declared output shapes, `IHostPlan.outputs` | `types.ts:630-641` |

Served endpoints that cross the boundary:

| route | body shape | path:line |
|---|---|---|
| `POST /edb/events` | `{batch:[{rel,sign,row}]}` row = `IRowValue[]` | `v6/tsv2/serve/4_http.ts:301` (`check_arrival_body`) |
| arrival validation is type-driven by DATA | reads `program.rel_columns` + `rel_column_types` | `4_http.ts:325`, `:332` |
| `GET /idb/:rel` | `{rows}` where rows = `IRow[]` | `4_http.ts:374-378` |
| host responses re-enter as ordinary arrivals | `__host_response_*` → `submit` | `v6/tsv2/serve/1_hosts.ts:6-12` |

**Where a generated interface plugs in.** Three candidate points, none of
which is the hot per-tick path:

1. `POST /edb/events` payload typing — `check_arrival_body` is the one trust
   boundary (`4_http.ts:236-247`) and already walks `rel_columns`/`rel_column_types`
   by hand. A generated `ArrivalRow<Rel>` would replace the hand-rolled column
   loop at `4_http.ts:325-338`.
2. `GET /idb/:rel` response — rows are returned raw at `4_http.ts:376`;
   a generated `Row<Rel>` would type `{rows: Row<Rel>[]}` instead of `IRow[]`.
3. Host input/output decoding — `IHostColumnPlan`/`IHostPlan` (`types.ts:621-641`)
   carry names+types as data; the template fill in `1_hosts.ts:131-150` splices
   untyped `IRowValue`.

## Q2 — sprefa-engine-rs Row/Value/Arrival and relation declarations

Same flat shape as tsv2, mirrored.

| seam | representation | path:line |
|---|---|---|
| one row | `pub type Row = Vec<Value>` | `v6/sprefa-engine-rs/src/types.rs:137` |
| one value | `enum Value {Integer, Real, Bool, Text, List}` | `types.rs:22-34` |
| arrival | `struct Arrival {rel, sign, row}` | `types.rs:139-144` |
| column type | `enum RowColumnType` (7 variants, `rename_all="lowercase"`) | `types.rs:6-16` |
| sql result | `QueryResult.rows = Vec<Vec<Value>>` | `types.rs:195-200` |
| read/normalize seam | `result_rows` in `v6/sprefa-engine-rs/src/sql.rs:149` | `sql.rs:149-172` |

Relation declarations represent columns as **parallel name + type arrays**:

| source | columns (names) | column types | path:line |
|---|---|---|---|
| dep_resolve | `DEP_REPO_COLUMNS`, `DEP_EDGE_COLUMNS`, … | parallel `RowColumnType` arrays | `v6/sprefa-engine-rs/src/dep_resolve.rs:11-14`, `:38-66` |
| source_bind | `REPO_COLUMNS` … `SPECIFIER_COLUMNS` | parallel arrays | `v6/sprefa-engine-rs/src/source_bind/_0_types.rs:5-10`, `:43-87` |
| emitted program | `GenProgram.rel_columns`/`rel_column_types` (HashMaps, data) | `v6/sprefa-engine-rs/src/program.rs:23-24` | `program.rs:19-44` |

Arrivals are built by hand-mapping domain structs to `Value` vectors:

| source | hand mapping | path:line |
|---|---|---|
| dep_resolve outcome → arrivals | `DepResolveOutcome::arrivals` | `dep_resolve.rs:220-268` |
| source bind → arrivals | `source_row` | `source_bind/_0_types.rs:137-143` |

**Where a generated Rust struct plugs in.**

| point | today | generated-struct shape |
|---|---|---|
| SqlRunner read seam | `result_rows` returns `Vec<Vec<Value>>` (`sql.rs:149`) | `serde_json`/`TryFrom<Value>` → generated structs at `sql.rs:149-172` |
| source_bind / dep_resolve → arrivals | hand `Value` vectors (`dep_resolve.rs:220`, `_0_types.rs:137`) | generated struct → `row` conversion |
| emitted harness | `GenProgram::from_json` (`program.rs:47`) deserializes `ProgramJson`; rows stay `Vec<Value>` | typed views over `Row` |

The emitted `.program.rs` fixture embeds `PROGRAM_JSON` as a raw string and
deserializes `ProgramJson` (`emit_rust_harness.rs:60-62`), then converts
schedule JSON values by hand to `Value` (`emit_rust_harness.rs:25-39`,
`value_from_json`). That conversion is a per-program-untyped seam today.

## Q3 — what already consumes a generated artifact

**Nothing.** No file imports, `include!`s, or `d.ts`-references any
`*.types.ts` / `*.types.rs` artifact.

| check | result | path:line |
|---|---|---|
| tsv2 imports of generated type files | none; the only `types.ts` imports are `runtime/types.ts` | `rows.ts:16`, `tickLoop.ts:20`, `serve/0_compile.ts:15` |
| engine-rs `include!` / build.rs | none; `Cargo.toml`/`build.rs` absent reference (no build.rs exists) | checked, no matches |
| emitted program module references its own `.types.ts` | no; the emitted `.ts` imports only `runtime/*` names (`IRow`, `IRowColumnType`) | `v6/prolog/compile/out/a_two_row_parent_cycle_is_rejected.ts:37-38` |
| emitted `.program.rs` references its `.types.rs` | no; embeds `PROGRAM_JSON` only | `v6/sprefa-engine-rs/tests/fixtures/live_shell_probe.program.rs:1` |

The only reader of the artifacts is the golden gate itself —
`v6/prolog/compile/test/typegen_golden.sh:120-173` diffs the dl6 render and
the prolog render against committed goldens. The sweep produces
`out/*.types.ts|.rs` (`v6/prolog/sweep.pl`) but nothing below the sweep reads
them back.

**The half-way-there fact.** The type info the host needs already crosses at
runtime **as data**: `IGenProgram.rel_columns` + `rel_column_types`
(`types.ts:511-512`) and `GenProgram.rel_columns`/`rel_column_types`
(`program.rs:23-24`) carry every relation's column names and `IRowColumnType`
tags. The served arrival validator and the emitted tick both read them
(`4_http.ts:325`, `rows.ts:75`). What does not exist is the compile-time
interface; the runtime spelling is the flat `IRow`/`Row`.

## Q4 — the shape question: per-program vs program-generic

The hosts are program-generic: `IGenProgram.tick`/`GenProgram::run_tick` run
arbitrary dl6 (`tickLoop.ts:55`, `program.rs:84`). Compile-time program types
cannot type the host core, because the core does not know which program it is
running until a module/JSON is loaded at runtime. Per-program types bind only
at seams where the program is already pinned.

| seam | what generation step would wire it | what stays untyped | size |
|---|---|---|---|
| served `POST /edb/events` payload (`4_http.ts:301`) | compiler emits a per-program `.types.ts` next to the served module in `gen_served/`; validator casts `batch` to generated `Arrival<Rel>` | tick internals | small |
| served `GET /idb/:rel` (`4_http.ts:374`) | same artifact; `{rows: Row<Rel>[]}` | the SQL text | small |
| host input/output columns (`types.ts:621`, `1_hosts.ts:131`) | generated interfaces for `__host_demand_*`/`__host_response_*` | template quoting, spawn | small |
| emitted harness program (rust) (`emit_rust_harness.rs:25`) | emit `include!` of a `.types.rs` module beside the `.program.rs`; `value_from_json` → typed struct | driver seam | medium |
| engine-rs source_bind / dep_resolve arrivals (`dep_resolve.rs:220`) | hand-built generated structs for `source_file`/`dep_*` rels | crawl internals | medium |
| SqlRunner read seam (`sql.rs:149`) | `serde`/`TryFrom<Value>` → generated struct at `result_rows` | boundary normalization | medium |
| golden gate scripts (`typegen_golden.sh:120`) | already consumes the rendered text | — | small |
| client-side consumers | publish the `.types.ts` alongside a served program | — | small |

Sizing rationale. The served-route and host-column seams are small because the
program is already loaded as `IServedProgram` and the type data
(`rel_columns`/`rel_column_types`) is already in hand at exactly those points;
the generated interface just replaces a hand-rolled walk. The rust seams are
medium because they sit below the program module (the rust core loads a JSON
document, not a typed module) so the generated structs must be threaded through
a conversion layer (`value_from_json` at `emit_rust_harness.rs:25`,
`result_rows` at `sql.rs:149`).

Constraint to note: the artifacts render **author rels only** — `minted_rel`
filters the `__`-namespace compiler rels (`render_ts.dl6:40-44`,
`render_rust.dl6:43-47`), and `renderable_rel` keeps a minted rel only when it
has a `concrete_type` (`render_ts.dl6:56-64`). The rels the host actually
shuttles at runtime — `__host_demand_*`, `__host_response_*`,
`__delta_*`, `__frontier_*` (`1_hosts.ts:6-12`, `program.rs` DDL) — are minted
and therefore **excluded from the artifact**. Any host-seam typing must either
relax that filter or accept that the typed surface covers author rels only.

## Q5 — forks needing Chris

| # | fork | citation proving it exists |
|---|---|---|
| 1 | minted-rel exclusion: typegen skips `__` rels, but the host shuttles exactly those (`__host_demand_*`, `__host_response_*`). Decide whether host-seam typing extends the renderer to minted rels or accepts author-only coverage. | `render_ts.dl6:40-44`, `render_rust.dl6:43-47`, `1_hosts.ts:6-12` |
| 2 | duplicate truth: the runtime already carries `rel_columns`/`rel_column_types` as data (`types.ts:511`, `program.rs:23`). Decide whether generated interfaces coexist with that data map or replace it; the host core is program-generic, so the data map is the program-generic mechanism. | `types.ts:511-512`, `4_http.ts:325`, `program.rs:23-24` |
| 3 | nested-vs-flat: a `ref` column renders as a nested struct (`Tree.site: Patch`, `Patch.at: Plot`) while the boundary `IRow`/`Row` is a flat `IRowValue[]`/`Vec<Value>` and a ref crosses as "a scalar (canonical text) or any JSON value". The generated nested interface does not structurally type the flat wire row. Decide where the type graph's shape question is answered. | `declaration_order_preserves_struct_refs.types.ts:1-14`, `types.ts:17-40`, `4_http.ts:270-280` |

No other real forks surfaced; the remaining seams are size/sequencing choices
within Q4, not decisions.
