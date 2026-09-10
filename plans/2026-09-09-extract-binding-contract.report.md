> Historical read-only report. Proposals are superseded by 2026-09-10-dl6-as-dl7-userland.md; this is evidence, not implementation authority.

/opt/homebrew/bin/bash: warning: setlocale: LC_ALL: cannot change locale (C.UTF-8): No such file or directory
# Extract binding contract checkpoint
Source-read only. Claims are IMPL (at the cited line), PROP or OPEN.

## 1. Architecture
```
  types.rs:3006 FlatFact (60) --serde--> JSONL wire (text)
  schema/1_facts.tsp (63)     --gen----> 4_facts.sql, 7_writers_auto.rs (61 tables)
  tsi/registry.rs:51 REGISTRY (36 rows: name + &[ArgKind])
      |                 |                        |
      v                 v                        v
 [A] PROLOG        [C] RUST/LIB            [B] SQLITE DOOR
 subprocess,       in-process,             bin/extract/0_sqlite.rs
 0c_extract_       source_bind/            writers::insert_all
 loader.pl        _1_runtime.rs:542        caller owns the txn
 tsi_relation/2    dispatch()+flatten()    temp file, persist_noclobber
 arity only        Specifier ONLY (:549), 59 variants dropped (:574)
```
Three seam formats: JSONL text, a SQLite file, in-process Rust values; nothing
crosses more than one. [A'] `v7/src/4_tool/5_extract_sqlite_query_mainer.pl`
shells `extract --sqlite DB -- SRC...` then sqlite3; zero callers.

## 2. Authority map
| authored declaration | generated artifact | consumer today | covers |
|---|---|---|---|
| `types.rs:3006` FlatFact, 60 variants | none (serde) | CLI JSONL, `flatten` callers, `_1_runtime.rs:549` | the wire, IMPL |
| `schema/1_facts.tsp`, 63 models | `4_facts.sql`, `7_writers_auto.rs`, 61 tables each | `--sqlite` CLI only | SQL columns + writers, IMPL |
| `tsi/registry.rs:51` REGISTRY, 36 rows | none | `tsi::sink`, `--ingest`, `schema_text()` | TSI arity AND kinds, IMPL |
| `0c_extract_loader.pl:15-50` `tsi_relation/2` 36 rows, `:145-200` `foreign_record/1` 56 rows | none | `install_tsi_graph/6`, `decode_known_record/5` | arity only (no kinds), hand copy |

## 3. Contracts today (IMPL). No authority spans all three doors
```rust
pub trait Source: Sync + Send {                                   // types.rs:2708
    fn name(&self) -> &'static str;  fn matches(&self, path: &str) -> bool;
    fn extract(&self, path: &str, content: &[u8], mask: FamilyMask) -> ExtractOutput; }
pub trait Resolve<F: Family>: Source {                            // types.rs:2353
    fn resolve(&self, output: &ExtractOutput, cx: &ProjectCx) -> Vec<ProjectEdge<F>>; }
pub fn dispatch(path:&str, content:&[u8], mask:FamilyMask) -> Option<Arc<ExtractOutput>>;
pub fn flatten(out: &ExtractOutput) -> Vec<FlatFact>;   // dispatch.rs:48 (cached), wire.rs:39
pub fn check(name: &str, args: &[Arg]) -> Result<(), String>;     // tsi/registry.rs:215
pub struct Source<'a> { row: i64, input_path: Option<&'a str>,
                        content_id: Option<&'a str> }             // 7_writers_auto.rs:31
pub fn insert_all(conn:&Connection, source:&Source<'_>, rows:&[Fact])
    -> Result<usize, InsertError>;                                // :1245
```
```prolog
load_tsi_stream(+JsonlPath, -Rows, -Diagnostics)              % 0c_extract_loader.pl:74
accepted_rows(+Rows, -Accepted)                               % :289, semantic-witness filter
install_tsi_graph(+Rows, +Bs0, +Os0, -Bs, -Os, -Diagnostics)  % :363
tsi_expression_environment(+Rows, +Importers, -Environment)   % :313
```
Argument ordering is positional, five shapes at `:265-283`: `id(Id)`,
`span(Digest,Start,End)`, `text(T)`, `int(N)`, `atom(A)` — the same five as
`ArgKind` at `registry.rs:9`. Prolog never checks kinds against the relation.
SYNTAX = `Source::extract`/`dispatch`, one blob, pure, cached `dispatch.rs:61`.
SEMANTIC = `Resolve::resolve` + SCIP + `--resolve`, project-scoped, subprocess
indexers; `accepted_rows/2` admits only facts with a `semantic` run witness, so
syntax records reach the loader only as `foreign_record/1` skips.

## 4. Worked example, `demo.ts` = `import {a} from "./m";` (shapes off the cited declarations, not a captured run)
```
1  flatten(dispatch(..)) -> FlatFact::Specifier{span:{8,9}, name:"a", module:"./m"}
2a JSONL {"record":"specifier","family":"cst","span":{"start":8,"end":9},"name":"a",
          "kind":"named","module":"./m","imported":null}
2b flush insert_all(&conn, &Source{row:first_row, input_path:Some("demo.ts"),
          content_id:Some(digest)}, &pending)                       0_sqlite.rs:136
   `specifier` row: _row,_input_path,_content_id,record,family='cst',span__start=8,
          span__end=9, name='a', kind, module, imported=NULL
2c engine relations.span(file,8,9) + relations.specifier(owner_json,'./m','a',kind)
          SELECT _input_path, name, module FROM specifier;   _1_runtime.rs:553-571
```

## 5. Ownership (a binding minting its own ordinals or txn breaks all three doors)
| concern | owner | cite |
|---|---|---|
| DDL + transaction | caller, at open inside `BEGIN IMMEDIATE`; writers never BEGIN/COMMIT/SAVEPOINT | `0_sqlite.rs:63-66,152` |
| ordinal `_row`, batching | caller; `Source.row` is the batch's FIRST ordinal, writers keep per-row offsets, `InsertError::OrdinalOverflow` guards; `max_batch_rows(conn)` ceiling, 8 MiB byte budget | `0_sqlite.rs:35,67,112,135` |
| source identity, publication | caller sets `input_path`/`content_id`, flushing on change; temp file beside destination, `persist_noclobber` | `0_sqlite.rs:57,81-89,155` |

## 6. Counts and the two gaps
FlatFact 60 · SQL tables = `Fact` arms = `TABLE_COUNT` 61 (60 + `capture`) ·
REGISTRY 36 · `tsi_relation/2` 36 · `graph_relation/1` 11 · `foreign_record/1` 56.
Set difference of writer tables against `foreign_record` + the TSI six: writer
tables the loader does not know **none**; loader names with no table **`ast_rule`**.
- GAP 1, kinds. Rust `check/2` validates arity AND kinds, Prolog arity only, so a
  right-arity wrong-kind argument passes Prolog and fails Rust. Parity by hand.
- GAP 2, `ast_rule` comes from `lang/1_ast_rule.rs:298` `query_ast_rule`, not
  `flatten`; no TypeSpec model, no SQL table, so it never reaches the SQLite door.

## 7. Smallest first task (PROP), reusable by all three doors
```rust
pub fn catalog_json() -> &'static str;   // PROP, new `catalog` module. Shape:
// {"protocol":1, "records":[{"name","table","sql_columns":[..]}, ..],
//  "tsi_relations":[{"name":"tsi.denotes","args":["id","id"]}, ..]}
```
```prolog
load_extract_catalog(+CatalogJsonPath, -Catalog, -Diagnostics)   % PROP
catalog_tsi_relation(+Catalog, ?Name, ?ArgKinds)                 % kinds, not bare arity
catalog_foreign_record(+Catalog, ?Record)
```
`records` comes from the already-generated `5_facts.json` (README:35),
`tsi_relations` from REGISTRY; only the join and the emission are new, and
`insert_all`, `dispatch`, `Source`, `Resolve` are untouched.
| file | change |
|---|---|
| `schema/2_gen.mjs`, `schema/1a_fact_emit.mjs` | expose `emitFacts`'s record/table/column triples, write `generated/8_catalog.json` |
| `v6/sprefa-extract/src/tsi/registry.rs`, `src/schema.rs` | serialize REGISTRY rows, add `catalog_json()`; `schema_text()` unchanged |
| `v7/src/2_comptime/0c_extract_loader.pl` | `tsi_relation/2` and `foreign_record/1` read the catalog instead of hand-copied rows |

Forbidden: `insert_all`, `0_sqlite.rs`, `_1_runtime.rs`, the parked mainer, any
transport, any new dependency. Tests: catalog table count equals `TABLE_COUNT`;
catalog TSI rows equal REGISTRY name-for-name and kind-for-kind; the loaded
catalog reproduces today's 36 `tsi_relation` and 56 `foreign_record` rows; one
negative case, a right-arity wrong-kind argument now rejected by Prolog. Not run.

## 8. Needs user review (OPEN)
1. Transport for the in-process Prolog binding: SWI foreign predicate over the
   Rust library, or JSONL over the parked wrapper. Unchosen.
2. `ast_rule`: a TypeSpec model, or the source-query path stays outside storage.
3. DL7 schema bootstrap parity gate against TypeSpec (`schema/README.md:63`).
4. Whether the engine door widens past `FlatFact::Specifier` or stays narrow.
