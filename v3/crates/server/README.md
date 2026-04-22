# server

sprefa v3 stdio LSP.

## Status

- Parse diagnostics: wired. ERROR / MISSING from the host grammar plus
  injected pattern-op grammars (glob / re) publish on open + change.
- Hover / completion: stubbed to `None`. Needs DocSession to grow past
  the parse layer (parse.md §14.6).
- Transport: stdio only. HTTP/WebSocket port from v2 is deferred.

## Build

```
cd v3
cargo build -p server --bin sprefa-lsp --release
ls -l target/release/sprefa-lsp
```

## Smoke test in VS Code

Simplest path: install the `helix-lsp` / `generic-lsp-client` style
extension, or use `vscode-languageclient` in a tiny extension.

Settings snippet for a generic LSP extension (example with
`generic-lsp-client`):

```jsonc
{
  "genericLspClient.servers": {
    "sprefa": {
      "command": "/absolute/path/to/v3/target/release/sprefa-lsp",
      "languageIds": ["sprefa"],
      "filePatterns": ["**/*.sprf"]
    }
  },
  "files.associations": { "*.sprf": "sprefa" }
}
```

Enable tracing on stderr with `RUST_LOG=info` (or `debug`) on the
`env` entry.

## Fixture

`fixtures/smoke.sprf`:

```
foo > bar           # valid — one pipe, two ops
glob(**/*.rs)       # valid — injected glob sub-grammar parses the body
foo > > bar         # syntax error — publishes a parse/syntax diagnostic
```

Opening the file should:
1. Show no diagnostics on lines 1 and 2.
2. Show a red underline inside the second `>` on line 3, code
   `parse/syntax`.
