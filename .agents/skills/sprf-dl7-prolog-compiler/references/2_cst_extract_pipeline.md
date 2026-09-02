# DL7 CST, extraction, and emitter pipeline

## Contents

1. Current implementation
2. Boundary model
3. Missing adapters
4. Target flow

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
YAML facts have been extracted. Format decoding may normalize source syntax,
but type interpretation and generated application logic remain compiler data.

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
external source code ---------> language Tree-sitter --------------|
OpenAPI / JSON Schema --------> JSON or YAML extractor ------------|
                                                                    v
                                                         sprefa-extract facts
                                                                    |
                                                                    v
                                                          DL7 compiler inputs
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
