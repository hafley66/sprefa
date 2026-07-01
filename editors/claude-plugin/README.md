# dl-rails — Claude Code plugin

Registers `dl --lsp` as a language server so Claude Code's LSP tool gets:

- diagnostics from the repo's `.dl/*.dl` rails on every save
- go-to-definition over `module_edge` (import → target file)
- find-references over the ref spine (every span sharing the string)

No program argument: the server discovers `<root>/.dl/*.dl` and caches in
`.dl/cache.db` (shared with hook `--check` runs; WAL + busy_timeout makes the
cross-process writes wait instead of failing).

## Use

```sh
cargo install --path v5 --bin dl   # puts `dl` on PATH (~/.cargo/bin)
claude --plugin-dir editors/claude-plugin
```

The target repo needs a `.dl/` directory with at least one `.dl` file, else
the server exits loudly at startup (by design: a typo'd setup must not look
like a clean one).

`extensionToLanguage` lists which files route to this server (`.rs` here).
Add the extensions your rails scan.

## Division of labor

This plugin is navigation + passive diagnostics. Enforcement — the loop that
blocks an agent and feeds violations back — is the PostToolUse hook on
`dl --check`, which is independent of this plugin. See `docs/rails.md`.
