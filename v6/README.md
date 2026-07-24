# V6

> **2026-07-20 snapshot, partially stale.** The 7-Rust-crate map and the daemon
> framing predate the 2026-07-23 pivot (TS engine on actual rxjs; no daemon
> concept: process lifetime = sum of held subscriptions; CLI speaks http unless
> `--inline`). Current sequencing: `v6/plans/2026-07-23-v6-rest-epic-golden-plan.md`.
> Current pins: `v6/DECISIONS.md`. The rules and buy-dont-build stance below
> still stand.

V5 works. It also kept re-solving the same five problems — the storage seam,
the daemon wire, build-vs-buy, repo/rev identity, observability — because
everything lives in one crate and every seam was negotiable.

V6 is not a rewrite. It is an **extraction**: the seams V5 already grew
(`src/db.rs`, the daemon shell, the lang pipeline) get promoted into crates
with trait boundaries, and the boundaries get rails.

Status: **planning**. No V6 code exists. First code = hollow types, hooked to
nothing, reviewed by a human before any call site moves.

## What this is for

A cross-repo codegen / type-math Swiss Army knife: type spec, type
generation, type macros across languages and codebases. The daemon and the
database are infrastructure — bought, not built. Language changes are in
scope for V6 when the tool needs them (landed in `sprefa-lang` with a spec
update, per rule 2).

Evaluation is **cold by default**: nothing computes without a subscription —
rels are cold observables, a subscription activates a query's dependency
cone (including the cross-repo cut), and the last unsubscribe puts it back
to sleep. See the demand plan.

**MVP:** the ghcacher service as a served dl program
(`bench/ghcacher_vs_dl.sh`, `tests/it/ghcacher_parity.rs`) — poll effects
plus its own declared HTTP/MCP interface, with zero program-specific Rust.
It used to be a Rust service. That is the point.

## Rules

1. **Buy, don't build.** A library exists for every problem we hand-rolled:
   axum (HTTP on UDS), tower-lsp (LSP on any stream), rmcp (MCP), sea-query
   (SQL builder), tracing-appender (log files). Hand-rolling needs a written
   excuse in the relevant plan. Foreign tools (Go included) are fine behind
   the effect boundary — precedent: the ghcacher port shells to `gh` + `jq`.
   Graph algorithms: petgraph, with `petgraph::Csr` as the snapshot
   representation.
2. **`sprefa-lang` is untouchable.** It depends on nothing else in the
   workspace, and other crates never reach into it. It changes only with the
   language spec.
3. **All persistence lives in `sprefa-store`.** The API speaks rels, rows,
   and plans — never SQL text, never a driver type. SQL generation goes
   through the sea-query builder with the dialect as config; the backend is
   SQLite today, swappable without an API change.
4. **One server, thin clients.** CLI, LSP, and MCP all talk to the same
   process over the same socket. No in-proc fallback engines.
5. **One observability stack.** `tracing` facade everywhere, one init
   (a module in `sprefa-server`, not its own crate), logs to files, never
   stdout.
6. **Size is a rail, not a vibe.** The File-Size Law
   (`scripts/filesize-rail.sh`: ≤500/file, ratchet-only allowlist) scales up
   to per-crate line ceilings in V6 — see the crate-map plan's size budget.
   Ceilings may only shrink.
7. **Practicality over purity.** Crates are dependency firewalls, not art —
   split only where the boundary earns its keep. Concrete types until a
   second implementation actually arrives (the 2026-07-18 seam ruling,
   `.dl/no-new-rusqlite.dl`, generalized). No DI containers, no
   generic-for-generics'-sake. Reactive machinery tops out at
   BehaviorSubject + combineLatest — and dl itself is already that layer.

## Crate map

Seven crates. Each one is a firewall: it exists to wall off dependencies or
to ratchet a size ceiling, and for no other reason.

```
sprefa-lang        AST, grammar, lowering, spec.        deps: none (workspace)
sprefa-store       storage API, backend-neutral.        deps: lang, tokio, sea-query
sprefa-graph       CSR snapshots + graph algorithms.    deps: lang, store, petgraph
sprefa-rels        builtin relation families.           deps: lang, store
sprefa-engine      incremental evaluator.               deps: lang, store, graph, rels
sprefa-server      the daemon: axum + tower-lsp + rmcp. deps: engine, store, lang
sprefa-cli         clap + rendering, thin client.       deps: server (client lib)
```

Dependency direction is strictly downward, enforced by `cargo tree` rails.

## Docs

- [plans/2026-07-19-v6-crate-map.md](plans/2026-07-19-v6-crate-map.md) —
  boundaries, type/struct/singleton/dataflow math, size budget
- [plans/2026-07-19-v6-storage-crate.md](plans/2026-07-19-v6-storage-crate.md) —
  the storage API, backend-neutral; repo/rev on every table
- [plans/2026-07-19-v6-daemon.md](plans/2026-07-19-v6-daemon.md) —
  one process, all protocols, prior art
- [plans/2026-07-19-v6-language-interfaces.md](plans/2026-07-19-v6-language-interfaces.md) —
  pluggable protocols in dl; MVP ghcacher
- [plans/2026-07-19-v6-demand.md](plans/2026-07-19-v6-demand.md) —
  cold-by-default evaluation; Observable + subscribe

## Agent skills

Primed context for V6 work lives in `v6/.agents/skills/`: **v6-plan** (rules,
rulings, reading order, doc conventions) and **v6-deps** (pinned versions +
verified traps for every adopted crate). Load before touching anything under
`v6/`.

Note: `v6/plans/` is deliberately outside `plans/`, so its todo comments do not
feed PLANS.md (`examples/gen-plans-index.dl` scans `plans/*.md` only). Fold
them into the index when the first crate lands.
