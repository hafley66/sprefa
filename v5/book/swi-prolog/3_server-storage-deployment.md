# 3. Server, Storage, and Deployment Toolbox

## Data formats and protocols

| Facility | Included capability |
|---|---|
| HTTP | Client and multithreaded server libraries |
| JSON | Parse and generate JSON terms |
| WebSocket | HTTP upgrade and framed messaging libraries |
| TLS | Secure sockets through SSL libraries |
| URI | Parsing, resolution, and query components |
| HTML/XML/SGML | Parsing and generation |
| RDF | Triple stores, RDF parsing, and semantic-web querying |
| YAML | YAML parsing through the supplied ecosystem |
| CSV | Row parsing and writing |
| MIME | Message and content handling |

An LSP over stdio only needs JSON and `Content-Length` framing. A browser-facing semantic daemon can use the same program through HTTP or WebSocket.

## Persistence choices

| Store | Shape | Use |
|---|---|---|
| Dynamic predicates | Process memory | Current editor documents and semantic indexes |
| Recorded database | Process memory | Simple keyed term records |
| Tries | Process memory | Structural sets and interned terms |
| `library(persistency)` | File-backed predicates | Small durable fact sets |
| RDF store | In-memory or persistent RDF | Graph and ontology workloads |
| ODBC | External SQL databases | Existing relational systems |
| RocksDB pack | Embedded key/value database | Larger durable indexes |

The RocksDB pack is a C++ binding and has a substantial native build. Its official pack notes document missing callback features and possible multithreading concerns around callbacks.

## Packaging

```prolog
qsave_program('soup-lsp', [
    goal(main),
    stand_alone(true)
]).
```

`qsave_program/2` creates a saved application state containing loaded Prolog code. Distribution still depends on SWI's runtime format and platform packaging. It provides a single executable entry point from the user's perspective.

## Pack ecosystem

SWI packs are installable source or native extension bundles containing `pack.pl` metadata and a `prolog/` directory. Pin a tag or commit for compiler infrastructure. Avoid resolving unbounded latest versions during a reproducible build.

## LSP-related packs

| Pack | Contents | Status note |
|---|---|---|
| `lsp_server` | Prolog LSP server entry point | Existing transport and method implementation |
| `prolog_lsp` | JSON-RPC, stdio/TCP, indexing, definitions, references, completions | Project describes itself as immature |
| `debug_adapter` | Debug Adapter Protocol server | Broad DAP method/event implementation |

For Soup, the protocol framing can be reused while parsing, indexing, and semantic methods remain language-specific.
