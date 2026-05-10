---
name: sprf-incr-salsa-cost
description: [v4 planning] Salsa coordination cost, scaling ramifications at 500+ repos, and when NOT to use it. Macros prevent abstraction. Load when considering adding Salsa anywhere in v3.
---

# Salsa: real cost, narrow benefit

## The "pull = free" lie

Salsa is pull-based for *recomputation*, not for *coordination*. Three things you can't escape:

1. **Single global revision counter.** Every input setter bumps it. Every memo-table read checks "is my recorded revision still durable?". Touching one input bumps a number all queries observe. Early cutoff still saves recomputation; the dep-graph *walk* is not free.

2. **Single-writer lock.** Only one thread holds `&mut Db` at a time. Concurrent reads via `&dyn Db` are fine. A writer cancels in-flight readers when it arrives. Indexing throughput is bounded by serial writes.

3. **Cancellation as cross-query coupling.** A hover on repo A and an edit in repo Z share the same Cancelled storm. Anything in flight when a setter arrives panics with `Cancelled`. Caught at the LSP request boundary; propagates through every `tracked` call.

```
                 ┌────────────────────────────────────────────┐
                 │  Database (Salsa storage)                   │
                 │   revision: AtomicU64           ← global    │
                 │   memo_tables: per-query DashMap            │
                 │   inputs: Mutex<...>            ← single    │
                 │                                    writer   │
                 └────────────────────────────────────────────┘
                              ▲           ▲
                  reader threads          writer thread
                  (LSP requests)          (didChange)
                  &dyn Db, parallel       &mut Db, exclusive
                  panic Cancelled         bumps revision
                  if writer arrives       cancels readers
```

## Memory at scale

```
   500 repos × ~10k files/repo            =  5M files
   5M files × parse memo (CST + arena)    ≈  500GB worst case
   5M files × lower memo (HIR body)       ≈  100GB
   ───────────────────────────────────────────────────
   total in-memory Salsa working set      ≈  600GB    ← obviously dead
```

ra survives 100k-file workspaces by:
- rowan structural sharing (10x reduction common)
- `#[salsa::tracked(lru = N)]` per query
- `ItemTree` is the only thing crossing files; bodies on demand
- Durability tiers: stdlib + vendored crates marked HIGH, computed once

## Durability tiers (the primary scaling lever)

```
   ╔══════════════════════════════════════════════════════════════════╗
   ║  HIGH durability    │   bumped only on workspace reload          ║
   ║  (read-only repos)  │   - parse + lower memos: keep, no LRU      ║
   ║  ~470 of 500 repos  │   - sit in memory forever (or paged out)   ║
   ╠══════════════════════════════════════════════════════════════════╣
   ║  MEDIUM durability  │   bumped on git pull / branch switch       ║
   ║  (active repos)     │   - parse memos: LRU 2k entries            ║
   ║  ~25 of 500         │   - lower memos: LRU 500 entries           ║
   ╠══════════════════════════════════════════════════════════════════╣
   ║  LOW durability     │   bumped per keystroke                     ║
   ║  (open files)       │   - parse + lower: no LRU, full retention  ║
   ║  ~5 files           │   - hover/diag/tokens computed on demand   ║
   ╚══════════════════════════════════════════════════════════════════╝
```

Without tiers, every revision bump walks every memo. LRU evicts hot entries under load; durability tells Salsa "these inputs are stable, don't even check them."

## Indexing 500 repos

Single-writer bottleneck:

```
   naive (will not work):
     for file in 5M_files {
         db.set_source(file, read(file));   ← exclusive lock per call
     }
     5M files × 1ms lock acquisition = 80 minutes

   batched (the only way):
     1. Walk filesystem in parallel (rayon) → Vec<(FileId, Bytes)>
     2. Acquire writer lock once
     3. db.batch_set_sources(all)
     4. Release
```

Plan for ~1 setter call per "logical event," not per file.

## Cancellation thunderstorm

```
   t=0    edits in 5 open files    ──►  bump revision 1000
   t=1    bg index queues 500 set ──►   each one bumps revision 1001, 1002, ...
   t=0..1 hover handler running     ──►  every memo-table read checks rev →
                                          Cancelled panic → user typing in
                                          repo Z killed hover in repo A
```

Defenses:
1. Treat indexing as one transaction (buffer 100-1000 set calls, bump once).
2. Run indexing during idle periods (no LSP request for N ms).
3. Two databases (user-facing tier + indexed tier; some forks try this).

## No disk persistence

Salsa keeps everything in RAM. Cold start = re-index from scratch. ra eats 30-90s for rust-lang/rust; at 500 repos, minutes per LSP restart.

```
   tier 0: nothing persisted    ──►  cold start every time (default Salsa)
   tier 1: persist parse cache  ──►  serialize tree-sitter trees per file
   tier 2: persist HIR layer    ──►  serialize lowered IR, validate via source hash
                                     (ra exploring this in 2025-2026)
   tier 3: SCIP / scip-syntax   ──►  pre-index offline, serve as Salsa inputs
                                     (sourcegraph approach)
```

## Macros prevent abstraction

The `#[salsa::tracked]` attribute generates types and impls the runtime reflects on. The function body runs inside a generated wrapper that consults the memo table, records edges into the *currently-executing* query (via thread-local), checks revisions, and panics on cancellation. None of that is exposed as a trait you can `impl` against.

A would-be `trait Memoizable<I, O>` has nowhere to plug in the dep-edge recording, because that has to happen *before your function body runs*, on the database, with knowledge of the call stack. The macro is the only place that knows the call site.

You can't have both ergonomics and trait-shaped extensibility. Pick one.

## When Salsa is worth its macro tax

```
                value ▲
                      │
   rust-analyzer ─────┤●  100k+ files, cross-file inference, complex
                      │   dep graph, sub-100ms hover/complete required
                      │
   medium IDE    ─────┤  ●  10k files, mostly per-file analysis,
                      │     hover acceptable at ~200ms
                      │
   sprefa today  ─────┤    ●  small per-file IR, cross-file goes to
                      │       relation store, hover already <50ms
                      │       hand-rolled
                      └──────────────────────────────────────► macro tax
                                  (cost roughly constant)
```

Below the line: Salsa costs more than it gives.

## What to steal without adopting the crate

1. **Output-hash early cutoff.** Add `cache_key()` on PureEffect *outputs*, not just inputs. On revalidation, compare new output hash to cached; short-circuit downstream.
2. **Durability tiers as a first-class concept.** Each cache entry carries a tier; revision bumps scoped to a tier. Vendored repo tier never invalidates from a workspace edit.

Both fit ~150 LoC of additions to `effect_runtime::CacheLayer`. No macros. No second db. No Cancelled storm.

## When Salsa actually slots into v3

Only one place: pure `lower(parse_tree) -> Pipeline` as the single `#[tracked]` query.

- Edit a comment in a live rule → parse changes, but `lower` output hashes equal → all downstream consumers (registry, hover, diag, run) reuse the prior `Pipeline` for free.
- Edit a real op → `lower` re-runs once, every consumer demands the new value lazily.

Skip caching parses (tree-sitter incremental + rowan sharing already handle that). One query, one boundary, only if ≥3 independent consumers read lowered IR for the same file with overlapping but different cancellation needs.

Today only the LSP session uses lowered output. `OnceCell<Vec<Vec<LoweredOp>>>` in `v3/crates/server/src/session.rs:59` is the salsa-equivalent for one file, and it works. Don't add Salsa.
