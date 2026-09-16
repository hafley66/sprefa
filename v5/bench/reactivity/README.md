# Reactivity micro-probe

This is deliberately small. It generates 10, 100, and 1,000 Rust files under
`target/reactivity/probe`, then drives the prebuilt Rust example directly.

The probe records cold, unchanged, one-file edit, and fresh rebuild time plus
physical parse counts and a canonical call-graph digest. It fails if the edit
silently widens to a full tick or differs from the rebuild.

```text
just perf-reactivity-build
just perf-reactivity
```

Each size is measured `warmup + repeats` times (default 1 warmup + 5
measured; override with `just perf-reactivity repeats=N warmup=M`) — a fresh
deterministic fixture (same seed, so every iteration's generated corpus is
byte-identical) and a fresh database per iteration, so nothing carries state
across repeats. The warmup iterations are discarded; `summary.json` under
each `files-<N>/` reports mean/stdev/min/max **milliseconds** per phase over
only the measured repeats, plus `deterministic_output` (every measured
iteration produced the same `rows`/`semantic_digest`) and
`equivalent_every_iteration` (the probe's own incremental-vs-rebuild check).
`raw.json` keeps every iteration's full result, warmup included, for
after-the-fact inspection. A single run is not a measurement — read the
`stdev_ms` before treating any `mean_ms` delta between two probe runs (e.g.
before/after a code change) as real; if the spread swamps the delta, it
isn't one.

The run command never builds, invokes `dl`, starts a daemon, or writes outside
this repository. Remove `target/reactivity/probe` explicitly before rerunning.
