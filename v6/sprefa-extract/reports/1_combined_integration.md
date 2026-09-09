# Combined extract integration

Date: 2026-09-08

## 0. Inputs and commits

The integration branch starts from `d83d9cc91`, the telemetry and Falcon
topology branch. It incorporates the seven-file primary-worktree snapshot at:

```text
/private/tmp/extract-combined.HLDj6H/0_existing_extract.patch
sha256 0b0707be1ae21a1c979d36d729c18008d794723a4f89fc486f25c3fe409564d3
```

Commit `7d6272bbc` records that integration. Commit `5eca5163a` moves the Rust
re-export case into scratch test input, keeping the fixture corpus captured at
`946460d75` byte-identical. Commit `c8865c730` fixes the only failure found by
the final complete gate: `--ingest /dev/stdin` now bypasses ordinary-file
existence probing while all source-file modes retain it.

The final integration preserves:

- structured indexer telemetry, bounded stderr, exit status, duration, command,
  arguments, PID, and staged working directory;
- Falcon's containing-repository layout, including ancestor workspace,
  sibling, and transitive Cargo path dependencies;
- root and workspace `Cargo.lock`, Cargo config, and toolchain staging, plus
  stale-lockfile pruning;
- readable source-tree results when a compiler index names a missing generated
  file;
- Rust module re-export references without attributing them as executable
  calls;
- executable calls after comments containing `use`.

No compiler or kernel behavior changed. The required formatter rewrote layout
only in `scip.rs` and `scip_ensure.rs`.

## 1. Complete gate

The final command ran from `v6/sprefa-extract` with one lane-dedicated target:

```sh
CARGO_TARGET_DIR=/private/tmp/extract-final-gate.ODHlZ8/cargo-target \
  cargo test --features cli
```

Final committed-tree result on `c8865c730`:

```text
status:  0
passed:  778
failed:  0
ignored: 14
wall:    152.84 seconds
```

Complete attempt 1 reached `tests/97_ingest.rs` and failed one of its 17
parallel cases because `/dev/stdin` transiently returned false from the
ordinary-file existence probe. The other 16 cases passed. The exact focused
test then passed 30 consecutive pre-fix reruns, identifying a concurrency
flake rather than an ingest validation error.

After the path fix, focused validation passed 5 resolve-door tests, 17 ingest
tests, and 12 syntax/TSI tests. Complete attempt 2 passed before the required
pre-commit formatter run. Complete attempt 3 passed on the committed and
formatted source tree. One allowed complete attempt remains unused.

Gate receipts:

```text
/private/tmp/extract-final-gate.ODHlZ8/gate1.stdout
/private/tmp/extract-final-gate.ODHlZ8/gate1.stderr
/private/tmp/extract-final-gate.ODHlZ8/gate1.status
/private/tmp/extract-final-gate.ODHlZ8/gate2.stdout
/private/tmp/extract-final-gate.ODHlZ8/gate2.stderr
/private/tmp/extract-final-gate.ODHlZ8/gate2.status
/private/tmp/extract-final-gate.ODHlZ8/gate3.stdout
/private/tmp/extract-final-gate.ODHlZ8/gate3.stderr
/private/tmp/extract-final-gate.ODHlZ8/gate3.status
```

The final stdout and stderr SHA-256 values are:

```text
7ab5282e27947d2a39f455ca1003aa05e8eec1200ce68968e7bd023bd388afd9  gate3.stdout
9deaf4979f836f88712ce4cbf9902867a64a8a0071298772d41b42294a5fa63e  gate3.stderr
```

## 2. CI coverage

The combined integration adds these semantic checks:

- `exact_file_source_skips_missing_generated_paths` keeps readable files from
  an exact source-tree batch when a generated path is absent;
- `rust_staging_copies_and_prunes_the_workspace_lockfile` checks both lockfile
  copy and stale-lockfile removal;
- `rust_indexer_preserves_the_locked_dependency_graph` pins the Rust staging
  policy and its `Cargo.toml` and `Cargo.lock` root set.

`the_discrimination_holds_through_rust_analyzer_too` now builds a scratch Rust
fixture that checks both halves of call filtering: a module-level `pub use`
does not become a call from the preceding function, and a real `helper()` call
after a comment containing `use` remains present.

Existing CI tests retain coverage for nested workspace and transitive path
dependency topology, structured success and failure telemetry, bounded UTF-8
stderr, signal termination, timeout behavior, and stale stage pruning. The
stdin fix changes no test count because the existing 17-case ingest binary and
the syntax/TSI ingestion test already exercise `/dev/stdin`. No CI test or
pipeline configuration was removed.

## 3. Real Falcon command

The tested executable and indexer identify as:

```text
extract 0.1.0
git hash: c8865c730393
datetime: 2026-09-08T16:54:24Z
rust-analyzer 1.100.0-nightly (17fd5b8a3 2026-08-28)
```

The original command ran from `/Users/chrishafley/projects/hafley-rs` with
fresh cache, Cargo target, and output products under
`/private/tmp/extract-falcon-final.dKsvIj`:

```sh
CARGO_BUILD_JOBS=2 \
CARGO_TARGET_DIR=/private/tmp/extract-falcon-final.dKsvIj/cargo-target \
HAFLEY_LOG_FORMAT=json \
DL_TRAIL=0 \
/private/tmp/extract-final-gate.ODHlZ8/cargo-target/debug/extract slow \
  --scip-timeout 45 \
  --scip-cache /private/tmp/extract-falcon-final.dKsvIj/scip-cache \
  games/blender-godot-sqlite-proof/falcon-lab
```

Cold result:

```text
CLI status:                 0
wall time:                  31.59 seconds
semantic JSONL rows:        2,583
SCIP documents:             18
scip_skip rows:             0
SCIP index bytes:           853,222
```

Row counts were 1 `scip_index`, 291 `scip_def`, 474 `scip_ref`, 80
`scip_edge`, 738 `scip_fn_edge`, 39 `scip_callee_type`, 670 `scip_local`,
and 290 `scip_name`.

The same command and cache exited 0 in 0.13 seconds and emitted the same 2,583
rows. Normalized output is byte-identical after changing only
`scip_index.reused` from `false` to `true`.

The exact command inherited no `RUST_LOG`, so its successful cold run emitted
only the index-location line on stderr. A second fresh-cache probe added
`RUST_LOG=sprefa_extract=debug` and retained every other argument. It exited 0
in 13.52 seconds, emitted the same semantic bytes, and recorded the structured
indexer span with status 0, duration 11,556 ms, PID, arguments, timeout, and the
repository-shaped staged working directory.

The Falcon subtree had clean Git status before and after all runs. SHA-1
inventories over 16,256 files are byte-identical:

```text
inventory-file sha256 2cae1c4980321e7cb585ff80be116378fd4799ec68ea635223a320d041a1dbbc
```

## 4. Falcon receipts

```text
/private/tmp/extract-falcon-final.dKsvIj/build-provenance.txt
/private/tmp/extract-falcon-final.dKsvIj/run1.stdout.jsonl
/private/tmp/extract-falcon-final.dKsvIj/run1.stderr.jsonl
/private/tmp/extract-falcon-final.dKsvIj/run1.status
/private/tmp/extract-falcon-final.dKsvIj/run2.stdout.jsonl
/private/tmp/extract-falcon-final.dKsvIj/run2.stderr.jsonl
/private/tmp/extract-falcon-final.dKsvIj/run2.status
/private/tmp/extract-falcon-final.dKsvIj/telemetry.stdout.jsonl
/private/tmp/extract-falcon-final.dKsvIj/telemetry.stderr.jsonl
/private/tmp/extract-falcon-final.dKsvIj/telemetry.status
/private/tmp/extract-falcon-final.dKsvIj/source-before.sha1
/private/tmp/extract-falcon-final.dKsvIj/source-after.sha1
/private/tmp/extract-falcon-final.dKsvIj/source-final.sha1
/private/tmp/extract-falcon-final.dKsvIj/source-before.status
/private/tmp/extract-falcon-final.dKsvIj/source-after.status
/private/tmp/extract-falcon-final.dKsvIj/source-final.status
```

There are no remaining blockers.
