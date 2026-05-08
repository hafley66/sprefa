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
just v4-v3-parity-targets
just v4-lsp-test
```

`v4-target-tests` fails because it encodes future runtime semantics.

`v4-v3-parity-targets` fails because it encodes V3 parity targets that are not implemented yet:

- runtime `lsp_warn` publication through app/LSP diagnostics
- backtick path body for `write_file`
- markdown aggregate render/write idempotence

`v4-lsp-test` currently has a known failing semantic-token test for glob body tokens.

Implemented since this tracker was written:

- runtime barrier lifecycle for `dispatch` / `idle` / `complete`
- `collect()` as completion-only aggregate over cursor values
- `collect_ready(:snapshot)` and `collect_ready(:append)` as partial barrier flush modes
- `collect() > write_file(PATH)` writes one aggregate value when the upstream batch completes

## Red Target Tests

Target tests live in:

```text
v4/tests/rule_future_semantics_target.rs
v4/tests/v3_parity_target.rs
```

They are ignored by default so normal `just v4-test` stays green.

Run them explicitly when working a parity slice:

```bash
just v4-target-tests
just v4-v3-parity-targets
```

Expected current failures:

| Test | Target |
| --- | --- |
| `empty_rule_fully_bound_apply_sends_identity` | old fact/send folded into empty rule |
| `mounted_query_reacts_to_late_relation_write` | live query reactivity and retraction |
| `bodied_rule_apply_runs_body_and_emits_outputs` | bodied rule apply/run/cache |
| `runtime_lsp_warn_publishes_diagnostics_for_open_buffer` | runtime diagnostic effects collected into app/LSP diagnostics |
| `write_file_backtick_path_writes_cursor_value` | path strings use backticks, not symbols |
| `render_markdown_aggregate_writes_file` | render markdown from rows and idempotently write artifact |

Current failure modes:

| Test | Current failure |
| --- | --- |
| `runtime_lsp_warn_publishes_diagnostics_for_open_buffer` | `get_diags` returns parse/walk diagnostics only; runtime `RenderCtx` diagnostics are not collected into `DocState` |
| `write_file_backtick_path_writes_cursor_value` | `write_file` rejects a DSL body with `lower/slot-not-allowed` |
| `render_markdown_aggregate_writes_file` | `render` is an unknown op and `write_file` rejects a DSL body |

## Review Before Implementation

LSP, render, and write behavior are complex enough to review before locking code.

Open decisions:

| Topic | Review question |
| --- | --- |
| cursor-flow hover | Should hover show counts only, sample cursors, full rows, rule schema, refs/provenance, or dispatch to separate commands? |
| runtime diagnostics | Should `lsp_warn` write diagnostic rows into a table, emit `RenderCtx` diags collected by app state, or both? |
| diagnostic lifetime | Are runtime diagnostics recomputed per open buffer, per run generation, or per mounted query subscription? |
| `write_file` path body | Is `write_file\`path\`` the final target syntax, and should interpolation in that path be allowed? |
| write target address | Should writes target disk paths only first, or support repo/rev/file refs before parity? |
| render op | Should `render(:markdown)\`...\`` emit one row per input, aggregate rows, or require an explicit aggregate op? |
| aggregate ordering | Should render order preserve input order, SQL `ORDER BY`, source span order, or explicit group/order args? |
| idempotence | Should render/write compare content before writing, or always write and emit dirty events? |
| markdown recap | Should markdown generation be plain template rows first, SQL `group_concat` first, or a dedicated markdown renderer? |
