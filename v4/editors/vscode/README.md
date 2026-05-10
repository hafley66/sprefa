# sprefa v4 VS Code extension

Declarative `.sprf` syntax highlighting plus LSP support for the v4 host
grammar.

The extension spawns `sprefa-lsp` by default. `sprefa-lsp` uses the shared
`v4::app::SprfClient` RPC surface with an in-process backend today. The app
layer also has an HTTP client for `sprefa-daemon`; wiring VS Code to a remote
daemon is a client-selection change, not a new LSP feature.

This extension can be installed alongside the v3 `sprf` extension; the two
share the language id `sprf` so VS Code will use whichever is enabled.

## Install

```bash
./install.sh
```

The script prefers `vsce package` + `code --install-extension`. If those CLIs
are missing, it falls back to symlinking the extension folder into
`~/.vscode/extensions/`.

## What is highlighted

- line comments (`#`)
- atoms (`:greet`)
- strings (`"..."`, `r"..."`, `r#"..."#`)
- backtick `dsl_body` (whole body as string scope; per-DSL semantic tokens
  arrive in Lane D)
- numbers
- op names (identifier immediately followed by `[` `(` `` ` `` or `{`)
- predicate suffix (`name?`)
- chain `>`, fork `;`, brackets, parens, braces, commas

## What was dropped from v3

The v3 extension carried highlighting for four host-level carveouts that v4
removed from the grammar:

- `${X}` / `${X?}` carveouts
- `&{...}` Address carveout
- `${{...}}` shell literal
- `$IDENT` term-ref shorthand

It also carried a tag-named slot palette (`json` / `ast` / `lsp` / `comment` /
`fs` / `repo` / `rev` / `folder` / `file` / `marker` / `md` / `print` /
`line`). v4 makes those domain-neutral op names; semantic per-DSL coloring
will come from a tree-sitter or tokens provider later.

## Verify

Open a `.sprf` file:

```
rule(:greet) { str `hello world` }
```

`rule` and `str` should color as op names, `:greet` as atom, the backtick
body as string, and parens/braces as bracket pairs. Hover, inlay hints,
diagnostics, completions, and semantic tokens should come from `sprefa-lsp`.
