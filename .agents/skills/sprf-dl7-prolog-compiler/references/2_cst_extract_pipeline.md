# DL7 CST, extraction, and emitter pipeline

## Contents

1. Current implementation
2. Boundary model
3. Missing adapters
4. Target flow
5. Measured common-wire probe

## 1. Current implementation

DL7 currently has two disconnected text paths.

```text
.dl7 text
  ├─ SWI reader -> reader nodes + source rows -> active DL7 compiler
  └─ Tree-sitter grammar -> generated C parser -> corpus tests only
```

`v7/src/0_reader/0_parser.pl` is the active compiler reader. It returns ground
reader terms and source spans, but discards comments and layout. It therefore
provides an AST-shaped reader product rather than a lossless CST.

`v7/tree-sitter-dl7` contains `grammar.js`, generated `parser.c`, node metadata,
a C ABI header, and one corpus. Its README explicitly leaves the adapter from
Tree-sitter nodes to canonical DL7 syntax, source, and diagnostics for later.

`v6/sprefa-extract` already provides:

- Tree-sitter and ast-grep parsing infrastructure;
- a generic named-node `CstF` projection;
- typed ast-grep rule and pattern execution;
- per-language `CallF`, `TypeF`, dataflow, and resolution families;
- a DL6 source arm;
- JSON, JSONL, YAML, and TOML data extraction.

It does not currently expose `ExtractLang::Dl7`, route `.dl7`, link the DL7
grammar, or project DL7 calls and type edges.

## 2. Boundary model

Keep three responsibilities distinct while allowing them to share rows.

```text
Tree-sitter       exact syntax tree, spans, errors, incremental reparsing
ast-grep          structural matches and captures over a Tree-sitter tree
sprefa-extract    per-file source facts describing what is written
DL7 comptime      rules deriving type and compiler meaning from those facts
DBSP emitter      monomorphic reactive program artifact from checked logic
```

The extractor remains pure, per-file, parallel, and cacheable. OpenAPI and
JSON Schema vocabulary belongs in DL7 derivation rules after generic JSON or
YAML facts have been extracted. Their type vocabulary is an ingress and egress
projection over the semantic type graph. Native source-language compiler facts
retain nominal identity, generics, conformance, conditional or mapped types,
ownership, and other semantics outside those schema vocabularies. Format
decoding may normalize source syntax, but type interpretation and generated
application logic remain compiler data.

The V7 compiler already emits a target-neutral logical program containing
relations, keys, seeds, rules, dependencies, strata, calls, arguments, and
variables. That is the input boundary for the DBSP application emitter.

## 3. Missing adapters

The smallest connected path needs:

1. Package `tree-sitter-dl7` as a Rust grammar dependency exposing a
   `LANGUAGE` value in the shape consumed by ast-grep and `sprefa-extract`.
2. Add `ExtractLang::Dl7` and `.dl7` source routing.
3. Project the DL7 named-node tree into `CstF`.
4. Project prefix calls and `<-` rules into `CallF`.
5. Project `(: Name (* ...))`, products, sums, generic applications, and
   labeled edges into `TypeF` or a DL7-specific reader-fact family.
6. Define one adapter from extracted DL7 reader facts to the compiler's
   canonical `node/2` and `source/8` inputs.
7. Feed extracted JSON or YAML facts to userland OpenAPI and JSON Schema
   normalization rules.
8. Feed the compiler's reified logical-program rows to the DBSP emitter.

Until step 6 lands, compiler parsing and Tree-sitter parsing can diverge.
Until `ExtractLang::Dl7` lands, ast-grep cannot compile DL7 structural patterns
through the existing extractor language abstraction.

## 4. Target flow

```text
DL7 source -------------------> Tree-sitter DL7 -------------------+
external source code ---------> syntax + native semantic adapter --|
OpenAPI / JSON Schema --------> JSON or YAML extractor ------------|
                                                                    v
                                                         sprefa-extract facts
                                                                    |
                                                                    v
                                                symbol + semantic type facts
                                                                    |
                                             userland comptime/type rules
                                                                    |
                                                                    v
                                          closed type graph + checked logic
                                              |                     |
                                              v                     v
                                    schema/API artifacts    DBSP app artifact
```

Tree-sitter owns concrete syntax and incremental edits. The SWI compiler owns
fixpoint meaning until the runtime boundary changes. ast-grep supplies
structural source queries as data-producing operations. `sprefa-extract` is
the source-fact ingress shared by code intelligence, schema ingestion, and
compiler effects.

## 5. Measured common-wire probe

On 2026-09-02, the release `extract` binary was run over equivalent generic
TypeScript and Rust fixtures containing an interface or trait, a generic
`User<T>`, a mapping method, and one conformance declaration.

```text
extract --family type,call /private/tmp/tsi_extract_probe.ts
extract --family type,call /private/tmp/tsi_extract_probe.rs

extract --resolve --family type --project-root /private/tmp \
  --ts-checker /private/tmp/tsi_extract_probe.ts

extract --resolve --family type --project-root /private/tmp \
  --rust-checker /private/tmp/tsi_extract_probe.rs
```

Both source languages emitted the same `FlatFact` JSONL envelope. TypeScript
produced `node` rows for `Mapper` and `User` with `kind: interface`; Rust
produced `node` rows with `kind: trait` and `kind: struct`, callable rows, type
signature rows, and `method_owner` rows. Resolve mode produced one common
`resolved_type_edge` shape from each language:

```json
{"owner_name":"User","target_name":"Mapper","kind":"generic","resolution_origin":"same_file"}
{"owner_name":"User","target_name":"Mapper","kind":"impl","resolution_origin":"same_file"}
```

The common wire is `FlatFact` in `v6/sprefa-extract/src/types.rs`, serialized
as one tagged JSON object per line by `v6/sprefa-extract/src/wire.rs`. The
human-readable contract is available through `extract --schema` and is defined
in `v6/sprefa-extract/src/schema.rs`.

The probe did not emit generic parameter declarations, concrete generic
arguments, TypeScript optional or readonly edges, Rust associated-type
bindings, or resolved callable type expressions. Those rows require the TSI
semantic adapter above the existing syntax and checker-resolution facts.

The current wire is an output seam. `FlatFact` derives `Serialize` and has no
`Deserialize` implementation. A repository search found zero JSON decoders for
`FlatFact`, zero fact-ingest CLI paths, and no adapter from foreign FlatFact
JSONL into DL7. A foreign producer can spell compatible JSONL today, while a
reverse-door lab still needs:

1. a versioned fact-stream envelope or a version row;
2. decoding and schema validation;
3. canonical sorting and re-emission;
4. an adapter from accepted rows into DL7 comptime relations;
5. open namespaced TSI rows for semantics beyond the closed `FlatFact` enum.
