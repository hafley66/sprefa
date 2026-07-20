---
name: v6-deps
description: V6 dependency dossier — pinned versions, API sketches, and traps for sea-query, rusqlite_migration, petgraph, rmcp, tower-lsp-server, axum-UDS, and the tracing stack. Researched 2026-07-19. Load before adding or touching dependencies in V6 crates.
---

# V6 dependency dossier (researched 2026-07-19)

Rule 1 is buy-don't-build; this file is what we bought and why. Pin exactly
as shown. Every trap listed is one we verified, not folklore.

| crate | pin | used by | one-line reason |
|---|---|---|---|
| sea-query | `=1.0.1` (`backend-sqlite`, `derive`) | sprefa-store | typed SQL AST, dialects as config, no async |
| rusqlite_migration | `2` (2.6.0) | sprefa-store | user_version migrations, sync-primary |
| petgraph | `0.8.3` (`default-features=false`, `["std"]`) | sprefa-graph | Csr + algo core |
| rmcp | `2.2` (+`schemars`, `transport-streamable-http-server`, `transport-io`) | sprefa-server | official MCP SDK, mounts into axum |
| tower-lsp-server | `0.23` | sprefa-server | maintained LSP framework (Biome's) |
| axum | `0.8` | sprefa-server | UDS is first-class (`Listener` trait) |
| tracing-appender | `0.2` / tracing-subscriber `0.3` (`env-filter`, `json`) | sprefa-server | rolling files + per-layer filters |
| interprocess | — (skip) | — | only for Windows named pipes; revisit then |

## sea-query (sprefa-store)

- **Covers everything we need with builders**: CTEs (incl. recursive),
  window functions, `ON CONFLICT … DO UPDATE`, INSERT…SELECT, RETURNING.
  `WITHOUT ROWID` goes through `.extra("WITHOUT ROWID")` — static DDL suffix,
  no injection surface.
- **TRAP: skip `sea-query-rusqlite`.** It's pinned to a sea-query RC and
  rusqlite 0.38 (we're on 0.40) — two rusqlite versions in the tree. Write
  the ~40-line `sea_query::Value → rusqlite::types::Value` adapter in-crate.
  This also keeps sea-query types and rusqlite types from ever leaking.
- Runtime table names: `&str`/`String` implement `Iden` directly in 1.0 —
  `Query::select().from(rel_name.as_str())`; identifiers are quoted by the
  backend. Generated `rel_*` tables are a non-issue.
- Rules: `.build(SqliteQueryBuilder)` → `(sql, Values)` with bound params;
  never `.to_string()` for execution (interpolates values). `Expr::cust*`
  only with static fragments — **no `format!` into `cust`** (that's the
  review invariant replacing "no string SQL"). Use `.values()` (Result), not
  `values_panic`, in codegen.
- Maintenance: 1.0 shipped 2026-05-28 after a year of RCs; SeaQL org active
  (SeaORM 2.0 builds on it). Residual risk is bus factor, not activity;
  `forbid(unsafe)`, zero-dep core → fork is cheap.

## rusqlite_migration (sprefa-store)

- `Migrations::new(vec![M::up("..."), M::up_with_hook("...", |tx| …)])`,
  `to_latest(&mut conn)` / `to_version(...)`; `user_version` pragma tracking
  (nothing else may write it). `MIGRATIONS.validate()` is a one-line test.
- TRAP: no PRAGMAs inside migrations (`journal_mode` no-ops in the tx,
  `foreign_keys` is per-connection) — apply outside; use `.foreign_key_check()`.
- IMPEDANCE NOTE: rusqlite_migration is append-only; our `rel_*` DDL is
  generated fresh from decls. Hash the full generated DDL into one synthetic
  migration step (or use user_version as schema-hash guard) — decide in the
  store arc, don't improvise.

## petgraph (sprefa-graph)

- `Csr<(), ()>`: build ONLY via `Csr::from_sorted_edges(&edges)` — input must
  be `sort_unstable()` + `dedup()` first (duplicates are an *error*), and
  node_count is inferred as max_endpoint+1 (isolated nodes vanish — track
  them in the snapshot sidecar). Never bulk-build with `add_edge` (O(V·E)).
- **TRAP: `algo::tarjan_scc` is recursive** — deep call chains overflow an
  8MB stack at 1M edges. Run on a big-stack spawned thread, or keep V5's
  iterative tarjan over `neighbors_slice()` as the fallback. Benchmark the
  deepest real graph first.
- **No reverse traversal on Csr** (`IntoNeighborsDirected` missing): for
  `reached_by`, build a second transposed Csr (sort by `(v,u)`).
- `condensation` and `toposort` don't run on Csr — build condensation
  yourself from `TarjanScc::node_component_index` (~5 lines, stays in CSR).
- No transitive closure anywhere in the crate — reachability/closure stays
  in the dl layer, as planned.
- Hot walks can keep using slices: `neighbors_slice(a) -> &[u32]`.
- Pin ≥0.8.3 (0.8.0–0.8.2 had an edge_count bug). Repo trunk is mid-migration
  to multi-crate; the 0.8 branch is the shipping line.

## rmcp (sprefa-server)

- Mount into the existing axum router:
  `Router::new().nest_service("/mcp", StreamableHttpService::new(factory, LocalSessionManager::default().into(), config))`
  — axum 0.8 is rmcp's own dev-dependency, the pairing is tested upstream.
- Tools: `#[tool_router] impl` + `#[tool(description=…)]` methods,
  `#[tool_handler] impl ServerHandler`; params derive `serde::Deserialize +
  schemars::JsonSchema`. **Use `rmcp::schemars` re-export** — schemars 1.0 is
  a rewrite; never mix a second schemars version.
- Pass a `CancellationToken` child into the server config or per-session SSE
  workers outlive graceful shutdown.
- Sessions are stateful and in-memory; clients that don't DELETE can
  accumulate them — check `SessionConfig` timeout knobs at pin time.
- Bonus: feature `transport-streamable-http-client-unix-socket` gives an
  MCP-over-UDS *client* — the stdio↔UDS proxy becomes first-class.
- Churn warning: 1.x→2.0→2.2 in six weeks. Pin a minor; expect upgrade work.

## tower-lsp-server (sprefa-server)

- Use `tower-lsp-server = "0.23"`, NOT `tower-lsp` (dead since 2023).
- **TRAP: 0.23 switched to `ls-types`** (from `lsp-types`). Import
  `tower_lsp_server::ls_types` only; never both.
- LSP over UDS: one `LspService` per accepted connection;
  `let (r, w) = tokio::io::split(stream); Server::new(r, w, socket).serve(service)`.
  Per-session initialize handshake falls out for free.
- Default handler concurrency is 4 (`buffer_unordered`) — all backend state
  needs interior mutability; CPU-heavy work in `spawn_blocking` (except
  inside the store writer bridge — the crate-map rail).
- `shutdown` request ≠ `exit` notification; `exit` ends one connection's
  loop, process lifetime is ours. Biome's `is_initialized` +
  `stop_on_disconnect` pattern is the reference.
- The stdio→UDS lsp-proxy shim needs ZERO LSP crates — LSP framing is
  transport-agnostic; it's a pure `tokio::io::copy_bidirectional`.

## axum 0.8 on UDS (sprefa-server)

- `axum::serve(UnixListener::bind(&path)?, app)` works directly;
  `.with_graceful_shutdown(f)` identical to TCP. Same router on UDS +
  ephemeral TCP = two `serve` calls with `router.clone()`.
- TRAPS: `remove_file` the stale socket before bind (and `create_dir_all`
  the parent); `set_permissions(&path, 0o600)` right after bind (tokio
  applies umask perms); unlink the socket on shutdown; sun_path ≈ 104 bytes
  — keep paths short (XDG_RUNTIME_DIR / TMPDIR, not deep $HOME).

## tracing stack (sprefa-server)

- **THE classic trap: `WorkerGuard` lifetime.** `non_blocking()` returns
  `(writer, guard)`; drop the guard early and logging *silently dies* (fmt
  swallows write errors). Guards live in the top-level daemon struct and are
  dropped last on shutdown.
- Per-layer filters: `.with_filter(env_filter)` on each fmt layer (file at
  INFO, stderr at WARN, jsonl event layer with its own target filter) —
  not a global filter.
- Rotation is time-based only (DAILY etc.) — no size cap, no compression.
  Budget an external reaper; V5's 4MB-rotate RollingWriter logic is the
  reference if we keep one.
- Non-blocking is lossy under overload (bounded queue) — right for a daemon;
  know it.

## Deferred / rejected

- **interprocess** — skip until Windows support is real; our two seams
  (axum `Listener`, tower-lsp-server's AsyncRead+Write) are already
  transport-abstract, so the port is mechanical.
- **jsonrpsee** — no UDS transport (reth hand-rolls it); axum JSON-RPC-over-
  POST is fine.
- **tonic/gRPC** — proto toolchain for zero external clients.
- **sqlx** — rejected for the tick hot path (see crate-map sync/async
  ruling); bench todo can flip it.
- **sea-orm / diesel** — ORM magic; generated `rel_*` tables don't fit.
