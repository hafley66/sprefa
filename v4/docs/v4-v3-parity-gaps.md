# V4 V3 Parity Gaps

This document tracks gaps between the old V3 rule/fact/next boundary and current V4.

Current V4 model:

```text
rule = named relation with optional producer body
old fact = empty-body rule / relation subject
next = imperative event/state-machine flow
sql = relational escape hatch over current batch + rule tables
```

## Usable Now

| Area | Current V4 support |
| --- | --- |
| CLI run | `sprefa-run` executes a `.sprf` file and prints rule tables |
| basic editor | VS Code extension registers `.sprf` syntax and starts `sprefa-lsp` |
| syntax highlighting | TextMate grammar covers host, `re`, `glob`, `ast`, `json`, plain backticks |
| parse/walk diagnostics | LSP publishes parse/lower diagnostics for open buffers |
| cursor-flow visibility | inlay hints show probe cursor counts per op span |
| DSL hover/completion | provider path exists for supported DSL bodies |
| file read | `read` loads `cursor.value` path into `cursor.value` bytes/text |
| git rev reads | `repo() > rev(...) > fs > read` uses `git ls-tree` and `git show` |
| regex extraction | `re` captures into terms |
| JSON/TOML/YAML-ish extraction | `json` brace walker captures terms |
| AST extraction | `ast` ast-grep slice exists |
| shell escape | `sh`, `sh(:filter)`, `sh(OUT?)`, `sh!` exist |
| rule writes | declaration plus projected writes work |
| rule reads | `rule_name(...)`, `rule_name?(...)`, dotted forms work |
| SQL query | batch-local `sql`` supports joins and anti-joins |

Human smoke commands live in the root `justfile`:

```bash
just v4-flow-smoke
just v4-test
just v4-lsp-build
just v4-app-host-test
```

## Known Gaps

| Gap | Current status | Lock-in needed |
| --- | --- | --- |
| cursor-flow hover | inlay counts only | hover shape: row samples, count, schema, or provenance |
| runtime `lsp_warn` publication | op exists, app/CLI/LSP collection path missing | diagnostic row schema and collection timing |
| SQL TextMate highlighting | missing from VS Code grammar | scope strategy for nested SQL |
| SQL LSP completions from rule schemas | partial body provider, no full schema intelligence | rule namespace and column metadata surface |
| full V3 AST hover parity | partial/missing | AST DSL hover payload and capture positions |
| empty-rule send identity | ignored target test | direct call syntax and write/query disambiguation |
| bodied rule apply | ignored target test | cache key, output table behavior, tail-call behavior |
| mounted query reactivity | ignored target test | subscription identity, invalidation, retraction |
| `next` workflow parity | substrate exists | channel names, persistence, wake/replay rules |
| live invalidation kernel | missing | generation boundary and dirty-key model |
| file/git watcher | missing | watch source, debounce, branch/rev invalidation |
| ghcacher integration | missing | cache ownership and import path from V2/V3 |
| markdown render | not rebuilt cleanly | render op surface and aggregate semantics |
| aggregate render | weaker than V3 | grouping key, ordering, idempotent writes |
| write invalidates read/fs caches | missing | write event and cache dependency keys |
| cross-rev write/worktree materialization | missing | worktree policy and target address form |
| full V3 server parity | partial | whether HTTP daemon or generic app_host is canonical |
| LSP test suite health | one known failing semantic-token test | fix glob token provider or update test expectation |

## Drive Order

Use this order to get back to parity without depending on future indexing:

1. CLI dogfood loop

```text
scan files -> extract rows -> query rows -> render markdown -> write artifact
```

2. Editor confidence loop

```text
open sprf -> syntax highlight -> diagnostics -> inlay cursor counts -> DSL hover/completion
```

3. V3 parity loop

```text
runtime lsp_warn publication
cursor-flow hover
markdown/aggregate render
empty-rule send identity
bodied rule apply
mounted query reactivity
next workflows
watch/invalidation
```

4. Future index loop

```text
string/norm index
identifier index
import/export index
rough call graph
rough type graph
blast radius
cross-repo dependency graph
```

## Under-Specified Lock-In Points

Bring these back for human decision before implementation:

| Topic | Decision needed |
| --- | --- |
| cursor-flow hover payload | count-only, sample rows, full rows, schema, provenance, or all behind commands |
| runtime diagnostics | collect from `lsp_warn` components directly, or require diagnostic rule tables |
| empty-rule send syntax | direct `frontend_hooks(OP, FILE)` send vs explicit `rule(:frontend_hooks, ...)` write only |
| bodied rule apply | whether `rule_name(...)` ever runs body or always reads relation rows |
| mounted query | whether subscriptions are automatic at rule read sites or explicit through an op |
| `next` storage | transient event queue vs durable event table |
| markdown render | SQL aggregation first vs host `render` op first |
| daemon canon | keep current HTTP `sprefa-daemon` as canonical or migrate to generic `app_host` |
| watcher source | filesystem watcher first, git polling first, or ghcacher first |

## Current Test Notes

Passing:

```bash
just v4-test
just v4-app-host-test
just v4-flow-smoke
just v4-lsp-build
```

Known failing target/health checks:

```bash
just v4-target-tests
just v4-lsp-test
```

`v4-target-tests` fails because it encodes future runtime semantics.

`v4-lsp-test` currently has a known failing semantic-token test for glob body tokens.
