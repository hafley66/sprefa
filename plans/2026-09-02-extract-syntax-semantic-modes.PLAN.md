# plan: extract syntax mode and semantic mode over one TSI fact stream

Implementation plan for issue `@extract-semantic-fact-roundtrip`
(`issues/extract-semantic-fact-roundtrip/item.md`, origin/main `9213095f9`,
filed untriaged by a Codex session, disposition still the user's).
Plain-words twin: `2026-09-02-extract-syntax-semantic-modes.PLAN.visual.human.unga.md`.

## TOC

1. Sources this plan answers to
2. Acceptance criteria mapped to arcs
3. Where extract is today (cited)
4. Wire design: TSI relations on the FlatFact JSONL
5. Identity
6. Reverse door: decode, validate, canonicalize, re-emit
7. Type signatures
8. Storage layout, reads and writes, uniqueness
9. Arcs (PR each, files owned, receipts)
10. Test plan
11. Forks for the user
12. Out of scope

## 1. Sources

| source | where | what it fixes |
|---|---|---|
| the issue | `issues/extract-semantic-fact-roundtrip/item.md` on origin/main, `15e95de83` | nine acceptance criteria (section 2); the `## Decisions` note of 2026-09-02T21:19:27Z is the self-contained contract: the 18 `tsi.*` relations, the five identity rules, the mode contract, the native namespaces |
| TSI reference | `.agents/skills/sprf-dl7-prolog-compiler/references/4_polyglot_type_fact_protocol.md`, branch `perf/v7-cold-compile` `368a1eebd`, worktree `/private/tmp/sprefa-v7-value-nodes` | the longer form of the same vocabulary, the `accepted/1` view, six open decisions; commits `a99d7c3bf`, `5b98ea5e8`, `430d69fc7`, `6187a6ede`, `368a1eebd` |
| pipeline reference | same dir, `2_cst_extract_pipeline.md` section 5 | the measured probe; the five reverse-door gaps |
| the probe run | `extract --resolve --family type --ts-checker` / `--rust-checker` over `/private/tmp/tsi_extract_probe.{ts,rs}`, 2026-09-02 | one `resolved_type_edge` per language, nothing else |

The issue's `## Decisions` note is the contract every lane cites. The lab
reference stays on its branch; no copy onto main is needed.

## 2. Acceptance criteria mapped to arcs

| # | criterion (issue text, shortened) | arc | receipt |
|---|---|---|---|
| 1 | stream has an explicit protocol version | A1 | first row of every `--witness` stream is `record=protocol version=1` |
| 2 | rows decode and validate, not only serialize | A3 | `FlatFact: Deserialize`; `extract --ingest` rejects a bad arity with a named error |
| 3 | a reverse door accepts foreign rows and emits canonical order | A3 | `extract --ingest foreign.jsonl` prints sorted canonical JSONL |
| 4 | syntax runs identify themselves, declare partial coverage | A1, A4 | `record=run mode=syntax`; `record=coverage coverage=partial` per relation |
| 5 | semantic runs declare complete only where every reachable row was enumerated | A5, A6 | `coverage=complete` emitted by the adapter after its walk, never by default |
| 6 | TS semantic: generic params and args, optional, readonly, callable in/out, conditional and mapped operators | A5 | fixture rows for each, section 10 |
| 7 | Rust semantic: generic params and args, trait impls, associated types, callable in/out, lifetimes, ownership | A6 | fixture rows for each |
| 8 | equivalent TS and Rust fixtures share TSI relations, differ in namespaced rows | A8 | the intersection query over both streams returns equal row sets |
| 9 | DL7 imports accepted rows as comptime relations, semantic replaces syntax | A7 | `accepted/1` plunit; a syntax row disappears when a complete semantic run lands |

## 3. Where extract is today

Wire envelope: `FlatFact` `v6/sprefa-extract/src/types.rs:2811`, family tag
`src/types.rs:88`. Contract text `src/schema.rs`. Flatten `src/wire.rs:31-80`.
`Serialize` only; zero `Deserialize`, zero ingest paths (probe receipt,
confirmed by `grep -rn "Deserialize" src/types.rs` = 0 on `FlatFact`).

Per-file identity: `record=file path digest bytes lines` (`schema.rs:55`).

Tiers are a bench axis only: `Tier { Syntax, Checker, Scip }`
`tests/bench/mod.rs:69`. Library flags `--rust-checker`, `--ts-checker`
(`src/bin/extract.rs:98-112`), `ResolveRequest` `src/project.rs:112-115`.

The checker tiers answer DESTINATIONS of references the parse found
(`ts_checker.rs:1-2`, `rust_checker.rs:1-2`):

| tier | sites walked | answer |
|---|---|---|
| ts `ts_checker.mjs:180-213` | Call/New via `getResolvedSignature`, Jsx tag, `TypeReferenceNode` | `[start,end,name,dstPath,dstName,dstOffset]` |
| rust `rust_checker_ra.rs:140-190` | `MethodCallExpr`, `CallExpr(PathExpr)`, `RecordExpr`, `Path` to Adt/Trait/TypeAlias | `CheckerRef`, same six fields |

What the syntax type plane drops, per TSI relation:

| TSI relation | today | drop site |
|---|---|---|
| `tsi.edge` label and position | `TypeEdgeCandidate (owner, to, kind)`, no label, no ordinal | `src/types.rs:342` |
| `tsi.parameter` | `Generic` edge to the constraint only | `ts.rs:843-860`, `rust_type_edges.rs` `generic_candidates` |
| `tsi.called` / `tsi.argument` | generic args flattened to unordered refs | `ts.rs:955,964,1036,1076`; `rust_type_refs.rs:81` |
| `ts.optional` / `ts.readonly` | `readonly` read as a filter, never emitted | `ts.rs:1001` |
| `tsi.input` / `tsi.output` | `Sig` rows, `ty` TEXT unresolved | `src/types.rs:281`; `schema.rs` legend "UNRESOLVED in phase 1" |
| rust associated types | none | `rust_checker_ra.rs:222-235` walks paths only |
| `tsi.primitive` | checker `External` drops the row | `ts_checker.rs:28-33`, `rust_checker.rs:27-32` |
| witness per method | ONE `resolution_origin`, first leg wins | `ResolutionOrigin` `src/types.rs:1522` |

Only consumer today: `v6/sprefa-engine-rs/src/hosts.rs:1041-1080` folds
`resolved_edge` and `resolved_type_edge`. It stays byte-compatible.

v7 side: `v7/src/2_comptime/0b_filesystem_grapher.pl` `install_project_graph/6`
is the ingest precedent (`basement_program(root_graph(Nodes, Edges), ...)`
consumed at `v7/src/2_comptime/1_checker.pl:18`). Identities `ref(...)`,
canonical ids via `intern/3` (`v7/src/1_libtime/0_evaluator.pl:439-497`).

## 4. Wire design

The `FlatFact` enum stays closed. Four envelope records carry the protocol,
and ONE open record carries every TSI relation, so the relation set grows
without an enum edit (pipeline reference gap 5).

```text
record=protocol  version=<u32>
record=run       run=<u32> mode=syntax|semantic tool=<slug> version=<string> scope=[<digest>...]
record=fact      fact=<u32> relation=<ns.name> args=[<arg>...]
record=witness   fact=<u32> run=<u32> method=<slug>
record=coverage  run=<u32> relation=<ns.name> coverage=partial|complete
record=diagnostic run=<u32> relation=<ns.name> detail=<string>
```

A `diagnostic` row is mandatory beside every `partial` coverage row a
semantic run emits (contract: "unsupported coverage stays partial with an
explicit diagnostic"). Syntax runs emit none; partial is their mode.

`<arg>` is one of:

```text
{"id": <u32>}                        a run-local type, edge, symbol or list id
{"span": [digest, start, end]}       a source range (tsi.origin, tsi.has_type)
{"text": "..."} | {"int": n} | {"atom": "..."}
```

Relation names are the TSI reference's, verbatim:

| family | relations |
|---|---|
| kernel | `tsi.type/1 tsi.denotes/2 tsi.has_type/2 tsi.origin/3 tsi.product/1 tsi.sum/1 tsi.callable/1 tsi.primitive/2 tsi.edge/5 tsi.parameter/4 tsi.called/3 tsi.argument/3 tsi.input/3 tsi.output/3` |
| semantic | `tsi.subtype/3 tsi.assignable/3 tsi.conforms/3 tsi.equivalent/3` |
| ts | `ts.interface/1 ts.conditional/5 ts.mapped/4 ts.readonly/1 ts.optional/1` |
| rust | `rust.trait/1 rust.impl/3 rust.lifetime/2 rust.ownership/2 rust.assoc/3` |
| go | `go.interface/1 go.type_set/2 go.embedding/2` |

`rust.assoc(OwnerTypeId, Name, TargetTypeId)` is the one relation added
beyond the reference; associated types are an issue criterion with no row
in the reference vocabulary.

Every relation has one row in a registry table (`src/tsi/registry.rs`):
name, arity, arg kinds. The registry is data, printed by `--schema`, and
is what the decoder validates against. A relation not in the registry is
a named stop, never a silent skip.

`method` is today's `ResolutionOrigin` vocabulary plus `parse`,
`checker_walk` (a semantic-mode enumeration, not a per-site answer), and
`foreign` (a row that arrived through `--ingest`).

Existing rows (`node`, `edge`, `resolved_type_edge`, ...) are untouched.
With `--witness` off, every golden under `tests/fixtures/resolve/` stays
byte-identical. With it on, existing rows additionally carry `fact=<u32>`
so a witness can name them.

The syntax tier emits TSI rows too (criterion 4): `tsi.product`, `tsi.sum`,
`tsi.edge` with label and position, `tsi.parameter`, `ts.optional`,
`ts.readonly`, `tsi.input`, `tsi.output` with targets as `tsi.type` ids
whose `tsi.origin` is the written name's span. It cannot emit
`tsi.called` for a computed type, `ts.mapped`, `ts.conditional`,
`rust.assoc`, `rust.lifetime`, `rust.ownership`, or any `tsi.conforms`
beyond an explicit `implements`/`impl` clause. Its coverage rows say so.

## 5. Identity

The contract's five identity rules, applied to the wire:

| rule | on the wire (before closure) | canonical (v7 `intern/3`) |
|---|---|---|
| 1 nominal type = resolved symbol | run-local `{"id"}` plus `tsi.origin(Id, Lang, span)` of the declaration name | `intern(nominal, [Lang, Digest, Start, End])` |
| 2 anonymous structural type = closed ordered edge graph | run-local id plus its `tsi.edge` rows | `intern(structural, [ordered (Label, Target) list])` |
| 3 call result = callee plus ordered args | `tsi.called(Result, Callee, ArgList)` + `tsi.argument` | `intern(Callee, Args)` (existing application identity, `0_evaluator.pl:492`) |
| 4 generic parameter = declaration symbol plus ordinal | `tsi.parameter(Param, Ctor, Pos, Variance)` | `intern(parameter, [Ctor, Pos])` |
| 5 fact id = relation plus canonical args | run-local `fact=` ordinal; `witness(fact, run, method)` | dictionary row keyed `(relation, canonical args)`; two runs witness one row |

Rule 5 is the natural key `(relation, canonical args)`. Two runs over the same digests spell the same key; the store's
dictionary table mints the INTEGER id (`.claude/skills/sql-relational-design`).
No hash string travels on the wire. A resolution result is never part of a
key; it is a witnessed row of its own (`tsi.denotes`, `tsi.has_type`), which
is what lets a syntax guess and a checker answer be two witnesses on
different facts about one occurrence, and lets `accepted/1` pick.

Recursive types (`type Node<T> = { next: Node<T> }`, `struct List { next:
Box<List> }`) close through ids: the adapter emits the `tsi.edge` row whose
target is the owner's own id and stops. No adapter unrolls; no depth cap
exists on the wire. `--ingest`'s id closure (section 6) is a fixpoint over
the row set, so a cycle is one pass.

## 6. Reverse door

`extract --ingest <jsonl>...` (conflicts with every other mode flag):

```text
step 0  read line       -> serde `FlatFact` decode; a closed-arm row that fails is error `ingest_decode(line, serde msg)`
step 1  fact row        -> registry lookup: relation known, arity equal, arg kinds match; else `ingest_relation(line, relation, why)`
step 2  id closure      -> every `{"id"}` referenced is declared by a `tsi.type`/edge/list row in the same run; else `ingest_dangling(line, id)`
step 3  coverage        -> a `complete` row for a relation with zero fact rows in scope is `ingest_coverage(run, relation)`; complete with rows is accepted
step 4  canonicalize    -> renumber ids in first-appearance order, sort with `sorted_lines` (`src/project.rs:567`)
step 5  re-emit         -> stdout JSONL, `method=foreign` witness added to every ingested fact, protocol row first
steady state: ingest(ingest(x)) == ingest(x), tested
```

Build-vs-buy for decode and validation:

| candidate | fit | verdict |
|---|---|---|
| `serde` derive `Deserialize` on `FlatFact` (already a dep) | closed arms, tagged enum, zero new deps | take |
| `schemars` derive on `FlatFact` | publishes a JSON Schema for `--schema --json`, foreign producers validate offline | take, feature-gated |
| `jsonschema` crate validating each line against that schema | catches shape errors for foreign streams | take for `--ingest --strict` only; the registry check is the fast path |
| hand-written per-record validators | every arm twice | no |
| SWI `library(http/json)` on the v7 side | reads the accepted stream, already in SWI | take for A7 |

## 7. Type signatures

Rust, `v6/sprefa-extract/src/tsi/`:

```rust
// types.rs
pub enum Mode { Syntax, Semantic }

pub struct RunOut { pub run: u32, pub mode: Mode, pub tool: &'static str, pub version: String, pub scope: Vec<String> }

pub enum Arg { Id(u32), Span { digest: String, start: u32, end: u32 }, Text(String), Int(i64), Atom(String) }

pub struct FactOut { pub fact: u32, pub relation: &'static str, pub args: Vec<Arg> }
pub struct WitnessOut { pub fact: u32, pub run: u32, pub method: Method }
pub struct CoverageOut { pub run: u32, pub relation: &'static str, pub complete: bool }
// FlatFact gains: Protocol { version } | Run(RunOut) | Fact(FactOut) | Witness(WitnessOut) | Coverage(CoverageOut) | Diagnostic(DiagnosticOut)
// and `fact: Option<u32>` (skip_serializing_if none) on every existing arm.

// registry.rs
pub enum ArgKind { Id, Span, Text, Int, Atom }
pub struct Relation { pub name: &'static str, pub args: &'static [ArgKind] }
pub const REGISTRY: &[Relation];           // one row per relation in section 4
pub fn relation(name: &str) -> Option<&'static Relation>;

// sink.rs: what every adapter writes into, syntax or semantic
pub struct TsiSink { ids: u32, facts: Vec<FactOut>, witnesses: Vec<WitnessOut>, coverage: Vec<CoverageOut>, run: u32, method: Method }
impl TsiSink {
    pub fn fresh_id(&mut self) -> u32;
    pub fn fact(&mut self, relation: &'static str, args: Vec<Arg>) -> u32;   // registry-checked in debug
    pub fn complete(&mut self, relation: &'static str);
    pub fn partial(&mut self, relation: &'static str);
}

// ingest.rs
pub enum IngestError { Decode { line: usize, detail: String }, Relation { line: usize, relation: String, detail: String }, Dangling { line: usize, id: u32 }, Coverage { run: u32, relation: String } }
pub fn ingest(lines: impl Iterator<Item = String>) -> Result<Vec<String>, IngestError>;
// pseudo: decode each; validate facts against REGISTRY; collect declared ids;
// check references; check coverage; renumber; add foreign witness; sorted_lines.
```

Semantic adapters:

```text
ts_checker.mjs, new `tsi` arm per file (ts-checker feature):
  for each declaration symbol: type = checker.getDeclaredTypeOfSymbol(sym)
    tsi.type, tsi.denotes(symbolId, typeId), tsi.origin
    product/sum/callable by type.flags; ts.interface for interfaces
    tsi.parameter per typeParameter (variance from checker.getTypeParameterVariance where exposed, else invariant)
    tsi.edge per checker.getPropertiesOfType(type), position = declaration order
      ts.optional if prop.flags & Optional; ts.readonly if isReadonlySymbol
    tsi.input/output per checker.getSignaturesOfType(type, Call)
    ts.mapped / ts.conditional from the declaration's type node kind
    tsi.called/argument for every TypeReference with typeArguments
  for each identifier occurrence: tsi.has_type(occurrenceSpan, typeId)
  coverage complete for every relation the arm enumerates; partial for tsi.conforms

rust_checker_ra.rs, new `tsi` walk (rust-checker feature):
  hir::Adt (Struct/Enum/Union) -> tsi.product|sum, fields -> tsi.edge, generic_params -> tsi.parameter
    lifetime params -> rust.lifetime; field ty Ref/Box/Arc -> rust.ownership(edge, shared|exclusive|owned)
  hir::Trait -> rust.trait; items TypeAlias -> rust.assoc; hir::Impl -> rust.impl + tsi.conforms(Type, Trait, implSymbol)
  hir::Function -> tsi.callable, params -> tsi.input, ret_type -> tsi.output
  hir::Type::type_arguments -> tsi.called / tsi.argument
  hir::Type::as_builtin -> tsi.primitive(Id, class)
```

Prolog, `v7/src/0_reader/6_extract_loader.pl`:

```prolog
%% load_tsi_stream(+JsonlPath, -Rows, -Diagnostics) is det.
%  json_read_dict/2 per line; rows are extract_run/5, extract_fact/3,
%  extract_witness/3, extract_coverage/3 exactly as the reference spells them.

%% accepted(?Fact)  the reference's two stratified clauses, verbatim.

%% install_tsi_graph(+Rows, +Basements0, +Origins0, -Basements, -Origins, -Diagnostics) is det.
%  accepted facts only. tsi.product -> product node; tsi.edge -> :(Owner, Label, Target, Position);
%  tsi.called/argument -> intern(Callee, Args); tsi.primitive -> prelude primitive by class;
%  namespaced rows -> comptime relations of the same name, untouched.
```

## 8. Storage layout, reads and writes, uniqueness

| type | born | dies |
|---|---|---|
| `RunOut` | one per tier that ran in a `ResolveRequest` | end of `resolve_project` |
| run-local id | `TsiSink::fresh_id` | end of stream; renumbered by ingest |
| `WitnessOut` | per (fact, method) in the fold | flattened, dropped |
| v7 node | loader | with the basement |

Store (one db, `~/.agent/dl6.db`; natural keys once, in dictionaries):

```sql
CREATE TABLE tsi_run      ("__id" INTEGER PRIMARY KEY, mode_id INTEGER, tool_id INTEGER, version TEXT, protocol INTEGER);
CREATE TABLE tsi_relation ("__id" INTEGER PRIMARY KEY, name TEXT UNIQUE, arity INTEGER);
CREATE TABLE tsi_fact     ("__id" INTEGER PRIMARY KEY, relation_id INTEGER, key TEXT, UNIQUE (relation_id, key));
CREATE TABLE tsi_arg      ("__id" INTEGER PRIMARY KEY, fact_id INTEGER, position INTEGER, kind INTEGER, int_value INTEGER, text_id INTEGER, UNIQUE (fact_id, position));
CREATE TABLE tsi_witness  ("__id" INTEGER PRIMARY KEY, fact_id INTEGER, run_id INTEGER, method_id INTEGER, UNIQUE (fact_id, run_id, method_id));
CREATE TABLE tsi_coverage ("__id" INTEGER PRIMARY KEY, run_id INTEGER, relation_id INTEGER, complete INTEGER, UNIQUE (run_id, relation_id));
```

`tsi_fact.key` is the canonical argument spelling after `intern`, the one
TEXT natural key per fact, UNIQUE with its relation. Writes on ingest: run,
relations, facts (one `insert_rows`), args, witnesses, coverage. Reads:
`accepted/1` is one join of fact, witness, run, coverage.

Uniqueness: witness unique per `(fact, run, method)`; a re-run mints a new
run row and its witnesses; `accepted/1` reads the newest complete semantic
run per `(scope, relation)`.

## 9. Arcs

Every arc is one PR from `origin/main`, native opus lane, disjoint files.
Briefs: `plans/2026-09-02-tsi-A1-envelope.BRIEF.md`, `plans/2026-09-02-tsi-A3-ingest.BRIEF.md`, `plans/2026-09-02-tsi-A2-multi-witness.BRIEF.md`, `plans/2026-09-02-tsi-A4-syntax-rows.BRIEF.md`.
Order: A1, then A3, then A2 (each shares `extract.rs` with the one before); A4 after A3 (`TsiSink`); A5, A6 after A4; A7 any time after A3 (fixture streams hand-written to the wire spec); A8 last.
Gate for every extract arc:

```bash
cd v6/sprefa-extract && cargo test --features ts-checker,rust-checker 2>&1 | tail -3
cd v6/sprefa-extract && cargo run --bin extract -- --resolve --family type tests/fixtures/ts/sample.ts | diff - tests/fixtures/resolve/5_resolved_type_edges.jsonl
```

The diff line prints nothing on every arc.

| arc | files owned | deliverable | receipt |
|---|---|---|---|
| A1 envelope | `src/tsi/{mod,types}.rs`, `src/types.rs` (arms), `src/schema.rs`, `src/wire.rs`, `src/bin/extract.rs` (`--witness`) | `protocol`, `run`, `witness`, `coverage` records; `fact=` on every row under the flag; `Deserialize` on every closed arm | `tests/96_witness_wire.rs`: first row is `protocol`; goldens byte-identical without the flag; decode(encode(x)) == x over every golden |
| A2 multi-witness fold (after A3 merges; shares `extract.rs`) | `src/types.rs` (`ProjectEdge.witnesses`), `src/lang/ts.rs`, `src/lang/rust.rs` (fold sites), `src/project.rs`, `src/bin/extract.rs` (`--witness` over `--resolve`) | every leg's answer is a witness; `resolution_origin` = top rank; `run` rows per tier on the resolve path | `tests/98_resolve_witness.rs`: one site answered by `same_file` and `checker`: 1 fact, 2 witness rows |
| A3 registry and reverse door (after A1 merges; shares `extract.rs` and `tsi/mod.rs`) | `src/tsi/{registry,sink,ingest}.rs`, `src/bin/extract.rs` (`--ingest`), `src/schema.rs` (registry lines) | open `fact` record; registry; `--ingest` decode, validate, canonicalize, re-emit; `schemars` and `--strict` are a follow-up arc | `tests/97_ingest.rs`: bad arity named; dangling id named; idempotent; foreign row gains `method=foreign` |
| A4 syntax-mode TSI rows (ts and rust; go and kotlin follow) | `src/types.rs` (`TypeFAux.tsi`), `src/lang/ts.rs`, `src/lang/rust_type_edges.rs`, `src/wire.rs`, `tests/99_syntax_tsi_rows.rs` | `tsi.product/sum/edge/parameter/input/output`, `ts.optional/readonly`, `rust.impl` from explicit clauses; `coverage=partial` rows | probe fixture `.ts`: `tsi.edge(_, User, id, T, 0)` with `ts.readonly`; `tsi.edge(_, User, name, string, 1)` with `ts.optional` |
| A5 TS semantic mode | `src/lang/ts_checker.mjs`, `src/lang/ts_checker.rs` | `tsi` arm per section 7; `ts.mapped`, `ts.conditional`; `tsi.called/argument` for `User<number>`; `tsi.has_type`; complete rows | criterion 6 rows on the probe fixture; `Partial<User<number>>` yields two optional edges the syntax tier lacks |
| A6 Rust semantic mode | `src/lang/rust_checker_ra.rs`, `src/lang/rust_checker.rs` | `tsi` walk per section 7; `rust.trait/impl/assoc/lifetime/ownership`; complete rows | criterion 7 rows on the probe fixture; `type Output = Vec<T>` yields `rust.assoc` |
| A7 v7 adapter | `v7/src/0_reader/6_extract_loader.pl`, `v7/prelude/5_tsi_primitives.dl7`, `v7/test/4_extract_loader.test.pl` | `accepted/1`; TSI kernel into product nodes and `:/4`; namespaced rows as comptime relations | plunit: syntax stream alone imports `User`; adding the semantic stream with `complete` for `tsi.edge` retracts the syntax edges; `Conforms` proves over the imported struct |
| A8 shared fixture and bench | `tests/fixtures/tsi/{probe.ts,probe.rs}`, `tests/100_tsi_intersection.rs`, `tests/bench/mod.rs` (`Case.mode`), `plans/extract-bench-2026-08-29/RATCHET.tsv` | criterion 8 test; bench mode axis | the `tsi.*` row set of both streams, after canonical renumbering, is equal; `ts.*` and `rust.*` sets are disjoint and non-empty |

Forbidden for every lane: `v6/tsv2/**`, `v6/prolog/**`, `emit_ts.pl`, the
issue file itself.

## 10. Test plan

What breaks if this is wrong: v7 imports a type whose shape is a guess and
proves `Conforms` over it; or a foreign producer's stream is accepted with
a dangling id and the comptime closure never terminates on it.

Units under test: envelope flatten and ordinal assignment; `FlatFact`
round-trip; registry validation; ingest canonicalization; syntax TSI
walkers per language; the two semantic adapters; `accepted/1`; the
intersection query.

| case | input | expected | why |
|---|---|---|---|
| protocol first | any `--witness` stream | line 1 is `record=protocol version=1` | criterion 1 |
| flag off | every golden | byte-identical | consumers already on the wire |
| round trip | every golden line | `serde_json::from_str::<FlatFact>` then `to_string` equals input | criterion 2 |
| bad arity | `tsi.edge` with 4 args | `IngestError::Relation` naming line and relation | a silent skip would hide a producer bug |
| dangling id | `tsi.edge` naming id 9, never declared | `IngestError::Dangling` | closure must not chase a missing node |
| empty complete | `coverage complete tsi.edge`, zero edge rows | `IngestError::Coverage` | criterion 5 |
| idempotent ingest | output of ingest | ingest again is byte-identical | canonical order is a fixed point |
| two witnesses | probe `.ts`, `same_file` and checker both answer | 1 fact, 2 witnesses, origin `checker` | criterion 9's read side |
| readonly and optional | `interface User<T> { readonly id: T; name?: string }` | two `tsi.edge` rows, positions 0 and 1, one `ts.readonly`, one `ts.optional` | criterion 6, both modes |
| generic argument | `User<number>` occurrence | `tsi.called`, one `tsi.argument(_, 0, number)`, `tsi.has_type(span, result)` | criterion 6, semantic only |
| mapped type | `type Q = Partial<User<number>>` | syntax: zero edges for Q, `coverage partial`; semantic: `ts.mapped` plus two optional edges, `coverage complete` | shows a coverage claim the syntax tier cannot make |
| associated type | `impl Mapper<T> for User<T> { type Output = Vec<T>; }` | `rust.impl`, `tsi.conforms(User, Mapper, implSymbol)`, `rust.assoc(User, Output, Vec<T>)` | criterion 7 |
| lifetime and ownership | `struct View<'a> { text: &'a str }` | `rust.lifetime(a, ...)`, `rust.ownership(edge, shared)` | criterion 7 |
| intersection | probe `.ts` and `.rs` | equal `tsi.*` row sets after renumbering | criterion 8 |
| accepted view | syntax stream, then semantic stream with `complete tsi.edge` | the syntax `tsi.edge` rows leave `accepted/1` | criterion 9 |
| newest run wins | two semantic runs, second differs | `accepted/1` reads the higher run | duplicate witnesses across runs never double-count |

Untested here: scip as a TSI producer (scip carries symbols and
occurrences, no type graph); kotlin and python semantic modes (no checker
tier exists for them); `tsi.subtype`, `tsi.assignable`, `tsi.equivalent`
(no adapter emits them in this plan; they stay `partial` with a
`diagnostic` record naming the relation, per the contract's rule for
unsupported coverage).

## 11. Forks for the user

The reference's six open decisions. The contract note settles 3 (rule 5) and 5
(native operators stay namespaced); the rest keep the default this plan takes:

| # | decision | default here | alternative |
|---|---|---|---|
| 1 | serialization | JSONL on the existing FlatFact wire | binary columnar later, same registry |
| 2 | cross-repo identity | interned canonical id; SCIP symbol text retained as `tsi.denotes` arg when `--family scip` also ran | SCIP symbol strings as the id |
| 3 | witness granularity | one method slug per witness (settled by rule 5 and the `extract.witness` row) | derivation graph with premises |
| 4 | checker boundary | compiler API (tsc in-process, rust-analyzer as a lib), already the shape of both tiers | SCIP producer extension |
| 5 | operator normalization | native operators only (`ts.mapped`, no shared `tsi.mapped`); settled by the contract's mode section | derive a shared family beside them |
| 6 | version negotiation | one `protocol` row, one integer, registry pinned to it | per-namespace versions |

Language-side forks (Chris in the room):

| fork | A | B | default |
|---|---|---|---|
| computed TS types with no declaration | structural identity by edge graph (rule 2) | skip | A |
| syntax-only fact reaching `Conforms` | marked, proofs over it derive `proof_partial/3` | unmarked | A |
| `rust.assoc` as an added relation | keep | fold into `tsi.edge` with a reserved label | A |
| v7 reads JSONL or SQLite | `library(http/json)` | attach `~/.agent/dl6.db` | A now, B after A7 |

## 12. Out of scope

- `--family scip` rows gain no `fact` ordinal.
- `hosts.rs:1041` keeps reading `resolution_origin`.
- No hash-string ids on the wire.
- Import syntax on the language side (`dl7-module-system`).
- The issue's disposition: it is `untriaged`; `issuectl intake accept
  extract-semantic-fact-roundtrip` is the user's call, from a main checkout.
