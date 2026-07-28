# PERF TRACING BUY VERDICT (fills SLOT-LIB of 2026-07-27-v5-port-perf-header.md)

Build-vs-buy analysis (sonnet research agent, 2026-07-27 late) for the tracing
layer the v5-port arc installs first. Full candidate-by-candidate reasoning was
returned in-session; this file is the durable distillation. AWAITING USER WORD
on the one dependency it adds (pino) before P0 dispatch.

## Candidates

| candidate | role verdict | key facts |
|---|---|---|
| node:diagnostics_channel `tracingChannel` | **PRIMARY spine** | builtin, stable in 24.x; near-free publish when unsubscribed; `context` object carries {tick, rel, kind} natively; channel≈subscriber, tracePromise≈span, publish≈event -- the closest structural analog to the Rust `tracing` crate, zero deps |
| pino | **emit layer** | JSONL is its native output; child loggers bind per-tick context; v10.3.1 (~6mo), 26-40M/wk, 4 maintainers; the ONE new dependency |
| node:perf_hooks | innermost primitive only | performance.now() deltas / RecordableHistogram.record() are the cheapest duration capture; no span tree, no export format -- feeds the spine, is not the spine |
| OpenTelemetry JS (api + sdk-trace-node) | escalation path, not rejected | custom SpanExporter gives collector-free JSONL; heavier object model; becomes right the day the standardized tick-log needs a schema rust/python runners also target (item 9); call sites stay identical, only the subscriber/emit layer swaps |
| tinybench | bench harness | zero-friction (ships inside Vitest ecosystem, ~49M/wk); for the bench/org sweep side, never in-app |
| mitata | bench harness, sharper | JIT-deopt detection when a hot-path number must be trusted vs the v5 yardstick; v1.0.34 ~1yr old |
| node --cpu-prof | ad-hoc profiler default | builtin, .cpuprofile opens in DevTools; 0x as flamegraph fallback; clinic.js skipped (core package ~3yr stale) |

## Why this pairing

- Every timing call site (SqlRunner.execute, HostRunner.runEffectOnce,
  ingestFile) already receives tick/rel context as explicit arguments;
  SqlRunner already has a `trace?: TraceStatement` hook. So NO
  AsyncLocalStorage (benchmarks put ALS at ~4-12% depending on workload) and
  NO new rxjs: subscriber + explicit callbacks only.
- Unsubscribed channels are near-free, so tracing can stay resident in
  production paths and be enabled by attaching a subscriber.

## Integration shape (P0 contract addendum)

One module (suggested `v6/dl/src/0_trace.ts`, interface in 0_types.ts per the
header law): declares the channels (`sprefa:sql`, `sprefa:effect`,
`sprefa:ingest`), one pino destination, one subscriber that aggregates
statement events per tick and emits ONE JSONL line per tick
`{tick, wall_ms, stmt_count, stmt_ms_total, stmt_ms_max, effects, ingest,
rss_kb}` (rss from the existing memcap sampler). Seam edits are argument
passing into the existing trace hooks, not new operator chains.

## Open

- USER WORD: buy pino (only new dep)? Alternative: hand `fs.appendFile` JSONL
  writer, zero deps, loses worker-thread transport + serializer speed.
- Overhead receipt required by the header: instrumented vs uninstrumented
  ingest_corpus within 5%.
