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
| cursor-flow visibility | inlay hints and host-op hover show probe cursor counts per op span |
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
| mounted SQL reactivity | SQL reads park on referenced table dirty keys and rerun after late writes |
| mounted SQL diffing | reruns emit only newly visible output cursor hashes |
| mounted SQL retraction | disappearing anti-join outputs retract supported downstream rule rows |
| runtime diagnostics | `lsp_warn` rows are collected for CLI run reports and open-buffer LSP diagnostics |
| write file | `write_file(:path)` and `write_file` backtick paths write `cursor.value` |
| write invalidation | `write_file` / `write_cursor` publish file dirty events for dependent reads |
| markdown render | `render(:markdown)` renders the current batch into one markdown value |
| aggregate write | `render(:markdown)` / `collect()` can feed `write_file` for one artifact write |
| durable queue | `SqliteQueue` can revive parked continuations after reopen |
| app SQLite backends | `sprefa-run` and `sprefa-daemon` accept `--queue-db` and `--fact-db` |
| unified config | `$SPREFA_CONFIG` / `~/.config/sprefa/config.toml` provide repo, store, run, daemon defaults |
| ghcacher dirty seam | default-on `ghcache` feature maps `change_log` rows to dirty wake keys |
| daemon ghcache polling | `sprefa-daemon --ghcache-db` polls ghcacher and drains ready continuations |
| config examples | `v4/examples/sprefa.config.example.toml` plus just recipes cover config smoke paths |
| LSP test health | `just v4-lsp-test`, `just v4-lsp-build`, and `just v4-vscode-compile` pass |

Human smoke commands live in the root `justfile`:

```bash
just v4-flow-smoke
just v4-test
just v4-config-test
just v4-run-with-config
just v4-lsp-build
just v4-lsp-test
just v4-app-host-test
just v4-ghcache-test
just v4-no-ghcache-test
```

## Known Gaps

| Gap | Current status | Lock-in needed |
| --- | --- | --- |
| cursor-flow hover | count/span hover exists | row samples, schema, and provenance payloads |
| SQL TextMate highlighting | missing from VS Code grammar | scope strategy for nested SQL |
| SQL LSP completions from rule schemas | partial body provider, no full schema intelligence | rule namespace and column metadata surface |
| full V3 AST hover parity | partial/missing | AST DSL hover payload and capture positions |
| mounted query restart | partial | reopen app with both SQLite backends and resume existing mounts |
| `next` workflow parity | substrate exists | channel names, persistence, wake/replay rules |
| live invalidation kernel | table/file dirty keys exist | column-value dirty keys and watcher sources |
| file/git watcher | partial | ghcacher source exists; direct fs watcher and git blob invalidation policy still missing |
| ghcacher integration | partial | dirty source and daemon polling exist; repo subscription/config ownership still missing |
| aggregate render policy | basic batch markdown works | grouping key, ordering, idempotent write policy |
| write invalidates read/fs caches | basic file dirty path exists | git/blob/cache dependency keys |
| cross-rev write/worktree materialization | missing | worktree policy and target address form |
| full V3 server parity | partial | whether HTTP daemon or generic app_host is canonical |
| LSP test suite health | green | keep `just v4-lsp-test` and `just v4-vscode-compile` in the release gate |

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

## Closest Tasks Out Of 10

1. **Done: refresh parity docs after every green slice**

   Current tracker had stale ghcacher/config rows. Keep this file aligned with tests before starting larger work.

2. **Done: add config examples and justfile commands**

   Added `v4/examples/sprefa.config.example.toml` and commands for:

   ```bash
   just v4-config-test
   just v4-run-with-config
   just v4-ghcache-test
   just v4-no-ghcache-test
   ```

   Current config loader uses `$SPREFA_CONFIG`, but the CLI has no explicit `--config` flag.

3. **Done: run and update LSP test health**

   Verified:

   ```bash
   just v4-lsp-test
   just v4-lsp-build
   just v4-vscode-compile
   ```

   Stale glob semantic-token syntax fixture is fixed.

4. **Done: route app `/lsp/hover` through `sprefa-lsp`**

   `sprefa-lsp` hover now calls the app `lsp_hover` RPC. Completion still uses `lsp_locate_dsl` for body-local DSL providers.

5. **Done: first cursor-flow hover payload**

   Implemented first payload:

   ```text
   op name
   cursor count
   source span
   ```

   Next payload fields:

   ```text
   bounded sample rows
   known terms
   source refs when present
   ```

   Keep sample bounded so hover never dumps a V0-sized blast radius.

6. **SQL schema completions**

   Surface rule declarations and columns to SQL body completions:

   ```text
   input
   input.<term>
   rule table names
   rule columns
   core tables later
   ```

7. **Ghcacher repo subscription/config ownership**

   Current daemon polls a known DB. Next slice should define how sprefa config says:

   ```toml
   [daemon]
   ghcache_db = "..."

   [[repos]]
   slug = "owner/name"
   root = "..."
   ghcache = true
   ```

   Then decide whether sprefa calls ghcacher `/subscribe` or only tails existing cache.

8. **File watcher source**

   Add direct filesystem watcher or polling source for local worktree file changes, mapped to existing `FILE_DOMAIN` dirty keys. This is separate from ghcacher branch/repo dirty.

9. **`next` language workflows**

   Lock the smallest user-facing `next` pattern:

   ```text
   emit event
   park waiting cursor
   wake by channel/key
   persist through SQLite queue
   ```

   Runtime substrate exists; language surface is the gap.

10. **Dogfood invariant pack**

   Build one real `.sprf` pack that uses current primitives:

   ```text
   repo config
   fs/read
   json/ast/re
   rule rows
   SQL anti-join
   lsp_warn
   render markdown recap
   ```

   Target: one command that proves a repo-local invariant and produces both diagnostics and a recap file.

## Under-Specified Lock-In Points

Bring these back for human decision before implementation:

| Topic | Decision needed |
| --- | --- |
| cursor-flow hover payload | count-only, sample rows, full rows, schema, provenance, or all behind commands |
| mounted query | whether subscriptions are automatic at rule read sites or explicit through an op |
| `next` storage | transient event queue vs durable event table |
| daemon canon | keep current HTTP `sprefa-daemon` as canonical or migrate to generic `app_host` |
| watcher source | ghcacher exists first; decide direct filesystem watcher and git blob invalidation policy |
| config flag | keep `$SPREFA_CONFIG` only or add explicit `--config` to run/daemon/LSP |

## Current Test Notes

Passing:

```bash
just v4-test
just v4-app-host-test
just v4-flow-smoke
just v4-config-test
just v4-run-with-config
just v4-ghcache-test
just v4-no-ghcache-test
just v4-vscode-compile
just v4-lsp-test
just v4-lsp-build
```

Health checks to run when changing targets or LSP:

```bash
just v4-target-tests
just v4-lsp-test
```

`v4-target-tests` currently includes promoted mounted-query and rule-apply targets.

`v4-lsp-test` now passes. The previous glob body token failure was a stale `glob \`...\`` fixture; the current host syntax is `glob\`...\``.

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
- mounted SQL parks on table dirty keys and reruns after late relation writes
- mounted SQL output sets are replaced per `mount_id + input_key`
- mounted SQL reruns emit only newly visible output cursor hashes
- anti-join outputs disappearing retract downstream supported rule rows
- `SqliteQueue` can persist, reopen, wake, and resume parked rows
- app-mounted pipe identity is stable across source edits that shift statement order
- barrier scope identity is stable across later `expand_tick` resumes
- `SqliteFactStore` identity follows declared columns, not hidden cursor fields
- `sprefa-run --queue-db` creates a durable queue database
- `sprefa-run --fact-db` creates a durable fact database
- `sprefa-daemon` accepts `--queue-db` and `--fact-db`
- `SprfState` can run with memory facts/queue, SQLite queue only, SQLite facts only, or both SQLite backends
- `SprfConfig` loads unified store/run/daemon/repo config from `$SPREFA_CONFIG` or XDG path
- `sprefa-run` and `sprefa-daemon` use config defaults with CLI flag override
- `ghcache` feature gates ghcacher code; default build includes it
- `sprefa-daemon --ghcache-db` tails ghcacher `change_log` and dispatches dirty keys
- `/git/ghcache-change` RPC dispatches one ghcacher change into the dirty wake path
- `/lsp/hover` RPC resolves cached DSL hover payloads in app state
- `sprefa-lsp` hover calls the app `/lsp/hover` RPC
- host-op hover reports cursor-flow count and source span from app probe state
- `v4/examples/sprefa.config.example.toml` documents runnable config shape
- root just recipes pin `RUSTC_WRAPPER=""` and `CC="cc"` so smoke commands avoid sandbox-blocked `sccache`

## Target Tests

Target tests live in:

```text
v4/tests/rule_future_semantics_target.rs
v4/tests/v3_parity_target.rs
```

Some future tests are ignored by default so normal `just v4-test` stays green.

Run them explicitly when working a parity slice:

```bash
just v4-target-tests
just v4-v3-parity-targets
```

Expected remaining failures should be checked with the current test output before editing this list. The mounted-query reactivity tests listed below have been promoted and should pass in the normal suite.

| Test | Target |
| --- | --- |
| future ignored rule semantics in `rule_future_semantics_target.rs` | design targets not yet promoted |

Promoted target tests now run green in the normal suite:

| Test | Locked behavior |
| --- | --- |
| `empty_rule_fully_bound_apply_sends_identity` | old fact/send folded into empty rule apply |
| `bodied_rule_apply_runs_body_and_emits_outputs` | dotted rule apply runs the stored body |
| `runtime_lsp_warn_publishes_diagnostics_for_open_buffer` | runtime diagnostics flow into open-buffer diagnostics |
| `write_file_backtick_path_writes_cursor_value` | path strings can use backtick bodies |
| `render_markdown_aggregate_writes_file` | markdown rows render and write an idempotent artifact |
| `collect_does_not_mix_two_pipe_instances` | barrier state is keyed by mount scope |
| `mounted_query_reacts_to_late_relation_write` | late table writes wake mounted SQL and clear missing rows |
| `mounted_query_rerun_emits_only_new_output_hashes` | rerun emits additions without replaying unchanged outputs |
| `mounted_query_retraction_cascades_to_supported_rule_rows` | stale downstream fact rows retract when support disappears |
| `sqlite_queue_revive_smoke` | parked continuations survive SQLite queue reopen |
| `app_can_use_sqlite_queue_for_mounted_sql_parks` | app driver can park mounted SQL work in SQLite queue |
| `app_can_use_sqlite_fact_store_for_rule_rows` | app driver can persist rule rows in SQLite facts |
| `app_reopens_sqlite_backends_and_retracts_mounted_sql_outputs` | reopened SQLite facts/queue retain mounted output state and retract stale rows |
| `sprefa_run_accepts_sqlite_queue_db` | CLI accepts durable queue DB |
| `sprefa_run_accepts_sqlite_fact_db` | CLI accepts durable fact DB |

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
