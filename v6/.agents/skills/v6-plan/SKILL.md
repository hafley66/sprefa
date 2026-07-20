---
name: v6-plan
description: Primed context for working on sprefa V6 — what V6 is, the 7 rules, the 5 owner rulings, plan reading order, and v6/plans doc conventions. Load before writing or editing anything under v6/.
---

# V6 primed context

V6 is an **extraction, not a rewrite** of V5 (89,151 lines, one real crate).
The seams V5 grew get promoted to crates with trait boundaries and rails.
Home: `v6/` — README first, plans in `v6/plans/`.

## The 7 rules (README is canonical)

1. Buy, don't build — hand-rolling needs a written excuse in the plan.
2. `sprefa-lang` is untouchable — no workspace deps; changes only with the spec.
3. All persistence lives in `sprefa-store` — API speaks rels/rows/plans, never
   SQL text or driver types; sea-query builder, dialect is config.
4. One server, thin clients — CLI/LSP/MCP all talk to the same process; no
   in-proc fallback engines.
5. One observability stack — `tracing` facade, one init (module in
   `sprefa-server`), logs to files.
6. Size is a rail — per-crate line ceilings in `scripts/crate-size-allow.txt`,
   ratchet-down-only, mirroring `scripts/filesize-rail.sh`.
7. Practicality over purity — concrete types until a second implementation
   arrives; no DI; reactive machinery tops out at BehaviorSubject +
   combineLatest, and dl is already that layer.

## The 5 owner rulings (crate-map plan has full text)

- **Practicality** (2026-07-19): crates are dependency firewalls; the
  2026-07-18 "seam stays a struct until a second backend" ruling generalized.
- **Backend-neutrality**: backend is config, not identity. `BackendConfig` at
  `Store::open`; no driver/dialect type in any public signature (greppable
  rail). SQLite is the first backend, not the permanent one.
- **Demand**: nothing computes without a subscription. Rels are cold
  observables; a refcounted `(root, query rel)` subscription activates the
  static dependency cone (incl. cross-repo cut); last unsub → cone cold.
- **Sync/async**: server 100% async; tick hot path sync (nothing to await in
  a fixpoint). `Store` sync on the writer thread; `StoreHandle` async for the
  server (reader pool + writer channel, actor pattern). No `spawn_blocking`
  per request. Bench todo can flip this.
- **File-Size Law** (2026-07-11, pre-existing): ≤500 lines/file, target 300,
  allowlist ratchets down only.

## Crate map (7 crates, downward deps only)

lang → store → graph, rels → engine → server → cli. Ceilings (start, ratchet
only down): lang 5.5K, store 9.5K, graph 12.5K, rels 3.7K, engine 26K,
server 8.5K, cli 2.5K. Extraction order: lang → store → graph → rels →
engine → server+cli. First code = hollow types, human-reviewed before any
call site moves.

## Plan reading order

1. `v6/README.md` — rules, crate map.
2. `v6/plans/2026-07-19-v6-language-interfaces.md` — the point of V6
   (pluggable protocols in dl; MVP = ghcacher as a served dl program).
3. `v6/plans/2026-07-19-v6-crate-map.md` — the big one: rulings, type/struct/
   singleton/dataflow math, size budget, census.
4. `v6/plans/2026-07-19-v6-demand.md` — cold-by-default evaluation.
5. `v6/plans/2026-07-19-v6-storage-crate.md` — storage API detail.
6. `v6/plans/2026-07-19-v6-daemon.md` — transports, prior art.

## v6/plans doc conventions

- Naming: `v6/plans/YYYY-MM-DD-<slug>.md`, one plan per arc.
- Required sections: Context (cite files/lines/commits), Decisions (rejected
  alternatives named), Verification (tests/rails/numbers), Staffing (agent,
  worktree, base SHA, suite budget). Extra design sections between Context
  and Verification.
- Open items: `<!-- todo(category): text -->`, category in
  `perf|bug|feature|docs|triage|decision`.
- `v6/plans/` is deliberately NOT scanned by `examples/gen-plans-index.dl`
  (it reads `plans/*.md` only) — no PLANS.md regen needed until the first
  crate lands and the docs fold in.
- Base SHA for all arcs: `8d7b6092` (branch `next`). Worktrees under
  `.worktrees/`. Every arc ends green on `scripts/verify.sh`.

## Verification rails to honor when implementing

Dependency rails (`cargo tree` greps), persistence rails (no `rusqlite::` /
`sqlx::` / `sea_query::` outside sprefa-store; no `format!` SQL; store's
public API names no driver type), crate-size rail, move-vs-delete report per
arc, demand tests (zero-subscriber root never ticks), no `spawn_blocking` in
sprefa-server outside the writer-thread bridge.
