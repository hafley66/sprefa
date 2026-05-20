# v4 worst-of audit — 2026-05-19

Five-agent architecture review of `v4/src`, `v4/crates/*`, `v3/crates/effect_runtime`.
333 raw findings → 17 worst items. Worst = silent wrong answer, production panic,
or invariant breach that compiles. No fixes here; see paired plan files.

## Tier 0 — silent wrong answer, already breaking

1. **Parallel `op_cursor_binds` registries actively disagree.**
   `compile/binding_graph.rs:1292` hardcodes ast → `["FILE","LO","HI"]`.
   `compile/lower/ops.rs:858` declares ast → `["LO","HI"]`.
   Binding-graph and lowering walk different cursor schemas.

2. **`insert_batch` off-by-N false dirty publish.**
   `v3/effect_runtime/src/v2/fact_store.rs:1118-1156`. Multi-row `INSERT OR IGNORE`
   counts `changed > 0` then extends `accepted` with ALL ids in the chunk.
   One real insert in a 100-row chunk publishes 99 phantom dirties.

3. **MemoSeam freezes a wrong git root forever.**
   `app.rs:1620` admits the bug in comment form. `OnceLock<SourceIndex>` is sealed by
   the first `probe` whose `hint` may be `.` (daemon CWD). Wrong root for the
   process lifetime; every memo dependency keyed off it.

4. **Cursor codec silently re-injects a missing FOCAL term on decode.**
   `cursor_codec.rs:130-132`. Corrupted blob → "successful" decode with synthetic
   focal. Distinguishable from a real focal only by content.

5. **Two recursion detectors that can disagree.**
   `stratify.rs:199` Tarjan SCC over string-keyed deps. `fuser.rs:689` substring
   match `joined_tables.contains(self_facts)`. Both decide "is this rule
   recursive?" Run independently in `app.rs:1771`. Names are `String` on both sides.

## Tier 1 — production panic surface

6. **`Any`-downcast dispatch panics across effect_runtime.**
   `v3/effect_runtime/src/lib.rs:112, 230, 329`. `Box<dyn Any>` keyed by
   `TypeId::of::<E>()`. `expand.rs:114` same shape for `memo_seam`. One bad
   registration path turns silent miswire into runtime panic.

7. **Sqlite queue decode panics on partial / drifted rows.**
   `v3/effect_runtime/src/v2/sqlite_queue.rs:377` `unreachable!("unknown wake_kind")`.
   `:383` `copy_from_slice(&wake_key.unwrap())` from a possibly-non-32-byte blob.
   `codec.rs:25` `from_utf8(...).unwrap()`. Panics mid-pull while holding the
   connection mutex.

8. **`runtime_bridge.rs:29` `Codec::decode` `expect("valid cursor codec bytes")`**
   at the queue boundary. Sqlite/network corruption → daemon crash.

9. **`locate.rs:193` panics on unknown `PathItem` kind.**
   Two separate `clone_box` registries (`path.rs:77`, `locate.rs:187`) that must
   drift together. LSP hover reaches this.

10. **`bounded_batched.rs:68-72` worker-thread `assert_eq!(outs.len(), replies.len())`**
    kills the worker. Every pending oneshot reply silently dropped after.

## Tier 1 — concurrency invariants prose-only

11. **EventBus self-dispatch deadlocks.**
    `v3/effect_runtime/src/v2/event_bus.rs:66`. Mutex is non-reentrant
    `std::sync::Mutex`. A listener calling `dispatch` re-enters and hangs.

12. **Generation RMW races, two locations.**
    `v4/src/source_clock.rs:115-135` reads `cold_gen` under hot lock, drops it,
    calls `persist`. `v3/effect_runtime/src/v2/runtime_graph.rs:642-660`
    `mark_dirty` SELECT-then-INSERT. Both write last-write-wins.

13. **`Generation(pub u64)` is publicly constructible.**
    `v3/effect_runtime/src/generation.rs:19`. "Only `GenCounter::bump` produces these"
    is a comment.

14. **LSP path holds `std::sync::Mutex` across CPU-heavy sync work in `async fn`.**
    `app.rs:1443` `ingest` runs full `host_parse + walk + expand` synchronously
    inside async; five mutexes total on the request path with no documented
    ordering (`Backend × SprfState`). Doc state duplicated across two stores
    keyed differently (Url vs String) with no transactional close.

## Tier 2 — silent miswiring surface (structural)

15. **Every id family is a bare type alias, not a newtype.**
    `lib.rs:99-103`: `RepoId=u32, RevId=u32, FileId=u64, RefId=u64, PathId=u64,
    BlobId=u64`. Assignment across families compiles. Mirrored in
    `runtime_graph.rs:306-336` where owner/instance/source/edge ids are all `String`.

16. **Content-hash truncation creates real collision horizons.**
    `lib.rs:160` Ref/WhereBytesId to u64 (≈2^32 birthday over a 63k-file corpus).
    `store.rs:511, 559` RepoId/RevId to u32 (≈2^16). No detection on insert.
    `lib.rs:170` zero-prefix collision routes silently to `SYNTHETIC`.

17. **Magic term names embedded in cursors.**
    `mounted_query.rs:20` `SUPPORT_CURSOR_ID = "__support_cursor_id"`.
    `v3/effect_runtime/src/v2/effect_dispatch.rs:70` `:mutation_key`.
    `chan.rs:28` `:nextq:<chan>`. Any user op that writes those names corrupts the
    support graph or the channel queue. No reserved-namespace check.

## Cross-cutting amplifiers

- **Default trait impls that lie.** `Queue::cascade_delete=0`,
  `has_parked_domain=true`, `pending_summary_before_or_at=Default::default()`
  (`v3/effect_runtime/src/v2/queue.rs:122-188`); `DslBodyLsp::*=Vec::new()/None`
  (`cst/lsp/providers.rs:31`); `OperatorDef::lsp_binders_in_dsl=Vec::new()`
  (`lsp.rs:367`). Default value is the wrong answer; no introspection of "real impl".
- **Two `lsp_types` versions** glued by serde round-trip (`crates/sprefa-lsp/Cargo.toml:15,25`,
  `main.rs:404`). Schema drift drops items via `filter_map`, no diag.
- **Hand-rolled SQL tokenizers, three.** `sql.rs:50`, `app.rs:1076`,
  `cst/dsls/sql/mod.rs:410` + `sql_where:292`. None handle CTEs, subselects,
  `AS` aliases.

## What this audit deliberately ignores

Performance cliffs (O(n) LSP hot paths, unbounded caches, doc-text cloning per
request) and dead-code/cleanup nits. Those are real but separable; the items
above are correctness, panic, or silent-corruption.
