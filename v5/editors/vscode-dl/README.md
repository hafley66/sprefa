# dl LSP

Minimal VSCode shim that runs the `dl` datalog engine as a language server. Drop a `.dl` file in `.dl/`, get live squiggles on the rust/ts/py/go/kt/json/yaml/toml source your rules scan, plus type-check squiggles on the `.dl` program itself.

## Setup (the neat path)

1. Install the engine: `cargo install --path v5 --bin dl` (puts `dl` on PATH).
2. Install the extension: `code --install-extension dl-lsp-0.1.0.vsix`.
3. Add rules: create `<repo>/.dl/lint.dl`.
4. Open any file the rule scans. Squiggles appear on save (the engine reads disk).

No other config. The extension starts `dl --root <workspace> --lsp` and proxies LSP over stdio. definition/references work over the engine's `ref(id,string,file,lo,hi)` spine, so a capture in one language links to the same string in another.

## Settings

| setting | default | effect |
|---|---|---|
| `dl.binaryPath` | `dl` | path to the `dl` binary (PATH by default) |
| `dl.program` | `""` | specific `.dl` file; empty = discover `<root>/.dl/*.dl` |
| `dl.root` | `""` | scan root; empty = first workspace folder |

Point `dl.program` at a single example to try it without the `.dl/` convention:
```json
{ "dl.program": "v5/examples/openapi-lsp.dl" }
```

## Build

```sh
cd v5/editors/vscode-dl
npm install
npm run compile
npx @vscode/vsce package     # -> dl-lsp-0.1.0.vsix
```

No grammar contribution yet (`.dl` renders as plain text). Syntax highlighting inside `.dl` op bodies is the DslBodyLsp port, tracked separately.
