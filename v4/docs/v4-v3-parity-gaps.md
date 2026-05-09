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
| rule reads | `rule_name(...)` relation reads work; `rule_name?(...)` is rejected by the locked V4 surface |
| SQL query | batch-local SQL DSL supports joins and anti-joins |
| runtime diagnostics | `lsp_warn` rows are collected for CLI run reports and open-buffer LSP diagnostics |
| write file | `write_file(:path)` and `write_file` backtick paths write `cursor.value` |
| markdown render | `render(:markdown)` renders the current batch into one markdown value |
| aggregate write | `render(:markdown)` / `collect()` can feed `write_file` for one artifact write |

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
| SQL TextMate highlighting | missing from VS Code grammar | scope strategy for nested SQL |
| SQL LSP completions from rule schemas | partial body provider, no full schema intelligence | rule namespace and column metadata surface |
| full V3 AST hover parity | partial/missing | AST DSL hover payload and capture positions |
| mounted query reactivity | ignored target tests | subscription identity, invalidation, retraction |
| `next` workflow parity | substrate exists | channel names, persistence, wake/replay rules |
| live invalidation kernel | missing | generation boundary and dirty-key model |
| file/git watcher | missing | watch source, debounce, branch/rev invalidation |
| ghcacher integration | missing | cache ownership and import path from V2/V3 |
| aggregate render policy | basic batch markdown works | grouping key, ordering, idempotent write policy |
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
cursor-flow hover
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
| mounted query | whether subscriptions are automatic at rule read sites or explicit through an op |
| `next` storage | transient event queue vs durable event table |
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

`v4-target-tests` still runs ignored future runtime semantics:

- mounted anti-join should retract stale missing rows after later writes
- mounted query rerun should diff old/new mounted query outputs and emit only additions

`v4-lsp-test` currently has a known failing semantic-token test for glob body tokens.

Implemented since this tracker was written:

- runtime barrier lifecycle for `dispatch` / `idle` / `complete`
- `collect()` as completion-only aggregate over cursor values
- `collect_ready(:snapshot)` and `collect_ready(:append)` as partial barrier flush modes
- `collect() > write_file(PATH)` writes one aggregate value when the upstream batch completes
- runtime `lsp_warn` diagnostics publish through `RunReport` and open-buffer `get_diags`
- `write_file` accepts a backtick path body and interpolated path template
- `render(:markdown)` emits one batch aggregate markdown cursor
- `render(:markdown) > write_file` writes an idempotent markdown artifact in the parity smoke
- collect/barrier buffers are keyed by mount scope, so shared component objects do not mix pipe instances

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
| `mounted_query_reacts_to_late_relation_write` | live query reactivity and retraction |
| `mounted_query_rerun_emits_only_new_output_hashes` | mounted query diffing should emit additions without replaying old outputs |

Promoted target tests now run green in the normal suite:

| Test | Locked behavior |
| --- | --- |
| `empty_rule_fully_bound_apply_sends_identity` | old fact/send folded into empty rule apply |
| `bodied_rule_apply_runs_body_and_emits_outputs` | dotted rule apply runs the stored body |
| `runtime_lsp_warn_publishes_diagnostics_for_open_buffer` | runtime diagnostics flow into open-buffer diagnostics |
| `write_file_backtick_path_writes_cursor_value` | path strings can use backtick bodies |
| `render_markdown_aggregate_writes_file` | markdown rows render and write an idempotent artifact |
| `collect_does_not_mix_two_pipe_instances` | barrier state is keyed by mount scope |

## Review Before Implementation

LSP, render, and write behavior are complex enough to review before locking code.

Open decisions:

| Topic | Review question |
| --- | --- |
| cursor-flow hover | Should hover show counts only, sample cursors, full rows, rule schema, refs/provenance, or dispatch to separate commands? |
| runtime diagnostics | Should `lsp_warn` write diagnostic rows into a table, emit `RenderCtx` diags collected by app state, or both? |
| diagnostic lifetime | Are runtime diagnostics recomputed per open buffer, per run generation, or per mounted query subscription? |
| write target address | Should writes target disk paths only first, or support repo/rev/file refs before parity? |
| aggregate ordering | Should render order preserve input order, SQL `ORDER BY`, source span order, or explicit group/order args? |
| idempotence | Should render/write compare content before writing, or always write and emit dirty events? |
| markdown recap | Should markdown generation stay host-template first, move to SQL `group_concat`, or grow a dedicated markdown renderer? |
