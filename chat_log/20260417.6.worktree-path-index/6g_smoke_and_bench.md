# 6g — Smoke + benchmarks (Z3)

Prove the path in numbers. Runs after 6e wires the index in production.

## Acceptance gates

| scenario | current (pre-6) | target (post-6e) |
|---|---|---|
| swc cold `fs("*.rs")` first run, single rule | ~14000 ms | < 4000 ms |
| swc warm same rule, second run same session | ~14000 ms | < 100 ms |
| swc warm across daemon restart (index persisted) | n/a | < 200 ms |
| LSP reparse 1000× on swc rule | RSS flat | RSS flat, task count flat |
| sprefa self-repo g4 warm | ~134 ms | ~134 ms (no regression) |

Miss → must produce rows; hit → must not fork a process.

## Step 1 — Enable index in smoke config

The smoke script uses its own `.sprefa.toml`. Add runtime knobs so the
script runs with the index on:

```toml
# v2/tests/smoke/fixtures/sprefa.toml (or wherever smoke cfg lives)
[runtime]
worktree_cache_dir = "{TMP}/sprefa-wt"
path_index_db      = "{TMP}/sprefa-pi.sqlite"
```

`{TMP}` substituted by the script runner.

## Step 2 — Double-run smoke script

```bash
# v2/tests/smoke/_3_large_repo_timed.sh
set -euo pipefail
ROOT="$(mktemp -d)"
export SPREFA_STATE_DIR="$ROOT"
trap 'rm -rf "$ROOT"' EXIT

# Render cfg with $ROOT substituted.
sed "s|{TMP}|$ROOT|g" fixtures/sprefa.toml.tmpl > "$ROOT/sprefa.toml"

# Run 1 — cold
echo "=== cold ==="
time ./run_rules.sh swc 2>&1 | tee "$ROOT/cold.txt"

# Run 2 — warm (state preserved in $ROOT)
echo "=== warm ==="
time ./run_rules.sh swc 2>&1 | tee "$ROOT/warm.txt"

# Gates
cold_ms=$(grep -oP 'fs\s+\K\d+(?=ms)' "$ROOT/cold.txt" | head -1)
warm_ms=$(grep -oP 'fs\s+\K\d+(?=ms)' "$ROOT/warm.txt" | head -1)
: "${cold_ms:=999999}"; : "${warm_ms:=999999}"

[[ "$cold_ms" -lt 4000 ]] || { echo "COLD FAIL: ${cold_ms}ms"; exit 1; }
[[ "$warm_ms" -lt  100 ]] || { echo "WARM FAIL: ${warm_ms}ms"; exit 1; }
echo "PASS cold=${cold_ms}ms warm=${warm_ms}ms"
```

## Step 3 — Observability hooks

Print per-phase timings to stderr when `SPREFA_TIMING=1`. The hook lives
inside `GitBlobReader::files()`:

```rust
# v2/src/readers/_2_git.rs  inside the async block
use std::time::Instant;
let t0 = Instant::now();
# ... phase 1 ...
if hit { eprintln!("[timing] path_index hit {:?} repo={} rev={}", t0.elapsed(), repo_s, rev_s); }
# ... phase 2 ...
eprintln!("[timing] worktree.ensure {:?} + ls-tree {:?} + upsert {:?}",
          d_ensure, d_lstree, d_upsert);
```

Guard every line with `if std::env::var("SPREFA_TIMING").is_ok()` or use
`tracing` if already in deps. Do NOT add `tracing` as a new dep for this.

## Step 4 — Criterion bench

```rust
# v2/benches/worktree_path_index.rs
use criterion::{Criterion, criterion_group, criterion_main};
use std::sync::Arc;

fn bench_fs_cold_swc(c: &mut Criterion) {
    # Per-iter cost: fresh WT dir + fresh pi.sqlite, drop between iters.
    c.bench_function("fs_cold_swc", |b| {
        b.to_async(tokio_rt()).iter_custom(|iters| async move {
            let mut total = std::time::Duration::ZERO;
            for _ in 0..iters {
                let (reader, _td) = fresh_reader_with_index();
                let t0 = std::time::Instant::now();
                let _ = reader.files("swc", "HEAD", &pat("**/*.rs"))
                    .collect::<Vec<_>>().await;
                total += t0.elapsed();
            }
            total
        });
    });
}

fn bench_fs_warm_swc(c: &mut Criterion) {
    # Primed index + primed WT; measure phase 1 only.
    c.bench_function("fs_warm_swc", |b| {
        let (reader, _td) = fresh_reader_with_index();
        # Prime once.
        tokio_rt().block_on(async {
            let _ = reader.files("swc", "HEAD", &pat("**/*.rs"))
                .collect::<Vec<_>>().await;
        });
        b.to_async(tokio_rt()).iter(|| async {
            let _ = reader.files("swc", "HEAD", &pat("**/*.rs"))
                .collect::<Vec<_>>().await;
        });
    });
}

fn bench_fs_fallback_libgit2(c: &mut Criterion) {
    c.bench_function("fs_fallback_libgit2", |b| {
        let reader = fresh_reader_no_index();
        b.to_async(tokio_rt()).iter(|| async {
            let _ = reader.files("swc", "HEAD", &pat("**/*.rs"))
                .collect::<Vec<_>>().await;
        });
    });
}

criterion_group!(benches, bench_fs_cold_swc, bench_fs_warm_swc, bench_fs_fallback_libgit2);
criterion_main!(benches);

# Helper — requires SPREFA_BENCH_SWC env var pointing at a swc checkout.
fn fresh_reader_with_index() -> (Arc<GitBlobReader>, tempfile::TempDir) { ... }
fn fresh_reader_no_index() -> Arc<GitBlobReader> { ... }
```

Add `criterion` as a dev-dep if not already present. If it's not and the
rule is "no new deps," skip the criterion bench and rely on the shell
gates in Step 2.

## Step 5 — Disk budget spot-check

Post-bench, measure:

```bash
du -sh "$ROOT/sprefa-wt"        # expect < 80MB for swc single rev
du -sh "$ROOT/sprefa-pi.sqlite" # expect < 2MB for ~11k paths
```

Append to session commit body. Flag if >10× expected.

## Step 6 — LSP reparse stability

Re-run `tests/stress.rs` (the existing reparse harness) with the index on:

```
SPREFA_RUNTIME_PATH_INDEX=$(mktemp -d)/pi.sqlite \
SPREFA_RUNTIME_WORKTREE_DIR=$(mktemp -d)/wt \
cargo test -p v2 --test stress -- --nocapture
```

Assert:
- Task count stays flat across 1000 reparses.
- RSS stays within existing budget (same assertion as today).
- `has_rev` is true after first parse, remains true across reparses.

## Absolute stop conditions

- Bench requires a swc checkout the user doesn't have — gate behind
  `SPREFA_BENCH_SWC` env var; bench no-ops without it.
- Warm target (<100ms) missed: do NOT modify 6c/6d; record the number
  and flag. Budget for follow-up perf pass.
- Cold target (<4s) missed: likely `git ls-tree` is the bottleneck; note
  in commit and flag. Do not optimize in this task.
- Adding `tracing` or `criterion` if not already in deps — use plain
  `eprintln!` + shell timing instead.

## Commit body shape

```
perf(v2): worktree-backed path index — fs() cold 14s→Xs, warm 14s→Yms

Session 6: rev wildcard ban (6a), WorktreeProvisioner (6b),
PathIndex trait + sqlx impl (6c), GitBlobReader 3-phase files() (6d),
RuntimeConfig + workspace wiring (6e), invalidation surface (6f),
bench harness (6g).

Numbers (swc, single rule, M1 Pro):
  cold:   Xms   (was 14s)   -> includes WT add + ls-tree + upsert
  warm:   Yms   (was 14s)   -> one GLOB query
  libgit2 fallback:  Zms    (unchanged, phase 3)
  disk:   WT 60MB, index 1.1MB per rev
```

## Blast radius

| file | change | lines |
|---|---|---|
| `v2/tests/smoke/_3_large_repo_timed.sh` | double-run + gates | +30 |
| `v2/tests/smoke/fixtures/sprefa.toml.tmpl` | new fixture | +10 |
| `v2/benches/worktree_path_index.rs` | criterion bench (optional) | +80 |
| `v2/src/readers/_2_git.rs` | SPREFA_TIMING eprintln hooks | +15 |

## Depends on / depended on by

- Depends: 6d (phases implemented), 6e (wired in production). 6f is
  independent; bench runs with invalidation off.
- Depended on: session close. Numbers go in the commit.
