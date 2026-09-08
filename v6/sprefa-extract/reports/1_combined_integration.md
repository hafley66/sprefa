# Combined extract integration

Date: 2026-09-08

## 0. Inputs

The integration branch starts from `d83d9cc91`, the telemetry and Falcon
topology branch. It incorporates the seven-file primary-worktree snapshot at:

```text
/private/tmp/extract-combined.HLDj6H/0_existing_extract.patch
sha256 0b0707be1ae21a1c979d36d729c18008d794723a4f89fc486f25c3fe409564d3
```

Commit `7d6272bbc` records the integration. The one textual conflict retained
repository-shaped staging and the extended root-only Rust staging list,
including `Cargo.lock`, toolchain files, and Cargo config. The captured source
patch still has the same SHA-256 after integration.

The integrated behavior preserves:

- readable source-tree results when a compiler index names a missing generated
  file;
- Rust module re-export references without attributing them as executable calls;
- real calls after comments containing `use`;
- workspace lockfile copy and stale-lockfile pruning;
- nested Cargo workspace, sibling, and transitive path-dependency topology.

Commit `5eca5163a` moves the new re-export case into scratch test input. This
keeps the fixture corpus captured at `946460d75` byte-identical while exercising
the same real rust-analyzer distinction.

## 1. Tests

The focused combined command passed 60 tests covering source-tree reads, SCIP
projection and real indexers, staging, freshness, tracing, and CLI identity.
The exact re-export regression and the `946460d75` wire golden also pass after
isolating the re-export fixture.

Two full `cargo test --features cli` attempts were used, matching the requested
limit. The first stopped at an existing timing threshold in
`barrel_resolve_wall_grows_linearly_with_file_count`; its immediate exact rerun
passed. The second passed that timing test and later exposed the shared-fixture
wire-golden drift. The drift is fixed and both directly affected tests pass.
The full gate has not run to completion after that correction because the two-run
limit is exhausted.

## 2. Real Falcon command

The combined binary identifies commit `5eca5163accc`. From the hafley-rs root,
the original `extract slow --scip-timeout 45` command used fresh cache, Cargo
target, and output directories below
`/private/tmp/extract-falcon-combined.zqc7Wd`.

```text
CLI status:                 0
wall time:                  31.13 seconds
rust-analyzer status:       0
rust-analyzer duration:     29.013 seconds
semantic JSONL rows:        2,583
SCIP documents:             18
scip_skip rows:             0
```

The same command and cache exited 0 in 0.10 seconds and emitted the same rows,
apart from the expected `scip_index.reused` change from `false` to `true`.

The Falcon subtree's file checksum inventory is byte-identical before and after
both runs. Its Git status changed concurrently from three modified paths to
clean, while the before and after bytes remained identical; the byte receipt is
the extraction nonmutation evidence.

## 3. Receipts

```text
/private/tmp/extract-falcon-combined.zqc7Wd/run1.stdout.jsonl
/private/tmp/extract-falcon-combined.zqc7Wd/run1.stderr.jsonl
/private/tmp/extract-falcon-combined.zqc7Wd/run1.status
/private/tmp/extract-falcon-combined.zqc7Wd/run1.time
/private/tmp/extract-falcon-combined.zqc7Wd/run2.stdout.jsonl
/private/tmp/extract-falcon-combined.zqc7Wd/run2.stderr.jsonl
/private/tmp/extract-falcon-combined.zqc7Wd/run2.status
/private/tmp/extract-falcon-combined.zqc7Wd/run2.time
/private/tmp/extract-falcon-combined.zqc7Wd/source-before.sha1
/private/tmp/extract-falcon-combined.zqc7Wd/source-after.sha1
```
