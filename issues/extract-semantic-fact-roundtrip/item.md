---
created: 2026-09-02
updated: 2026-09-02
type: bug
reporter: chris
status: open
priority: high
provenance: other
provenance_detail: Codex session measured local release extractor on 2026-09-02
source_ref: chat:01a02a8b-b49e-7231-8150-238258b6be1e/extract-tsi-roundtrip
---

# Extract semantic fact mode and reverse fact ingestion are incomplete

## Description

## Report

`sprefa-extract` exposes a self-described common `FlatFact` JSONL wire through
`extract --schema`, but the wire is output-only and the checker flags provide
selected resolution edges rather than a complete semantic type fact graph.

This blocks two intended consumers:

1. TypeScript, Rust, Go, and other native compiler adapters cannot emit the
   common fact format and feed it back through extract or DL7.
2. DL7 cannot distinguish cheap syntax guesses from complete native-checker
   semantics or determine when absence from a semantic relation is meaningful.

## Reproduction

Equivalent generic TypeScript and Rust fixtures were created with:

- `Mapper<T>` as an interface or trait;
- `User<T>` extending or implementing it;
- a generic mapping method;
- TypeScript readonly and optional fields;
- a Rust associated generic output type.

Commands run against the release binary on 2026-09-02:

```text
extract --family type,call /private/tmp/tsi_extract_probe.ts
extract --family type,call /private/tmp/tsi_extract_probe.rs

extract --resolve --family type --project-root /private/tmp \
  --ts-checker /private/tmp/tsi_extract_probe.ts

extract --resolve --family type --project-root /private/tmp \
  --rust-checker /private/tmp/tsi_extract_probe.rs
```

Both languages emitted the same tagged JSONL envelope. Resolve mode emitted
one `resolved_type_edge` per language:

```json
{"owner_name":"User","target_name":"Mapper","kind":"generic","resolution_origin":"same_file"}
{"owner_name":"User","target_name":"Mapper","kind":"impl","resolution_origin":"same_file"}
```

Repository search receipts:

- `FlatFact` in `v6/sprefa-extract/src/types.rs` derives `Serialize` only.
- There are zero `FlatFact` JSON decoders.
- There are zero fact-ingest CLI paths.
- There is no foreign FlatFact JSONL to DL7 adapter.

## Actual result

Syntax mode emits common nodes, signatures, sites, and selected resolved edges.
The checker flags replace selected resolution answers. The stream omits generic
parameter declarations, concrete generic arguments, TypeScript optionality and
readonly edges, Rust associated-type bindings, and resolved callable type
expressions.

The stream carries no run mode, fact witness, protocol version, or relation
coverage declaration. Consumers cannot tell whether a missing fact is false,
unsupported, or unexamined.

## Expected result

Expose two producers over one versioned fact vocabulary:

```text
syntax mode   -> candidate witnesses + partial coverage
semantic mode -> native-checker witnesses + complete coverage
```

Canonical rows need equivalents of:

```text
extract.run(RunId, Mode, Tool, Version, Scope)
extract.fact(FactId, Relation, Arguments)
extract.witness(FactId, RunId, Method)
extract.coverage(RunId, Relation, partial | complete)
```

Semantic mode emits every reachable fact represented by the protocol and
retains language-native operators in namespaced relations. Foreign producers
can submit the same stream through a decoder that validates, canonicalizes,
and re-emits it before DL7 ingestion.

## Acceptance criteria

- [x] The common fact stream has an explicit protocol version.
- [x] Fact rows can be decoded and validated as well as serialized.
- [x] A CLI or library reverse door accepts foreign-produced fact rows and
      emits the canonical ordering.
- [x] Syntax runs identify themselves and declare partial per-relation
      coverage.
- [x] Semantic runs identify themselves and declare complete per-relation
      coverage only where the native adapter enumerated every reachable row.
- [x] TypeScript semantic extraction emits generic parameters and arguments,
      optionality, readonly edges, callable input/output types, and native
      conditional or mapped operators used by the fixture corpus.
- [x] Rust semantic extraction emits generic parameters and arguments,
      trait implementations, associated types, callable input/output types,
      lifetimes, and ownership facts used by the fixture corpus.
- [x] Equivalent TypeScript and Rust fixtures produce shared TSI relations for
      their intersecting semantics and namespaced relations for native meaning.
- [x] DL7 imports accepted rows as comptime relations and can replace syntax
      candidates when semantic coverage becomes complete.

## Evidence

The measured probe and architecture notes are recorded in:

- `.agents/skills/sprf-dl7-prolog-compiler/references/2_cst_extract_pipeline.md`
- `.agents/skills/sprf-dl7-prolog-compiler/references/4_polyglot_type_fact_protocol.md`

## Decisions

### 2026-09-03 · Chris, via claude-299

`tsi.name(Id, Text)` joins the contract: the spelling a consumer prints for a type or symbol id, so a renderer never opens the file behind a `tsi.origin` span. Syntax tier: the written text the id is keyed on (`Vec<Option<T>>`, `std::fmt::Result`), the primitive class (`unit` prints `()`), the declared parameter or callable name; tuples and impl blocks stay nameless. Semantic tier: the definition name for nominal types and symbols, the checker's rendering for structural types. Rows: `src/tsi/registry.rs`, `src/types.rs` `TsiNames::name`, `tests/110_tsi_name.rs`.

### 2026-09-02T21:19:27Z · @codex

TSI contract required by this card. The issue's original `extract.run/fact/witness/coverage` rows describe production provenance and completeness; the semantic payload uses these relations:

```text
tsi.type(TypeId)
tsi.denotes(SymbolId, TypeId)
tsi.has_type(OccurrenceId, TypeId)
tsi.origin(TypeId, Language, SourceRange)

tsi.product(TypeId)
tsi.sum(TypeId)
tsi.callable(TypeId)
tsi.primitive(TypeId, PrimitiveClass)

tsi.edge(EdgeId, OwnerTypeId, Label, TargetTypeId, Position)

tsi.parameter(ParameterTypeId, CalleeTypeId, Position, Variance)
tsi.called(ResultTypeId, CalleeTypeId, ArgumentListId)
tsi.argument(ArgumentListId, Position, ArgumentTypeId)

tsi.input(CallableTypeId, Position, InputTypeId)
tsi.output(CallableTypeId, Position, OutputTypeId)

tsi.subtype(Source, Target, Witness)
tsi.assignable(Source, Target, Witness)
tsi.conforms(Source, Contract, Witness)
tsi.equivalent(Left, Right, Witness)
```

Identity rules:

1. Nominal source types derive identity from the resolved source symbol.
2. Anonymous structural types derive identity from the closed ordered edge graph.
3. A type call result derives identity from callee plus ordered argument IDs.
4. A generic parameter derives identity from declaration symbol plus position.
5. Fact IDs derive from relation plus canonical arguments, allowing syntax and semantic runs to witness the same fact.

Mode contract:

- Syntax mode emits candidate facts and `partial` coverage.
- Semantic mode uses the native checker and emits every reachable fact represented by the protocol.
- Semantic adapters retain native operators in namespaced relations such as `ts.conditional`, `ts.mapped`, `ts.optional`, `ts.readonly`, `rust.trait`, `rust.impl`, `rust.lifetime`, `rust.ownership`, `go.interface`, `go.type_set`, and `go.embedding`.
- A semantic run advertises `complete` only after enumerating every reachable row for that relation. Unsupported coverage stays `partial` with an explicit diagnostic.
- Recursive graphs close through IDs rather than bounded expansion.

Current implementation receipt: `FlatFact` is a closed output-only enum deriving `Serialize`. The reverse door needs decoding, protocol versioning, validation, canonical re-emission, open namespaced semantic rows, and DL7 relation import.

The longer design references currently live on branch `perf/v7-cold-compile` in commits `a99d7c3bf`, `5b98ea5e8`, `430d69fc7`, `6187a6ede`, and `368a1eebd`; they are absent from `main` as of this note. This card and note are the self-contained implementation contract.

### 2026-09-02T23:05:00Z · @claude-299

Vocabulary additions, user-approved 2026-09-02 ("yes go do the things"), in `v6/sprefa-extract/src/tsi/registry.rs`:

```text
tsi.symbol(SymbolId)                                   declares a symbol id (the subject of tsi.denotes; rust.impl position 0 declares its own)
tsi.value(ValueId, TypeId)                             declares a value entity; tsi.argument stays type-only
tsi.value_argument(ArgumentListId, Position, ValueId)  a value in argument position
tsi.scip_symbol(SymbolId, Text)                        optional bridge to SCIP symbol text; identity stays run-local
```

Variance: the atom is `unspecified` where no checker exposes one; `invariant` is never claimed by default. An alias mints no type id: `tsi.symbol` plus `tsi.denotes(Alias, Target)`. `--ingest` declaring positions: `tsi.type` 0, `tsi.symbol` 0, `tsi.value` 0, `tsi.edge` 0, `rust.impl` 0, `tsi.called` 2.

### 2026-09-03T00:20:00Z · @claude-299

All nine criteria have landed receipts on origin/main: A1 #645 (1, 2), A3 #648/#651 (2, 3), A2 #652 (4, 9 read side), A4 #653 (4), A5 #657 (5, 6), A6 #659 (5, 7), A8 #661 (8), A7 #655 (9). ARCH.pl rows tsi_a1..tsi_a8. Open follow-ups: tsp.* registry rows (codex-tsi), tsi_contract_vs_trait_shape fork, extract_emit_throughput_budget.
