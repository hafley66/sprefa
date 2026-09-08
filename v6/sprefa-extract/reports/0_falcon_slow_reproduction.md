# Falcon `extract slow` reproduction

Date: 2026-09-08

## 0. Executable

The tested binary was `/private/tmp/sprefa-extract-slow-telemetry-target/debug/extract`.

```text
extract 0.1.0
git hash: 7592bb5db0ab
datetime: 2026-09-08T13:31:06Z
rust-analyzer 1.100.0-nightly (17fd5b8a3 2026-08-28)
```

## 1. Original command

Run from `/Users/chrishafley/projects/hafley-rs` with all generated products
redirected below `/private/tmp/extract-falcon-repro.inFqyO`:

```sh
CARGO_BUILD_JOBS=2 \
CARGO_TARGET_DIR=/private/tmp/extract-falcon-repro.inFqyO/cargo-target \
HAFLEY_LOG_FORMAT=json \
DL_TRAIL=0 \
/private/tmp/sprefa-extract-slow-telemetry-target/debug/extract slow \
  --scip-timeout 45 \
  --scip-cache /private/tmp/extract-falcon-repro.inFqyO/scip-cache \
  games/blender-godot-sqlite-proof/falcon-lab
```

Result: CLI exit 0 in 0.63 seconds, one `scip_skip` JSONL row, and one JSON
warning event on stderr. The indexer exit status was 1 and its measured duration
was 570 ms. Exit 0 is the existing `slow` contract for a named skip.

Bounded skip detail:

```text
command ["rust-analyzer", "scip", ".", "--output", ".../index.scip"] exited with code 1
error: failed to load manifest for dependency `core-labs`
failed to read `/private/var/.../T/core-labs/Cargo.toml`
No such file or directory (os error 2)
```

Telemetry included `process.command=rust-analyzer`, the full arguments, staged
cwd, pid, `process.status=exited with code 1`, `duration_ms=570`, and the
bounded stderr evidence. The formerly reported `8: __pthread_joiner_wake` is
the last backtrace frame, after the Cargo metadata error.

## 2. Staged-input comparison

The persistent staged root remained at:

```text
/private/var/folders/z2/cwfm40fn65n176q8m227wl0r0000gn/T/sprefa-scip-stage-rust-analyzer-978048ede4a04714
```

Its copied `Cargo.toml` contains these dependencies from the source manifest:

```toml
falcon-simulation = { path = "../simulation-core" }
core-labs = { path = "../core-labs" }
```

Neither sibling manifest exists beside the staged root. A direct bounded run
over those same staged inputs exited 1 in 0.36 seconds with the same missing
`core-labs/Cargo.toml` error.

## 3. Source-tree comparison

A direct run over the unchanged Falcon source root used a separate scratch
`CARGO_TARGET_DIR` and a 60-second process-group timeout:

```sh
CARGO_BUILD_JOBS=2 \
CARGO_TARGET_DIR=/private/tmp/extract-falcon-repro.inFqyO/direct-cargo-target \
timeout --kill-after=5s 60s \
rust-analyzer scip . \
  --output /private/tmp/extract-falcon-repro.inFqyO/direct-source.scip
```

Result: exit 0 in 33.15 seconds. The generated SCIP index is 800,997 bytes.
Loading it through `extract --scip-facts` exited 0 in 0.38 seconds and emitted
10,675 semantic index rows, 3,404,344 JSONL bytes, with empty stderr.

## 4. Evidenced cause and boundary

`Staging::Always` copies Rust sources, `Cargo.toml`, and `Cargo.lock` only from
below the requested root. Falcon is a deeply nested monorepo crate whose
manifest depends on sibling crates. Relocating that crate to a top-level temp
directory changes the meaning of `../simulation-core` and `../core-labs`.
Cargo metadata fails before rust-analyzer indexes the project.

The source-tree comparison excludes the Falcon source, rust-analyzer binary,
45-second budget, and sandbox as causes of this failure. The failing and
succeeding runs used the same rust-analyzer executable and scratch target
policy. The runtime fix preserves the containing Git worktree's
repository-relative layout when a nested Cargo project reaches an ancestor
workspace or uses a parent-relative manifest path. Soopy supplies the worktree
snapshot and verified reads. Rust-analyzer still runs in the persistent scratch
stage with a redirected Cargo target, so it does not write into the source
checkout.

## 5. Fixed-command verification

The fixed binary identifies commit `044f71bfe010`. The original command was
repeated from the same hafley-rs root with fresh products under
`/private/tmp/extract-falcon-fixed.T4YqfZ` and the same 45-second SCIP timeout.

First run:

```text
CLI status:                 0
wall time:                  31.33 seconds
rust-analyzer status:       0
rust-analyzer duration:     29.258 seconds
semantic JSONL rows:        2,382
SCIP documents:             17
scip_skip rows:             0
```

The row kinds were 1 `scip_index`, 260 `scip_def`, 446 `scip_ref`, 71
`scip_edge`, 669 `scip_fn_edge`, 36 `scip_callee_type`, 640 `scip_local`, and
259 `scip_name`. Structured stderr records the indexer command,
repository-shaped staged cwd, exit code 0, and duration. The source subtree's
Git status and file checksums were identical before and after the run.

The same command and cache were then repeated. It exited 0 in 0.04 seconds,
emitted the same 2,382 semantic rows, and did not spawn the indexer. The decoded
rows differ only in the expected `scip_index.reused` field, which changes from
`false` to `true`.

## 6. Scratch receipts

The uncommitted raw receipts remain in `/private/tmp/extract-falcon-repro.inFqyO`:

```text
stdout.jsonl                         original extract JSONL
stderr.jsonl                         original structured telemetry and stderr evidence
elapsed.txt / status.txt             original timing and exit status
direct-stage.stderr.txt              direct staged-input failure
direct-stage.elapsed.txt             0.36-second staged comparison
direct-source.stderr.txt             direct source-tree indexer log
direct-source.scip                   successful 800,997-byte index
direct-source.elapsed.txt            33.15-second source comparison
scip-facts.stdout.jsonl              10,675 decoded semantic rows
scip-facts.stderr.txt                empty successful decoder stderr
```

The fixed-run receipts remain in `/private/tmp/extract-falcon-fixed.T4YqfZ`:

```text
run1.stdout.jsonl / run1.stderr.jsonl  first fixed semantic output and telemetry
run1.status / run1.time                 first fixed exit status and timing
run2.stdout.jsonl / run2.stderr.jsonl  cached semantic output and CLI stderr
run2.status / run2.time                 cached exit status and timing
source-before.txt / source-after.txt    scoped Git status comparison
source-before.sha1 / source-after.sha1  source subtree byte comparison
scip-cache/index.scip                   reusable fixed-run SCIP index
```
