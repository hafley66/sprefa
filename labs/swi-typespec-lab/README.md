# SWI-Prolog TypeSpec semantics lab

This isolated lab tests six claims:

1. A DCG can parse the TypeSpec-shaped source in `schema.soup` into semantic Prolog terms.
2. A second DCG can parse typed delimiter patterns.
3. One `pattern_value/3` relation can match strings into bindings and render bindings into strings.
4. Prolog relations can validate nested JSON-shaped values and enumerate typed structural paths.
5. The parsed schema can emit compilable Rust models and a syntactically valid JavaScript fetch client.
6. SWI-Prolog can host the language's stdio LSP and package it as a saved executable application.

Run from this directory:

```sh
swipl -q -s 4_demo.pl
rustc --crate-type lib generated/models.rs -o generated/libmodels.rlib
node --check generated/client.mjs
```

## Language server

Run through the SWI interpreter:

```sh
node 7_lsp_smoke.mjs
```

Build and test the saved executable image:

```sh
swipl -q -s 8_build.pl -g build -t halt
SOUP_LSP=./generated/soup-lsp node 7_lsp_smoke.mjs
```

On the tested Homebrew macOS installation, the result is a 290 KiB Mach-O executable containing the saved state and depending on `@rpath/libswipl.10.dylib`. A distributable file with no SWI shared-library dependency requires a static SWI runtime build.

Implemented methods:

```text
initialize
textDocument/didOpen
textDocument/didChange
textDocument/didClose
textDocument/hover
textDocument/definition
textDocument/references
textDocument/completion
textDocument/documentSymbol
textDocument/publishDiagnostics
shutdown
exit
```

`5_documents.pl` stores versioned documents, indexes identifier spans, converts code-point offsets to LSP UTF-16 positions, resolves declarations and references, and derives parser, undefined-type, and duplicate-declaration diagnostics. `6_lsp.pl` implements UTF-8 byte-counted JSON-RPC framing over stdio.

The authored schema is `schema.soup`. `0_schema.pl` parses and asserts its declarations as the relational database consumed by the remaining modules. The experiment does not contain a general TypeSpec parser, recovery parser, decorator runtime, package resolver, workspace import resolver, contextual completion engine, rename support, formatter, semantic tokens, or Alloy integration.
