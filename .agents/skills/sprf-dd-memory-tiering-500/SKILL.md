---
name: sprf-dd-memory-tiering-500
description: DD memory strategy for sprefa at 500-repo polyglot scale. Four strategies (DD-only metadata, intern strings, custom Trace, tiered). Compaction policy. Pinned perf test. Load when reasoning about RAM budget, SQLite spill, or before adopting DD at full string-literal scale.
---

# DD memory at 500-repo scale

## The constraint

Stock DD/timely is in-memory only. No automatic spill to disk. Compaction is the only memory-control mechanism. v0 hit ~1 GB raw strings DB; 500 polyglot repos is the scale target.

## Peak memory pathology

```
100M rows written, never compacted               ~ 30 GB
100M rows written, compacted every 100 gens       ~ 3 GB
100M rows written, compacted every gen            ~ 1.5 GB
```

Compaction every gen has CPU + locking cost. Every-N is the sweet spot. Tune N empirically.

## Four strategies, ordered by cost

**1. DON'T put raw strings in DD.** DD holds metadata only: `(FileId, LineRange, ContentHash)`. Strings live in SQLite cold + trigram index + LRU rowan tree cache. Joins in DD are over IDs/hashes, never bytes. When the actual content is needed, `ctx.put(ReadContent(FileId))`.

```
DD trace memory:        ~200–500 MB for 500 repos
loses:                  retraction-aware string queries
```

**2. INTERN strings, pass `Arc<str>`.** Global interner. Every `(path, line, text)` row holds 16-byte Arc handle regardless of length.

```
100M references × 16 bytes = 1.6 GB pointers
+ unique strings ~5–20 GB depending on dedup ratio
total: 7–25 GB. doable on 32 GB box. tight on 16.
no SQLite needed. pure DD.
```

**3. SQLITE-BACKED TRACE.** Custom Trace implementation that pages to SQLite. Stock DD doesn't ship this. Materialize has `mz_persist` for essentially this purpose; vendoring is heavy.

```
DD memory:    working-set arrangements (~hundreds MB)
total state:  terabytes possible
cost:         ~2000 LoC custom trace + cache + eviction-correctness testing
```

**4. TIERED (recommended for sprefa).** Combine: DD holds the relational graph (facts, rules, joins, antijoins). Row content is metadata + Arc<str> ID for string fields. String content lives in SQLite (canonical) + LRU rowan cache (hot). Bulk grep that doesn't need retraction goes through trigram index, bypasses DD.

```
DD trace               1–3 GB
trigram index          1–3 GB     (~25–40% of source)
string cache (LRU)     500 MB
SQLite buffer pool     1 GB
                       ────
total live RSS         ~5–8 GB     for 500 repos / 50 GB src
```

## The rule

**Put in DD what NEEDS retraction propagation:**
- symbol declarations
- references
- note assertions / invariants
- computed rules

**Keep OUT of DD what doesn't:**
- raw source bytes
- full string content
- trigram index data

The seam: DD rows carry IDs (`Arc<str>` intern handle, FileId, ContentHash). Effects fetch content on demand via `PureEffect` cache.

## Compaction policy

```
set_logical_compaction(now() - K_GENS) every M gens.
```

| Param   | Controls | Starting value | Notes |
|---------|----------|----------------|-------|
| K_GENS  | how far back queries can ask | 5 | K=1 fine for LSP-current-state |
| M       | compaction overhead          | 10 | M=1 wasteful for big traces |

Tune under typing-burst load. RSS target drives N.

## The pinned perf test (gates DD adoption at this scale)

Test plan (5 phases):

1. **Bulk cold load.** Walk + parse + extract + push to InputSession. Measure: wall clock, peak rayon RSS, rows/sec into DD.
2. **DD trace at rest.** After advance_to(1) seal. Measure: trace RSS, arrangement count, per-fact bag sizes.
3. **Compaction sweep.** Call set_logical_compaction at K=1. Measure: compaction wall clock, post-compaction RSS, retained tuple count.
4. **Synthetic edit churn.** Inject 1k random file-deltas / sec for 60s. Measure: per-gen latency p50/p95/p99, RSS over time, effect-dispatch rate.
5. **SQLite tier comparison.** Re-run with strategy 4 (DD + IDs, SQLite content) vs strategy 2 (Arc<str> intern). Compare RSS + cold-start replay time.

Harness:
- criterion for per-op micro-benches
- hyperfine for end-to-end CLI A/B
- custom RSS sampler for memory-over-time
- jemalloc pinned per perf:biome-benchmarks

What this answers:
- Does DD's in-memory trace fit on 16 GB / 32 GB for 500 repos at full string density?
- What is realistic peak RSS without compaction?
- Does SQLite tier (strategy 4) buy meaningful headroom or just complexity?
- Does ast-grep parse throughput stay matched to InputSession push rate (no backpressure stalls)?
- What is the per-edit p99 latency when the trace is at full size vs empty?

## Decision flow

```
small project (≤50 repos, ≤5 GB src)
    → strategy 2 (intern + Arc<str>). Easiest. ~3–5 GB RSS.

medium (50–500 repos, 5–50 GB src)
    → strategy 4 (tiered). ~5–10 GB RSS.

large (500+ repos or >50 GB src)
    → run perf test. If strategy 4 RSS exceeds budget, strategy 3.
       Strategy 3 is a major commitment. Don't pre-emptively adopt.
```

## What this is NOT

- Not a final answer. Numbers are anchors from comparable systems (zoekt, livegrep, Materialize). Real numbers come from integration + benchmark.
- Not a substitute for measuring. v0 had 1 GB strings DB; if DD adoption changes that ratio, the strategy may shift.

## Sources

- chat_log/20260501.0.dd-mental-model-walkthrough.md (compaction strata section)
- chat_log/20260501.1.dd-effects-control-flow-types.md (memory tiering + perf test)
- ref-v0-goals.md item 6 (SQLite + DD scaling with strings/refs table)
- DD/timely 0.12 source: Trace, set_logical_compaction, set_physical_compaction
