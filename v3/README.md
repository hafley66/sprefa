# sprefa v3

Top-level for sprefa v3 work. Everything here is pre-stable; the v2
crate in the sibling `v2/` directory remains the running system.

## Layout

```
v3/
├── Cargo.toml                           workspace root
├── crates/
│   └── effect_runtime/                  framework: EffectKind + PureEffect,
│                                        Batcher (CancellationToken),
│                                        RtCtx (ArcSwap registry,
│                                        rebind, invalidate_domain,
│                                        cancel_all), five batchers
│                                        incl. CacheLayer, core telemetry
├── experiments/
│   ├── effect_proof/                    consumer: demo effects + 3 benches
│   │                                    (ast-grep parity, sqlite batched
│   │                                    inserts, git blob walk) + tests
│   └── parse_experiment/                earlier parse-direction probe
│                                        (own inner workspace)
└── docs/
    ├── FINDINGS.md                      teaching doc: perf levers, topology
    │                                    taxonomy, plugin perf affirmation,
    │                                    sqlite write leg, git bench,
    │                                    design-doc join, followups
    ├── PRIOR_ART.md                     ten-project survey of typed-
    │                                    effect-dispatcher libs with
    │                                    shape-match matrix
    ├── v3-plugin-author-surface.md      Phase A/B/C/D row table for the
    │                                    op-author lifecycle
    ├── v3-min-author-ops.md             target metric + validation
    ├── v3-vs-v2-reading-preview.md      what a v3 op reads like vs v2
    └── convergent-evolution-effect-dispatcher.md
                                         Haxl / redux-saga / tower / v3
                                         unification
```

## Quick tour

1. `docs/FINDINGS.md` — the measurements and the rules they support.
   Start here.
2. `docs/PRIOR_ART.md` — survey of the Rust + adjacent ecosystem,
   ranked by shape-match.
3. `crates/effect_runtime/src/lib.rs` — framework, ~240 LoC.
4. `crates/effect_runtime/src/batchers/` — four topologies +
   `CacheLayer` (moka-backed, keyed on `PureEffect::Key`, domain
   invalidation via `ctx.invalidate_domain(d)`).
5. `experiments/effect_proof/src/bin/` — four benches that run
   against `v2/tests/smoke/.fixtures/linux` (+ an in-memory cache A/B
   harness) and report per-effect throughput via core telemetry.

## Build and test

```bash
cd v3
cargo test                    # 10 tests green
cargo build --release         # builds three bench binaries
./target/release/ast_grep_v3_bench --root ../v2/tests/smoke/.fixtures/linux \
    --workers 8 --trials 3 --pattern 'printk($$$)' --lang c --mode batch
./target/release/sqlite_v3_bench --root ../v2/tests/smoke/.fixtures/linux \
    --workers 8 --trials 3 --chunk 256 --cap 16 --max-batch 8
./target/release/git_tree_bench --repo ../v2/tests/smoke/.fixtures/linux \
    --trials 3 --needle printk
```

Or source the helpers and drive by name:

```bash
source v3/experiments/effect_proof/helpers.bash
_.sprfv3.bench.help                         # list all functions
_.sprfv3.bench.build && _.sprfv3.bench.build-probe
_.sprfv3.bench.probe-no-prefilter 8 3       # expose the 6x lever
_.sprfv3.bench.head-to-head-ast-grep 8      # probe vs ctx.put, 5 trials
_.sprfv3.bench.three-domains                # ast-grep + sqlite + git
```

Each bench prints a per-effect table from `ctx.telemetry().summary()`:

```
effect                  count       p50       p95       p99     mean    total_MB    wall   MB/s_wall
ScanBatch                   1     3.75s     3.75s     3.75s    3.75s    1342.2 MB   3.75s     358.4
```

## Status

- Framework surface: `EffectKind`, `PureEffect`, `Batcher<E>` (with
  `CancellationToken`), `RtCtx`/`RtCtxBuilder`, five batchers
  (`Passthrough`, `WorkSteal`, `BoundedWorkSteal`, `BoundedBatched`,
  `CacheLayer`), telemetry.
- Four benches: ast-grep batch/per-file (3.69s / 4.17s), sqlite
  extract+insert (2.07s / 746k rows/s), git walk (git2 4.13s vs
  shell-out 1.92s ≈ 2.14×), cache A/B (~4.3× warm-pass speedup).
- 19 tests green. Cancellation, ArcSwap-backed handler rebinding,
  domain-bucketed cache invalidation, and jemalloc pinning landed
  against the same measured perf numbers.

## Crate boundary intent

`effect_runtime` stays neutral: tokio + rayon + crossbeam, nothing
else. Anything domain-specific (ast-grep, sqlite, git, regex, specific
format) lives in the consumer (`effect_proof` today, `sprefa-v3`
later).

Cancellation is decided (token on `Batcher::run`, root token on `RtCtx`
with `cancel_all`). Once sprefa v3 exercises the surface end-to-end,
`effect_runtime` is a cratesio publication candidate. Names floated:
`hopp`, `taxon`, `fable`, `kit`, `effect-dispatch`.

## Related

- `v2/examples/throughput_probe_v2.rs` — the perf probe that produced
  the baseline numbers referenced throughout `FINDINGS.md`.
- `chat_log/20260420.0.v3-perf-plugin-synthesis-and-library-split.md`
  — session notes for the work that landed this directory.
