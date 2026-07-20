# V6 language interfaces — pluggable protocols in dl, MVP ghcacher

## Context

The product is the language: a cross-repo codegen / type-math Swiss Army
knife. The daemon and the database are infrastructure — bought, not built.
This plan is the one that makes V6 *the tool*, not just a tidier V5.

What exists today:

- **Ports are generic in the engine.** Rels marked `@in(class)`/`@out(class)`
  (examples/mcp-echo.dl:6-28); the tick machinery reads the program's port
  rels and injects/drains rows (`src/engine/tick.rs:369-376,784-789`,
  `src/engine/declare.rs:149-150`). The class is data.
- **Effects are generic too.** `@async` / `@stream` / `sh` runtime with an
  `EffectExec` trait and off-tick `pending_effect` drain
  (`src/effect.rs:1-10`), `clock()` builtin, `@next` state.
- **What's bespoke is the transport serve loops.** The MCP loop injects into
  `@in(rpc)` and drains `@out(rpc)` (`src/mcp.rs:1-29`); hook has its own
  path (`src/hook.rs`); LSP mirrors RPCs locally (`src/lsp.rs:532,547,591`).
  Every new protocol so far = a new Rust loop. There is no way for a program
  to declare an HTTP route, a WebSocket, or a new socket protocol.
- **The ghcacher port proves the shape matters.** `bench/ghcacher_vs_dl.sh`
  generates the full gh-cache service as ~100 lines of dl (etag/304
  discipline, paginated PR list, `@async` + `clock` + `sh` calling `gh` and
  `jq`) and races it against the real Rust ghcacher service
  (`~/projects/ghcacher`); parity test: `tests/it/ghcacher_parity.rs`. That
  program used to *be Rust*. That is the point of the whole project.

## Decisions

**Interfaces are declared in dl, materialized by the server.** Port classes
become an open registry in `sprefa-server`: `rpc`, `http`, `ws`, `lsp`,
`mcp`, `hook`, `clock`. The server owns transport; the engine owns tick; the
program owns logic. New transports = new registry entries, not new engine
code, and definitely not new per-program Rust.

The serve loop is generic over class *shape* (request/response, stream,
pub/sub), not over protocol. Same MCP pattern everywhere — inject `@in`,
tick, drain `@out`:

```
% sketch (syntax is a human-reviewed decision, not final)
rel route(path: text, method: text) @serve(http).
rel http_req(id: int, path: text, body: text)  @in(http).
rel http_resp(id: int, status: int, body: text) @out(http).

route("/stars", "GET").
http_resp(id, 200, body) <- http_req(id, "/stars", _), stars_json(body).
```

WebSocket is the same idea with stream semantics — the `@stream` machinery
already exists (`src/effect.rs`).

**A registered interface is a standing subscription.** A live route holds a
subscription to the rels its handler reads; that demand is what keeps its
cone computing (demand plan). Remove the route and the cone goes cold. In
the ghcacher MVP, the route is the *only* reason the poll cone stays warm.

**Language changes are in scope for V6.** Rule 2 ("`sprefa-lang` is
untouchable") governs *dependencies*, not evolution: the language may change
— ports, type spec, type math, codegen macros — but changes land only in
`sprefa-lang` with a spec update, never smuggled into another crate.

**Best tool for the job, any language.** Foreign tools (Go, whatever) are
allowed behind the effect boundary — subprocess/IPC, never bespoke FFI.
Precedent is already in the tree: the ghcacher port shells to `gh` + `jq`
instead of hand-rolling an HTTP client (`bench/ghcacher_vs_dl.sh:67-84`).
`EffectExec` is the seam; V6 keeps it and treats "which executor" as
configuration, not architecture.

**MVP = the ghcacher service as a served dl program.** The full gh-cache
shape on V6, declaring (a) its poll effects and (b) one served query
interface (an HTTP route or MCP tools), with **zero program-specific Rust**.
If the MVP needs a language change to say what it means, the language
changes — that is the pilot for every future interface.

## Verification

- **Registry test:** a dl program declaring an HTTP route gets a live
  endpoint on the V6 server: request → `@in` row → tick → `@out` row →
  response, with no program-specific Rust in the diff.
- **MVP parity:** `bench/ghcacher_vs_dl.sh` on V6 — request count and 304
  ratio vs the Rust ghcacher, plus a live query answered over the program's
  own declared route.
- **WebSocket echo:** a dl program over the `ws` class, streaming both ways.
- **Memory:** daemon RSS stays under the existing ceiling pattern
  (`DL_DAEMON_MEM_MB`, `src/daemon/budget.rs:177-245`) while serving the MVP
  at kubernetes-org cardinality.

## Staffing

One agent (opus-class), worktree under `.worktrees/`, base SHA `8d7b6092`
(branch `next`). Lands after the server arc (needs the registry host).
The `@serve`/port syntax is reviewed by the human before the lang arc
touches it. Suite budget: registry test + MVP parity run + ws echo +
`scripts/verify.sh`.

<!-- todo(decision): @serve syntax — separate annotation vs route metadata on @in/@out; human reviews before the lang arc -->
<!-- todo(feature): port-class registry in sprefa-server — one generic serve loop over @in/@out class shapes (request/response, stream, pub/sub), replacing per-protocol Rust loops -->
<!-- todo(decision): which foreign tools get first-class effect adapters (gh, jq precedent) vs generic sh — weigh subprocess cost at ghcacher cardinality -->
