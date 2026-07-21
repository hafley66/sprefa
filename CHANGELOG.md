# Changelog

All notable changes to `dl` (sprefa v5) are recorded here. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions track the `v*` release
tags consumed by cargo-dist.

## [Unreleased]

## [0.12.0] - 2026-07-21

### Added
- `dl daemon events [--kind K] [--root R] [--limit N]` reads `events.jsonl`, a
  trail beside `why.jsonl` that records the ARGUMENTS of each discrete IO event
  at the moment it happens, where `why` samples cost every 2s and renders detail
  to a string. Instrumented kinds: `file_changed` (full path list),
  `tick_start`/`tick_end`, `digest_moved`, `cold_extract_node`,
  `cold_staged_enqueued`, `git_ref_advanced`, `path_change_verdict` (hash_differs
  vs no_prior_row, old/new hash), `gen_write` (wrote vs skipped-identical),
  `effect_call`/`effect_result` (filled template args, credentials redacted),
  `db_write` (per batch, offered vs affected rows). Emission rides `tracing` on
  the `dl::event` target; the writer is a subscriber Layer installed in the
  daemon only, so one-shot runs and tests no-op. File-only, so it answers with
  the daemon down.
- `dl daemon health`: dbstat buckets, per-table rows/data/index bytes,
  identical-rowset duplicate probe, static copy-rule scan, orphan `roots/`
  directories against `roots.json`, and the db/corpus ratio. Read-only opens
  inside one read transaction, so it answers with the daemon live or down.
- Daemon HTTP/JSON transport: one axum 0.8 router serves both the UDS socket
  and TCP, `/watch` upgrades to Server-Sent Events (replacing the old framed
  subscribe pump), and `dl daemon url` prints the HTTP base URL from
  `http.json`.
- `dl daemon install`/`uninstall` wire the daemon into the OS-native service
  manager (launchd on macOS, a systemd unit on Linux) instead of hand-rolled
  daemonization; re-running `install` re-points a stale binary reference at
  the current launchd job.
- `dl daemon why` reads a durable self-diagnosis trail (`why.jsonl`):
  tick/phase/detail plus cumulative CPU and disk I/O, sampled every ~2s with
  no RPC and no engine lock, so it answers even after a SIGKILL or a
  mid-rebuild wedge.
- The daemon keeps a global invocation log independent of any one root: every
  `dl` invocation records pid, arguments, and exit outcome, so a killed
  process is still visible even if its root's own state is gone.
- `diag_stage(code, stage)` builtin sink routes diagnostics by presentation
  stage; `--check`/`--diag-json` gain `--stage
  <live|commit|agent-turn|agent-session>` (default `commit`) to filter which
  codes surface where. The database still keeps every diag row regardless of
  routing.
- `dl query --format json` prints one JSON array of `{col: val}` row-objects
  per query, additive to the existing `--query-json` NDJSON envelope form.
- `dl daemon jobs` lists queued, running, and completed tick jobs.
- Datapath extraction (`data`/`pattern` matchers) gains JSONL/NDJSON support:
  each line parses as its own JSON document.
- Daemon logging rides the `tracing` crate: rolling `<home>/log/dl.log`
  (level from `DL_LOG`, default info) and `<home>/log/error.log` (always
  warn-and-up), an stderr layer via `RUST_LOG`/`DL_TRACE`, and an optional
  `DL_TRACE_CHROME=<path>` chrome-trace export that finalizes cleanly even on
  a kill.
- `stale-binary` and `db-ratio` diagnostic rails: a warning when the running
  daemon's binary is older than the one on disk, and a verdict on the
  db-to-corpus size ratio.
- Callable coverage extended: `EntityKind::Lambda` plus
  lambda/constructor/nested-function/trait `call_def` emitters, TS/JS nested
  named function declarations included, backed by a fixture corpus, a
  reactive coverage rail, and a two-tier AST/SCIP proof stratum.
- `std/measures.dl`: a keep-all verdict plus rank-based top-K views.
- `examples/doc-marks.dl`: an `@@doc` marker routes chat/commit content into
  docs via `gen`.
- A static N+1 hunter example flags nested-loop paths that write per-row
  instead of batching.

### Changed
- Storage: the sym identity space is normalized behind one dense-surrogate
  allocator (`Db::sym_alloc`) shared by every write path — the `sprf_sym_intern`
  SQL UDF (facts, derived-rule heads), the Rust encode path, and flush — so an
  interned `sym` rel column stores a dense `_sym_dict` surrogate id (a ~5-byte
  varint) instead of the 8-byte `StringId` hash, and no join has to bridge two id
  spaces. `SCHEMA_EPOCH` 12 -> 13 drops and re-extracts every `rel_%` table into
  the dense space on first open of an older db. Behavior-preserving: gated by a
  build-time bijection check (distinct dense == distinct text per rel) and
  native-vs-SQL row parity on reachability closures (`sym_dict_bijection`,
  `halt_bfs` rails). Single-writer per root db (the daemon) is a documented
  invariant of the in-memory allocator.
- `DL_RAYON_THREADS` now defaults to 2, bounding extraction and hashing CPU by
  default while preserving an explicit override for larger worker pools.
- Rust type, call, and dataflow extraction now share one `syn` parse per changed
  file on both full and daemon/LSP path ticks; only projected facts enter the
  existing bounded caches, while source text and ASTs are dropped immediately.
  `DL_DISABLE_ANALYSIS_BUNDLE=1` restores separate family extraction for
  production A/B measurements.
- SQLite cache allocation now has a 32 MiB process ceiling and 16 MiB
  per-connection ceiling by default. `DL_CACHE_MB` controls the process ceiling,
  `DL_CONNECTION_CACHE_MB` controls one connection, `DL_MMAP_MB` explicitly
  enables a process-wide mmap budget, exhausted connections receive no
  unaccounted page-cache grant, and temporary work is file-backed.
- **BREAKING:** implicit daemon autostart is off by default. `dl file.dl`,
  `--check`, `--mcp`, and `--lsp` now attach to a daemon only if one is
  already running, falling back to their in-process path otherwise, instead
  of silently spawning one. `DL_AUTOSTART=1` restores the old
  spawn-on-attach behavior (test harnesses use it); explicit verbs
  (`dl daemon start`, `dl watch`) still spawn.
- `--no-daemon` is no longer a documented flag: the public no-daemon split is
  erased in favor of one server code path. The flag still parses
  (`DL_NO_DAEMON=1` is the same escape hatch) but is hidden from `--help` and
  the docs.
- The daemon's job queue moved onto `apalis-sqlite` (workers, storage, and
  crash recovery bought instead of hand-rolled); a job carries the request id
  that caused it, so a running tick can be cancelled mid-flight at its next
  component boundary, and `ColdExtract` jobs root-serialize so concurrent
  roots no longer cold-rebuild at the same time.
- The daemon runs under a background CPU budget by default: QoS plus `nice`
  plus a capped thread pool, and a duty-cycle governor that caps rather than
  merely advises (it also recognizes an existing `XPC_SERVICE_NAME`/launchd
  management context and skips its own redundant nice/IOPOL calls). A
  one-shot `dl` run inherits the same budget, plus a wall-clock watchdog and
  a bounded daemon-attach wait.
- The daemon enforces single-instance via an `fd-lock` file; its serving
  shell runs on tokio while the engine itself still executes synchronously
  behind `spawn_blocking`.
- Cadence-driven roots (`@async clock`/`every`) now tick only on a bucket
  flip instead of every poll cycle, and an effect-free root gets one
  settle-confirmation tick instead of polling forever.

### Fixed
- `Db::flush_syms` gained a connection-scoped `persisted_strings` cache, so
  already-durable `StringId`s are no longer re-offered to `_strings`. Ids are
  cached only when the flush ran outside a caller-owned transaction, so a
  rollback cannot leave the cache claiming a string is durable. The
  within-batch collision guard still runs over the full pending set before
  cache filtering.
- A linked git worktree with no warm database and no `roots.json` entry now
  skips a `--check` run instead of cold-building a root database that orphans
  when the worktree is deleted. `DL_ALLOW_WORKTREE_COLD=1` opts out.
- Extraction is deterministic and hermetic: every file-set query carries
  `ORDER BY` so cached facts emit in input order, a non-UTF-8 source file is
  skipped instead of aborting the whole tick, and a daemon-served engine no
  longer inherits ambient config-repo state.
- Fixed the exe-swap write storm: a rebuilt `dl` binary used to force a full
  re-derivation on every tick regardless of whether the corpus changed. The
  identity check now runs per-engine per-tick (not per-process), a re-derive
  of an unchanged corpus flips zero call rows and cascades nothing, and a
  lint rail with a prev-rev oracle bans the four code forms that caused it.
- Identical re-derivations write nothing: a digest-before-write check on
  derived rebuilds, unchanged rel-view DDL (a quiet tick's WAL went from
  2.48MB to 217KB), and settle bookkeeping all skip the write when content
  has not changed.
- `rebuild_derived` deletes and marks completion per component instead of one
  upfront whole-database `DELETE`, so a crash mid-rebuild only re-derives the
  interrupted component on restart.
- Fixed a daemon poll storm: a cadence root used to run a full tick (corpus
  walk plus dirty derived rebuilds, one rule measured at 9.7s) every 2
  seconds forever regardless of whether anything changed. An idle check now
  skips the cycle when nothing is pending, poll errors back off
  exponentially (capped near 60s) per root, and a registered root whose
  directory no longer exists is evicted instead of retried forever. A no-op
  tick also no longer broadcasts `diag_changed`, which had been amplifying
  into client render storms.
- Dynamically declared effect templates were invisible to the executor (an
  interned-id column was read as text and always returned none), parking
  their effects orphaned at boot; effects now re-queue on their own once a
  template appears, and a real execution failure is marked terminal instead
  of retried forever.
- Dataflow coverage: TS/JS class methods now reach the dataflow walker, a
  loop's break value and a value-position `if`/`match`/block branch tail
  into their expression node instead of dropping the edge, and nested named
  function declarations become `call_def`s.
- SCIP reindex now fires on incremental ticks with the newest index winning;
  `.dl/.state/index.scip` is preferred over a stale root index, and
  qualified call paths plus struct constructors are captured.
- Query output: text format no longer indents data rows, `query_log` is
  queryable on a fresh database, and graph-op edge columns are quoted.
- The `why.jsonl` trail appends as a single write with `shutdown_cleanup`
  running exactly once, samples and `perf.jsonl` records carry the served
  root, and root attribution survives the sink-drain half of a daemon job.
- `dl update`'s argument parser scans every argument instead of bailing at
  the first unknown flag.
- Fixed a `df_node` sym-prefix mismatch that zeroed blast-radius results for
  any rule joining `call_def` spans the same way the rusqlite-coupling rail
  does.
- A render/derive failure no longer drops the tick's memo (T2 failpoint
  findings).
- The daemon's own state writes (`perf.jsonl`, `why.jsonl`, `cache.db*`)
  never schedule a tick, closing a recursive self-tick loop.

### Performance
- Dense `_sym_dict` surrogate for interned sym columns shrinks both the stored
  cell and every index built on it. Measured A/B on the full `.dl/` program set
  over this repo (516 rels, identical corpus, epoch 12 raw-hash vs epoch 13
  dense): **624.2 MB -> 515.4 MB, -17.4% (-108.9 MB)**. Gross saving on interned
  cells + indexes is ~115.6 MB; the dictionary overhead (`_sym_dict` + its
  autoindex, ~9.4 MB) is netted out and pays for itself ~12x. `_strings`,
  `_df_node_dict` coordinate ids, and dataflow rows are unchanged (already dense
  or not interned syms).
- Auto-index demand is now planner-honest: PK-prefix on rowid tables, a
  tiny-relation floor, and a constant-column check replace the old broad
  index-everything policy, cutting the auto-index count from 771 to ~260 and
  reclaiming on the order of 120MB. Both figures are single `dbstat` readings
  of the sprefa root taken once, at dc9b67b1; a sibling reading of the same
  arc put the saving at 130MB, so treat the magnitude as approximate. That
  database has since been rebuilt, so neither number is reproducible today.
- `WITHOUT ROWID` applied to vouched builtin junction relations drops their
  redundant primary-key autoindex twin.
- `_strings.norm` column and its index are dropped; normalized comparison now
  happens via the query-time `norm()` scalar instead of a stored column.
- Call-graph relations (`call_def`, `call_edge`, `call_name`, `call_kind`,
  and their `_rev` twins) now derive from a self-registering family router
  with row-level incremental reconcile: an edit retracts and re-inserts only
  the changed rows instead of rebuilding the whole relation, and every
  family/op file passes a no-raw-SQL audit rail.
- Cold-start extraction (a blank database's first tick) is staged in
  MB-bounded chunks across ticks instead of one blocking pass, extended from
  dataflow to comment/template/unresolved relations. This shortens the longest
  single blocking node 3.2x (dataflow/0 at 2468ms becomes dataflow/3 at 766ms,
  one release-build run over a 3.37MB corpus). It trades total throughput for
  responsiveness: the dataflow family's end-to-end time rises about 20% from
  the per-slice re-parse and flush. Measurements in
  plans/2026-07-17-cold-start-staging.md.
- The `_source_stage_owner` per-call `INSERT` (a runtime N+1 that tripped the
  scream at roughly 390 writes per tick; two runs recorded 386 and 388) now
  batches at flush, and deltaflow per-change writes batch per loop instead of
  per row.
- Several slow rules (`port_of_reach`, `call_node`, `loop_entry_fn`, the
  `flow_edge` lambda-hop, `named_call_site`) are factored through
  intermediate relations and brought under the tick budget.
- Cache-database I/O relaxes (`synchronous=OFF`, no autocheckpoint) around a
  full rebuild.
- A program edit rebuilds only the moved derived subgraph instead of the
  whole derived layer.
- Read-only daemon RPCs (query, status) now route through a WAL read-only
  connection and a read snapshot instead of the engine mutex, so a query no
  longer blocks behind a running tick.

### Internal
- `Db` became the single SQL authority across the engine (the db-seam
  migration): every direct `rusqlite` call in `src/` and the test suite goes
  through the seam API, enforced by a containment ratchet
  (`no-new-rusqlite.dl`) with a `@rusqlite-ok` waiver for the rare exception;
  a sibling rail warns on a discarded `let _ =` over a SQLite call.
- Every `eprintln!` in `src/` converted to `tracing` macros;
  `.dl/no-new-eprintln.dl` ratchets the count to zero with `@eprintln-ok`
  waivers for the rare CLI-UX line that must bypass tracing.
- Large module splits for maintainability: `daemon.rs` became
  `src/daemon/{mod,root,home,budget,dispatch,client,read,http,shell}`, the
  CLI's `daemon.rs` renamed to `daemon_cmd.rs`, and the `reconcile`, `query`,
  `extract`, `modgraph`, and `typegraph` modules split into per-concern
  files.
- `verify.sh` now runs `cargo clippy` and `fmt --check` as part of the
  standard gate, following a mechanical clippy burn-down across the tree.
- `docs/failure-modes.md` catalogs every incident class with its rail and
  status, established as the standing incident ledger.
- Test infrastructure: an every-op every-rel soak program that validates its
  own coverage, T1-T4 reconcile/render-flip/edit-script/property test
  suites, and a crate-wide lock for the perf-log globals (a stray env race
  caused solo-run panics).

## [0.9.0] - 2026-07-11

### Added
- `dl setup` now journals every file it writes to `$XDG_STATE_HOME/sprefa/setup-manifest.json`. `dl setup --list` shows the record, `--undo [--dry-run]` reverses it (hash-verified; user-modified files are left in place with a note), `--adopt` records verifiable pre-manifest wiring, and `dl uninstall` removes wiring plus dl's state directory while leaving the binary. Undo removes empty directories it created and file husks it created, non-recursively, and refuses paths escaping the expected roots; a hermetic e2e asserts setup -> undo -> uninstall returns the tree to its pre-setup state byte-for-byte.
- README restructured for new users: pitch with real output, a `comment_node` quickstart, a generated capability gallery and stdlib table (`examples/gen-readme.dl`, guarded by `readme-drift`/`readme-orphan` `--check` rails), docs-search commands, dogfooding notes, uninstall, and a feedback policy for humans and AI agents (agents must never post to GitHub without explicit human approval).
- The embedded skill documents demand sinks (`scip_want`, `checkout`/`checkout_done`, `rev_cmp_want`, `repo`) and eight authoring gotchas landed at the surfaces agents read first (`match` id vs captured text, `=~` capture non-binding, `gen` ordering and `{var}` sigil, `ast_yaml` immediate-parent `inside:`, `(?i)` class folding, and more).
- `dl daemon --help`, `dl what/q/summary --help` exist and the CLI help text was audited against actual behavior.
- The `dl` language itself joined the tree-sitter grammar table, so `comment_node` works on `.dl` files.
- Recursive fixpoints now use semi-naive delta evaluation, with `DL_NAIVE_FIXPOINT=1` as a compatibility escape hatch; cold `--check` on this repo went 37.2s -> 5.6s. Aggregate and lattice components retain the naive path.
- `DL_CACHE_MB` (default 512) configures SQLite’s page cache and mmap budget, with temporary tables kept in memory.
- `_stmt_ms` now reports aggregate wall time and statement count, including engine-side derived-work buckets, making performance attribution substantially more complete.
- The engine and setup hook wiring are split into focused modules, making the codebase easier to navigate and maintain.
- Verdict-line logging adds a concise observability summary for runs.
- Discovery `--check` can attach to a warm daemon; PR polling avoids a cold-start stampede, and `verify.sh` skips redundant build/suite work when its digest is unchanged.

### Changed (BREAKING)
- **Interned join keys are now integer-backed.** `_strings`, `_where_bytes`, builtin string/ref ids, and dataflow ids use `int`/`sym` storage for faster joins; incompatible user joins now fail with an actionable type error (see `plans/2026-07-11-intern-string-keys.md`).
- Existing `_strings`/`_where_bytes` caches are recreated and extraction digests invalidated on open, so the next tick safely repopulates data without a migration.
- The `Sym`/`SymSink` spine API batches collision-checked string interning, and its type prevents accidental writes to text columns.
- Sym query decoding now qualifies the outer id correctly, while generic edge readers accept integer-backed ids instead of silently dropping rows.

## [0.8.0] - 2026-07-11

### Added
- **Mixed source+derived rels auto-desugar.** A rel headed by both a source/
  extract rule and a derived rule no longer bails: the engine rewrites the
  heads to internal `<rel>__src`/`<rel>__drv` twins and unions them back
  under the visible name. Twins are invisible everywhere (queries, catalog,
  diags, and `rel_count`/`stmt_ms` telemetry, which fold twin rows under the
  visible rel). Lattice (`key`/`merge`) rels and `@in`/`@out` ports still
  bail, on purpose. Term-extract rules (json/jsonp/sg term-form) can now
  co-head with derived rules too, including feeding `@next` carries.
- **`module_binding(file, local_name, source_module, imported_name, kind)`**
  (+ `_rev` twin): every local import binding, index-free — kind = named |
  default | namespace | side_effect | reexport per language. Sibling of
  `module_binding_resolved` (which deliberately excludes library imports);
  "which library does this local name bind to" is a two-line join.
- **`template_parts(file, line, node, idx, kind, text)`**: template literals
  split into ordered static/expr pieces (TS/TSX/JS/JSX/MJS/CJS, tagged and
  nested included). Own extract family — no second parse, programs reading
  only it don't pay for type/call/dataflow. `node` is the `df_node`/`df_lit`
  id for the SAME template occurrence (`{file}:{byte}:template`, matching
  `ts_push`'s scheme), not a bare byte offset — a piece joins `df_lit.id`/
  `df_edge.to` directly, with no separate id math. A tagged template's
  `node` anchors at the `TaggedTemplateExpression`'s own span start (the
  tag's position), matching where the dataflow lift mints its id, not the
  quasi's start the two walks previously disagreed on.
- **`std/strings.dl`: `const_string_member(file, object, member, value)`
  derived view** — string-valued const object members (route maps, lookup
  tables, key registries), joined from `const_value ⋈ type_entity` (flat
  fields only, `kind = "lit"`). Was shipped as a builtin relation in
  unreleased code; retired to a `use`-able view during a reconcile pass
  against the overlapping `const_value` builtin (same corpus, no
  duplication). The evidence diff against a real corpus found exactly one
  gap — a const declared inside a function body — closed in
  `const_value`'s TS lift (`TsNestedConstWalker`) before the retirement, so
  the view's coverage is byte-identical to the old builtin's.
- **`unresolved(file, line, reason, detail)`**: first-class marker for edges
  whose target is computed at runtime (dynamic-import, computed-member, ...),
  limited to cases the extractors already detect.
- **`dl --version` / `-V`.**
- **`.dl/perf-woes.dl` rail**: onsite tick-cost diagnostics — slow-rule WARN
  positioned at the rule head, tick-over-budget ERROR, rel row-count blowup
  WARN, all budget-fact driven.
- **Perf telemetry**: every perf.jsonl record carries the writing PID; tick
  records carry `full_reason` (blank-slate | program-edit | carry-changed |
  derived-missing:<rels> | ...); one-shot runs now emit derived-rebuild phase
  records (previously daemon-only).

### Changed
- **BREAKING: import-binding rel family renamed.** The v0.7.0
  `module_binding(file, local, source, dst)` (+ `_rev`) — the alias-only,
  dst-resolved subset consumed by the resolver alias hop and `dl what` — is
  renamed to **`module_binding_resolved`** (+ `module_binding_resolved_rev`).
  The wider syntactic tier introduced above (every local import binding incl.
  external/unresolved modules, kind column) now takes the base name
  **`module_binding`** (+ `module_binding_rev`) instead of `import_binding`.
  Any query reading the 4-ary `module_binding(file, local, source, dst)` shape
  must switch to `module_binding_resolved`; any `import_binding` query renames
  to `module_binding`. Semantics unchanged, names only.

### Fixed
- **`--check` full-rebuild floor (P1).** `need_full` treated ANY empty derived
  rel as "needs full rebuild"; 34 legitimately-empty rels forced every tick to
  re-derive all 154 derived rels (~15s inside SQLite, warm or not). A
  `_derived_complete` marker now distinguishes "derived to empty by a real
  run" from "never populated"; warm `--check` drops to sub-second.
- **Writer-flood mitigation (P2).** `Db` close checkpoints the WAL
  (TRUNCATE), and opening a db another process is writing warns loudly;
  one-shot runs on a daemon-served root were silently queueing behind each
  other on busy_timeout.
- **`dl q who-calls` reported the caller's declaration line**, not the call
  site. Now joins `call_site` for the actual call line (1-based).
- **`hover_note` builtin sink.** `hover_note(path, line, col, end_line,
  end_col, md)` in the diag pattern: rules head it to attach markdown to a
  source span (0-based, inclusive ends); the LSP appends each matching note
  to the hover at that position, and notes alone hover where no entity
  matches.
- **Goto-flow recorder.** `dl.recordFlow` in the VS Code extension
  (cmd+alt+g): named recording takes land every goto jump as a
  `hook_event("goto", session, seq, json)` row via the new `dl/hookEvent`
  LSP request (mirrors the daemon `hook_event` RPC). `examples/goto-flows.dl`
  unions and anti-unifies takes into named flows: `flow_union_edge` (any
  take), `flow_common_edge` (every take), a `flowmark` panel layer with one
  legend chip per flow, hover membership via `hover_note`, and
  `? flow_stat(name, takes, edges)`.
- **Panel test harness.** vitest + playwright under `editors/vscode-dl/`
  (`npm test` / `npm run test:e2e`): hermetic fixture bridge serving the
  dl-bridge `/rpc` shape from canned tables, 9 unit + 6 e2e including list
  and trace view screenshot baselines.
- **documentHighlight / workspaceSymbol / documentSymbol (B2).** Three
  standard LSP features off existing engine tables: highlight = the
  identifier's same-string spans within the request file; workspace symbols
  = LIKE-contains over `type_entity` + `call_def` names (prefix matches
  first, cap 200, multi-repo URIs); document symbols = the file's
  `type_entity` rows nested by parent sym (outline view).
- **BOM table preset + where-used (C1/C2).** `.dl/bom.dl` derives
  `bom_node`/`bom_edge`: every member-flow part annotated with
  member_count, fan_in, fan_out (distinct, set-deduped across type + call
  links), and weight (callable span lines). The panel renders them as a
  right-aligned numeric band with sort chips (fan-in descending default),
  shows subtree totals on a collapsed group, and opens a where-used
  overlay on alt-click (callers, incoming type refs by kind, field
  fill/read, importers — all sym-pinned queries).

### Fixed
- **Flow panel list view rendered zero rows in current Chromium.** The wave-2
  virtualization resolved the toolbar offset via `gutterLeft.offsetTop`, but
  `#gutterLeft` is an svg and `SVGElement` has no `offsetTop` — the window
  bounds went NaN and no rows materialized (webview included). Found by the
  new playwright harness on its first run; all three sites now read the
  `#listRows` div.

## [0.7.0] - 2026-07-10

### Added
- **`dl what <anchor>` + `dl summary <path>` — the turnkey meta-query.** One
  command answers "what is this name / what's in this file" across every
  built-in graph family with zero .dl authoring: the anchor resolver classifies
  name | path | path:line (glob `*` allowed) and unions
  `type_entity`/`call_name`/`scip_name`/`scip_binding`/`module_binding`/
  `df_node.var`, then fans out per family (def sites, caller/callee counts,
  type links, sig slots, doc presence). Daemon-first (`what`/`summary` RPCs)
  with an in-process fallback that forces extraction via a synthetic probe
  program. Plan: `plans/2026-07-10-turnkey-query-surface.md`.
- **`module_binding` — aliased imports resolve without an index.**
  `module_binding(file, local, source, dst)` (+ `module_binding_rev`) captures
  aliased-import local bindings from the existing module-resolver parse — Rust
  `use x::y as z`, TS/JS `import { a as b }` and default imports, Kotlin
  `import a.b.C as D` — and feeds an alias hop in the type/call resolvers: a
  reference to the local alias resolves to the aliased def, dst-pinned, only
  when the file declares no same-named local (shadowing wins), never guessing
  on a miss. `dl what <alias>` finds the canonical def index-free — the
  syntactic twin of `scip_binding`. Barrel re-exports, namespace imports, and
  default-export resolution stay honestly unresolved (rows carry
  `source="default"` for a future bridge).
- **JS/JSX type + call + dataflow extraction.** `.js`/`.jsx`/`.mjs`/`.cjs` now
  ride the TypeScript TypeLang (oxc parses them as JS), so plain-JS repos get
  `type_entity`/`call_edge`/`df_*` rows instead of nothing.
- **Import-scoped ambiguity narrowing in the name resolver.** When a name has
  several same-repo defs, candidates narrow to the referencing file's own
  imports (`module_edge_rev`), itself, or its directory — and resolve only on
  a lone self-or-imported survivor. Cuts cross-file same-name misjoins at the
  syntactic tier; a same-dir-only tie stays unresolved.
- **SCIP-parity oracle (`call_resolution_parity_vs_rust_analyzer`).** One
  ignored test scores the index-free call resolver against a rust-analyzer
  SCIP index with confirmed-positives-only math: unconfirmable resolutions are
  excluded, contradicted ones are a separate bounded bucket (precision >= 0.95
  asserted), and every fuzzy comparison step fails toward exclusion — the
  reported percent can under-count, never inflate. First snapshot on this
  crate: 48.1% parity, 0.994 precision.
- **Aggregation heads in `?` query items.** A query head carrying an aggregate
  call switches from `SELECT DISTINCT` to `GROUP BY`, the same aggregate surface
  rule heads have: `? sale(dept, json_group_array(items), _)` collects one JSON
  array per `dept`, `? sale(dept, _, sum(revenue))` sums the price column per
  group, `? sale(_, _, sum(revenue))` is a whole-rel aggregate (one row). Plain
  var terms are the grouping key (and the deterministic ORDER BY), literals stay
  WHERE filters, wildcards collapse; the aggregate's arg var names the output
  column. `json_group_object(key, value)` consumes two adjacent columns (query
  arity stays exact): place it at the key column with `_` at the value column
  (`? line(order_id, json_group_object(items, prices), _)`). json aggregates keep
  their internal `ORDER BY` so output is byte-stable tick to tick. Non-aggregate
  function calls in a query head still bail (derive a relation and query it). Runs
  through every query consumer: one-shot `?`, the daemon `query` RPC, and the LSP
  paging wrapper. Example lift: no more derive-a-rel-then-query just to aggregate
  a `?`.
- **Dev-loop just recipes (deterministic ceremony as scripts, not agents).**
  `just verify` = build + full suite with the FSEvents flake solo-rerun policy +
  the magic-rel/recompute-guard rails; `just regen-docs` = every doc generator
  with a fresh db, second-pass convergence required, checked-claims rail;
  `just cut X.Y.Z` = verify + changelog gate + `scripts/release.sh` + commit
  audit (never pushes). Gotchas live as header comments in
  `scripts/verify.sh`/`scripts/regen-docs.sh`.
- **Tracked agent + memory homes under `.agents/`.** `.agents/agents/` carries
  the subagent definitions (magic-rel-auditor, builtin-rel-implementer,
  extraction-op-implementer) and `.agents/memory/` the session-memory corpus;
  `.claude/` stays gitignored. `assets/sprefa-flow-panel-layers.skill.md` gains
  the tracked backing the other skills already had.
- **`env(name, value)` built-in relation.** Projects the process environment
  captured at start, scoped to a prefix allowlist (`SPREFA_`/`DL_`/`SG_`) plus
  the `CI`/`GITHUB_ACTIONS`/`GITLAB_CI` markers so tokens and credentials never
  reach the on-disk db. Constant for the process lifetime (fills once, then
  self-diffs to a no-op). Enables env-gated rails, e.g.
  `diag(...) <- hit(path), env("CI", "true").`
- **`dl/refs` grouped references lens (B1).** New custom LSP request: params
  `{uri|path, line, character}` (0-based) -> a `RefLens`
  `{tier, symbol, display_name, declarations, uses, containing_types, callers,
  callees}`, every hit carrying `{repo, path, line, col, end_line, end_col,
  role, container}`. Tier `resolved` joins the identifier by name through
  `type_entity`/`call_def` (declarations), `type_link`/`call_site`/
  `module_import` (uses by role), and 1-hop `call_edge` (callers/callees, no
  closure); tier `textual` is the ref-spine same-string fallback, role `text`.
  Backed by `Engine::refs_lens`. The extension gains a "dl References"
  explorer TreeView (tier -> repo -> role) behind `dl.findReferences`
  (cmd+alt+r).
- **`dl/graphChanged` push (A5 v1).** The LSP sends an outbound
  `dl/graphChanged {}` notification after a daemon `diag_changed` broadcast or
  an in-process didSave tick; the extension forwards it to the flow panel,
  which re-runs its query debounced 250ms behind a new default-off "auto"
  toolbar toggle (visible tab only).
- **Flow panel list virtualization (A6).** The list view windows its rows
  (fixed 22px rows, viewport + 10 rows overscan, absolute positioning in a
  full-height spacer) instead of rebuilding up to 2,000 divs per toggle.
  Gutter arcs still draw for all rows; centering on an off-screen row scrolls
  it into the window.

### Fixed
- **`scip_occurrence.role` no longer collapses import/read/write to
  `reference`.** The role column now reports
  definition/import/write/read/reference (compound read+write reports write),
  so import sites and mutation sites are filterable — v0.6.24 discarded those
  SymbolRole bits.
- **`--lsp` no longer auto-spawns a daemon when `--db` is explicit.** `run_lsp`'s
  subscriber thread spawned a DETACHED `dl daemon start` whenever the root owned
  `.dl/`, ignoring the explicit `--db` every e2e test passes for isolation — each
  LSP test sandbox leaked a live daemon (12 found running against real sandboxes
  and, via an unrelated cwd accident, the dev repo's own socket). The gate now
  mirrors `run_file` (`db_path.is_none() || db_defaulted`); the lsp_protocol and
  diag_mute test harnesses additionally set `DL_NO_DAEMON=1` as defense in depth.
- **`dl daemon start <program>` refuses a program outside the resolved root.**
  Root resolution never looked at the program path, so `dl daemon start
  /tmp/x/p.dl` run from a repo cwd silently bound THAT repo's socket while
  serving the /tmp program (observed wedging reactive doc regen for a day). An
  absolute program that canonicalizes outside the root now exits 2 with the
  mismatch named; set `DL_DAEMON_ROOT` to serve cross-root deliberately.
- **`--lsp --stdio` killed the LSP at spawn.** `--stdio` is an alias of `--lsp`
  (added v0.6.22), and newer vscode-languageclient appends `--stdio` to stdio
  servers even when the extension already passed `--lsp` — clap's SetTrue
  rejects the repeat, so the server exited code 2 five times and VS Code gave
  up ("dl stopped"). The flag now self-overrides (`overrides_with`), so any
  mix of the two spellings parses.
- **madge oracle vs colorized madge.** Newer madge (chalk) colorizes even
  non-tty stdout, so ANSI escapes broke the text-parsed `--warning` skip list
  in `tests/it/oracle_madge.rs`. The test now sets `NO_COLOR=1` /
  `FORCE_COLOR=0` on every madge invocation and strips ANSI sequences before
  parsing.

### Changed
- **Singleton daemon + registered roots (de-root C).** One daemon process now
  lives at a constant home (`$XDG_STATE_HOME/sprefa`, else `~/.local/state/
  sprefa`) and serves EVERY `.dl` root over ONE socket; cwd only picks WHICH root
  a query addresses. Each root gets its own warm engine + db under
  `<home>/roots/<key>/db.sqlite`; a `root` envelope key on every RPC routes to it
  (absent = the config view). Per-root daemons + `<root>/.dl/daemon.sock`/`.pid`/
  `db` are RETIRED — the spawn-if-missing-per-root mechanism that leaked one
  daemon per test sandbox (and once bound a real repo's socket to a throwaway
  program) is gone. **Attach IS registration**: the first RPC naming an
  unregistered `.dl` root auto-registers + cold-ticks it inside the daemon;
  `roots.json` persists the set and a restart replays it (warm from each db).
  `dl daemon start` now DETACHES a background singleton by default (`--foreground`
  is the debug path); `dl daemon stop` is global; new `dl daemon drop <root>
  [--purge]` deregisters one root; `dl daemon status` lists every registered root
  with its tick count. Program edits always hot-reload (one process exit would
  kill every root); the idle timer exits only when ALL roots are idle. LSP,
  one-shot, `--mcp`, and `--hook` route through the singleton with the workspace
  root as the RPC key. Tests set `XDG_STATE_HOME` to a sandbox, making the
  "disc2" class (a stray test daemon binding a developer's socket) structurally
  impossible. MIGRATION: an existing `<root>/.dl/db` is NOT imported — the first
  attach cold-starts a fresh per-root db under the home; a leftover per-root
  `daemon.sock`/`.pid` is inert. Plan:
  `plans/2026-07-10-singleton-daemon-registered-roots.md`.
- **`textDocument/references` rides the refs lens.** Results flatten
  declarations + uses; each hit's URI is built from its OWN repo's root
  (`repo_roots`), fixing the multi-repo bug where every location was joined
  against the primary root. Unknown slugs keep the old primary-root fallback.
- **Dead pre-cytoscape canvas renderer deleted (A7, -413 lines).** The DOM-card
  renderer, Sugiyama layout stack, pan/drag/marquee gesture system, and flip
  overlay were unreachable (canvas mode renders via cytoscape); the empty
  `nodes`/`edges` Maps they fed were WHY hover cards, pins, and centering were
  silently broken in canvas mode. Canvas interactivity is rebuilt on
  cytoscape's own API (A8): mouseover hover card, class-toggle pin/highlight
  styling, sym-equality `cy.animate` centering, tap-to-open.

- **`dl/query` paging (A3).** The custom LSP request grows optional `limit`
  (int), `offset` (int, default 0), and `count` (bool) params, backward
  compatible with the bare `{sql, params}` form the browser bridge uses.
  `count == true` -> `{"total": int}` (a `SELECT COUNT(*) FROM (<sql>)`);
  `limit` present -> `{"rows": [...], "total": int}` where the page runs
  `SELECT * FROM (<sql>) LIMIT ? OFFSET ?` with the two placeholders bound after
  the caller's own params; neither -> `{"rows": [...]}`, exactly as before. The
  user SQL is embedded verbatim (never parsed or rewritten), so a malformed
  query surfaces as the same `-32603` error it did.

### Changed
- **LSP `dl/query` no longer logs to `_query_log` (A4).** The panel auto-refresh
  polls this door and every read took two writes (`log_query` +
  `refresh_query_log`) on the single engine lock before the read — hot-path
  waste. The `query_log` relation now reflects daemon `query`/`query_sql` RPCs
  only (`src/daemon.rs` unchanged).


## [0.6.24] - 2026-07-10

### Added
- **`scip_occurrence` — SCIP occurrence spans (S1).** The importer already
  decoded every occurrence's range but dropped it at the rel layer (only
  `scip_ref`'s file-level rows survived). New
  `scip_occurrence(file, symbol, line, col, end_line, end_col, role, repo)`
  surfaces the 0-based line/col span and `role` (`definition`|`reference`, from
  the `symbol_roles` bitmask) for every occurrence, so a positional tag→symbol
  mapping is finally possible from SCIP alone. Pure addition —
  `scip_def`/`scip_ref`/`scip_edge` output is byte-identical.
- **`scip_binding` — local binding names (S2).** New
  `scip_binding(file, symbol, local_name, line, col, repo)` pairs each
  occurrence's LOCAL source text (sliced from WORK content at the occurrence
  range) with its canonical symbol. An aliased/default import
  (`import { foo as bar }`) now joins on the local name `bar` — a name-based
  join that `scip_name` (canonical-only) silently dropped. WORK-only (SCIP
  indexes reflect the on-disk tree); file content read once per file, never
  per-occurrence. Both rels are catalogued (group `scip`), reserved, and carried
  through the `scip_want` multi-repo merge with per-repo attribution.
- **JSON output: aggregation heads + json scalar fns.** dl stays flat in storage;
  nesting is OUTPUT, built by SQLite's own json functions (bundled SQLite 3.45).
  Two new head-position aggregates beside count/sum/min/max: `json_group_array(x)`
  (the group's values as a JSON array) and `json_group_object(k, v)` (the group's
  key/value pairs as a JSON object — the first two-arg aggregate; its value arg
  rides a new `Rule::agg_args2` vec parallel to the head terms). Three new scalar
  head/comparison fns: `json_object(k1, v1, ...)` (variadic, even arity >= 2),
  `json_array(x1, ...)` (variadic >= 1), and `json(x)` (validate/minify). Both
  aggregates emit `ORDER BY` inside the aggregate call so element order is a pure
  function of the group's rows — the rel's content digest is stable tick to tick
  (no spurious daemon rebuild). Composition is by stratification, no new value
  model: `item_json(order_id, json_group_array(item_name)) <- order_item(...)`
  then `order_json(json_object("id", order_id, "items", json(items))) <- orders(...),
  item_json(...)`. A json aggregate into an int column is a `brand-mismatch` diag;
  a json fn/agg in a SOURCE-rule head is refused as `json-in-source` (derived-only).
  `examples/json-out.dl` (corpus-free, over `rel_catalog`): one JSON object per
  builtin rel group with the rel names as an array, embedded across two strata.
- **Derived shapes: `type_decl_row(shape, pos, col, type)`.** A reserved builtin
  sink a DERIVED rule may head to compute a relation schema from data. The
  engine persists the sink's rows to a new `_shapes` meta table at the end of
  a tick (digest-guarded full replace); a `rel name: shape.` decl whose shape
  has no syntax `type name(...)` resolves its columns from those rows at the
  NEXT tick's declare — a one-tick phase delay (the @next carry precedent;
  invisible under the daemon). Until then the rel is pending: rules heading it
  are skipped for the tick and --check carries a `shape-pending` info diag.
  Syntax shapes win a name clash (`shape-shadowed` warn); a shape row naming
  an unknown type stays pending (`shape-unknown-type` warn, ty = base type or a
  declared brand). A shape's row set changing migrates the rels using it via
  the existing column-drift migration (re-derives next tick). A user
  `rel type_decl_row(...)` decl bails — head it directly, like diag.
  `examples/type-from-json.dl`: a schema inferred from a JSON sample (`int`
  when every observed value is an integer) becomes a live checked rel, plus
  the mapped-type trick — `type_decl_row("partial_" + rel, pos, col, ty) <-
  rel_col(rel, pos, col, ty, _)` mints a partial_* shape per built-in
  relation from live introspection.
- **Text `+` concatenation (S4).** `+` is overloaded by operand type: int + int
  stays SQL addition, text + text lowers to SQLite `||` (source rules
  concatenate in `val_of`), so `url = "https://" + host + "/v1"` builds the
  string instead of silently returning `0` (the old numeric coercion). Mixed
  int/text is a `plus-mismatch` typecheck error naming the fix (interpolate or
  `int(..)`); `- * / %` stay int-only and now reject text operands at
  typecheck. Works in head position and in body binds.
- **Body-bind hardening (S3).** The body computed-bind
  (`callee = replace(callee_q, ".", "::")`, already shipped) gains: a
  typecheck-time `unbound-bind` error naming the fix ("bind `callee_q` before
  computing `callee`") when a bind's RHS var is bound by nothing or only a
  LATER bind (was a lower-time abort); a bind var referenced in a NEGATION now
  joins the outer row (the Cmp pass runs before the Neg pass — previously the
  var silently became an unconstrained subquery local); the source-rule
  refusal note now names both escape hatches (head-inline for source rules,
  body binds for derived rules).
- **Builtin kind columns are enum-checked.** `type_edge.kind` /
  `type_edge_rev.kind`, `type_entity.kind` / `type_entity_rev.kind`,
  `df_node.kind` / `df_node_rev.kind`, and `checkout_done.action` /
  `checkout_plan.action` carry ambient enum brands (`type_edge_kind`,
  `type_entity_kind`, `df_node_kind`, `checkout_action`) whose variant sets
  mirror the extractor emit sites. A literal pin outside the set — the #1 agent
  failure mode, e.g. `type_edge(_, _, "fields", _)` — is a loud
  `enum-variant-unknown` error with a did-you-mean suggestion instead of silent
  zero rows. No user `type` decl needed; a user `type` reusing one of these
  names is a load error. Variables and joins are unchanged (an enum-branded var
  flowing into a plain text column is silent — the brand is a vocabulary gate on
  literals, not a path-like refinement). `doc_tag.tag` stays unbranded (open
  set: any `@word` passes through).
- **`rel_col` introspection rel.** `rel_col(rel, pos, col, ty, variants)` — one
  row per builtin relation column; `variants` is the JSON array of allowed
  values for an enum-vocabulary column ("" for open columns). Query
  `? rel_col("type_edge", pos, col, ty, variants).` instead of guessing a kind
  literal. Registered on the catalog RelKind beside rel_catalog.
- **Enum brands (typecheck-time).** `type severity = "error" | "warn" | "info" |
  "hint".` declares a brand whose value set is a closed list of text literals. A
  string literal filling an enum-branded column (rule head, fact, or query pin)
  that is not in the set is an `enum-variant-unknown` error, with a
  nearest-variant suggestion (Levenshtein). A sub-brand `type x <: severity`
  inherits the variant set. Storage stays text; the check is load-time only.
- **Named row shapes (typecheck-time).** `type finding(path: text, line: int,
  sev: severity).` declares a reusable column list; `rel finding_rel: finding.`
  declares a rel whose columns come from the shape. The shape reference expands
  into a plain `RelDecl` at load (frontend + typecheck seam) so the engine only
  ever sees ordinary rel decls. A shape column may reference a brand or enum
  brand. An unknown shape name is an `unknown-shape` load error naming the fix.
  No shape introspection rels, exhaustiveness, or json validation (later rungs).

## [0.6.23] - 2026-07-09

### Added
- **Raw strings for JS/HTML/regex gen.** `r"..."`, `` r`...` ``, and Rust-style
  `r#"..."#` (any number of `#`s) disable dl's `${NAME}` interp and `\` escape
  — every byte is literal until the matching close. A JS template literal like
  `` `hello ${name}` `` survives verbatim. Use `r#"..."#` when the body contains
  BOTH `"` and `` ` `` (plain `r"..."` closes at the first inner `"`).
- **`$${...}` literal-dollar escape in plain `"..."` strings.** Collapses one
  literal `${...}` that would otherwise interp. Scoped to `$${` so ast-grep
  variadic metavars (`$$$NAME`) are untouched. For whole-hog no-interp, use a
  raw string.
- **Mustache `{{` / `}}` escape in `gen` templates.** `{{x}}` renders as literal
  `{x}` (NOT a hole), so a gen template targeting a language with its own
  `{...}` syntax (JSON, CSS, Go text/template) no longer collides with dl's
  `{var}` hole form. Bare `{`/`}` not part of a `{ident}` run also pass through.
- **`gen(:zone, path, name, tmpl)` named-marker splice.** Replace the content
  STRICTLY between a `BEGIN: name` / `END:` marker pair, keeping the markers.
  Immune to surrounding-text edits (the trap with the line-number `gen(p, l0,
  l1, ...)` form). Comment-prefix-tolerant (`//`, `#`, `/*`, `;`, `<!--`).
  Unknown name bails loudly. Indentation inherited from the BEGIN marker line.
- **Embedded examples load via `use "std/<name>".`** Real std libs still win on
  name clash; examples now fill in the same `std/` library namespace, so
  `use "std/gh-checkout.dl"` resolves the embedded `examples/gh-checkout.dl`
  from a prebuilt binary with no source tree. The `std/` prefix is required
  for examples (bare names reject — the namespace is the discipline). Demo `?`
  queries are stripped on splice so an example loads as a library, not a demo;
  rules / `rel` / `gen` / `sh` / `def` splice in unchanged. (Std libs have no
  `?` items, so the strip is a no-op for them.)

### Changed
- **`checkout` sink is non-destructive.** It no longer stashes dirty work or
  `reset --hard`s. On the default branch + clean working tree it fast-forwards
  via `merge --ff-only origin/<branch>` (the only mutation). On the default
  branch + dirty, it SKIPs (the operator's work is left untouched, no stash
  created). On the default branch + diverged, it SKIPs (no force-update, no
  reset). On any other branch it moves only the ref pointer (`git branch -f`)
  and leaves HEAD + the working tree exactly as they are. Outcome action names
  are now `ff`/`branch-f`/`skip` (was `reset`/`branch-f`/`skip`).
- **Effectful sinks no longer fire on read paths.** `tick_report` is now pure:
  the `repo` pulls + `checkout` sweeps (the network/mutating sinks) moved to a
  separate `drain_external_sinks` phase. The daemon's poll loop calls it on
  cadence; `--watch` and `--settle` drain inline (they are the in-process daemon
  twins). A bare `dl prog.dl` one-shot is a READ by default — a `?` query never
  triggers a 90s destructive network sweep. Opt a one-shot in with `--apply`
  (or `DL_APPLY_SINKS=1`). `--check`, LSP, MCP, and `--parse-only` never drain.

### Added
- **`checkout` dry-run preview.** `DL_CHECKOUT_DRY_RUN=1` makes the sink compute
  each repo's planned action (ff/branch-f/skip-dirty/skip-diverged) WITHOUT
  running `merge --ff-only` or `git branch -f` — nothing in any checkout is
  mutated. The rows land in a new `checkout_plan` rel (same shape as
  `checkout_done`). Implies `--apply` so it works as a bare one-shot.

## [0.6.22] - 2026-07-09

### Added
- **Daemon live activity status.** `dl daemon status` (and the `ping` RPC) now
  report *what the current tick is doing and why* — which phase, which file/rel,
  how long — instead of just `tick_count`/`settled`. A new `doing` line reads,
  e.g., `doing extract call — src/engine/mod.rs (1.8s, tick 42)`, or `doing idle`
  when settled. The activity slot lives off the engine Mutex (swapped in
  microseconds at phase boundaries), so `status` never blocks mid-tick. Phases:
  cold-tick, declare, reconcile, extract, derived, operators, effects, query.
- **Tail-able perf log (`<root>/.dl/perf.jsonl`).** One JSONL record per phase
  transition + one per tick, so `tail -f <root>/.dl/perf.jsonl | jq` shows where
  each tick spends its time and what moved: `select(.type=="phase") |
  select(.ms>100)` for the slow step, `select(.type=="tick") |
  {tick,total_ms,strategy:.derived.strategy}` for the spike timeline. Default on;
  `DL_PERF_LOG=0` off, `DL_PERF_LOG=<path>` to redirect.
- **Hard perf-facts test matrix** (`tests/it/perf_facts.rs`): cold/warm/
  incremental/comment-only-digest-skip/source-rule-fallback/derived-edit, each
  asserting the exact set of rels re-derived (`last_derived_rebuilt`). The
  "reactivity is not atrocious" proof, pinned.
- **`dl update` is in `--help`** and now refreshes on-disk wiring after install:
  re-writes the embedded `SKILL.md` everywhere `dl setup` wrote it, and reinstalls
  the VSCode extension *only if already installed* (opt-in preserved). Project
  repos get a hint to re-run `dl setup --project`.

### Changed
- **Checkout (ghcacher) sink throttled — stops the all-core storm.** The
  `checkout(repo, branch, pr_heads)` sink used to `git fetch`+`reset --hard` every
  repo across every CPU core (rayon default pool) on every tick it had rows. Behind
  an `every(N)` clock that was a full-core pin every N seconds. Now: a dedicated
  narrow pool (`DL_CHECKOUT_WIDTH`, default **2** — fetch is network+disk bound, 2
  is as fast as full-core), and a per-repo min-fetch gate (`DL_CHECKOUT_MIN_SECS`,
  default **300** — a repo can't be re-fetched more often than this regardless of
  the rule). Throttled repos surface a `checkout_done` `skip` row. `DL_NO_FETCH=1`
  remains the hard kill switch.
- **Global rayon cap (`DL_RAYON_THREADS`)** for the extract/hash hot paths — the
  lever when the fans spin on a many-core box. Honored at startup; default
  unchanged.
- **Family reactivity gates (perf gap D).** The `module` and `spine` extract
  families used to return "changed" every tick they were `used`, forcing every
  dependent to re-derive on clock-driven full ticks even with zero file changes.
  `module` now self-gates on a real per-rev input digest (files **+ manifests**, so
  a Cargo.toml/package.json edit still moves it); `spine` is corpus-gated on
  `recon.changed` (its output is a pure function of file content). A warm no-change
  tick no longer re-derives their dependents — pinned by new tests. (The
  type/call/dataflow/doc families were already digest-self-skipping and depend on
  the scip index, so they are unchanged.)

## [0.6.21] - 2026-07-09

### Changed
- **`sprefa-dl` skill gained a "Common tasks" recipe section.** So an agent stops
  re-deriving the basics each session: a most-important-commands table, how to
  check for more (`dl docs`/`dl examples`/`rel_catalog`), how to config a repo set
  (`[[repos]]`/`[[org]]` + `SPREFA_CONFIG` precedence), how to load files (the
  `scan` forms, that a bare `scan` already triggers AST/SCIP extraction, `use`/
  `--load`), how to watch (`--lsp`/`--daemon`/`--watch`), and both ghcacher halves
  (`gh-cache.dl` + `gh-checkout.dl`). Plus a "Which extractor (NEVER default to
  `match`)" ladder — SCIP > `ast`/`sg` > built-in graph rels from a bare scan >
  `match` as last resort — and how to run SCIP without token-burn (`dl index
  --install` / `scip_want` / `$SPREFA_SCIP_INDEX` / `dl doctor`).

## [0.6.20] - 2026-07-08

### Added
- **`dl --restart` — the post-`cargo install` one-liner.** A long-running daemon
  keeps its OLD in-memory image after you reinstall `dl`; an auto-attaching
  one-shot detects the drift and replaces it, but a purely reactive daemon
  (nothing attaches, it just re-ticks on file changes) never self-heals and keeps
  running stale code (silently reverting generated files from its old catalog).
  `dl --restart` stops the daemon for this root and respawns it with the current
  binary — no `kill`/`nohup`/pid-file dance. Fire-and-forget: a slow cold first
  tick on a big repo reports "starting", not a failure.

### Changed
- **`dl --help` now leads with QUICK START, ROOT & DAEMON, and RULE BASICS.**
  Surfaces the two things that waste the most time — ROOT is the cwd (there is NO
  `--root` flag) and the daemon/`--no-daemon`/`--restart` lifecycle — plus that
  KWARGS (named head columns, `diag(path: p, line: l)`) exist. The installed
  `sprefa-dl` skill got the same treatment: a dedicated "Root, daemon, no-daemon"
  section, the reinstall trap + `--restart`/`--stop`/`--rows`, a kwargs bullet up
  top, and every fictional `--root` flag purged (it never existed).

## [0.6.19] - 2026-07-08

### Fixed
- **`gh-checkout.dl` swept ZERO repos against a real config.** The shipped example
  gated `repo(slug, root, url), url != ""`, but the `repo` relation fills its `url`
  column only for repos configured WITH a clone url — an already-cloned staging
  repo (slug + root, no url) has an empty url, so the ghcacher deployment the
  example is FOR matched nothing and produced no rows. The gate is gone
  (`checkout(slug, "", "0") <- repo(slug, root, url).`), with a header warning that
  a config-less run targets the self checkout.

### Added
- **`checkout_done(repo, branch, action, ok, detail)`** — the checkout sweep now
  writes a queryable outcome relation (action reset/branch-f/skip, ok 1/0). Under
  the daemon the `[checkout]` log goes to daemon.log (invisible to a query); this
  rel is how a program confirms the sweep fired and diags failures (`ok=0`).
- **`dl --rows <REL>`** prints a relation's current rows from the running daemon
  (its live engine state) — the `?`-query shortcut for inspecting a demand sink's
  output, e.g. `dl --rows checkout_done`. Backed by a new `query_rel` daemon RPC.
- **The `repo` sink accepts ground facts.** `repo("slug", "/root", "url").` (a
  body-less, all-literal head) now registers the repo directly; previously any
  literal head term was rejected (the body compiled to a SELECT), forcing an
  intermediary rel. A ground fact is treated as explicit: it bypasses the
  github-org allowlist (which gates dynamic pulls) and registers a present root
  without cloning.

## [0.6.18] - 2026-07-08

### Added
- **`checkout` demand sink: the git keep-current sweep (ghcacher's second half).**
  The dl port of ghcacher only did one of its two jobs — caching the GitHub API
  into SQLite (`examples/gh-cache.dl`). The other job, keeping local git
  *checkouts* current on disk (ghcacher's `checkout.rs`), was missing. It now
  exists as a built-in demand sink: a rule heads
  `checkout(repo, branch, pr_heads)` and each row, in parallel, clones a missing
  config repo, fetches origin, and fast-forwards `branch` to `origin/<branch>` —
  hard-resetting (after stashing dirty work) when that IS the current branch, or
  `git branch -f`-ing the ref without touching the working tree when it is not.
  `branch` empty discovers `origin/HEAD`; `pr_heads` `"1"`/`"true"` also mirrors
  `+refs/pull/*/head` into `refs/remotes/pr/*`. `DL_NO_FETCH=1` skips the network
  (re-points to already-fetched refs only). It reads its own config: head it off
  the `repo` builtin to keep every configured repo current from one rule. A
  `dl --lsp` daemon on an interval IS the watch loop. Catalogued as the `demand`
  group (reserved like `scip_want`/`repo`, so `rel checkout(...)` bails). New
  `examples/gh-checkout.dl`; `examples/gh-cache.dl` cross-links it. Tests:
  `tests/it/checkout_sweep.rs` (hard-reset on-branch, `branch -f` off-branch,
  offline no-op).

## [0.6.17] - 2026-07-08

### Fixed
- **Diagnostic hover actually renders its markdown now.** The server joins a
  diagnostic as `msg\nhint: …` with a SINGLE newline, which markdown collapses
  onto one line — so `code`, links, and lists a `.dl` wrote into the diag read as
  plain text in the squiggle hover. The extension's hover provider now promotes
  the hint to its own paragraph and turns lone newlines into markdown hard
  breaks, so the formatting shows. (VS Code still stacks its own plain copy of
  the message above the rich block — `Diagnostic.message` is plain-text by LSP
  protocol and can't be suppressed; markdown lives in the hover section.)

## [0.6.16] - 2026-07-08

### Changed
- **Rev resolution stops re-spawning git for revs it already has.** `resolve_rev`
  cleared its whole cache every tick, so each scanned `repo`/`rev` re-ran
  `git rev-parse` (plus a `cat-file` existence probe) on every tick — re-resolving
  immutable data. Now: an immutable hex-SHA rev is cached ACROSS ticks
  (`rev_sha_cache`, resolved once for the daemon's lifetime), while a movable ref
  (branch/tag/HEAD name) keeps the per-tick cache so it re-resolves as the ref
  advances. The `cat-file -e` probe (needed only because a hex SHA echoes back
  from `rev-parse` without proving its object exists) is skipped for names, whose
  `rev-parse` success already implies presence — 2 spawns down to 1. Both caches
  are checked before any spawn.

## [0.6.15] - 2026-07-08

### Changed
- **On-demand rev fetch now handles tag/branch names and shallow clones.** The
  0.6.14 single `git fetch origin <rev>` only wrote `FETCH_HEAD` (it lands a full
  SHA's object but creates no ref), so a tag or branch NAME never resolved on the
  `rev-parse` retry. `resolve_rev` now escalates, cheapest first, re-resolving
  after each and stopping at the first hit: `origin <rev>` (full-SHA fast path) →
  `origin tag <rev>` (creates `refs/tags/<rev>`) → `--tags origin` → and, for a
  shallow clone only, `--unshallow --tags origin` (deepen history for an object
  below the shallow boundary). `DL_NO_FETCH=1` still bails offline before any
  fetch; present revs and `rev_cache` are unchanged.

## [0.6.14] - 2026-07-08

### Added
- **`norm(str)` string builtin** — normalize for comparison (keep ASCII
  alphanumerics, lowercase, drop the rest; the same fold as the
  `string(id,text,norm)` rel's `norm` column). `nx = norm(a), nx = norm(b)` is a
  punctuation/case-blind compare, and arbitrary text joins against `string.norm`.
- **VS Code extension activates on startup** (`onStartupFinished`), so the LSP
  and the daemon it attaches to serve a workspace that has no `.dl` file yet
  (pure source repos, background indexing from the moment the window opens).
  Empty windows still no-op via the existing no-folders guard.

### Changed
- **`resolve_rev` fetches a missing rev on demand instead of bailing.** A scanned
  `repo`/`rev` whose object is absent locally — an unknown ref/name, or a pinned
  full SHA not in the object db — now triggers `git fetch origin <rev>` and one
  re-resolve rather than failing the tick. Offline mode (`DL_NO_FETCH=1`) skips
  the network and throws instead. Present revs are untouched: the sha returned
  and the `rev_cache` behavior are identical; only the miss path is new.

## [0.6.13] - 2026-07-07

### Changed
- **Flow panel canvas view now renders on a real `<canvas>` via cytoscape.** The
  hand-rolled renderer built one DOM card per node plus one SVG per edge and ran
  its own layered layout, which hung or crashed the webview at high volume
  (custom queries and the graph-layer UNIONs out-run the presets' `LIMIT`s, and
  the multi-repo db is much larger). Cytoscape (vendored into `media/`, loaded
  before the panel script — CSP blocks CDNs) owns layout, pan, zoom, and edges on
  canvas; the query rows map straight to cy elements. Two volume guards added
  regardless of view: the `dl/query` postMessage caps at 20000 rows, and
  `render()` slices to 2000 nodes / 4000 edges with a warn pill showing the true
  total. Canvas mode drops the per-node DOM affordances that lived on the old
  cards (member pins, hover cards, mark highlight, follow-cursor centering,
  marquee select, flip arrows); **list view keeps all of them**.

## [0.6.12] - 2026-07-07

### Added
- **Multi-root workspaces: one daemon, one database, every open folder.** The VS
  Code extension writes the open workspace folders as `[[repos]]` into a
  per-workspace `$SPREFA_CONFIG`, so a single dl engine (and its single
  `cache.db`) serves them all instead of only `folders[0]`. The flow panel spans
  every repo (nodes prefixed by folder); jump-to-disk, follow-cursor, marks, and
  type-seed resolve to the folder that OWNS each file. A git-excluded
  `_workspace-scan.dl` dropped into the primary folder's `.dl/` fans extraction
  over all repos (`scan("*")`) beside the existing rails, and both the LSP and
  the daemon it attaches to discover the same set — one shared database, no
  clobber. The LSP engine now loads the same repo set as the daemon
  (`set_repos`), and `load_repos_eager` is reused for that. Single-folder
  workspaces (empty config) are byte-for-byte unchanged. (Connected cross-repo
  *flow edges* — a call in repo A resolving into repo B — remain a follow-up: the
  resolver deliberately repo-scopes today.)

## [0.6.11] - 2026-07-07

### Changed

- The VS Code extension and the `dl` binary now ship as ONE version. Cargo.toml
  is the single source of truth: `scripts/build-vsix.sh` stamps the extension's
  `package.json` to the crate version and rebuilds the VSIX at a FIXED filename
  (`editors/vscode-dl/dl-lsp.vsix`, no version in it), so `src/setup.rs`'s
  `include_bytes!` never changes per release. `build.rs` refuses to compile if
  the two versions drift, and `.dl/vsix-version-drift.dl` is the same guard for
  `dl --check` / the LSP. No more hand-bumping `package.json` + re-embedding.
- Flow panel empty-state now tells you what to do instead of going blank:
  seed-driven presets say "press `cmd+alt+t` / follow cursor", scan presets name
  the SCIP index (`dl index`), and derived presets name the missing rel. Dropped
  the stale `dl --daemon --root .` hint (`--root` was removed).

## [0.6.10] - 2026-07-07

Consolidated release: builds the prebuilt binaries for all the 0.6.6–0.6.9 work
(diag markdown hover + graph sinks + panel fixes, the `--root` removal, the
`--load`/zero-match/config-warn trio, and use-yell) plus the CI dogfood fix.
This is the version `dl update` fetches.

## [0.6.9] - 2026-07-07

### Fixed
- **An unresolvable `use` no longer crashes the LSP server (or aborts the
  load).** A `use "missing.dl"` that resolves on no disk root and has no
  embedded-std fallback now emits a `use-unresolved` **diagnostic** at the
  `use` line and skips that import, so the rest of the program still loads —
  the LSP stays up and squiggles the bad line instead of dying, and `--check`
  reports it (exit non-zero) with the roots it tried. Downstream unknown-rel
  diagnostics from the missing import fire too, which is intended and more
  informative than a single opaque bail.

## [0.6.8] - 2026-07-07

### Fixed
- **A bad `dl --load` no longer wedges the daemon.** A watched load that fails
  to reload (parse/type error) rolls the file back out of the program set, so
  the daemon keeps ticking on its last-good program and a subsequent good load
  still succeeds. A deleted watched program file is skipped on reload (parse the
  files that still exist) instead of failing every tick.
- **Zero-match scan warning is quieter for expected-empty shapes.** A polyglot
  rel headed by several scans (one per language) no longer warns about the empty
  globs when a sibling glob matched (`seen` scanning both Rust and `{ts,tsx}` in
  a Rust-only repo went silent); a scan whose rel feeds a downstream rule
  (consumed) gets a one-line note instead of the loud fix-it text. Only a
  genuinely dead scan (unmatched, no sibling, unread) still gets the full
  warning — now worded for the cwd root (there is no `--root`).

### Added
- **Unknown config keys warn instead of silently vanishing.** A typo'd or
  renamed key in `config.toml` (`folder` vs `foldername`, a misspelled
  `[[repos]]`/`[[org]]` field) now prints a `[config] unknown key` line naming
  the table and key, rather than deserializing to the default and being ignored.

## [0.6.7] - 2026-07-07

### Changed
- **`--root` is gone. `dl` is a daemon over a repo SET, not a rooted tool.**
  The CLI no longer has a `--root` flag. The working root is the current
  directory: a client (the vscode extension, a test harness, a shell) points
  `dl` at a folder by spawning it with that `cwd`.
  - `dl --daemon` / `--stop` / `--load` / `--settle` address the **rootless
    singleton** at the XDG state home, which serves the config repo set (static
    `[[repos]]` / `[[org]]` allowlist) plus dynamic runtime adds (the `repo`
    sink). No privileged self repo — `df_node`/`type`/`call`/`doc` lift every
    repo in view, not just one root.
  - One-shots (`dl prog.dl`, `--check`, `--lsp`, `--move`) resolve the root
    from cwd; the program path itself may live anywhere.
  - The per-repo auto-attach daemon a one-shot spawns learns its root from the
    internal `DL_DAEMON_ROOT` env (+ cwd), never a user-facing flag.
  - The VS Code extension spawns the LSP server with `cwd` = the workspace
    folder instead of passing `--root`.

## [0.6.6] - 2026-07-07

### Added
- **`graph_node` / `graph_edge` builtin sinks** (group `graph`): head-writable
  builtin rels (mirror the `diag`/`repo` pattern) for drawing a graph in the
  flow panel without bespoke per-preset SQL. `graph_node(id, label, kind, file,
  line, parent)` and `graph_edge(src, dst, kind)`; the tables always exist
  (empty until a rule heads them), so the panel's "Graph (node/edge sink)"
  preset is always available. `.dl/git-graph.dl` migrated to the named-head
  form; `examples/madge.dl`'s 1-ary module node renamed `graph_node` ->
  `mod_node` to free the reserved name.
- **diag messages render markdown on hover**: the VS Code extension registers a
  HoverProvider that re-renders any dl-sourced diagnostic overlapping the cursor
  as a MarkdownString (the LSP `Diagnostic.message` field is plain-text-only).
  `.dl` programs can now write markdown (links, `code`, lists) into diag's
  msg/hint and it renders in the editor hover.

### Fixed
- **Flow panel no longer shows a red wall of text** when a preset's `rel_*`
  tables are missing: `run()` renders an empty graph plus a one-line grey note
  for any `no such table: rel_X`, `updatePresetAvailability()` disables presets
  whose tables are absent (labelled " - needs .dl"), and an empty-state overlay
  names the daemon/data cause. The `madge` preset synthesizes nodes from builtin
  `rel_module_edge` endpoints so it works on a bare scan.
- **VS Code extension is no longer blank under a GUI-launched editor**: probe
  `~/.cargo/bin` (and Homebrew paths) for the `dl` binary, augment the server's
  spawn PATH, and surface an actionable error with an "Open Settings" action
  when the binary is missing (GUI editors don't inherit the shell PATH).

## [0.6.5] - 2026-07-06

### Fixed
- **daemon.log no longer floods**. Three per-tick spam sources were writing to
  the log on every reactive wake-up (observed 3.9 MB / 118k lines):
  (1) the tick re-rendered every `?` query table to stdout on each tick — now
  suppressed on quiet (reactive) ticks; the RPC `query` capture is the daemon's
  read path (foreground `dl prog.dl` / `--watch` still print). The daemon's
  reactive tick calls now pass `quiet=true`, so the `[tick]` telemetry line is
  suppressed too. (2) `load_repos_eager` printed `[config] N repo(s)
  registered` on every call, and it is called from `on_git_event` (every `.git`
  change) — silenced; the cold-serve path announces the count once. (3) the
  `[daemon] git change` line logged even for pure metadata churn (`0 refs
  advanced, 0 files`) — now only logs a real advance/diff. Plus a backstop:
  a respawn starts the log fresh once it exceeds 8 MB.

## [0.6.4] - 2026-07-06

### Added
- **Self-contained flow-panel presets**: "Call graph (all)" and "Data flow
  (all)" query only PREBAKED builtin rels (`rel_call_def` / `rel_call_name` /
  `rel_call_edge`, `rel_df_node` / `rel_df_edge`), so they render on a bare
  `scan` with no derived `.dl`. The older flow-oriented presets still depend on
  their program's derived `rel_*` tables.
- **"Follow cursor" toggle** in the flow panel: with it on, moving the editor
  caret highlights and centers the graph/list node at the cursor's file:line
  (falls back to the word under the caret). The extension posts a debounced
  selection event to the webview; the panel acts on it only when the box is
  checked. Purely visual — no pin, no re-query. vsix bumped to 0.4.7.

## [0.6.3] - 2026-07-06

### Added
- **JSX/TSX prop-value dataflow** now chases the expression shapes that
  previously dead-ended at an unlinked node, so `df_field` prop values flow in
  both diet-SCIP and SCIP modes: conditional (`ok ? a : b` — both branches),
  logical (`a && b` flows the value side, `a || b` / `a ?? b` flow both;
  the `&&` guard is excluded), parenthesized, template + tagged-template
  interpolations, optional chaining (`obj?.title`), arrays (`[a, b, ...rest]`),
  sequence, assignment value, and the transparent TS casts (`as` / `satisfies`
  / `<T>x` / `x!` / `f<T>` / `await`). `UnaryExpression` is a deliberate
  non-flow (a `!x` value is a fresh boolean). The call/member arms were
  factored into `ts_flow_call` / `ts_flow_member` so optional-chained calls and
  members reuse the exact positional-`df_arg` and member-name logic.
- **Flow-panel saved queries**: name any node/edge SQL and store it locally;
  saved queries appear under a "Saved" group in the preset dropdown and render
  in both the graph and list views (same node/edge path as the built-in
  presets). vsix bumped to 0.4.6.

### Fixed
- **`.dl` discovery walks the ancestry**: a subdir with no `.dl/` inherits the
  nearest ancestor's chain, and every `.dl/` up to (and not past) the git root
  merges into one program. Unblocks `--lsp` when the editor opens a nested
  folder whose parent holds the `.dl/`. No new flag.

## [0.6.2] - 2026-07-06

### Added
- **`[[org]] foldername`** overrides the slug prefix (default: the `dir`
  basename). Set `foldername = "."` to FLATTEN — drop the org prefix so a repo
  addresses by its bare path under `dir` (`~/projects/my-long-ass-org-name/repo-a`
  → slug `repo-a`, not `my-long-ass-org-name/repo-a`). Only the slug flattens;
  the on-disk path is unchanged. Flattening can collide same-named repos across
  subfolders — the caller's call.

## [0.6.1] - 2026-07-06

### Changed
- **Magic-rel pattern eliminated: demand/overlay conventions are now first-class
  builtin sinks.** `scip_want`, `rev_cmp_want`, `def_target`, and `effect_cmd`
  used to be relations the engine read back by a hardcoded string name with
  nothing in `rel_catalog` advertising them — an invisible API. They are now
  pre-declared, catalogued builtin **sinks** (group `demand`), head-written from
  a rule exactly like `diag`/`repo`. Head them directly; a `rel scip_want(...)`
  declaration now bails ("head it directly, like diag/repo"). They appear in
  `dl docs relations` and `docs/reference/magic-rels.md`.

### Added
- **Magic-rel ban rail (`.dl/magic-rel-audit.dl`).** Dogfood check: scans the
  engine's own `src/**/*.rs` for any `rels.get("<name>")` / `FROM rel_<name>`
  literal and fails `dl --check` (exit 2) if the name is not a catalogued
  relation. Runs in CI's bare `dl --check` and the PostToolUse hook, so the set
  of name-matched relations can only shrink or become catalogued, never silently
  grow. Regression test `tests/it/magic_rel_audit.rs`; maintainer skill
  `assets/sprefa-v5-no-magic-rels.skill.md`.

## [0.6.0] - 2026-07-06

### Added
- **`dl --settle` — run a program to a fixpoint.** A plain one-shot ticks once,
  which leaves effectful (`@async`/`sh`/`sh*`), demand-tier (`scip_want`), and
  `repo`-sink programs half-run — their requests stuck queued, their demanded
  rows absent. `--settle` drives tick + off-tick effect drain in-process until
  the program is quiescent (no non-timer rel moved, no `@next` carry staged, no
  non-stream effect in-flight), then prints `?` once. It is the first non-daemon
  path that runs the effect runtime. `--settle-max N` (default 200) bounds it; a
  non-converging program bails loudly naming the still-moving rels/effects
  instead of hanging. Recurring timers (`every`/`clock`/`@stream`) are steady
  state and excluded, so a poller still settles at a quiet point.
- **`dl --await-settle` + `await_quiescent` RPC.** The daemon-side twin: block on
  a running daemon until its poll loop reaches the same quiescent state (exit 0
  settled / 3 timed out). `ping` gains a `settled` field.
- **`[[org]] dir=` multi-root config.** Point at a folder of checkouts and every
  git repo under it expands, at load, into a `[[repos]]` entry (slug
  `<dir-basename>/<path-under-dir>`, descent stops at each `.git`, an explicit
  `[[repos]]` at the same root wins). `max_depth` (default 3) caps the walk; a
  leading `~` in `dir` expands to `$HOME`. The declarative multi-root shape,
  usable from one-shot / `--check` with no daemon — the single way to point `dl`
  at an org-of-repos folder.

### Changed
- Documented the effect/settle model in `docs/daemon.md` (a "Running effectful
  programs to completion" section) and `book/tutorial/12`.

### Fixed
- Two stale examples (`propose_demo`, `kernel_compare`) called the 1-arg
  `scip_import::load`; updated to the 3-arg `(path, root, slug)` form.

## [0.5.0] - 2026-07-06

### Added
- **Learner GitBook** (`book/`, `.gitbook.yaml`). A `quickstart` (install → query
  → CI gate against your own repo), a hands-on `tutorial` track (setup through a
  server-made-of-rules, one lesson at a time, every transcript a real capture),
  the existing theory + math tracks, and `what-if` essays (rendering HTML trees
  from relations, the exits from stratification, bridging to v0's nested-block
  DSL). Every list — `SUMMARY.md` zones, both track READMEs, `dl docs` indexes —
  is spliced from one scan of `book/` by `gen-doc-indexes.dl`, with `--check`
  drift rails.
- **Turnkey VSCode extension install.** `dl setup --vscode` now builds a fresh
  VSIX from `editors/vscode-dl` when run in a checkout (always current), falling
  back to the VSIX embedded at build time for a prebuilt `dl`. It installs
  uninstall-first to dodge the same-version reinstall no-op. A new `.dl/`
  drift rail (`vsix-version-drift`) fails `dl --check` if the embedded VSIX
  version and `editors/vscode-dl/package.json` disagree — the coupling that
  silently rotted the embedded VSIX to 0.3.0.
- **VSCode "Add Type Seed"** command + keybinding (extension 0.4.4).

### Changed
- **File-watcher scaling.** The daemon watcher now mirrors the scan corpus: a
  shared `WatchGate` drops gitignored build output and `.git/objects` churn the
  engine would never scan, watching `.git` only at the narrow
  `HEAD`/`packed-refs`/`refs/` ref paths. Bursts coalesce through a quiet-period
  debounce (was a fixed 150 ms drain); a dropped/overflowed event forces a loud
  full-corpus recovery tick; and the idle timer resets only on events that
  survive the gate, so a repo under pure build churn can finally idle out.

### Fixed
- **Deep-root daemon could not bind.** `<root>/.dl/daemon.sock` for a deeply
  nested root overran the macOS `sun_path` cap (104 bytes), so `bind` failed and
  every invocation fell back to in-process after the attach timeout. The socket
  now relocates to a short hashed path under `$TMPDIR/dl-sock/` when the natural
  path is too long; bind and every connect derive it from the same root.

## [0.4.4] - 2026-07-05

### Fixed
- **Daemon self-write tick loop.** The watcher watches the scan root
  recursively, but the daemon writes its own bookkeeping there every tick
  (`.dl/cache.db*` sqlite WAL, `.dl/daemon.log` stderr redirect). Those writes
  re-fired the watcher and re-ticked forever — a no-op "files 0/0 parsed" loop
  that also kept resetting the idle timer, so the daemon never idled out (seen
  as a daemon pinned at high CPU doing nothing). The watcher now drops its own
  bookkeeping paths (`is_daemon_internal`) from each batch; a batch that is
  entirely self-writes is skipped before it can tick or reset idle. Program
  files (`.dl`, `marks.dl`) are unaffected and still trigger reloads.
- **Rebuilt/reinstalled `dl` attached to a stale daemon.** `ensure_daemon` only
  checked that a socket answered `ping`, so a freshly built binary attached to
  the old daemon and the new code never ran. The daemon now reports a
  `build_id` (crate version + exe mtime) captured at startup; the client
  respawns on mismatch, attaches on match, and leaves a pre-`build_id` daemon
  alone.

## [0.4.3] - 2026-07-05

### Fixed
- **VS Code extension: LSP client failed to start with `command
  'dl.toggleDiagCode' already exists`.** v0.4.2 registered the diag-mute
  quick-pick under the same id the server advertises in
  `executeCommandProvider`, so vscode-languageclient's auto-registration
  collided, initialize failed, and every flow-panel query returned "Client is
  not running". The palette command is now `dl.pickDiagCode`; the server-side
  `dl.toggleDiagCode` / `dl.listDiagCodes` executeCommand ids are unchanged.
  Extension 0.4.3 (`dl-lsp-0.4.3.vsix`); no engine changes.

## [0.4.2] - 2026-07-04

### Added
- **Rev-aware extraction (the diff spine).** Per-rev `extract:<family>:<rev>`
  digests; `type_entity_rev`/`type_link_rev`/`call_def_rev` twins (rev is a
  column, syms stable cross-rev) and df twins with rev-salted ids; a vanished
  rev retracts from all twins the same tick. `diff_pair(base_rev, head_rev)`
  drives `.dl/graph-diff.dl` on ONE checkout (shipped inert as
  `("WORK","WORK")`); `examples/pr-diff.dl` diffs a PR via `gh` -> shas -> scan
  rev slots.
- **`hook_event` seam + chat-marks.** Built-in
  `hook_event(kind, session, seq, json)` fed by `dl --hook` (daemon RPC with
  in-process fallback); `dl setup` registers UserPromptSubmit + PostToolUse.
  `examples/chat-marks.dl` sections chat logs on an `@@mark <title>` phrase —
  the phrase lives in the .dl program, never the engine.
- **CLI discovery + learning surfaces.** Grouped `--help` with SUBCOMMANDS /
  LEARN MORE / AUTHORING trailers; `dl docs` embeds the reference, the book
  (now 9 chapters incl. argmax), a hands-on 9-lesson tutorial, and the
  authoring skill; doc indexes are generated by `examples/gen-doc-indexes.dl`
  with a drift rail.
- **Authoring sharp edges closed.** `--parse-only` no-scan validate (parse +
  typecheck + metavar sanity + every regex literal compiled — lookahead
  fast-fails sub-second); `lowercase-metavar` warn lint; head-var-not-bound /
  unbound-constraint / regex errors now name the fix; per-op language matrix
  kept honest by `tests/it/lang_matrix.rs`.
- **23 `sg` grammars + term-form `sg`.** css, html, bash, csharp, java, scala,
  swift, ruby, php, lua, elixir, haskell, yaml join the table;
  `sg(:lang, bound_str, "pattern")` matches over a bound string for
  embedded-language rules (styled-components, markdown fences).
- **`comment_node` + `std/suppress.dl`.** Grammar-backed comment relation
  (line/block/doc, inline included, string-literal-safe); the
  eslint/biome-style disable grammar (`dl-disable-line`/`-next-line`/block
  pairs, code scoping, `-- reason`) written entirely in dl, with directive
  visibility diags (`dl-directive` info dots, malformed + unused warns).
- **`diag_mute` + editor toggle.** Writable `diag_mute(code)` builtin; LSP
  executeCommand `dl.toggleDiagCode`/`dl.listDiagCodes`; the filter sits at
  the publish seam only, so `--check`/`--parse-only` are unaffected. VS Code
  extension 0.4.2 ships the quick-pick command + `cmd+alt+d cmd+alt+d` chord.
- `examples/endpoint-flows.dl` (axum route -> call-graph reach -> hover shows
  "in endpoint flows: GET /users") and a presenterm slide deck under `deck/`.

### Fixed
- **Scope-correctness sweep.** Resolver double-registration made a whole repo
  resolve bare; SCIP importer collapsed multiple indexes across roots
  (`scip_def`/`scip_ref`/`scip_edge` gain a `repo` column); the dataflow
  family read config-repo files at the wrong root; cross-file `impl` parents
  now resolve to the declaring file; lattice `key(...)`/`merge(...)` edits
  with identical columns no longer wedge every tick on a stale primary key.
- **NULL-padded heads in recursive rules now refuse instead of hanging.** A `_`
  head slot (explicit, or the named-arg padding v0.4.0 introduced) lowers to SQL
  NULL, and NULL rows never dedup in the fixpoint delta (`NULL != NULL` under
  `INSERT OR IGNORE`) — a recursive rule like `n(a: y) <- n(y, _).` re-inserted
  the same row every iteration forever (measured: 2^24 rows, 422 MB, still
  climbing at kill). Two guards: typecheck emits `recursive-null-pad`
  (`--check`/LSP), and `rebuild_derived` bails before entering the fixpoint loop
  as the runtime defense. Non-recursive padded sinks (the `diag` shape) are
  untouched.

## [0.4.1] - 2026-07-02

### Fixed
- **Marking a selection no longer kills the daemon.** A discovery-mode daemon
  (serving `<root>/.dl/*.dl`) treated any content edit to an already-discovered
  program file as exit-for-respawn — but a discovery daemon has no positional
  args to respawn from, so the VS Code extension's mark command (one appended
  fact line in `.dl/marks.dl`) left it dead until the next `dl` client happened
  to run. A discovery daemon now hot-reloads the edit in place (re-parse, swap,
  re-tick) and keeps serving. Explicit-program daemons keep exit-for-respawn.
- **Flow panel "Module graph" preset now populates on any discovery daemon.**
  The preset read rels derived only by `examples/madge.dl` (`rel_dep`,
  `rel_cycle_member`), so it errored unless that example had run against the
  db — while silently reading `rel_seen` rows from flow-panel.dl's unrelated
  `seen` rel. It now reads `module_node`/`module_edge`, derived in
  `.dl/flow-panel.dl` from the engine's built-in module graph (cycle detection
  via a recursive reach rule). The panel's error banner also explains a
  `no such table: rel_*` failure as a .dl program the daemon hasn't loaded.

## [0.4.0] - 2026-07-02

### Changed
- **`diag` is now a fixed-schema built-in relation, not a magic name.** It was
  a user-declared rel whose columns the engine mapped BY NAME at read time —
  which meant every rail file carried its own `rel diag(...)` decl, and the
  merged `.dl/` discovery namespace collided the moment two files declared it
  with different columns. `diag` is now engine-declared with a fixed 9-col
  schema `(path, line, col, end_line, end_col, severity, code, msg, hint)`,
  reserved like every other built-in (a `rel diag(...)` decl is now an error
  pointing you at the sink form). `path` is TEXT so a synthetic origin
  (`"(engine)"`, `"(checked-notes)"`) is not file-checked away. No compatibility
  fallback — every example and test writes the built-in directly. **Migration:
  drop the `rel diag(...)` line and write only the columns you use** (see below).

### Added
- **Named args in rule heads.** `diag(path: p, line: l, msg: m) <- ...` names
  only the columns a rule writes; every unnamed column pads to `NULL` (the
  reader defaults it — severity `warn`, `end_line = line`, ints `0`). Works for
  any rel head, not just `diag`. A head can't mix named args with an aggregate
  call (the two shapes are incompatible).
- **Bare-name shorthand with no anchor.** A fully-positional atom whose terms
  are all Vars naming columns, and which has fewer terms than the rel has
  columns, resolves as all-puns — `diag(path, line, msg)` ==
  `diag(path: path, line: line, msg: msg)`, the JS `{a, b}` / Rust `Foo { c }`
  struct shorthand. It only fires when the atom would otherwise be an arity
  error, so a genuinely positional atom (term count == arity) is never
  reinterpreted and existing programs are untouched. `? diag(path, line, msg)`
  just works.
- **Positional literals mixed with named args (Python-style prefix).** In named
  mode, binding follows one rule: a term that carries a name binds by name (a
  bare var puns, interleavable in any order — unlike Python), a nameless literal
  fills the next column left open by the named + pun args, in declaration order.
  So `diag("synth.rs", 1, severity: "error")` puts `"synth.rs"` in `path` and `1`
  in `line` without spelling those column names. Over-filling (more literals than
  open columns) is a clear error. (Previously a bare literal alongside named args
  was rejected as ambiguous.)
- **`ast::Value::Null`** — the value model gains a null so a padded head column
  round-trips to SQL `NULL` through both the derived (SQL) and source-rule
  (Rust) head-projection paths.

## [0.3.0] - 2026-07-02

### Added
- **`std/flow.dl` — the shared value-flow base as a `use` module** — the
  lines every flow program copy-pasted (the `call_edge_bare` sym-space
  bridge, the `flow_edge` union of `df_edge` + the interprocedural hops,
  the `call_node` call-site name join) now live in one importable std lib;
  `examples/flow-interproc.dl`, `examples/taint.dl`, and
  `examples/flow-jsx.dl` are rebased on `use "std/flow.dl".` and keep only
  their own layers. New surfaces riding it:
  - **`flow_summary(callee, pos)` / `flow_sanitizer(callee)`** — user-asserted
    propagation MODELS for callees the lift can't see into (the
    CodeQL-models move as plain facts). The lift's default is maximal:
    every argument gets a blanket edge into the call result; a summary
    overrides that for its callee, keeping ONLY the summarized slots
    (`flow_sanitizer` = the zero-slot instance: nothing flows). Stratified
    cut via `flow_cut`; free when no facts are asserted.
  - **`call_target(call, caller, callee, callee_q)`** — per-CALL-SITE
    resolution: each call node tied to the defs carrying its own callee
    name (`call_node` ⨝ `call_edge_bare` ⨝ `call_name`). Both
    interprocedural hops now ride it, so `f(secret); g(benign)` in one
    caller no longer cross-talks (the old per-caller hop leaked every arg
    into every callee of the caller, and every callee's return into every
    call result). Factored as its own rel for the planner too: the inlined
    7-atom forward hop measured ~7s per tick on this repo, the factored
    shape ~0.5s for the whole graph.
  - **`arg_field_flow(value, field, call, target)`** — the JSX `prop_edge`
    pattern generalized to plain calls: a value stored into field F of a
    composite passed as an argument reaches the resolved callee's reads of
    the SAME field name (member reads and TS destructured-param pieces).
  - **`flow_lambda(callee, lam_pos, src_pos, param_pos)` /
    `flow_lambda_ret(callee, lam_pos)`** — higher-order propagation facts:
    how a callee invokes a lambda it receives (element hop + result hop).
    `std/flow-collections.dl` ships facts for the common combinator names
    (map/filter/forEach/fold/reduce/...), language-blind by name equality.
  Tests: `tests/it/flow_std.rs` (summary cut, sanitizer, field view,
  fact-driven collection hops per language, per-call-site cross-talk gates
  per language).
- **Inline lambdas lift as their own fn scopes** — Rust `|x| ..` closures,
  TS inline arrows / function expressions, and Kotlin `{ it + 1 }` /
  `{ x -> .. }` lambda literals (including trailing-lambda call syntax,
  which previously wasn't even an argument) now produce a lifted scope:
  kind `param` nodes with `df_param` slots (Kotlin's implicit `it` at slot
  0), the body walked under a synthetic `<enclosing>::closure::<pos>` sym,
  and a `ret` node fed by the body result. The `closure` VALUE node stays
  in the enclosing fn at the argument position and carries the lifted sym
  in `var` — the join key the flow_lambda hops ride. Rust/Kotlin share the
  enclosing scope so captures still resolve; `nest` still counts a call
  inside a closure inside a loop (loop-fn matching is `::closure::`
  prefix-aware). Tests: 3 typegraph units + the e2e collection gates.
- **`examples/flow-slice.dl` — the value slice of one local / instance** —
  forward ("what does `token` reach?"), backward ("what feeds it?"), and the
  field-accurate reads of a single instantiation, each a seeded recursive
  walk of `std/flow.dl`'s `flow_edge` (the closure-can't-be-read-unpinned
  idiom, cheaper by magic-set). A copy template for slicing on any repo:
  edit the seed's var literal, or seed one exact node id from a `? df_node`
  dump.
- **`examples/flow-services.dl` — the wire hop** — cross-SERVICE value flow
  where no call edge exists: a spec-seeded `service_op` inventory (every
  `operationId` in a scanned `openapi.yaml`; assert `service_op("x").`
  facts for runtime-only topologies), `op_endpoint` (every def carrying an
  operation's name), and two hops unioned into `flow_edge` — client
  argument -> endpoint param (positional) and endpoint return -> client
  call result. The stub and the handler usually SHARE the operation's
  name, so single-def resolution refuses exactly where the wire hop takes
  over. Tests: `tests/it/flow_services.rs` (end-to-end reach through the
  spec + the no-spec negative).

- **JSX dataflow** — `<Card title={t} {...rest}>{kids}</Card>` lifts as what
  it desugars to, `jsx(Card, {title: t, ...rest, children: kids})`: the
  element is a `new` df_node carrying the component/tag name, each
  attribute a `df_field` row (bare boolean prop = lit, spread under `".."`,
  non-text children under the `"children"` pseudo-prop). A component usage
  is also a call SITE (host elements skipped), so `call_edge` resolves
  caller -> Card and `call_name` gives an indexable name handle. TS
  object-destructured params (`function Card({title, count: n})`) now mint
  one param df_node per property — var carries the PROPERTY name (the
  JSX/name-match target), scope binds the LOCAL name, all pieces share the
  slot index (previously destructured params bound NOTHING — every React
  component body was a flow hole). New `examples/flow-jsx.dl`: `jsx_use`
  inventory + the `prop_edge` hop (prop value -> matching destructured
  param or `props.x` member read, `call_name` equality join, no suffix
  test). Tests: 2 typegraph units, `tests/it/flow_jsx.rs` (name-match
  positive + undeclared-prop negative + member-read shape).
- **Positional + constructor dataflow: `df_arg`, `df_field`, `new`/`member`
  nodes** — the intra-procedural lift now records WHICH slot each argument
  feeds (`df_arg(call, pos, arg)`, 0-based, method receiver at -1, aligned
  with `df_param.pos`/`type_sig.pos`) and named flow into composites
  (`df_field(id, field, value)`: Rust struct-literal fields, TS
  object-literal properties, Kotlin named arguments; `".."` for
  spread/functional-update bases). Instantiations are first-class `new`
  df_nodes carrying the constructed type name: Rust struct literals and
  capitalized tuple-struct/variant ctors, TS `new Foo()` and object
  literals, Kotlin capitalized ctor calls. Field reads become `member`
  nodes carrying the accessed name (Rust `Expr::Field` and Kotlin
  navigation previously fell into the `expr` catch-all with NO base edge —
  a real flow hole, now closed); method receivers flow into call results
  in all three languages. `examples/flow-interproc.dl` and
  `examples/taint.dl` upgrade the arg->param hop from positional-blind to
  positional (`df_arg.pos = df_param.pos`); new `examples/flow-ctor.dl`
  demos the instantiation inventory, per-field fills, and field-SENSITIVE
  flow (a value stored into field F reaches a member read of F, and only
  F, via a new-seeded recursive rule — closure rels can't be read unpinned
  in a rule body). `nest` now also counts `new` nodes (a ctor in a loop
  allocates per iteration). Tests: typegraph units per language,
  `tests/it/flow_ctor.rs`, and the position gate in
  `tests/it/flow_interproc.rs` (arg 0 must NOT reach param 1).
- **Per-family extraction skip + per-file fact cache (perf gap A)** — the
  type/call/dataflow/doc refreshers persist an `extract:<family>` input
  digest (corpus (repo, path, rev, content hash) rows + the `scip_ref`
  override + the running binary's identity) and skip the whole
  parse/resolve/write pass on a warm tick; when a file DOES move, an
  in-memory (repo, path, content hash)-keyed fact cache re-parses only it.
  Measured on this repo's flow-interproc program: type/call/dataflow refresh
  183/281/930ms -> ~0.3ms each on the no-change tick, which drops from
  ~1.5s to ~35ms in-engine. `Engine::extract_files_parsed` is the
  instrumentation; `tests/it/extract_cache.rs` pins both the skip and the
  single-file re-parse (including cross-process skip over a warm db).
- **Full-tick scoped rebuild (perf gap B)** — the full `tick` now attributes
  changes per relation (source-rel digests, family refresh results, RelKind
  returns, an `async:` content digest for @async/@stream response rels the
  off-tick drain writes) and rebuilds only the derived rels
  dependency-reachable from what moved — the same `affected_derived` walk
  `tick_paths` uses, now on the full path. A blank slate, a program edit, or
  a carried @next change still rebuilds everything.
  `Engine::last_derived_rebuilt` is the instrumentation;
  `tests/it/scoped_tick.rs` pins the two-chain isolation.
- **Family change reporting (perf gap C)** — `tick_paths` marks a family's
  rels changed only when its input digest actually moved, so an edited `.md`
  under a type-graph program (or an edited `.rs` under a doc-only program)
  no longer re-derives the other family's dependents.
- **`dl setup --project` wires repo-tracked skills** — every
  `assets/*.skill.md` in the target repo gets a gitignored
  `.claude/skills/<name>/SKILL.md` relative symlink (copy on non-unix), so a
  fresh clone of a repo following that convention (this one: the three
  maintainer checklists) exposes its project skills after one setup run.

- **`rel_count(rel, rows)` / `stmt_ms(rel, ms)` telemetry built-ins** — tick
  cardinalities and per-rel derived-statement wall costs as queryable facts
  (`--tick-audit` / `--profile` output, made joinable). Derived rels report
  the previous tick's counts (source-phase refresh); `stmt_ms` is empty until
  a rebuild has landed in the db. Closure-head VIEWS are excluded from the
  counts (counting one materializes the full closure).
- **`examples/perf-rails.dl`** — cardinality-blowup + slow-rule diags over the
  telemetry built-ins with budget facts; merge it beside the program under
  watch.
- **Multi-file one-shot merge** — `dl a.dl b.dl` now merges ALL positionals
  into one program for every one-shot mode (run/check/lsp/hook/mcp/verify/
  changed/watch), as the help text always claimed; previously everything
  after the first file was silently ignored. An explicit multi-file merge
  runs in-process (the daemon serves its own loaded set).
- **`closure-unpinned` lint in `dl_diag`** — a `?` query on a closure head
  with both endpoints free warns with the pin hint (the lint twin of the
  runtime guard below).

- **`git_ref(repo, refname, kind, sha)` built-in** — ref inventory across the
  self repo and every config repo: one row per branch/tag/remote ref plus a
  `("HEAD", "head")` row, annotated tags peeled to their commit.
- **`rev_behind(repo, refname, upstream, behind, ahead)` built-in,
  demand-driven** — derive an ordinary relation named
  `rev_cmp_want(repo, refname, upstream)` and each wanted pair fills with
  behind/ahead commit counts (`ahead > 0` = the ref diverged from upstream).
  One-tick latency, like a data-driven scan. Unresolvable refs skip loudly;
  a SHALLOW clone skips loudly per repo (grafted history makes ancestry
  counts wrong, not just incomplete — `git fetch --unshallow` fixes it).
- **`scip_want(repo)` — lazy multi-repo SCIP.** Derive `scip_want` rows and
  each wanted repo's index is ensured (an existing `index.scip` wins;
  otherwise detected+installed indexers run once to `.dl/index.scip`), then
  the self index and all wanted indexes merge into ONE load — so a
  cross-repo reference resolves its `def_file`. No schema change; monikers
  self-disambiguate.
- **`examples/pin-skew.dl`** — which repos pin an internal dep at a ref the
  dep's main line moved past (stale) or never contained (diverged)? go.mod
  manifest seam -> `pin` -> `rev_cmp_want` -> `stale_pin`/`diverged_pin`;
  bespoke lockfile formats union into `pin` with one rule per format.
- **Seven cross-repo / dataflow recipe examples** on the existing built-ins,
  each with honest-limit headers and validated end to end:
  - `taint.dl` — source/sink/sanitizer preset over the interprocedural flow
    graph; taint propagates recursively, stops at sanitized nodes, reports
    sink hits as `diag`.
  - `route-norm.dl` — client request paths vs declared server routes across
    template dialects (`{id}`/`:id`/`%s`), joined on the punctuation-stripped
    lowercase normal form; `route_hit`/`route_orphan`/`route_dead`.
  - `stale-doc.dl` — a documented declaration whose decl line is in the
    working diff (a pre-commit "confirm the doc is still true" rail).
  - `arch-conformance.dl` — declared layers (path prefixes) + allowed
    dependency arrows vs the real `module_edge` graph; every cross-layer edge
    without an arrow is a `violation`.
  - `version-skew.dl` — one dependency pinned at differing versions across the
    org (min/max witnesses per module, blast-radius by repo count).
  - `phantom-deps.dl` — Go imports covered by no `require` line in any of the
    repo's go.mod files (the transitively-available import that breaks the day
    its provider drops it).
  - `vendored-drift.dl` — a `third_party/` copy vs its upstream config repo by
    content address: `in_sync` / `drift` / `local_only`.

### Changed
- **df_node lines are 1-based in ALL three lifts** — the Kotlin dataflow
  lift normalizes tree-sitter's 0-based rows (+1, loop spans bumped in
  step so `nest` containment is unchanged), and the Rust method-call
  `call_res` node now sits at the METHOD ident's line (where the call-site
  extractor records it) instead of the receiver expression's start, so a
  multiline builder chain still joins. `call_node` (std/flow.dl) is
  therefore ONE equality join; the old dual-offset form (`cl = dl + 1` for
  the 0-based languages) is gone, and with it the false match against a
  call site on the line after an unrelated call.
- **taint.dl findings tighten under the per-call-site pin** — on this repo
  the demo preset drops from 161 findings to 9; the removed rows were the
  per-caller cross-talk (any tainted value in any fn that also calls a
  sink), not real flows.
- **`RelDecl` carries `group`/`doc`** — the parallel `builtin_rel_docs()`
  tuple registry is gone; every built-in relation's one-line doc and group
  live on its declaration, so the schema and the doc cannot drift.
  `rel_catalog`, the generated README table, and the `undocumented_builtins`
  CI guard all read the decls; rendered output is byte-identical.
- **Non-recursive derived rules evaluate in ONE pass.** `rebuild_derived` now
  splits each stratum into rel-level dependency components (Tarjan,
  dependencies first); only genuinely recursive components iterate to a
  fixpoint. Previously every statement re-ran until a whole-stratum delta hit
  zero, so every expensive non-recursive rule paid its cost twice (measured:
  a 40s join statement executed 2x per tick).
- **Unpinned closure queries are guarded.** A `?` on a closure head that
  falls through to the SQL reachability view is refused loudly when the edge
  rel exceeds `DL_CLOSURE_QUERY_MAX_EDGES` (default 20k, `0` disables) — a
  LIMIT cannot short-circuit the view, so on a dense graph it is effectively
  unbounded (measured minutes of CPU on a 471k-edge flow graph). Both-pinned
  closure queries now answer as an existence probe via the seeded
  condensation walk.
- **`flow-interproc.dl` / `taint.dl` sym bridge is an equality join.** The
  per-pair `replace(qual, bare, "") != qual` suffix test (unindexable; ~25M
  string evals, 40s per fixpoint pass on this repo) is replaced by
  `call_edge_bare`, which strips the repo qualifier once per `call_edge` row;
  the interproc hops join on it by equality. Cold derived phase on this
  repo: 130s+ -> 1.4s.

### Fixed
- **`Engine::rel_rows` no longer drops rows containing non-text columns.**
  Reading an INTEGER column as String is a per-row rusqlite type error that
  silently filtered the whole row from diagnostic reads; values now
  stringify from their stored type.

## [0.2.1] - 2026-07-01

### Fixed
- **`@async`/`@stream` effects now drain by default under `dl --daemon`.** The
  effect drain runs inside the daemon poll loop, which was opt-in behind
  `DL_POLL_SECS` — so a program with effects sat at `state='queued'` forever under
  a bare `dl --daemon` with no indication why. The daemon now polls at
  `DEFAULT_POLL_SECS=2` by default; the loop no-ops cheaply when the loaded program
  has no effect rules, so an effect-free daemon is unaffected. `DL_POLL_SECS=N`
  overrides the cadence, `DL_POLL_SECS=0` disables the drain entirely.
- **Actionable diagnostic for the multi-repo scan fan.** A source rule whose head
  var isn't produced by its source op (the common mistake: `scan("*", …), file(r,
  …)` trying to recover the repo by a join) reported a bare `head var r unbound in
  source rule`. It now explains that a source rule binds head vars only from its
  source op, and shows the fix — put the var in scan's repo slot: `repo(r, _, _),
  scan(r, rev, glob, path, rev_out)`.

### Added
- **`examples/npm-crawl.dl` + `examples/crawl` — progressive dependency-graph crawl
  of any public npm package.** Name one package; the `@stream` effect runtime
  crawls its dependency graph straight from the npm registry (one `curl` per
  package, content-addressed so each is fetched once), expands the frontier one BFS
  layer per tick, rewrites a d2 graph progressively as edges land, and optionally
  shallow-pulls each dep's source repo at its rev (`git clone --depth 1` — source
  only, no `npm install`, no build). The `crawl` driver owns the whole
  daemon+load+render lifecycle as one command; fan-in hubs fall out of the same
  graph. The self-seeding counterpart to the org-scale corpus scan — no pre-clone,
  no `config.toml`.

## [0.2.0] - 2026-07-01

### Added
- **Named args + field punning on relation atoms.** A body atom or `?` query may
  pass args by declared column: `type_edge(from: f, kind: "impl")`. Once any
  `col:` appears the atom is in named mode, where a bare identifier puns to its
  own column (`from` == `from: from`, the JS/Rust-struct shorthand), and any
  unmentioned column is a don't-care — so you name only the columns you use
  instead of counting positional `_`. Resolution rides the relation's declared
  columns (user `rel` decls and built-in schemas alike) in a frontend pass, so it
  works across a forward reference. Positional atoms are unchanged. Named args in
  a rule head are rejected for now (aggregate interaction deferred).
- **`dl update` — self-update to the latest release.** Re-runs the cargo-dist
  installer for the newest tag; `--check` reports the installed vs latest version.
- **`dl index` — turnkey SCIP generation.** Detects the language(s) at a root by
  marker file (Cargo.toml / tsconfig / package.json / pyproject / go.mod /
  build.gradle / pom.xml / compile_commands.json / CMakeLists), runs the matching
  indexer (rust-analyzer, scip-typescript, scip-python, scip-go, scip-java,
  scip-clang), and places the result at `<root>/.dl/index.scip`. `--install` runs
  the per-indexer install command; `--rev REV` prints the worktree-and-index
  recipe (SCIP covers the working tree only). A polyglot workspace produces one
  merged index via `scip_import::merge_files`.
- **`dl doctor` — SCIP health screen.** Reports detected languages, indexer
  availability, index presence + freshness (mtime vs HEAD), path-join sanity, and
  `scip_*` row counts. Turns each formerly-silent SCIP failure into a visible line.

### Changed
- The SCIP importer auto-loads `<root>/.dl/index.scip` in addition to
  `$SPREFA_SCIP_INDEX` and `<root>/index.scip`, so a `dl index`-generated index is
  found with no configuration. `dl index` appends `index.scip*` to
  `.dl/.gitignore`, so a generated index (often 100MB+) never lands in git.
- The indexer always runs with `cwd = root`, so SCIP `relative_path` keys join the
  paths the scanners see (removes the silent-empty-from-wrong-dir failure mode).

### Fixed
- **Undeclared head relation is a clear diagnostic, not a SQLite leak.** A rule or
  `?` query over a relation with no `rel` decl now reports `unknown-relation`
  (through `--check`/LSP) naming the relation, instead of failing at execution as
  a raw `no such table: rel_X`.
- **Independent `?` queries.** A query that fails at evaluation (e.g. wrong arity)
  reports its own failure and no longer aborts the rest of the query chain.
- **Zero-match `scan` warns.** A source rule whose glob matches no files prints a
  warning naming the rule, glob, and `repo@rev (root)` it looked under, instead of
  silently producing 0 rows downstream.
- **A bare `//` gives a clear message** ("dl comments start with `#`") instead of a
  baffling `Regex("")` parse error.

### Guardrails
- SCIP generation is explicit and single-root only. Nothing (daemon, reload gate,
  `scan("*")` fan-out) generates an index automatically; the daemon only imports
  one that already exists. `dl index` refuses an aggregation directory — the XDG
  serving home, or a folder containing nested git repos — unless `--force`, so on
  a machine whose daemon watches hundreds of repos a stray marker file cannot turn
  one command into hundreds of indexer runs.
