# Typegen for the host engines: can per-program types work for tsv2 and engine-rs?

TOC
1. tsv2 row representation and the plug seam
2. engine-rs row representation and the plug seam
3. What is already halfway there
4. The shape question: per-program vs program-generic
5. Forks needing Chris

Scope: investigation only. No code changed. Every receipt is `path:line`.

## 1. tsv2 row representation

A rel row is a positional, untyped value array. Column names and types ride
beside it in two maps, never in the row itself.

| seam | shape | receipt |
|---|---|---|
| the row | `IRow = readonly IRowValue[]` | `v6/tsv2/runtime/types.ts:40` |
| a cell | `IRowValue = IRowScalar \| IRowValueArray` | `types.ts:17` |
| scalar | `IRowScalar = string \| number \| boolean` | `types.ts:22` |
| list cell | `IRowValueArray extends ReadonlyArray<IRowValue>` | `types.ts:26` |
| declared col type | `IRowColumnType = "text"\|"int"\|"bool"\|"float"\|"ref"\|"json"\|"list"` | `types.ts:37` |
| arrival | `IArrivalRow { rel, sign, row }` | `types.ts:51` |
| batch | `IArrivalBatch = readonly IArrivalRow[]` | `types.ts:62` |
| col names | `IGenProgram.rel_columns: Record<string, string[]>` | `types.ts:511` |
| col types | `IGenProgram.rel_column_types?: Record<string, IRowColumnType[]>` | `types.ts:512` |
| served program | `IServedProgram extends IGenProgram` | `types.ts:734` |
| read a rel | `ILiveEngine.rows(rel): Observable<readonly IRow[]>` | `types.ts:778` |

The read seam shapes SQL results back into `IRow[]` by declared column order:

| step | receipt |
|---|---|
| `select_rows` runs SQL, maps each row over `columns` | `v6/tsv2/runtime/rows.ts:75` |
| `row_value_from_sql` coerces per `IRowColumnType` | `rows.ts:27` |
| positional build `columns.map((c,i) => row_value_from_sql(types[i], row[c]))` | `rows.ts:87-89` |

The emitted module imports these names directly (137 import sites), not a typed
per-program header:

```
import { select_rows } from "../runtime/rows.ts";
```
`v6/prolog/compile/out/<name>.ts:25`

The serve endpoints shuttle the same untyped arrays:

| endpoint | payload | receipt |
|---|---|---|
| `POST /edb/events` | `{batch:[{rel,sign,row}]}` | `v6/tsv2/serve/4_http.ts:6-7` |
| arrival validation | checks `program.rel_columns` + `rel_column_types` | `4_http.ts:325-337` |
| per-column check | `column_problem(type, value)` | `4_http.ts:284` |
| `GET /idb/:rel` | `{rows}` (raw `IRow[]`) | `4_http.ts:369-376` |

Where a generated interface plugs: nowhere today. The only typed surface is the
static header `v6/tsv2/runtime/types.ts`, which is program-generic. A
per-program interface could wrap exactly two seams: (a) the emitted module's
own column lists, typed as a tuple over `IRow` positions, and (b) the
`/edb/events` and `/idb/:rel` bodies as a published `.types.ts` the client
imports. Both are consumer-side; the serve core itself stays generic.

## 2. engine-rs row representation

| seam | shape | receipt |
|---|---|---|
| a cell | `Value = Integer(i64)\|Real(f64)\|Bool(bool)\|Text(String)\|List(Vec<serde_json::Value>)` | `v6/sprefa-engine-rs/src/types.rs:24` |
| a row | `Row = Vec<Value>` | `types.rs:137` |
| scalar bind | `ScalarValue = Integer\|Real\|Bool\|Text` | `types.rs:50` |
| arrival | `Arrival { rel, sign, row: Row }` | `types.rs:140` |
| delta | `RelDelta { rel, add: Vec<Row>, del: Vec<Row> }` | `types.rs:153` |
| program json | `ProgramJson.rel_columns: HashMap<String, Vec<String>>` | `types.rs:408` |
| program json types | `ProgramJson.rel_column_types: HashMap<String, Vec<RowColumnType>>` | `types.rs:409` |
| host col plan | `HostColumnPlan { name, column_type: String }` | `types.rs:383` |

Column declarations in the built-in relations are hand-written name/type
constant pairs, not generated structs:

| module | constant | receipt |
|---|---|---|
| source_bind | `REPO_COLUMNS`, `REV_COLUMNS`, ... `SPECIFIER_COLUMNS` | `v6/sprefa-engine-rs/src/source_bind/_0_types.rs:5-10` |
| source_bind decl | `SourceBindRelation { name, columns: &[&str], column_types: &[RowColumnType] }` | `_0_types.rs:97` |
| dep_resolve | `DEP_REPO_COLUMNS` ... `DEP_VISITED_COLUMNS` | `v6/sprefa-engine-rs/src/dep_resolve.rs:11-14` |
| dep_resolve decl | `DepResolveRelation { name, columns, column_types }` | `dep_resolve.rs:70` |
| git hosts | `GIT_REF_COLUMNS`, `GIT_TAG_COLUMNS`, ... | `v6/sprefa-engine-rs/src/hosts.rs:387-391` |

Host output decode builds `Value` from `serde_json::Value` by the declared
`column_type` string, then a positional `Vec<Value>`:

| step | receipt |
|---|---|
| `coerce(host, column, raw)` -> `Value` | `hosts.rs:922` |
| `decode_output` -> `Vec<Vec<Value>>` | `hosts.rs:1105` |

The schedule harness parses arrivals the same untyped way:

| step | receipt |
|---|---|
| `value_from_json(serde_json::Value)` -> `Value` | `v6/sprefa-engine-rs/src/bin/emit_rust_harness.rs:25` |
| emitted module's JSON extracted from a raw string literal | `emit_rust_harness.rs:42-63` |

Where a generated Rust struct plugs: `8_emit_rust_types.pl` already produces
serde-derive structs (`#[derive(Debug, Clone, PartialEq, serde::Serialize,
serde::Deserialize)] pub struct Node { pub node_id: i64, pub name: String }`).
A generated struct binds at two seams, both over `Row`/`Value`: (a) the
`emit_rust_harness` schedule parse could deserialize each rel's `row` into its
struct instead of `value_from_json`, and (b) `hosts.rs` `coerce`/`decode_output`
could serde into a struct for a named rel's response projection. Nothing in the
runtime core would change; the structs are a typed view over the same `Vec<Value>`.

## 3. What is already halfway there

Nothing consumes a per-program artifact today.

| check | result |
|---|---|
| import of any `*.types.ts` (generated) in tsv2/engine-rs | none; every match is the static header `v6/tsv2/runtime/types.ts` |
| `include!` / `include_str!` / build.rs codegen in engine-rs | none (`rg include!|OUT_DIR` over `v6/sprefa-engine-rs` = 0 hits) |
| the `~780` artifacts | untracked, neither tracked nor gitignored | `CLAUDE.md:246` |

The emitters and doors exist and are parity-gated, but are only written to
disk, never linked:

| piece | does | receipt |
|---|---|---|
| TS emitter | `ts_types_text/3` renders interfaces | `v6/prolog/compile/7_emit_ts_types.pl:5` |
| Rust emitter | `rust_types_text/3` renders structs | `v6/prolog/compile/8_emit_rust_types.pl:5` |
| artifact wrapper | `9_emit_type_artifact.pl` re-exports the three | `v6/prolog/compile/9_emit_type_artifact.pl:17-27` |
| dl6 door (TS) | reads `type_row/7`, renders interfaces | `v6/dl/typegen/render_ts.dl6` |
| dl6 door (Rust) | same, renders structs | `v6/dl/typegen/render_rust.dl6` |
| row dump | `dump_type_rows/2` -> `type_row/7` JSONL | `v6/prolog/compile/typegen_export.pl:23-29` |
| sweep writes artifacts | `TsTypesPath` / `RustTypesPath` | `v6/prolog/sweep.pl:139,151` |

The dl6 doors are the interesting halfway point: they prove the type plane can
be driven as datalog, but they are golden twins of the prolog emitters, not a
runtime input. The `type_row/7` JSONL (id, parent, ordinal, name, kind,
type_id, module_id) is the interchange format a host-facing emitter would read.

## 4. The shape question

The host cores run arbitrary dl6 programs, so compile-time per-program types
cannot type `IRow`/`Value` or the tick loop. Per-program types can bind only at
seams where one compiled program is in hand. Sized options:

| seam | what a generated type binds | generation step that wires it | stays untyped | size |
|---|---|---|---|---|
| emitted harness program (`emit_rust_harness` -> `program.rs`) | the schedule's per-rel `row` deserializes into `8_emit_rust_types` structs instead of `value_from_json` | emit `mod types { include!(concat!(env!("OUT_DIR"), ...)) }` or paste structs into the emitted `program.rs`; `sweep.pl:151` already writes `.types.rs` | the `ProgramJson`/`GenProgram` serde surface; the SQL statement text | small-med |
| served endpoint payloads (tsv2) | a published `.types.ts` types the `/edb/events` POST body and `/idb/:rel` rows for a client | `7_emit_ts_types.pl` artifact exposed beside the emitted module; `0_compile.ts:101` writes only `<name>.ts` today, not `.types.ts` | the serve core (`4_http.ts` validation, `rows.ts` coercion) stays generic | med |
| golden gate scripts | `scripts/run-fixture.ts`, `scripts/golden-run.ts` import the `.types.ts` for typed fixtures | import the existing artifact (already emitted by `sweep.pl:139`) | the oracle/schedule JSON remains positional | small |
| client-side consumers (lib surface) | any external consumer types its arrivals/queries against a rel's struct | publish the artifact; this is the open `plans/2026-08-13-generated-types-as-lib-surface.md` arc | the wire stays JSON, untyped on the server | small |
| linked built-in hosts (`hosts.rs`, `source_bind`, `dep_resolve`) | each hand-written rel's columns become a generated struct the `coerce`/`decode_output` path fills | these are fixed relations, so a one-time generated struct replaces the `&[&str]` constants | executors stay string-in/string-out | small |

The largest honest target is the harness + endpoint seam pair: both already
have a single program in hand at runtime and both today do positional untyped
decode by hand. The smallest are the client/golden seams, which need only the
existing artifact published.

## 5. Forks needing Chris

| fork | citation |
|---|---|
| track vs gitignore the ~780 untracked `.types.rs`/`.types.ts` artifacts (currently neither) | `CLAUDE.md:246` "Awaiting user word" |
| whether generated types become a published library surface vs compile-time-only | `plans/2026-08-13-generated-types-as-lib-surface.md` (arc exists, undecided) |
