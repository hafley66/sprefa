# sprefa-extract capability inventory

HEAD receipt: `git rev-parse HEAD` = `429e2008f45b9990812073aa1e7b121a6048972b`.

The named prior material was read first. The resolve brief records the original
library recipe and the phase-1 CLI boundary at
`plans/2026-07-29-extract-resolve-flag-brief.md:1-52`. The landed parity test
uses `--resolve` at `v6/sprefa-extract/tests/1_resolve_cli.rs:9-36`. The current
v5 surface counts are taken from
`plans/2026-07-30-v5-parity-table.tsv:49-160`.

## 1. Capability inventory

There are 10 JSONL-producing binary capabilities: 8 phase-1 fact records, one
pattern record, and one phase-2 record. `--schema` and `--bench` are control
surfaces and produce no fact stream. The CLI dispatch and mode constraints are
at `v6/sprefa-extract/src/bin/extract.rs:75-169`; the exact enum fields are at
`v6/sprefa-extract/src/types.rs:1255-1347`.

| capability | CLI entry | accepted source type | exact JSONL fields | phase | languages | test coverage |
|---|---|---|---|---|---|---|
| CST/type/call/DF node | `extract PATH`; optionally `--family cst,type,call,df` | `.rs`, `.go`, `.kt`/`.kts`, `.pl`/`.pro`/`.prolog`/`.datalog`/`.horn`, TS/JS extensions, or an ast-grep grammar fallback | `record=node`, `family`, `span={start,end}`, `kind`, `name` | 1, syntactic | Native type/call/DF sources emit their family; ast-grep fallback emits CST only | TS snapshots: `v6/sprefa-extract/tests/snapshot.rs:17-87`; Prolog all-family test: `v6/sprefa-extract/tests/0_prolog.rs:9-67`; parity facets: `v6/sprefa-extract/tests/golden_parity.rs:146-180` |
| CST/DF edge | `extract PATH`; selected by the family mask | Same paths as node | `record=edge`, `family`, `kind`, `from={start,end}`, `to={start,end}` | 1, syntactic | CST for every ast-grep grammar; DF for TS, Rust, Go, Kotlin, and Prolog | TS snapshot: `v6/sprefa-extract/tests/snapshot.rs:17-87`; DF CLI goldens for TS/Rust/Go/Kotlin: `v6/sprefa-extract/tests/2_df_aux_cli.rs:8-56`; parity normalizer: `v6/sprefa-extract/tests/golden_parity.rs:181-188` |
| Type signature | `extract PATH --family type` or default mode | TS, Rust, Go, Kotlin | `record=sig`, `family=type`, `owner={start,end}`, `owner_start`, `owner_end`, `slot`, `pos`, `ty` | 1, type name as written | TS, Rust, Go, Kotlin | parity normalizer and asserted facet: `v6/sprefa-extract/tests/golden_parity.rs:191-201`, `v6/sprefa-extract/tests/golden_parity.rs:235-280` |
| Call site | `extract PATH --family call` or default mode | TS, Rust, Go, Kotlin, Prolog | `record=site`, `family=call`, `span={start,end}`, `callee`, `callee_path` | 1, callee as written | TS, Rust, Go, Kotlin, Prolog | Prolog names and paths: `v6/sprefa-extract/tests/0_prolog.rs:25-57`; parity normalizer: `v6/sprefa-extract/tests/golden_parity.rs:203-208` |
| String const value | `extract PATH --family type` or default mode | TS and Rust | `record=const`, `family=type`, `owner={start,end}`, `field`, `text`, `kind` | 1, cooked literal or template text | TS and Rust | parity normalizer: `v6/sprefa-extract/tests/golden_parity.rs:209-220`; source writes: `v6/sprefa-extract/src/lang/ts.rs:2291-2442`, `v6/sprefa-extract/src/lang/rust.rs:422-437` |
| Module specifier | `extract PATH --family call` or default mode | TS and Prolog | `record=specifier`, `family=call`, `span={start,end}`, `name`, `kind` | 1, import/use text as written | TS import/export forms and Prolog `use_module`/`ensure_loaded`/`consult` | TS collection: `v6/sprefa-extract/src/lang/ts.rs:883-1000`; Prolog collection: `v6/sprefa-extract/src/lang/prolog/_0_source.rs:189-221`; output arm: `v6/sprefa-extract/src/wire.rs:147-155` |
| DF parameter slot | `extract PATH --family df` | TS, Rust, Go, Kotlin | `record=param`, `family=df`, `span={start,end}`, `pos` | 1, syntactic slot bridge | TS, Rust, Go, Kotlin | CLI goldens select and compare these rows: `v6/sprefa-extract/tests/2_df_aux_cli.rs:40-55`; type definition: `v6/sprefa-extract/src/types.rs:477-484` |
| DF argument slot | `extract PATH --family df` | TS, Rust, Go, Kotlin | `record=arg`, `family=df`, `call={start,end}`, `pos`, `arg={start,end}` | 1, syntactic slot bridge | TS, Rust, Go, Kotlin | CLI goldens select and compare these rows: `v6/sprefa-extract/tests/2_df_aux_cli.rs:40-55`; type definition: `v6/sprefa-extract/src/types.rs:486-494` |
| AST-grep captures | `extract --ast-pattern ID=PATTERN --ast-capture ID=NAME PATH`, with optional `--ast-selector` | Any path accepted by `SupportLang::from_path` | `record=capture`, `query`, `capture`, `text`, `start`, `end`, `match_start`, `match_end` | 1, syntactic pattern match | ast-grep grammar registry | Query implementation: `v6/sprefa-extract/src/lang/astgrep.rs:28-120`; CLI test and exact golden: `v6/sprefa-extract/tests/3_ast_pattern_cli.rs:5-49` |
| Resolved call edge | `extract --resolve PATH...` | Supplied paths dispatched to TS, Rust, Go, Kotlin, or Prolog sources; a source with no call bundle yields no edge | `record=resolved_edge`, `caller_path`, `caller_name`, `callee_path`, `callee_name`, `caller_site_start`, `caller_site_end`, `kind` | 2, project name resolution; SCIP is absent in this CLI recipe | TS, Rust, Go, Kotlin, Prolog | TS cross-file and Kotlin CLI goldens: `v6/sprefa-extract/tests/1_resolve_cli.rs:9-36`; project assembly and call-only dispatch: `v6/sprefa-extract/src/bin/extract.rs:247-337` |

`extract --schema` prints the complete contract, including the same 10 record
shapes, at `v6/sprefa-extract/src/bin/extract.rs:387-447`. `--bench` runs one
phase-1 extraction and flatten pass, then reports family node counts and total
fact count to stderr at `v6/sprefa-extract/src/bin/extract.rs:362-384`.

### Source roster and phase-1 family coverage

The first-match roster is `v6/sprefa-extract/src/lang/mod.rs:29-49`. Native
source implementations are TS at `v6/sprefa-extract/src/lang/ts.rs:2460-2529`,
Rust at `v6/sprefa-extract/src/lang/rust.rs:2114-2180`, Go at
`v6/sprefa-extract/src/lang/go.rs:1334-1403`, Kotlin at
`v6/sprefa-extract/src/lang/kotlin.rs:973-1041`, Prolog at
`v6/sprefa-extract/src/lang/prolog/_0_source.rs:487-530`, and the CST-only
fallback at `v6/sprefa-extract/src/lang/astgrep.rs:214-249`.

| source | extensions | CST | type | call | DF | const | specifier | `--resolve` call arm | relevant tests |
|---|---|---:|---:|---:|---:|---:|---:|---:|---|
| TS/JS | `.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`, `.mjs`, `.cjs` | yes | yes | yes | yes | yes | yes | yes | snapshots, parity, TS resolve CLI, TS SCIP ratchet |
| Rust | `.rs` | yes | yes | yes | yes | yes | no | yes | snapshots, parity, library resolve, Rust SCIP ratchet |
| Go | `.go` | yes | yes | yes | yes | no | no | yes | DF CLI, parity, library resolve, Go SCIP ratchet |
| Kotlin | `.kt`, `.kts` | yes | yes | yes | yes | no | no | yes | DF CLI, parity, Kotlin resolve CLI |
| Prolog | `.pl`, `.pro`, `.prolog`, `.datalog`, `.horn` | yes | yes | yes | yes | no | yes | yes | `0_prolog.rs`, including library call resolution |
| ast-grep fallback | any accepted ast-grep grammar | yes | no | no | no | no | no | no | roster and pattern tests cover routing/pattern mode; no fallback-family golden |

The language coverage prose in the binary repeats this matrix at
`v6/sprefa-extract/src/bin/extract.rs:45-55`. The Prolog implementation has
call sites and specifiers but no DF parameter or argument aux rows, as shown by
its call/specifier construction at `v6/sprefa-extract/src/lang/prolog/_0_source.rs:165-221`
and its DF construction at `v6/sprefa-extract/src/lang/prolog/_0_source.rs:302-310`.

## 2. Library versus binary parity

The library exports the typed families, phase-2 traits, SCIP seams, and flatten
functions at `v6/sprefa-extract/src/lib.rs:30-52`. The binary uses the phase-1
`dispatch -> flatten` path at `v6/sprefa-extract/src/bin/extract.rs:353-360`,
plus its own project call adapter at `v6/sprefa-extract/src/bin/extract.rs:253-337`.

| capability | library | binary | gap |
|---|---|---|---|
| Typed phase-1 bundles | `ExtractOutput`, `FamilyBundle<F>`, `Node`, `Edge`, all aux values | JSONL projection only | Binary does not expose typed nodes, candidate rows, or aux vectors |
| Phase-1 flat facts | `flatten` and sorted `flatten_jsonl` | default path calls `flatten` and serializes each fact | Same fact families, with binary path/name handling added at the CLI |
| Type resolution | `Resolve<TypeF>` exists for TS, Go, and Rust, and is asserted by the TS/Go/Rust tests at `v6/sprefa-extract/tests/golden_parity.rs:324-470` | No call to `Resolve<TypeF>` in `resolve_project`; `resolve_call_edges` dispatches only `CallF` at `v6/sprefa-extract/src/bin/extract.rs:324-335` | Binary cannot expose `ProjectEdge` type rows or a type-resolved JSONL record |
| Call resolution without SCIP | `Resolve<CallF>` exists for TS, Rust, Go, Kotlin, and Prolog | `--resolve` assembles a definition index and invokes the same call arms for all five sources | Binary exposes this recipe as `resolved_edge`; library callers receive typed `ProjectEdge<CallF>` |
| SCIP-backed call resolution | Library caller can build/load `ScipTypescript`, `ScipGo`, or `ScipRust`, place the index and a reader into `ProjectCx`, then call `Resolve<CallF>`; the implementations are at `v6/sprefa-extract/src/lang/ts.rs:2713-2775`, `v6/sprefa-extract/src/lang/go.rs:1601-1645`, and `v6/sprefa-extract/src/lang/rust.rs:803-850` | `resolve_project` sets `reader: None` and does not load a SCIP index at `v6/sprefa-extract/src/bin/extract.rs:271-283` | Binary cannot expose `ScipOverride` edges or run the SCIP path |
| Resolved edge wire | `flatten_project_type` emits `record=project_edge` fields `family`, `kind`, `from`, `to_blob`, `to` at `v6/sprefa-extract/src/wire.rs:158-181`; raw call `ProjectEdge<CallF>` is public | Binary emits `record=resolved_edge` with paths, names, and call-site byte offsets at `v6/sprefa-extract/src/bin/extract.rs:285-320` | The two consumers expose different phase-2 shapes. There is no library `flatten_project_call` helper |
| AST-grep batch mode | `query_patterns` is public at `v6/sprefa-extract/src/lib.rs:36-39` and implemented at `v6/sprefa-extract/src/lang/astgrep.rs:54-120` | CLI parses `ID=...` arguments and serializes captures at `v6/sprefa-extract/src/bin/extract.rs:184-244` | CLI adds argument validation and stdout framing; the extraction operation exists in both |
| SCIP index construction | `ScipTypescript`, `ScipGo`, `ScipRust` are public at `v6/sprefa-extract/src/lib.rs:41-48`; subprocess details are at `v6/sprefa-extract/src/scip.rs:47-220` | No SCIP CLI option | Library-only external index construction |
| Project context | Library caller supplies `FileSet`, `ManifestMap`, `ProjectCx`, `IndexBag`, reader, digest, and indexes; the types are at `v6/sprefa-extract/src/types.rs:786-825` | Binary constructs hollow `FileSet` and `ManifestMap`, default digest, no reader, and only the def index at `v6/sprefa-extract/src/bin/extract.rs:271-283` | Binary project mode has a narrower context than the library seam |
| Schema and benchmark control | No library equivalent is exported | `--schema` and `--bench` are binary-only at `v6/sprefa-extract/src/bin/extract.rs:129-169` | Binary-only operational surfaces |

The direct library tests named by the scout are therefore real capability tests:
`tests/0_prolog.rs:69-95` covers `Resolve::<CallF>`, def-index construction, and
the `ProjectCx` recipe. The CLI now covers only call resolution, with TS and
Kotlin fixtures, at `tests/1_resolve_cli.rs:9-36`. The missing assertion is a
single bin-vs-library parity test for the SCIP-enabled project recipe and for
type-edge CLI exposure.

## 3. V5 absent relations crossed against the extractor

The v5 catalog contains 112 relation names. Six are covered in v6's parity
table (`file`, `call_site`, `df_node`, `df_arg`, `df_edge`, `df_param` at
`plans/2026-07-30-v5-parity-table.tsv:155-160`), leaving 106 absent rows at
`plans/2026-07-30-v5-parity-table.tsv:49-154`.

Split labels:

* `E` means the current extractor has the source facts or SCIP artifacts, but
  the relation projection, wire arm, or CLI path is absent.
* `E+L` means a source spelling exists in at least one relevant language and the
  current extractor also lacks the resolver or document parser needed for the
  v5 relation. The language-specific portion needs its own junction.
* `B` means the relation requires repo, revision, daemon, host, storage,
  manifest, graph-sink, or derived-engine state. A source file has no spelling
  for that relation, and the extractor has no such context.

The v5 relation contracts used to identify these categories are in
`docs/reference/relations.md:7-75` and `docs/reference/relations.md:80-118`.

| rel | v5 files | split | extractor cross-check | receipts |
|---|---:|---|---|---|
| diag | 33 | B | Diagnostic sink, no extractor diagnostic type or wire arm | `plans/2026-07-30-v5-parity-table.tsv:49`; `docs/reference/relations.md:43`; `v6/sprefa-extract/src/types.rs:1255-1347` |
| type_entity | 22 | E | TypeF entity nodes are emitted and flatten to `record=node`, `family=type` | `plans/2026-07-30-v5-parity-table.tsv:50`; `docs/reference/relations.md:110`; `v6/sprefa-extract/src/types.rs:189-221`; `v6/sprefa-extract/src/wire.rs:88-98` |
| call_edge | 20 | E | Resolve<CallF> produces typed project edges and `--resolve` serializes them | `plans/2026-07-30-v5-parity-table.tsv:51`; `docs/reference/relations.md:12`; `v6/sprefa-extract/src/types.rs:341-390`; `v6/sprefa-extract/src/bin/extract.rs:285-320` |
| call_def | 17 | E | CallF definition nodes flatten to `record=node`, `family=call` | `plans/2026-07-30-v5-parity-table.tsv:52`; `docs/reference/relations.md:10`; `v6/sprefa-extract/src/types.rs:343-367`; `v6/sprefa-extract/src/wire.rs:122-137` |
| type_edge | 15 | E+L | Candidates and Resolve<TypeF> exist for TS, Go, Rust; Kotlin's type-edge arm is absent | `plans/2026-07-30-v5-parity-table.tsv:53`; `docs/reference/relations.md:108`; `v6/sprefa-extract/src/types.rs:226-238`; `v6/sprefa-extract/src/types.rs:1041-1046`; `v6/sprefa-extract/tests/golden_parity.rs:324-470` |
| call_name | 12 | E | Names are on CallF nodes and callee names are on Site rows; no dedicated relation projection | `plans/2026-07-30-v5-parity-table.tsv:54`; `docs/reference/relations.md:15`; `v6/sprefa-extract/src/types.rs:343-404`; `v6/sprefa-extract/src/wire.rs:122-155` |
| repo | 12 | B | Repo identity is outside Source and absent from the CLI path | `plans/2026-07-30-v5-parity-table.tsv:55`; `docs/reference/relations.md:85`; `v6/sprefa-extract/src/types.rs:786-818` |
| module_edge | 9 | E+L | TS has phase-1 specifiers and TS/Go/Rust SCIP builders exist; no module resolver or module relation wire exists | `plans/2026-07-30-v5-parity-table.tsv:56`; `docs/reference/relations.md:69`; `v6/sprefa-extract/src/types.rs:406-461`; `v6/sprefa-extract/src/types.rs:587-604` |
| scip_edge | 8 | E | SCIP index build/load exists, while the diet drops relationships and no relation projection exists | `plans/2026-07-30-v5-parity-table.tsv:57`; `docs/reference/relations.md:93`; `v6/sprefa-extract/src/scip.rs:47-220`; `v6/sprefa-extract/src/scip.rs:234-270` |
| changed | 7 | B | Git status is outside the Source trait and CLI input | `plans/2026-07-30-v5-parity-table.tsv:58`; `docs/reference/relations.md:17`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| comment_node | 6 | E+L | Native parsers see comments, but no comment aux or FlatFact arm exists; Markdown has no Source | `plans/2026-07-30-v5-parity-table.tsv:59`; `docs/reference/relations.md:24`; `v6/sprefa-extract/src/types.rs:1217-1226`; `plans/2026-07-29-extract-doc-formats-header.md:6-14` |
| scip_fn_edge | 6 | E | Call resolution can use SCIP occurrences, but the SCIP function-edge relation is not projected | `plans/2026-07-30-v5-parity-table.tsv:60`; `docs/reference/relations.md:94`; `v6/sprefa-extract/src/scip.rs:234-270`; `v6/sprefa-extract/src/lang/ts.rs:2713-2775` |
| true | 6 | B | Engine singleton, with no source-file spelling | `plans/2026-07-30-v5-parity-table.tsv:61`; `docs/reference/relations.md:106`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| type_sig | 6 | E | TypeSig aux rows flatten to `record=sig` | `plans/2026-07-30-v5-parity-table.tsv:62`; `docs/reference/relations.md:116`; `v6/sprefa-extract/src/types.rs:254-278`; `v6/sprefa-extract/src/wire.rs:99-108` |
| agent_touch | 5 | B | Agent session state is outside the extractor | `plans/2026-07-30-v5-parity-table.tsv:63`; `docs/reference/relations.md:8`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| changed_line | 5 | B | Git diff lines are outside the Source input | `plans/2026-07-30-v5-parity-table.tsv:64`; `docs/reference/relations.md:18`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| clock | 5 | B | Tick clock has no source spelling | `plans/2026-07-30-v5-parity-table.tsv:65`; `docs/reference/relations.md:23`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| head | 5 | B | Git HEAD state is outside Source | `plans/2026-07-30-v5-parity-table.tsv:66`; `docs/reference/relations.md:61`; `v6/sprefa-extract/src/types.rs:786-818` |
| rel_catalog | 5 | B | Built-in catalog metadata is engine state | `plans/2026-07-30-v5-parity-table.tsv:67`; `docs/reference/relations.md:82`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| scip_def | 5 | E | SCIP definitions are loaded into ScipIndex but have no FlatFact or relation adapter | `plans/2026-07-30-v5-parity-table.tsv:68`; `docs/reference/relations.md:92`; `v6/sprefa-extract/src/scip.rs:118-157`; `v6/sprefa-extract/src/lib.rs:41-48` |
| scip_name | 5 | E | ScipSymbolInfo retains display names, with no relation projection | `plans/2026-07-30-v5-parity-table.tsv:69`; `docs/reference/relations.md:97`; `v6/sprefa-extract/src/scip.rs:300-318` |
| scip_occurrence | 5 | E | ScipOccurrence retains range, symbol, and role, with no wire relation | `plans/2026-07-30-v5-parity-table.tsv:70`; `docs/reference/relations.md:98`; `v6/sprefa-extract/src/scip.rs:301-337` |
| type_link | 5 | E | TypeF resolution supplies target blobs/spans, but no sym-to-sym relation arm exists | `plans/2026-07-30-v5-parity-table.tsv:71`; `docs/reference/relations.md:113`; `v6/sprefa-extract/src/types.rs:681-717`; `v6/sprefa-extract/src/wire.rs:158-181` |
| df_field | 4 | E | DF walkers see field labels, but DfFAux contains only params and args | `plans/2026-07-30-v5-parity-table.tsv:72`; `docs/reference/relations.md:34`; `v6/sprefa-extract/src/types.rs:496-501`; `v6/sprefa-extract/src/lang/go.rs:1218-1240` |
| hook_event | 4 | B | Hook event log is external process state | `plans/2026-07-30-v5-parity-table.tsv:73`; `docs/reference/relations.md:62`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| loop_over | 4 | E | DF node vocabulary has `Loop`, while loop aux rows are dropped | `plans/2026-07-30-v5-parity-table.tsv:74`; `docs/reference/relations.md:64`; `v6/sprefa-extract/src/types.rs:503-530`; `v6/sprefa-extract/src/lang/go.rs:630-646` |
| module_unresolved | 4 | E+L | Imports are source syntax, but no resolver produces target/reason rows | `plans/2026-07-30-v5-parity-table.tsv:75`; `docs/reference/relations.md:72`; `v6/sprefa-extract/src/types.rs:406-423`; `v6/sprefa-extract/src/types.rs:587-604` |
| ref | 4 | E | Spans and interned names exist, but no occurrence/reference relation is flattened | `plans/2026-07-30-v5-parity-table.tsv:76`; `docs/reference/relations.md:81`; `v6/sprefa-extract/src/types.rs:30-139`; `v6/sprefa-extract/src/wire.rs:22-54` |
| scip_local | 4 | E | SCIP occurrence roles are retained, but enclosing-function/local projection is absent | `plans/2026-07-30-v5-parity-table.tsv:77`; `docs/reference/relations.md:96`; `v6/sprefa-extract/src/scip.rs:301-337` |
| checkout | 3 | B | Checkout is a demand sink outside Source | `plans/2026-07-30-v5-parity-table.tsv:78`; `docs/reference/relations.md:19`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| doc_comment | 3 | E+L | TS/Rust/Kotlin source comment spellings exist, with no docs facet in the extractor | `plans/2026-07-30-v5-parity-table.tsv:79`; `docs/reference/relations.md:47`; `v6/sprefa-extract/src/types.rs:1217-1226`; `v6/sprefa-extract/src/lang/kotlin.rs:27-33` |
| env | 3 | B | Process environment is outside Source | `plans/2026-07-30-v5-parity-table.tsv:80`; `docs/reference/relations.md:53`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| every | 3 | B | Tick gate has no source spelling | `plans/2026-07-30-v5-parity-table.tsv:81`; `docs/reference/relations.md:54`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| file_lines | 3 | E | Input bytes allow counting, but ExtractOutput and FlatFact have no line-count field | `plans/2026-07-30-v5-parity-table.tsv:82`; `docs/reference/relations.md:56`; `v6/sprefa-extract/src/types.rs:1217-1226`; `v6/sprefa-extract/src/types.rs:1255-1347` |
| fn_catalog | 3 | B | Scalar-function catalog is engine metadata | `plans/2026-07-30-v5-parity-table.tsv:83`; `docs/reference/relations.md:57`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| node | 3 | E | CST nodes are emitted as `record=node`, while no v6 relation projection names `node` | `plans/2026-07-30-v5-parity-table.tsv:84`; `docs/reference/relations.md:75`; `v6/sprefa-extract/src/types.rs:167-185`; `v6/sprefa-extract/src/wire.rs:56-81` |
| op_catalog | 3 | B | Operation catalog is engine metadata | `plans/2026-07-30-v5-parity-table.tsv:85`; `docs/reference/relations.md:76`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| string | 3 | E | The interner exists, but its table and normalized strings never cross the flat wire | `plans/2026-07-30-v5-parity-table.tsv:86`; `docs/reference/relations.md:104`; `v6/sprefa-extract/src/types.rs:103-139`; `v6/sprefa-extract/src/types.rs:1255-1347` |
| type_shape | 3 | B | Shape fingerprints are a derived type-shape experiment, with no extractor type | `plans/2026-07-30-v5-parity-table.tsv:87`; `docs/reference/relations.md:115`; `v6/sprefa-extract/src/types.rs:187-238` |
| agent_edit | 2 | B | Agent edit journal is external harness state | `plans/2026-07-30-v5-parity-table.tsv:88`; `docs/reference/relations.md:7`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| allocates | 2 | E | DF has `new` and call nodes, but no allocation relation projection | `plans/2026-07-30-v5-parity-table.tsv:89`; `docs/reference/relations.md:9`; `v6/sprefa-extract/src/types.rs:503-513`; `v6/sprefa-extract/src/wire.rs:183-229` |
| call_edge_rev | 2 | B | Call edges have no rev input or rev-bearing wire shape | `plans/2026-07-30-v5-parity-table.tsv:90`; `docs/reference/relations.md:13`; `v6/sprefa-extract/src/types.rs:681-699`; `v6/sprefa-extract/src/types.rs:786-818` |
| call_kind | 2 | E | Call sites and names exist, but the v5-specific read/write classifier is absent | `plans/2026-07-30-v5-parity-table.tsv:91`; `docs/reference/relations.md:14`; `v6/sprefa-extract/src/types.rs:393-404`; `v6/sprefa-extract/src/wire.rs:122-155` |
| checkout_done | 2 | B | Checkout result is demand-sink state | `plans/2026-07-30-v5-parity-table.tsv:92`; `docs/reference/relations.md:20`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| const_value | 2 | E | TS/Rust const values are extracted and flattened as `record=const` | `plans/2026-07-30-v5-parity-table.tsv:93`; `docs/reference/relations.md:25`; `v6/sprefa-extract/src/types.rs:281-305`; `v6/sprefa-extract/src/wire.rs:110-118` |
| content | 2 | E | BlobHash is computed, but there is no content relation arm | `plans/2026-07-30-v5-parity-table.tsv:94`; `docs/reference/relations.md:27`; `v6/sprefa-extract/src/types.rs:50-74`; `v6/sprefa-extract/src/bin/extract.rs:253-263` |
| crate_edge | 2 | E+L | Rust manifest data is a language/project input, while the extractor has no manifest resolver or relation output | `plans/2026-07-30-v5-parity-table.tsv:95`; `docs/reference/relations.md:28`; `v6/sprefa-extract/src/types.rs:786-818`; `v6/sprefa-extract/src/scip.rs:64-67` |
| created | 2 | B | File creation history is git metadata | `plans/2026-07-30-v5-parity-table.tsv:96`; `docs/reference/relations.md:29`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| def_target | 2 | E | Type/call definitions can be located, but no name-to-definition sink or wire row exists | `plans/2026-07-30-v5-parity-table.tsv:97`; `docs/reference/relations.md:30`; `v6/sprefa-extract/src/types.rs:876-910`; `v6/sprefa-extract/src/types.rs:953-1028` |
| df_lit | 2 | E | `DfNodeKind::Lit` exists, while literal text/kind aux storage is absent | `plans/2026-07-30-v5-parity-table.tsv:98`; `docs/reference/relations.md:36`; `v6/sprefa-extract/src/types.rs:503-540`; `v6/sprefa-extract/src/types.rs:496-501` |
| dl_diag | 2 | B | DL parser diagnostics belong to the engine and have no source-file spelling | `plans/2026-07-30-v5-parity-table.tsv:99`; `docs/reference/relations.md:46`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| doc_node | 2 | E+L | Markdown headings/code blocks have the v5 spelling, while v6 has no Markdown Source or doc family | `plans/2026-07-30-v5-parity-table.tsv:100`; `docs/reference/relations.md:48`; `plans/2026-07-29-extract-doc-formats-header.md:6-35`; `v6/sprefa-extract/src/lang/mod.rs:29-49` |
| doc_ref | 2 | E+L | The bridge requires doc nodes plus type entities; the current extractor has only the latter | `plans/2026-07-30-v5-parity-table.tsv:101`; `docs/reference/relations.md:49`; `v6/sprefa-extract/src/types.rs:189-221`; `plans/2026-07-29-extract-doc-formats-header.md:24-35` |
| effect_cmd | 2 | B | Effect command templates are runtime sink state | `plans/2026-07-30-v5-parity-table.tsv:102`; `docs/reference/relations.md:51`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| effect_log | 2 | B | Effect queue records are runtime state | `plans/2026-07-30-v5-parity-table.tsv:103`; `docs/reference/relations.md:52`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| hover_note | 2 | B | Hover notes are rule/LSP output | `plans/2026-07-30-v5-parity-table.tsv:104`; `docs/reference/relations.md:63`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| module_binding | 2 | E+L | Import spellings exist, while the extractor's Specifier omits source module and imported name | `plans/2026-07-30-v5-parity-table.tsv:105`; `docs/reference/relations.md:65`; `v6/sprefa-extract/src/types.rs:406-423`; `v6/sprefa-extract/src/lang/ts.rs:891-905` |
| module_binding_resolved | 2 | E+L | Alias bindings require a resolver target, absent from current ProjectCx and CLI | `plans/2026-07-30-v5-parity-table.tsv:106`; `docs/reference/relations.md:66-68`; `v6/sprefa-extract/src/types.rs:786-818` |
| module_edge_rev | 2 | B | Revision-aware module graph needs rev identity and module resolver output | `plans/2026-07-30-v5-parity-table.tsv:107`; `docs/reference/relations.md:70`; `v6/sprefa-extract/src/types.rs:587-604`; `v6/sprefa-extract/src/types.rs:786-818` |
| module_import | 2 | E+L | Import syntax exists for several languages, with only partial TS/Prolog phase-1 specifier capture | `plans/2026-07-30-v5-parity-table.tsv:108`; `docs/reference/relations.md:71`; `v6/sprefa-extract/src/types.rs:406-461`; `v6/sprefa-extract/src/lang/ts.rs:906-986` |
| nest | 2 | E | DF walkers carry loop nesting internally, but no nest aux or wire row exists | `plans/2026-07-30-v5-parity-table.tsv:109`; `docs/reference/relations.md:74`; `v6/sprefa-extract/src/types.rs:496-501`; `v6/sprefa-extract/src/lang/go.rs:630-646` |
| program | 2 | B | Program tracking belongs to the daemon | `plans/2026-07-30-v5-parity-table.tsv:110`; `docs/reference/relations.md:77`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| propose_clone | 2 | B | Clone proposal is a derived refactoring output | `plans/2026-07-30-v5-parity-table.tsv:111`; `docs/reference/relations.md:78`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| rel_col | 2 | B | Relation schema catalog belongs to the engine | `plans/2026-07-30-v5-parity-table.tsv:112`; `docs/reference/relations.md:83`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| rel_count | 2 | B | Relation cardinality is store/runtime state | `plans/2026-07-30-v5-parity-table.tsv:113`; `docs/reference/relations.md:84`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| rev | 2 | B | Revision identity is absent from Source and the binary path | `plans/2026-07-30-v5-parity-table.tsv:114`; `docs/reference/relations.md:86`; `v6/sprefa-extract/src/types.rs:76-82`; `v6/sprefa-extract/src/types.rs:786-818` |
| rev_behind | 2 | B | Git ancestry counts are outside Source | `plans/2026-07-30-v5-parity-table.tsv:115`; `docs/reference/relations.md:88`; `v6/sprefa-extract/src/types.rs:786-818` |
| rev_cmp_want | 2 | B | Git ancestry demand is outside Source | `plans/2026-07-30-v5-parity-table.tsv:116`; `docs/reference/relations.md:89`; `v6/sprefa-extract/src/types.rs:786-818` |
| scip_callee_type | 2 | E | SCIP symbol metadata is loaded, but moniker receiver-type projection is absent | `plans/2026-07-30-v5-parity-table.tsv:117`; `docs/reference/relations.md:91`; `v6/sprefa-extract/src/scip.rs:300-337` |
| skill_loaded | 2 | B | Skill session state is outside Source | `plans/2026-07-30-v5-parity-table.tsv:118`; `docs/reference/relations.md:102`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| stmt_ms | 2 | B | Statement timing is engine instrumentation | `plans/2026-07-30-v5-parity-table.tsv:119`; `docs/reference/relations.md:103`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| type_decl_row | 2 | B | Derived relation shape is engine state | `plans/2026-07-30-v5-parity-table.tsv:120`; `docs/reference/relations.md:107`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| type_edge_rev | 2 | B | Type edges have no revision input or rev-bearing wire shape | `plans/2026-07-30-v5-parity-table.tsv:121`; `docs/reference/relations.md:109`; `v6/sprefa-extract/src/types.rs:681-699`; `v6/sprefa-extract/src/types.rs:786-818` |
| type_entity_rev | 2 | B | Type entities have no revision input or rev-bearing wire shape | `plans/2026-07-30-v5-parity-table.tsv:122`; `docs/reference/relations.md:111`; `v6/sprefa-extract/src/types.rs:681-699`; `v6/sprefa-extract/src/types.rs:786-818` |
| type_lgg | 2 | B | Least-general-generalization is a derived type-shape experiment | `plans/2026-07-30-v5-parity-table.tsv:123`; `docs/reference/relations.md:112`; `v6/sprefa-extract/src/types.rs:187-238` |
| type_link_rev | 2 | B | Revision-aware type links need rev identity and a type-link projector | `plans/2026-07-30-v5-parity-table.tsv:124`; `docs/reference/relations.md:114`; `v6/sprefa-extract/src/types.rs:681-699`; `v6/sprefa-extract/src/types.rs:786-818` |
| call_def_rev | 1 | B | Call defs have no revision input or rev-bearing wire shape | `plans/2026-07-30-v5-parity-table.tsv:125`; `docs/reference/relations.md:11`; `v6/sprefa-extract/src/types.rs:681-699`; `v6/sprefa-extract/src/types.rs:786-818` |
| checkout_plan | 1 | B | Checkout preview is a demand sink | `plans/2026-07-30-v5-parity-table.tsv:126`; `docs/reference/relations.md:21`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| child | 1 | E | CST edges are present as `record=edge`, but no named `child` relation is projected | `plans/2026-07-30-v5-parity-table.tsv:127`; `docs/reference/relations.md:22`; `v6/sprefa-extract/src/types.rs:165-185`; `v6/sprefa-extract/src/wire.rs:56-81` |
| const_value_rev | 1 | B | Const rows have no revision input or rev-bearing wire shape | `plans/2026-07-30-v5-parity-table.tsv:128`; `docs/reference/relations.md:26`; `v6/sprefa-extract/src/types.rs:281-305`; `v6/sprefa-extract/src/types.rs:786-818` |
| df_arg_rev | 1 | B | DF arg rows have no revision input or rev-bearing wire shape | `plans/2026-07-30-v5-parity-table.tsv:129`; `docs/reference/relations.md:32`; `v6/sprefa-extract/src/types.rs:486-501`; `v6/sprefa-extract/src/types.rs:786-818` |
| df_field_rev | 1 | B | DF fields need both a missing aux row and revision identity | `plans/2026-07-30-v5-parity-table.tsv:130`; `docs/reference/relations.md:35`; `v6/sprefa-extract/src/types.rs:496-501`; `v6/sprefa-extract/src/types.rs:786-818` |
| df_lit_rev | 1 | B | DF literal text is absent and revision identity is absent | `plans/2026-07-30-v5-parity-table.tsv:131`; `docs/reference/relations.md:37`; `v6/sprefa-extract/src/types.rs:496-501`; `v6/sprefa-extract/src/types.rs:786-818` |
| df_node_repo | 1 | B | Repo attachment to DF nodes needs project identity | `plans/2026-07-30-v5-parity-table.tsv:132`; `docs/reference/relations.md:39`; `v6/sprefa-extract/src/types.rs:472-501`; `v6/sprefa-extract/src/types.rs:786-818` |
| df_node_repo_rev | 1 | B | Repo and revision attachments need project identity | `plans/2026-07-30-v5-parity-table.tsv:133`; `docs/reference/relations.md:40`; `v6/sprefa-extract/src/types.rs:472-501`; `v6/sprefa-extract/src/types.rs:786-818` |
| df_node_rev | 1 | B | DF nodes have no revision input or rev-bearing wire shape | `plans/2026-07-30-v5-parity-table.tsv:134`; `docs/reference/relations.md:41`; `v6/sprefa-extract/src/types.rs:472-501`; `v6/sprefa-extract/src/types.rs:786-818` |
| diag_mute | 1 | B | Diagnostic mute state belongs to the LSP session | `plans/2026-07-30-v5-parity-table.tsv:135`; `docs/reference/relations.md:44`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| doc_tag | 1 | E+L | JSDoc/KDoc/rustdoc spellings exist, while the extractor has no docs parser or doc wire family | `plans/2026-07-30-v5-parity-table.tsv:136`; `docs/reference/relations.md:50`; `v6/sprefa-extract/src/types.rs:1217-1226`; `plans/2026-07-29-extract-doc-formats-header.md:24-35` |
| git_ref | 1 | B | Git refs are external repository state | `plans/2026-07-30-v5-parity-table.tsv:137`; `docs/reference/relations.md:58`; `v6/sprefa-extract/src/types.rs:786-818` |
| graph_edge | 1 | B | Graph edges are rule-headed visualization output | `plans/2026-07-30-v5-parity-table.tsv:138`; `docs/reference/relations.md:59`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| graph_node | 1 | B | Graph nodes are rule-headed visualization output | `plans/2026-07-30-v5-parity-table.tsv:139`; `docs/reference/relations.md:60`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| module_binding_resolved_rev | 1 | B | Alias resolution needs module targets plus revision identity | `plans/2026-07-30-v5-parity-table.tsv:140`; `docs/reference/relations.md:67`; `v6/sprefa-extract/src/types.rs:587-604`; `v6/sprefa-extract/src/types.rs:786-818` |
| module_binding_rev | 1 | B | Binding rows need a richer module aux shape plus revision identity | `plans/2026-07-30-v5-parity-table.tsv:141`; `docs/reference/relations.md:68`; `v6/sprefa-extract/src/types.rs:406-461`; `v6/sprefa-extract/src/types.rs:786-818` |
| module_unresolved_rev | 1 | B | Unresolved module rows need resolver output plus revision identity | `plans/2026-07-30-v5-parity-table.tsv:142`; `docs/reference/relations.md:73`; `v6/sprefa-extract/src/types.rs:587-604`; `v6/sprefa-extract/src/types.rs:786-818` |
| propose_extract | 1 | B | Extract-function proposals are derived refactoring output | `plans/2026-07-30-v5-parity-table.tsv:143`; `docs/reference/relations.md:79`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| query_log | 1 | B | Query history belongs to the daemon | `plans/2026-07-30-v5-parity-table.tsv:144`; `docs/reference/relations.md:80`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| rev_advanced | 1 | B | Ref advancement is daemon/git state | `plans/2026-07-30-v5-parity-table.tsv:145`; `docs/reference/relations.md:87`; `v6/sprefa-extract/src/types.rs:786-818` |
| scip_binding | 1 | E | SCIP occurrences are retained, but local-name binding projection is absent | `plans/2026-07-30-v5-parity-table.tsv:146`; `docs/reference/relations.md:90`; `v6/sprefa-extract/src/scip.rs:301-337` |
| scip_impl | 1 | E | SCIP implementation relationships are dropped by the diet and have no relation adapter | `plans/2026-07-30-v5-parity-table.tsv:147`; `docs/reference/relations.md:95`; `v6/sprefa-extract/src/scip.rs:234-270` |
| scip_ref | 1 | E | SCIP definitions/occurrences are retained, but reference-to-definition rows are absent | `plans/2026-07-30-v5-parity-table.tsv:148`; `docs/reference/relations.md:99`; `v6/sprefa-extract/src/scip.rs:300-337` |
| scip_want | 1 | B | SCIP index demand is engine/repository orchestration | `plans/2026-07-30-v5-parity-table.tsv:149`; `docs/reference/relations.md:100`; `v6/sprefa-extract/src/scip.rs:69-116` |
| similar | 1 | B | Embedding backend output is outside Source | `plans/2026-07-30-v5-parity-table.tsv:150`; `docs/reference/relations.md:101`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| template_parts | 1 | E | TS AST sees templates, but no template aux or wire arm exists | `plans/2026-07-30-v5-parity-table.tsv:151`; `docs/reference/relations.md:105`; `v6/sprefa-extract/src/lang/ts.rs:2067-2080`; `v6/sprefa-extract/src/types.rs:496-501` |
| unresolved | 1 | E+L | Dynamic import/computed-call spellings exist in TS/JS, but no unresolved aux or FlatFact arm exists | `plans/2026-07-30-v5-parity-table.tsv:152`; `docs/reference/relations.md:117`; `v6/sprefa-extract/src/types.rs:1255-1347`; `v6/sprefa-extract/src/lang/ts.rs:838-865` |
| verb_catalog | 1 | B | CLI verb catalog belongs to the engine | `plans/2026-07-30-v5-parity-table.tsv:153`; `docs/reference/relations.md:118`; `v6/sprefa-extract/src/types.rs:1228-1236` |
| diag_stage | 0 | B | Diagnostic routing state has no source spelling | `plans/2026-07-30-v5-parity-table.tsv:154`; `docs/reference/relations.md:45`; `v6/sprefa-extract/src/types.rs:1228-1236` |

Counts from the labels in this table: `E` = 31, `E+L` = 13, `B` = 62. The
extractor-owned side is therefore 44 rows when `E+L` is included; the
language/project/runtime side is 62 rows. The highest-usage extractor rows in
the requested sample are `comment_node` at 6, `type_entity` at 22, `call_edge`
at 20, `call_def` at 17, and `type_edge` at 15. The v5 op usage counts for
`gen`, `comment`, `closure`, and `scc` are separate op rows at
`plans/2026-07-30-v5-parity-table.tsv:21-24`; their absent relation outputs are
captured by the relation rows above, especially `comment_node`, `type_entity`,
`call_edge`, and `type_edge`.

## 4. TypeScript module resolver and the Madge question

Confirmed. The v6 extractor has no resolved TypeScript `dep(src,dst)` relation.
The current data shape stores `Specifier { span, name, kind }`; it omits the
source module and imported name at `v6/sprefa-extract/src/types.rs:406-423`.
The ModuleF family is explicitly commented out at
`v6/sprefa-extract/src/types.rs:587-604`. The CLI resolve adapter dispatches
only `Resolve<CallF>` at `v6/sprefa-extract/src/bin/extract.rs:324-335`, so
`--resolve` does not add module edges.

Existing reachability:

| existing component | receipt | consequence |
|---|---|---|
| TS phase-1 import/export scan | `v6/sprefa-extract/src/lang/ts.rs:883-1000` | Import forms are already parsed once and can provide specifier spans and local names |
| SCIP TypeScript subprocess | `v6/sprefa-extract/src/scip.rs:47-116` | `scip-typescript index` can create a resolved index in a fresh temp directory |
| SCIP diet and occurrence joins | `v6/sprefa-extract/src/scip.rs:234-337` | Documents, occurrences, symbols, roles, and byte-range conversion exist; relationships are dropped from the diet |
| SCIP call ratchet | `v6/sprefa-extract/tests/golden_parity.rs:607-875` | The real indexer already resolves TS call occurrences inside the library test |
| v5 module resolver contract | `src/graph/modgraph/mod.rs:1-15`, `src/graph/modgraph/mod.rs:34-76`, `src/graph/modgraph/mod.rs:173-188` | v5's required file/external/unresolved target states, bindings, manifest indexes, and resolver registration are specified in source |

The Madge oracle is therefore reachable without first writing a bespoke TS
resolver if the implementation accepts SCIP as the project graph source. The
minimum projection would build `ScipTypescript` over the same TS/JS root, map
SCIP document paths and import/reference occurrences to file pairs, and emit a
sorted `dep(src,dst)` relation or equivalent JSONL. The current `ScipIndex`
diet drops SCIP relationships, so the loader would need to retain or derive
file-to-file edges from occurrences and definitions. A direct resolver route
would instead add a ModuleF or module-resolver seam, retain source-module text,
read `package.json`/`tsconfig.json` workspace context, implement relative,
package, extension, index, alias, and unresolved outcomes, then project
`module_import`, `module_edge`, and `module_unresolved`. The existing
`ProjectCx.manifests` field is declared but hollow in this crate at
`v6/sprefa-extract/src/types.rs:786-818`.

Conclusion: the missing TS module resolver is the blocker for a direct
extractor-native Madge projection. It is not a blocker for a SCIP-backed Madge
oracle, provided the SCIP file-edge projection is added and the dropped
relationship data is retained or reconstructed.

## 5. Document-source and identity gaps

Confirmed at HEAD. The CLI documents source coverage as TS/JS, Rust, Go,
Kotlin, Prolog, and ast-grep fallback at
`v6/sprefa-extract/src/bin/extract.rs:45-55`. The roster contains no Markdown,
HTML, XML, TOML, or YAML Source at `v6/sprefa-extract/src/lang/mod.rs:29-49`.
The existing document-format plan records the same gap and the proposed family
shape at `plans/2026-07-29-extract-doc-formats-header.md:6-45`.

| requested item | HEAD state | cost to add | receipts |
|---|---|---|---|
| Markdown Source | No Source or grammar roster entry. V5 has `doc_node` and comment parsing. | Add or buy a Markdown grammar, register a Source, define CST/doc/comment rows, add fixtures, snapshots, CLI golden, and decide block versus inline grammar. | `v6/sprefa-extract/src/lang/mod.rs:29-49`; `docs/reference/relations.md:48`; `plans/2026-07-29-extract-doc-formats-header.md:17-35`, `:42-57` |
| HTML Source | No Source or grammar roster entry. | Add grammar dependency and Source, define element/attribute/text doc rows and parse-error policy, then fixtures and tests. | `v6/sprefa-extract/src/lang/mod.rs:29-49`; `plans/2026-07-29-extract-doc-formats-header.md:17-35`, `:50-57` |
| XML Source | No Source or grammar roster entry. | Add grammar dependency and Source, define the same element-path doc shape, and add fixtures/tests. | `v6/sprefa-extract/src/lang/mod.rs:29-49`; `plans/2026-07-29-extract-doc-formats-header.md:17-35` |
| TOML Source | No Source or grammar roster entry. | Add grammar dependency and Source, define key-path/value doc rows, choose canonical path spelling, and add fixtures/tests. | `v6/sprefa-extract/src/lang/mod.rs:29-49`; `plans/2026-07-29-extract-doc-formats-header.md:17-32`, `:47-57` |
| YAML Source | No Source or grammar roster entry. | Add grammar dependency and Source, define key-path/value rows and anchor/alias cycle behavior, and add fixtures/tests. | `v6/sprefa-extract/src/lang/mod.rs:29-49`; `plans/2026-07-29-extract-doc-formats-header.md:17-32`, `:47-57` |
| BlobSource implementation | `BlobSource` is a trait only: `blob(path) -> Option<Vec<u8>>`; no implementation is present in the crate. | Implement a reader/cache over the selected file set, define path normalization and read failure behavior, then thread it into `ProjectCx` and phase-2 cache ownership. | `v6/sprefa-extract/src/types.rs:775-780`; `v6/sprefa-extract/src/types.rs:801-811`; `v6/sprefa-extract/src/types.rs:1391-1392` |
| Repo type | No `Repo` type is exported or stored in `ExtractOutput`, `ProjectCx`, or the flat wire. | Add repository identity and root ownership to project context and every repo-scoped output contract, then define joins and tests. | `v6/sprefa-extract/src/lib.rs:44-52`; `v6/sprefa-extract/src/types.rs:786-818`; `v6/sprefa-extract/src/types.rs:1217-1226` |
| Rev type | `ProjectDigest` exists as a 16-byte placeholder. It is not a revision identity and is not computed from a repository revision. | Add a rev identity type, connect it to Repo/FileSet/ManifestMap, compute the digest, and add rev-bearing rows and cache keys. | `v6/sprefa-extract/src/types.rs:76-82`; `v6/sprefa-extract/src/types.rs:804-811`; `v6/sprefa-extract/src/types.rs:1035-1039` |

The cost statements above are source-scoped. Dependency version, maintenance,
dirty-file parse behavior, and the canonical key-path spelling are left as
questions in the existing plan at `plans/2026-07-29-extract-doc-formats-header.md:16-22`
and `:47-57`.

## Verification receipt

Hermetic local extractor tests were run from `v6/sprefa-extract` with
`SPREFA_CONFIG=/nonexistent/x.toml`, `DL_NO_DAEMON=1`, and
`CARGO_TARGET_DIR=/private/tmp/sprefa-extract-spelunk-target`.

* `tests/1_resolve_cli.rs`: 2 passed.
* `tests/2_df_aux_cli.rs`: 1 passed.
* `tests/3_ast_pattern_cli.rs`: 2 passed.
* `tests/snapshot.rs`: 2 passed.
* `tests/0_prolog.rs` filtered to `prolog_all_families_and_names` and
  `prolog_name_arity_resolution`: 1 passed each.
* The unfiltered Prolog ledger test fails its existing corpus-size assertion:
  `tests/0_prolog.rs:120` reports 92 files while the committed expectation is
  56. No production or test file was changed.

The SCIP ratchet tests were not run. They invoke scip-typescript, scip-go, and
rust-analyzer over their fixture projects at
`v6/sprefa-extract/tests/golden_parity.rs:638-1222`.
