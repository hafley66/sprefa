---
created: 2026-09-02
updated: 2026-09-02
type: bug
reporter: chris
status: untriaged
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

- [ ] The common fact stream has an explicit protocol version.
- [ ] Fact rows can be decoded and validated as well as serialized.
- [ ] A CLI or library reverse door accepts foreign-produced fact rows and
      emits the canonical ordering.
- [ ] Syntax runs identify themselves and declare partial per-relation
      coverage.
- [ ] Semantic runs identify themselves and declare complete per-relation
      coverage only where the native adapter enumerated every reachable row.
- [ ] TypeScript semantic extraction emits generic parameters and arguments,
      optionality, readonly edges, callable input/output types, and native
      conditional or mapped operators used by the fixture corpus.
- [ ] Rust semantic extraction emits generic parameters and arguments,
      trait implementations, associated types, callable input/output types,
      lifetimes, and ownership facts used by the fixture corpus.
- [ ] Equivalent TypeScript and Rust fixtures produce shared TSI relations for
      their intersecting semantics and namespaced relations for native meaning.
- [ ] DL7 imports accepted rows as comptime relations and can replace syntax
      candidates when semantic coverage becomes complete.

## Evidence

The measured probe and architecture notes are recorded in:

- `.agents/skills/sprf-dl7-prolog-compiler/references/2_cst_extract_pipeline.md`
- `.agents/skills/sprf-dl7-prolog-compiler/references/4_polyglot_type_fact_protocol.md`
