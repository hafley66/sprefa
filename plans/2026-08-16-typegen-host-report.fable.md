# Typegen for the host engines — fable arm (in-process subagent, 26 tool uses, ~194s)

## 1. tsv2: how a rel row exists at runtime

| seam | representation | citation |
|---|---|---|
| row value | `IRowValue = IRowScalar \| IRowValueArray` | `v6/tsv2/runtime/types.ts:17` |
| row | `IRow = readonly IRowValue[]` — positional, no column names | `v6/tsv2/runtime/types.ts:40` |
| arrival | `IArrivalRow {rel, sign, row}` addressed by rel name string | `v6/tsv2/runtime/types.ts:51-55` |
| program contract | `IGenProgram` carries `rel_columns` and `rel_column_types` as DATA, never as types | `v6/tsv2/runtime/types.ts:511-512` |
| read seam | `ISelectRows` returns `IRow[]` shaped by a runtime `columns` array | `v6/tsv2/runtime/types.ts:1050-1057` |
| emitted-template path | `gen_emitted/*.ts` imports only generic runtime types | `v6/tsv2/gen_emitted/bounded_template_ground_instance.ts:28-40` |
| serve read | `GET /idb/:rel -> {rows: IRow[]}` | `v6/tsv2/serve/4_http.ts:8`, `:374-376` |
| serve write | `POST /edb/events` validated per value at runtime by `column_problem` | `v6/tsv2/serve/4_http.ts:284-297` |
| CLI client | `bop query` tab-joins rows; "No column names cross that seam" | `v6/tsv2/cli/bop.ts:260-278` |

Plug points: a typed decode layer zipping `rel_columns` with each `IRow` into the generated interface; the HTTP client side. The tick chain has no slot for a named struct.

## 2. sprefa-engine-rs

| seam | representation | citation |
|---|---|---|
| value | `enum Value {Integer, Real, Bool, Text, List}` | `src/types.rs:22-34` |
| row | `pub type Row = Vec<Value>` — positional | `src/types.rs:137` |
| arrival | `Arrival {rel: String, sign, row}` | `src/types.rs:139-144` |
| program | `ProgramJson.rel_columns` / `rel_column_types` as data | `src/types.rs:408-409` |
| SqlRunner read | `QueryResult {rows: Vec<Vec<Value>>}` decoded per declared `RowColumnType` | `src/sql.rs:22`, `:152-193` |
| host output decode | `coerce()` string-matches `column_type.as_str()` per value | `src/hosts.rs:922-973` |
| fixed-schema decls | hand consts `DEP_REPO_COLUMNS` etc.; `SourceBindRelations::declarations()` hand arrays | `src/dep_resolve.rs:11-14`, `src/source_bind/_0_types.rs:5-10`, `:43-60` |

Generated `.types.rs` structs already derive serde, so serde into them works at any JSON-holding seam. Plug points: `From<&Row>` typed views keyed by `IncrementalRelationPlan.columns` (`src/types.rs:233`); hosts.rs `decode_output` (`src/hosts.rs:1105`); the hand consts duplicating `source.dl6` by hand (`_0_types.rs:11-14`).

## 3. What consumes the artifacts today

| candidate | finding | citation |
|---|---|---|
| tsv2 imports of `*.types.ts` | zero; one grep hit is a comment | `v6/tsv2/tests/listReadSurface.test.ts:184` |
| engine-rs `include!` of `*.types.rs` | zero | — |
| `emit_rust_harness` | reads only `PROGRAM_JSON`; type structs never enter | `src/bin/emit_rust_harness.rs:42-49`, `:59-62` |
| typegen golden gate | ONLY consumer: text diff, no `tsc`/`rustc` compile check | `v6/prolog/compile/test/typegen_golden.sh:122-128`, `:150-156` |
| sweep | writer, never reader | `v6/prolog/sweep.pl:139`, `:151` |

## 4. Bindable seams (host cores are program-generic by construction)

| seam | generation step | stays untyped | size |
|---|---|---|---|
| A. emitted TS module row surface | emit per-rel decoders `IRow -> <RelName>` + typed `rows_of` wrappers; module imports its own `.types.ts` | tick chain, arrivals, SQL plans | med |
| B. emitted `program.rs` | append `.types.rs` structs + `impl TryFrom<&Row>` per rel | `run_schedule` fold, Value/Row core | med |
| C. served endpoint clients | typed client stub per program for `GET /idb/:rel` + `POST /edb/events` | server side (arbitrary program) | small-med |
| D. fixed-schema Rust hosts | generate structs + consts from `source.dl6`, check in; replaces hand consts | executor plumbing | small + new prolog→rust codegen edge |
| E. golden gate hardening | add `tsc --noEmit` / `rustc --emit=metadata` legs; prerequisite for A-D | everything | small |
| F. jsonschema/openapi plane | serve per-rel schema for external validators | — | small |

Caveat for A/B via dl6 doors: `render_ts.dl6:10-13` scopes Phase F to single module, no collisions; only the prolog emitters cover the full construct set today.

## 5. Forks needing Chris

- ~780 `out/*.types.*` untracked: commit-vs-regenerate must be decided before any consumer wiring.
- Which door generates consumer code: prolog emitters construct-complete vs dl6 doors scoped smaller (`render_ts.dl6:10-13`).
- Seam D build graph: swipl-generated file inside the engine crate, build-time regen vs checked-in.
- Positional-to-named zip soundness: `rel_columns` order vs typegen ordinal order both derive from `catalog_decl_rows/6` (`9_emit_type_artifact.pl:12-15`) but nothing PINS the equality — a pinning test is a design ask.
- `type_name/2` non-injective (`7_emit_ts_types.pl:61-64`): a typed client aggregating several programs inherits the collision behavior.
