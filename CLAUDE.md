# sprefa

Reactive datalog-over-code engine ("dl"), living at the **repo root** (v5 lifted
2026-07-01): SQLite-welded, facts extracted via `scan`+`regex`/`ast`/`sg`/`json`,
recursive rules lower to a SQL fixpoint. Prior iterations: v3/v4 working trees in
`~/projects/sprefa-archive-20260701` (also full git history); the OG coordinate
model (strings/refs/byte-spans) in `~/projects/sprefa-archive-20260428`.

User-facing overview (model, DSL surface, CLI, examples, known gaps): **`README.md`**.

Deep state lives in auto-memory (`project_v5_dl_engine`, etc.) + `chat_log/` session
logs + `plans/`. This file is the standing task ledger only.

**Completed-arc history** (85 landed items, full detail) lives in
`.agent/memories/sprefa-task-ledger.md` — read it on demand, not auto-loaded. This
file keeps only the standing laws + currently-open work.

## Standing laws (user-set, non-negotiable, apply to every agent at every level)

- **Build-vs-buy**: never assert "we should write our own" for any common-shaped
  problem (queues, servers, schedulers, parsers, telemetry) without FIRST running
  library research and presenting a written analysis of the candidates and why each
  does or does not fit. No one-line dismissals of libraries. The analysis comes
  before any bespoke line of code.
- **Self-diagnosis before execution**: the daemon does not run until `dl daemon why`
  can state, from the on-disk trail alone, what it was doing and what it consumed
  (CPU, disk I/O) — including after a SIGKILL or crash. No receipt runs, no smoke
  tests that start the daemon, until that capability is installed. Never make the
  user ask "why is it slow" — the system answers that itself.
- **Nothing seizes the machine**: CPU (QoS/nice), disk I/O (IOPOL_THROTTLE), and
  thread budget are all capped in `apply_daemon_budget`. First-run rebuild included.
  A change that can beachball the machine is a blocking defect, not a follow-up.

## v5 Work — Tasks Context

The recurring debt we keep re-hitting has two shapes: **(1) per-row write loops
(N+1)** and **(2) bespoke per-relation refresh functions**. A third,
**(3) string-inline-everywhere**, is the ref-spine debt.

Open items below are one-liners; full history + landed detail in the archive.

### Features / arcs
- [ ] **Auto-architect vision** — the umbrella doc for the capability ladder (facts -> measures -> effect coloring -> lock/channel interval analysis -> auto-suggested refactor seams by coupling+affinity), SOTA anchors, dogfood-first validation law: **docs/vision-auto-architect.md** (2026-07-18). In-flight children: callable completeness, resource-aware scheduler plan, decomposition plan, effect inventory.
- [ ] **Callable completeness (in flight 2026-07-18)**: EntityKind::Lambda (user-named; a lambda is an inner non-top-level fn) for anonymous callables in all 5 langs + nested named fns + TS/Kotlin constructors (user law: a ctor is philosophically always a fn call, behind the parens — without ctor+lambda rows taint tracking is broken for df/scip/call fams). Self-verifying table: `@callable <lang> <kind>` comment markers joined against call_def rows from tests/fixtures/callables/ by examples/callable-coverage.dl (diag error on rot). Audit matrix: docs/callable-coverage.md (4e7b297f).
- [ ] **Auto-refactor**, rides C: `edit(ref_id, new_string_id)` sink, `--fix`/LSP rename. Route A (`--move`) landed; residual = brace-head `use crate::{clk::X, ..}` + physical file move + moved file's own imports. Plan: `plans/2026-05-31-auto-refactor-use-path-rewrite.md`.
- [ ] **vscode Wave 4**: B4 dl/locate follow-user; B5 call/type hierarchy; C3 exploded stratum view; C4 3D iso go/no-go. Plan: `plans/2026-07-10-vscode-ext-review.md`.
- [ ] **LSP thin client over the daemon**: `--lsp` = stdio<->socket adapter (LspPump mirrors mcp::Pump); retires served-copy divergence. Plan: `plans/2026-07-10-lsp-thin-client-daemon.md`.
- [ ] **Turnkey query surface**: `dl q <verb>` runner (param injection + verb_catalog); then blast-radius/dependents verbs via run_reaches_pair + built-in MCP tools dl.what/dl.verb/dl.rows; `dl find` (Tier 3). Plan: `plans/2026-07-10-turnkey-query-surface.md`.
- [ ] Migrate deck graph (`examples/anim-self.dl` + anim AtlasPanel) from name-keyed `type_edge` to sym-keyed `type_link` + `type_entity` (optional; changes node identity).

### Bugs / gaps
- [x] **daemon cpu hog root-caused and fixed**: 2s poll full-tick storm (7af0e319) + per-process exe-identity cache forcing full corpus rebuilds every tick of the first post-install daemon (c351ed90). Ledger folklore about restart-after-install retired.
- [x] **exe-swap boot write storm root-caused and fixed**: nondeterministic extraction (file-set SELECTs in rowid order + cached-facts hits-then-misses feeding first-wins id dedups) made every re-extract emit different rows, honestly re-triggering the full derived cascade. Deterministic order landed (80617b6b); double-swap receipt 2026-07-17: 4.7GB → 111MB, cpu 72.9s → 8.5s. Crash-window per-component wipe/mark + deferred digest saves + bulk-rebuild I/O mode landed (7f4d9c58, 6afd2cf3, 5cf4be15); live kill -9 mid-derived receipt 2026-07-17: clean recovery, no storm, follow-up ticks <1s (component-scoping pinned by it-tests). RCA: docs/rca-exe-swap-write-storm.md. Rails: determinism it-test (a45c34d9) + 4 syntax rails w/ prev-rev oracle (792cc902, scripts/rails-oracle.sh).
- [x] **one-shot dl choke fixed** (e7d29829): apply_process_budget on every CLI entry (DL_NO_BUDGET/DL_BUDGET_DEBUG), DL_MAX_WALL_SECS watchdog (default 300s, exit 124 naming phase/root), attach-client 10s heartbeat + exit 75 bound. Live receipt from ~/projects/instant: client 0.03s cpu/10MB, phase-naming waits, 71s cold worst case.
- [x] **program-edit write storm fixed via dirty-rel scoping**: per-rel `drv:` rule-shape digests (derived twin of `src:`) attribute a program edit to the moved heads; the full-layer wipe downgrades to the scoped rebuild seeded with them. Receipt (warm src/rels corpus): edit tick 1 derived row vs 7,312 forced-full. Discriminating tests: tests/it/derived_scope.rs (proven fail-pre-fix), perf_facts contract updated. Residual: UNATTRIBUTABLE full rebuilds (blank slate, carry, edge-list change, crash-recovery derived-missing) still rewrite byte-identical tables in SQL — a content-skip (digest-before-write) would need rows through Rust or shadow-table compare; not landed.
- [x] **effects orphan mystery root-caused and fixed** (67ed59fe): dynamic effect templates were invisible — rel_effect_cmd stores interned INTEGER ids, both executor call sites read them via as_str() (None), so every boot parked dynamic-template effects orphaned. Read rel_effect_cmd_txt instead + unconditional orphan probe in requeue. Boot receipt: 6/6 effects done, 0 orphaned (was 5 re-parked every boot).
- [ ] **empty-scan guard covers no-scan only** (c3c587c9): a program with SOME scan rules still narrows an existing db to its own scope on --no-daemon runs (smashy snapshot 618→68 files). Inherent reconcile semantics; documented sharp edge in docs/arch-measures-review.md.
- [ ] **root attribution residual** (c33ffc04): single tick_root pairing slot mispairs if two ticks begin concurrently on different threads; process-global approximation accepted, job-context plumbing is the true fix.
- [x] **R7 diag stage routing landed** (73dbcc4a): diag_stage builtin sink + `--stage live|commit|agent-turn|agent-session` on --check + hook routing (agent-turn = touched-path gate, agent-session = per-code summary). Defaults: error -> every stage, unrouted warning -> commit only (storm rails stop spamming live/agent surfaces). User decision recorded: tracing crate, never eprintln (DL_TRACE cli / DL_LOG daemon); 223-site eprintln inventory + build-vs-buy analysis in the plan. Editor live surface deferred to vscode Wave 4.
- [ ] **measures verdict decided 2026-07-18: keep ALL** (research instruments; overlap is data). In flight: std/measures.dl with top-K views + review-doc verdict recording. `dl q` verb wiring still rides the unbuilt turnkey-verb-runner arc.
- [x] **enumerate_with_hash racy window closed** (f2205994): git-racy-index-style guard — fast path also requires `mtime < walk_ref_secs` (persisted per-walk reference second, whole-second only), so a same-length same-tick edit rehashes; quiet-tree rehash footprint ~0; fail-pre-fix test in tests/it/racy_mtime.rs.
- [x] **cold-start staging landed** (61878e5a/8829d74c/0ae36735/9aaeccb6): blank-slate daemon boot seeds `_cold_node` per used family, drains as throttled ColdExtract jobq jobs (single-flight), completion tick does the one derived rebuild; kill -9 resumes only pending nodes; --no-daemon stays inline; equivalence + crash-recovery it-tests.
- [x] **cold-start work-chunking landed** (d962ecf2..a201790c): measurement flipped the premise — parse is 4% of cold cost; dataflow was the hog (4.4s: emit 2.3 + write 2.1 over 115k rows), call is a 1.4s corpus-global barrier (honest floor). Dataflow now drains in deterministic 512KiB/64-file contiguous chunks of the byte-sorted file set (DL_COLD_CHUNK_BYTES/FILES); family digest saves once at the completion gate; SCIP is its own highest-priority scip-index node. Longest single job 2468ms -> 766ms (3.2x). Chunked==inline equivalence + crash-mid-chunk resume pinned. Scoped out with numbers: call/type/module/doc (barrier or not hogs), spine (no per-file dimension); comment/template/unresolved ride the same seam as follow-up. This closes the daemon-boot exposure item (staging was chosen over root-dropping levers).
- [x] **deltaflow per-row N+1 batched** (2bda577c): 4 write loops -> chunked multi-row INSERT/DELETE under 800-param ceiling inside the same BEGIN IMMEDIATE; loop at old line 206 is read-only (left per-row, commented); guard test pins write statements not scaling with change count.
- [x] **loop break-value df tails** (aa6722ea): `let x = loop { break v }` flows v -> x; labeled `break 'outer v` resolves through the loop_breaks frame stack; both tests proven fail-pre-fix; sprefa's own corpus has zero value-carrying breaks today (pure correctness closure).
- [ ] **S3** retired — body-level bind for pure-fn values lowers as inlined expr (src/lower.rs `bind_lowers_to_inlined_expr_sql`). **S4** retired — text `+` concat landed (docs/reference/syntax.md:23; heads + comparison sides, never in a binding atom). **S5** ast-grep patterns exact-shape (metavar-in-JSX `{ element: <$C/> }` matched nothing). **S6** source-extract rule body silently drops an extra joined rel atom (rel-level guard doesn't cover body-level mix).

### Debriefs / friction (backprop candidates)
- [ ] **Change-cost friction inventory** — 12 ranked items, fix shapes + sequencing: `plans/2026-07-10-change-cost-friction-inventory.md`. Top: ambient-config hermeticity, declared cross-family read edges, query --format=json, engine-monolith epic, resolution_source column.
- [ ] Recurring pains across agent debriefs: (a) **ambient config** — every ad-hoc `dl` run ingests `~/.config/sprefa/config.toml` repos; set `SPREFA_CONFIG` for hermetic smoke tests. (b) RESOLVED (75245073): line/col base documented per rel in RelDecl docs + docs/reference/relations.md, verified against extractors. (c) RESOLVED (3c0d9141 doc + 60f0847a fix): docs/df-coverage.md maps per-lang df coverage; ts_flow_class arm added — TS/JS class methods (instance/static/ctor/getters/setters) now emit df rows (sample class: 7 -> 38 df_nodes); residual gap = class field initializers (no enclosing fn scope), documented. Getter/setter share one fn sym. (d) RESOLVED (bc7e531f): data-row indent removed from every `?` printer. (e) RESOLVED (0615b7e0): public `Engine::ensure_families(&[&str])`, errors on unknown family, no derived rebuild. (f) `crate::daemon` vs `crate::cli::daemon` collision. (h) AST_LANG_TABLE buried at ~mod.rs:7674 (engine-monolith placement debt). (i) NEW: pre-commit `dl --check` in throwaway worktrees cold-starts a daemon and hangs — every delegated agent hit it; worktree-root detection or a hook fast-path for blank-db roots is the fix shape. Also landed: `dl query --format json` (fedcb388, friction-inventory item).

### Style notes for this repo
- dl variable names are descriptive, never single-letter: `path`/`line`/`callee_name`, not `p`/`l`/`q`. Applies to every snippet in skills, examples, book, tests, and agent prompts; rename opportunistically when touching old files.
- N+1: never a per-row write. Collect the set, call `Db::insert_rows` once. The tick counter screams if you don't.
- No `provenance`/`substrate`/`load-bearing`/`regime` as prose or identifiers (use source/base/critical/mode).
- Sync tick engine: plural-API + collect-then-flush, NOT async DataLoader (the redux-out-of-hand trap).
- One rel = one rule kind: never head a rel with both a source rule (scan/match/ast/sg/json/cmd/comment) and a derived rule. `rebuild_derived` does a full `DELETE FROM rel` that would wipe the reconciled source rows. The engine now bails; split into two rels and union in a third derived rule. SAME hazard, separately guarded, for a **term-extract** rule (a `json`/`jsonp` body predicate over a bound string) headed together with a derived rule: `eval_extract_rules` fills the extract rows, then `rebuild_derived` (which runs after it so derived rules can read the extract output) drops them. Notably a term-extract rule cannot feed a `@next` carry directly for this reason — route it through its own rel first (the `pr_number -> change_log` split in gh-cache.dl). Engine bails as of the ghcacher-parity arc.
- Recompute guard: a fn that re-derives a relation/embedding FROM SCRATCH (a global op like `embed_graph`, run on a reactive rule) must early-out when its input is unchanged — a `load_rel_digest` digest skip (see `eval_node2vec_rule`, the scc/closure `ConditionCache.digest`) — or carry a `// @recompute unguarded: <reason>` waiver in its body. `examples/recompute-guard.dl --check` (exit 2) is the rail that enforces it; an unguarded recompute re-runs on every git-checkout re-tick under the daemon lock.
