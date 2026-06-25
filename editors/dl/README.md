# sprefa-dl

VS Code extension for `.dl` rule programs (sprefa v5 engine): syntax highlighting
plus a language server that surfaces a program's `diag` relation as editor
diagnostics.

The server is the engine itself: `dl --lsp --root <workspace>` (no program
positional => discovery mode, merges every `.dl` in the program dir). Lint fires
on open/save (disk-truth).

## Build + install

```sh
cd editors/dl
npm install
npm run compile
npx @vscode/vsce package          # -> sprefa-dl-0.1.0.vsix
code --install-extension sprefa-dl-0.1.0.vsix
```

Reload the window. Requires `dl` on PATH (`cargo install --path v5 --bin dl`), or
set `sprefa-dl.serverPath`.

## Settings

- `sprefa-dl.serverPath` — path to the `dl` binary (default `dl`).
- `sprefa-dl.root` — project root for `--root` (default: first workspace folder).
