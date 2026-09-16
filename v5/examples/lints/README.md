# Starter lint pack — ast-grep → LSP

The workflow: **ask the AI for an ast-grep pattern, paste it into a `.dl` lint
rule, load the file, see squiggles.** The AI eats the verbose authoring; the tool
turns ast-grep matches into editor diagnostics and a commit gate.

## Files

| File | Lints |
|------|-------|
| [rust.dl](rust.dl) | `no-dbg` (error), `no-unwrap` (warn), `no-panic` (warn), `no-todo` (warn) |
| [ts.dl](ts.dl) | `no-debugger` (error), `no-console` (warn), `no-any-cast` (warn) |

## Run

```sh
# live squiggles in the editor (updates on save)
dl examples/lints/rust.dl --lsp

# block a commit — exits non-zero iff any `error`-severity row exists
dl examples/lints/rust.dl --check      # husky pre-commit
dl examples/lints/rust.dl --diag-json  # CI / JSON

# just print the hits
dl examples/lints/rust.dl
```

`scan("*", ...)` fans every rule over all repos in your `config.toml`; swap to
`scan("WORK", ...)` to lint only the `--root` repo. Set `SPREFA_CONFIG` or put
the config at `~/.config/sprefa/config.toml`.

## Adding a lint (the AI loop)

1. Ask: *"ast-grep pattern for an `await` inside a loop in TS"* → `for ($$$){ await $X }`.
2. Paste into a new `diag(...) <- scan(...), sg(path, rev, :tsx, "<pattern>", line, col, el, ec).` rule.
3. Reload (`--lsp` restarts on the next save / re-run `--check`).

Pattern syntax: `$X` = one node, `$$$A` = a variadic list, bare text = literal.
`severity` is `error` (blocks `--check`) / `warn` / `info` / `hint`. The trailing
`line, col, end_line, end_col` positionals bind the match span for a tight squiggle.

## Not yet (rides the ops-frontend arc)

"`unwrap` **only when reachable from `app/main.tsx` inside an effect**" needs the
call-graph std-lib + `reaches` join. That's Phase A of the programmable-LSP plan
([../../plans/2026-06-01-programmable-lsp-ops-frontend-arc.md](../../plans/2026-06-01-programmable-lsp-ops-frontend-arc.md)).
Today's lints are pure ast-grep matches — no reachability — which already covers
most ban-this-pattern needs.
