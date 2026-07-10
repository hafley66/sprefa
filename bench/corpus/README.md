# Parity corpus: OpenTelemetry SDKs, pinned

Real-world medium repos, one per supported language, pinned as git submodules at a
release commit so parity numbers are reproducible. Opt-in: nothing clones unless you run

```sh
git submodule update --init bench/corpus/otel-rust   # or otel-js / otel-go / otel-python / otel-kotlin
```

| submodule | upstream | tag | commit |
| --- | --- | --- | --- |
| otel-rust | open-telemetry/opentelemetry-rust | v0.31.0 | 285dc925f98403ff426acc70968f104dc820d4f2 |
| otel-js | open-telemetry/opentelemetry-js | v2.9.0 | 40d67b7690a61bd9af0a4e5b5b9f4a14b11fc50e |
| otel-go | open-telemetry/opentelemetry-go | v1.44.0 | b62d92831b2dd142f5a0cc89c828270274196877 |
| otel-python | open-telemetry/opentelemetry-python | v1.43.0 | fcbbeb8e4eeb785519c7d2efbe367e8fd79dd0b3 |
| otel-kotlin | open-telemetry/opentelemetry-android | v1.5.1 | 8b457d2474f8f8af1f6dd4968d7b32f5e0e30736 |

## What gets measured

Two arms per language, both scored by the shared confirmed-positives-only scorer
(`tests/it/oracle_parity.rs`) against the language's real compiler index as truth:

1. **without scip** — dl's syntactic tier scans the corpus with no index. This is the
   headline recall number on real code.
2. **with scip** — same scan with `SPREFA_SCIP_INDEX` pointed at the truth index. This is
   the plumbing ceiling; the gap from 1.0 is importer/resolution loss, not tier weakness.

Precision assert (>= 0.95) holds in both arms. Every number is confirmed-positives-only:
sites the compiler index cannot confirm are excluded from the denominator, contradictions
land in the `wrong` bucket.

## Truth indexes

Written to `bench/corpus/.indexes/<lang>.scip` (gitignored), cached across runs. Delete to
force a re-index.

| lang | indexer | notes |
| --- | --- | --- |
| rust | `rust-analyzer scip . --output <abs>` | needs cargo metadata; proc-macro expansion makes the first run minutes-long |
| js/ts | `scip-typescript index --output <abs>` | run `npm ci` in the submodule first (workspaces resolve via node_modules) |
| go | `scip-go --output <abs>` | run `go mod download` first; binary at ~/go/bin |
| python | `scip-python index . --project-name otel --project-version 1.43.0 --output <abs>` | index IN PLACE (walks parent dirs); exits 0 on fatal errors — check the index has documents |
| kotlin | scip-java | requires a JDK; runtime-skips on this box |

## Running

Corpus tests are `#[ignore]`d (slow, network/toolchain-dependent):

```sh
cargo test --test it oracle_corpus -- --ignored --nocapture
```

Each test skips loudly when its submodule is uninitialized or its indexer is missing.
