# 5. A Self-LSP for Soup

## Implemented status

The isolated lab now contains an executable SWI-Prolog language server:

- [`5_documents.pl`](../../labs/swi-typespec-lab/5_documents.pl): versioned documents, lexical symbol spans, semantic lookup, diagnostics, and UTF-16 conversion
- [`6_lsp.pl`](../../labs/swi-typespec-lab/6_lsp.pl): byte-counted JSON-RPC stdio transport and LSP dispatch
- [`7_lsp_smoke.mjs`](../../labs/swi-typespec-lab/7_lsp_smoke.mjs): end-to-end client transcript
- [`8_build.pl`](../../labs/swi-typespec-lab/8_build.pl): `qsave_program/2` executable build

Verified methods:

```text
initialize
didOpen, didChange, didClose
hover, definition, references, completion, documentSymbol
publishDiagnostics
shutdown, exit
```

Build command:

```sh
swipl -q -s 8_build.pl -g build -t halt
```

The tested Homebrew macOS artifact is a 290 KiB Mach-O executable with a dynamic dependency on `@rpath/libswipl.10.dylib`. A static SWI build is required for distribution without that shared library.

## Type signatures first

```prolog
parse_document(+Uri, +Version, +Text, -Ast, -Diagnostics).
index_document(+Uri, +Version, +Ast).
resolve_reference(+Uri, +Name, +UseSpan, -DefinitionSpan).
hover(+Uri, +Line, +Character, -Markdown).
definition(+Uri, +Line, +Character, -Location).
references(+Uri, +Line, +Character, -Locations).
completion(+Uri, +Line, +Character, -Items).
publish_diagnostics(+Uri, -LspDiagnostics).
```

## Instance timeline

1. `didOpen` stores URI, version, and source.
2. The DCG emits semantic terms while a lexical pass records identifier spans.
3. Indexing associates declarations and references with lexical spans.
4. Resolution links references to declarations.
5. Diagnostic relations enumerate violations.
6. The server publishes diagnostics.
7. Hover, definition, reference, and completion requests query the indexed rows.
8. `didChange` atomically replaces rows for the older version.
9. `didClose` retracts the document's rows.

## Implemented storage relation

```prolog
:- dynamic document/7.

document(
    Uri,
    Version,
    Text,
    Declarations,
    LexicalSymbols,
    Definitions,
    Diagnostics
).
```

The URI is the uniqueness key. Each update retracts the previous URI row, parses and indexes the complete replacement text, and asserts one new row.

## Current span implementation

```prolog
scan_symbols(Text, Symbols),
parse_schema(Text, Declarations),
declaration_index(Declarations, Symbols, Definitions).
```

The lexical index stores code-point offsets. LSP responses convert them to zero-based lines and UTF-16 columns. The test suite includes an emoji-prefixed document.

A recovery/CST parser will need to retain:

```text
byte offset
Unicode code-point offset
line
UTF-16 column
leading/trailing trivia when formatting is required
```

LSP columns are UTF-16 code units, so astral Unicode characters require two units. Current coverage includes ASCII, emoji, and LF. Accented characters and CRLF remain test gaps.

## Implemented relational diagnostics

```prolog
undefined_type_diagnostic(Declarations, Symbols,
                          diagnostic(Span, error, Message)) :-
    member(type_decl(_, Type), Declarations),
    referenced_type(Type, Name),
    \+ builtin_type(Name),
    \+ member(type_decl(Name, _), Declarations),
    first_symbol_span(Symbols, Name, Span),
    format(string(Message), "Undefined type ~w", [Name]).
```

## Completion as a query

```prolog
completion_item(Declarations, Name) :-
    member(type_decl(Name, _), Declarations).

completion_item(Declarations, Name) :-
    member(pattern_decl(Name, _), Declarations).
```

The current method returns every type and pattern. Syntax-context filtering and ranking remain future stages.

## Bootstrap surface

Soup can describe the protocol models it serves:

```typespec
type Position {
  line: Int;
  character: Int;
}

type HoverRequest {
  uri: String;
  position: Position;
}

consumer lsp {
  request HoverRequest -> HoverResult;
}
```

The schema can generate JSON validators, request dispatch tables, Prolog decoding terms, and client types. JSON-RPC framing, source parsing, document storage, and position conversion remain the trusted kernel.
